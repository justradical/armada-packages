//! Client for gamescope's DRM lease companion socket (armada's leasing
//! protocol; see gamescope patches 0012/0014/0015/0016/0017 in
//! armada-packages/gamescope). A single control byte is exchanged over a
//! `SOCK_STREAM` Unix socket, plus one SCM_RIGHTS-carried DRM lease fd on
//! connect, followed by a stream of fixed-size touch/control events.

use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::net::UnixStream as StdUnixStream;

use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

/// Size in bytes of the wire `DrmLeaseEvent` struct sent by the broker:
/// `{ u32 type; i32 touch_id; u32 time_ms; f32 x; f32 y; }`.
const RAW_EVENT_SIZE: usize = 20;

#[derive(Debug, Error)]
pub enum LeaseError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("lease is held by another companion")]
    Busy,
    #[error("lease broker sent an unexpected handshake byte: {0:#x}")]
    BadHandshake(u8),
    #[error("lease broker did not hand over a lease file descriptor")]
    NoFd,
}

/// Touch/control event as sent by the broker, in the units it uses on the
/// wire: `x`/`y` are normalized 0.0-1.0 across the whole leased panel.
#[derive(Debug, Clone, Copy)]
pub struct RawEvent {
    pub kind: EventKind,
    pub touch_id: i32,
    #[allow(dead_code)]
    pub time_ms: u32,
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    Down,
    Motion,
    Up,
    Suspend,
    Resume,
    Unknown(u32),
}

impl From<u32> for EventKind {
    fn from(value: u32) -> Self {
        match value {
            1 => EventKind::Down,
            2 => EventKind::Motion,
            3 => EventKind::Up,
            4 => EventKind::Suspend,
            5 => EventKind::Resume,
            other => EventKind::Unknown(other),
        }
    }
}

impl RawEvent {
    fn parse(bytes: &[u8; RAW_EVENT_SIZE]) -> Self {
        let kind = u32::from_le_bytes(bytes[0..4].try_into().unwrap()).into();
        let touch_id = i32::from_le_bytes(bytes[4..8].try_into().unwrap());
        let time_ms = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
        let x = f32::from_le_bytes(bytes[12..16].try_into().unwrap());
        let y = f32::from_le_bytes(bytes[16..20].try_into().unwrap());
        Self {
            kind,
            touch_id,
            time_ms,
            x,
            y,
        }
    }
}

/// Connects to the lease broker and completes the handshake synchronously:
/// receive the lease fd, request touch forwarding, and declare that we
/// yield the lease to drm-lease-v1 (VR-style) protocol clients since we
/// don't render anything of our own worth holding onto.
///
/// Runs plain blocking syscalls, so callers on a tokio runtime should run
/// this inside `spawn_blocking`.
pub fn connect(socket_path: &str) -> Result<StdUnixStream, LeaseError> {
    let stream = StdUnixStream::connect(socket_path)?;

    let (handshake_byte, lease_fd) = recv_handshake(&stream)?;
    match handshake_byte {
        b'L' => {}
        b'B' => return Err(LeaseError::Busy),
        other => return Err(LeaseError::BadHandshake(other)),
    }
    // We only forward touch input; we never render to the leased connector,
    // so the fd itself is of no use to us beyond completing the handshake.
    drop(lease_fd.ok_or(LeaseError::NoFd)?);

    send_byte(&stream, b'I')?;
    send_byte(&stream, b'Y')?;

    Ok(stream)
}

fn send_byte(stream: &StdUnixStream, byte: u8) -> io::Result<()> {
    use std::io::Write;
    (&*stream).write_all(&[byte])
}

/// Reads the one-byte handshake response and, if present, the SCM_RIGHTS fd
/// sent alongside it. `std::os::unix::net::UnixStream` has no ancillary-data
/// API, so this drops to a raw `recvmsg(2)`.
fn recv_handshake(stream: &StdUnixStream) -> Result<(u8, Option<OwnedFd>), LeaseError> {
    let raw_fd = stream.as_raw_fd();

    let mut data_buf = [0u8; 1];
    let mut iov = libc::iovec {
        iov_base: data_buf.as_mut_ptr() as *mut libc::c_void,
        iov_len: data_buf.len(),
    };
    // Comfortably larger than CMSG_SPACE(sizeof(int)) on every supported ABI.
    let mut cmsg_buf = [0u8; 64];

    let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
    msg.msg_iov = &mut iov;
    msg.msg_iovlen = 1;
    msg.msg_control = cmsg_buf.as_mut_ptr() as *mut libc::c_void;
    msg.msg_controllen = cmsg_buf.len() as _;

    let n = unsafe { libc::recvmsg(raw_fd, &mut msg, 0) };
    if n < 0 {
        return Err(LeaseError::Io(io::Error::last_os_error()));
    }
    if n == 0 {
        return Err(LeaseError::Io(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "lease broker closed the connection during handshake",
        )));
    }

    let mut fd = None;
    unsafe {
        let mut cmsg = libc::CMSG_FIRSTHDR(&msg);
        while !cmsg.is_null() {
            if (*cmsg).cmsg_level == libc::SOL_SOCKET && (*cmsg).cmsg_type == libc::SCM_RIGHTS {
                let data = libc::CMSG_DATA(cmsg) as *const libc::c_int;
                let raw: RawFd = data.read_unaligned();
                fd = Some(OwnedFd::from_raw_fd(raw));
                break;
            }
            cmsg = libc::CMSG_NXTHDR(&msg, cmsg);
        }
    }

    Ok((data_buf[0], fd))
}

/// Wraps the handshaken socket for the async event stream: framing the
/// fixed-size records the broker sends, and acknowledging suspend requests.
pub struct EventStream {
    stream: UnixStream,
    buf: Vec<u8>,
}

impl EventStream {
    pub fn new(handshaken: StdUnixStream) -> io::Result<Self> {
        handshaken.set_nonblocking(true)?;
        let stream = UnixStream::from_std(handshaken)?;
        Ok(Self {
            stream,
            buf: Vec::with_capacity(RAW_EVENT_SIZE * 8),
        })
    }

    /// Reads the next event, or `None` on a clean disconnect at a record
    /// boundary (the common case: the other end went away, or gamescope
    /// exited).
    pub async fn next_event(&mut self) -> io::Result<Option<RawEvent>> {
        let mut chunk = [0u8; 256];
        while self.buf.len() < RAW_EVENT_SIZE {
            let n = self.stream.read(&mut chunk).await?;
            if n == 0 {
                if self.buf.is_empty() {
                    return Ok(None);
                }
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "lease socket closed mid-event",
                ));
            }
            self.buf.extend_from_slice(&chunk[..n]);
        }
        let record: [u8; RAW_EVENT_SIZE] = self.buf[..RAW_EVENT_SIZE].try_into().unwrap();
        self.buf.drain(..RAW_EVENT_SIZE);
        Ok(Some(RawEvent::parse(&record)))
    }

    /// Acknowledges a `Suspend` control event once we've stopped acting on
    /// touch input, per the yield protocol in gamescope patch 0017.
    pub async fn ack_suspend(&mut self) -> io::Result<()> {
        self.stream.write_all(b"A").await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_motion_event() {
        let mut bytes = [0u8; RAW_EVENT_SIZE];
        bytes[0..4].copy_from_slice(&2u32.to_le_bytes()); // Motion
        bytes[4..8].copy_from_slice(&7i32.to_le_bytes()); // touch_id
        bytes[8..12].copy_from_slice(&1234u32.to_le_bytes()); // time_ms
        bytes[12..16].copy_from_slice(&0.75f32.to_le_bytes()); // x
        bytes[16..20].copy_from_slice(&0.25f32.to_le_bytes()); // y

        let event = RawEvent::parse(&bytes);
        assert_eq!(event.kind, EventKind::Motion);
        assert_eq!(event.touch_id, 7);
        assert_eq!(event.time_ms, 1234);
        assert_eq!(event.x, 0.75);
        assert_eq!(event.y, 0.25);
    }

    #[test]
    fn maps_known_event_kinds() {
        assert_eq!(EventKind::from(1), EventKind::Down);
        assert_eq!(EventKind::from(2), EventKind::Motion);
        assert_eq!(EventKind::from(3), EventKind::Up);
        assert_eq!(EventKind::from(4), EventKind::Suspend);
        assert_eq!(EventKind::from(5), EventKind::Resume);
        assert_eq!(EventKind::from(99), EventKind::Unknown(99));
    }

    /// Exercises the real handshake (including the raw `recvmsg`/SCM_RIGHTS
    /// code) and event framing against a mock broker that speaks the same
    /// protocol gamescope does, standing in for it end to end.
    #[tokio::test]
    async fn round_trips_against_a_mock_broker() {
        let socket_path = std::env::temp_dir().join(format!(
            "armada-bottom-touchpads-test-{}.sock",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&socket_path);
        let listener = std::os::unix::net::UnixListener::bind(&socket_path).unwrap();

        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            send_handshake_fd(&stream, b'L');

            // Client should request touch input and declare it yields.
            let mut req = [0u8; 2];
            std::io::Read::read_exact(&mut &stream, &mut req).unwrap();
            assert_eq!(&req, b"IY");

            for event in [
                encode_event(1, 5, 1000, 0.2, 0.5), // Down
                encode_event(2, 5, 1010, 0.6, 0.5), // Motion
                encode_event(3, 5, 1020, 0.0, 0.0), // Up
            ] {
                std::io::Write::write_all(&mut &stream, &event).unwrap();
            }

            // Suspend, then expect the client's 'A' ack.
            std::io::Write::write_all(&mut &stream, &encode_event(4, 0, 0, 0.0, 0.0)).unwrap();
            let mut ack = [0u8; 1];
            std::io::Read::read_exact(&mut &stream, &mut ack).unwrap();
            assert_eq!(&ack, b"A");
        });

        let socket_path_str = socket_path.to_str().unwrap().to_string();
        let std_stream = tokio::task::spawn_blocking(move || connect(&socket_path_str))
            .await
            .unwrap()
            .unwrap();
        let mut events = EventStream::new(std_stream).unwrap();

        let down = events.next_event().await.unwrap().unwrap();
        assert_eq!(down.kind, EventKind::Down);
        assert_eq!(down.touch_id, 5);
        assert_eq!(down.x, 0.2);

        let motion = events.next_event().await.unwrap().unwrap();
        assert_eq!(motion.kind, EventKind::Motion);
        assert_eq!(motion.x, 0.6);

        let up = events.next_event().await.unwrap().unwrap();
        assert_eq!(up.kind, EventKind::Up);

        let suspend = events.next_event().await.unwrap().unwrap();
        assert_eq!(suspend.kind, EventKind::Suspend);
        events.ack_suspend().await.unwrap();

        server.join().unwrap();
        let _ = std::fs::remove_file(&socket_path);
    }

    fn encode_event(
        kind: u32,
        touch_id: i32,
        time_ms: u32,
        x: f32,
        y: f32,
    ) -> [u8; RAW_EVENT_SIZE] {
        let mut bytes = [0u8; RAW_EVENT_SIZE];
        bytes[0..4].copy_from_slice(&kind.to_le_bytes());
        bytes[4..8].copy_from_slice(&touch_id.to_le_bytes());
        bytes[8..12].copy_from_slice(&time_ms.to_le_bytes());
        bytes[12..16].copy_from_slice(&x.to_le_bytes());
        bytes[16..20].copy_from_slice(&y.to_le_bytes());
        bytes
    }

    /// Sends the one-byte handshake response plus a dummy SCM_RIGHTS fd
    /// (an `/dev/null` fd; its identity doesn't matter, only that one
    /// arrives), mirroring what gamescope's broker does on connect.
    fn send_handshake_fd(stream: &StdUnixStream, byte: u8) {
        let dummy = std::fs::File::open("/dev/null").unwrap();
        let dummy_fd = dummy.as_raw_fd();

        let data = [byte];
        let mut iov = libc::iovec {
            iov_base: data.as_ptr() as *mut libc::c_void,
            iov_len: data.len(),
        };
        let mut cmsg_buf = [0u8; 64];
        let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
        msg.msg_iov = &mut iov;
        msg.msg_iovlen = 1;
        msg.msg_control = cmsg_buf.as_mut_ptr() as *mut libc::c_void;
        msg.msg_controllen = std::mem::size_of::<libc::cmsghdr>() as _;
        // Grow controllen to fit exactly one fd, mirroring CMSG_SPACE(4).
        msg.msg_controllen = unsafe {
            let cmsg = libc::CMSG_FIRSTHDR(&msg);
            (*cmsg).cmsg_level = libc::SOL_SOCKET;
            (*cmsg).cmsg_type = libc::SCM_RIGHTS;
            (*cmsg).cmsg_len = libc::CMSG_LEN(std::mem::size_of::<libc::c_int>() as _) as _;
            let data_ptr = libc::CMSG_DATA(cmsg) as *mut libc::c_int;
            data_ptr.write_unaligned(dummy_fd);
            libc::CMSG_SPACE(std::mem::size_of::<libc::c_int>() as _) as _
        };

        let n = unsafe { libc::sendmsg(stream.as_raw_fd(), &msg, 0) };
        assert!(n > 0, "sendmsg failed: {}", io::Error::last_os_error());
    }
}
