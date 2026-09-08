//! Bridges gamescope's DRM lease companion socket to InputPlumber.
//!
//! When a device's secondary/bottom panel is leased out (see
//! `--lease-connector` / `--drm-lease-client` in armada-packages/gamescope
//! and the AYANEO Pocket DS bottom-screen setup in armada), whatever holds
//! the lease socket receives that panel's touch input directly instead of
//! it flowing through the normal desktop session. This program is a
//! lightweight companion for that socket: rather than running a nested
//! compositor on the panel (as `armada-run-bottom` does for the Plasma
//! Mobile bottom screen), it treats the panel purely as a touch surface,
//! splits it down the center, and forwards each half to InputPlumber as an
//! independent Steam Deck style touchpad (`Touchpad:LeftPad` /
//! `Touchpad:RightPad`).
//!
//! It never touches the DRM lease fd itself (no rendering), so the panel's
//! contents are left exactly as gamescope's own lease-release blanking
//! (patch 0016) leaves them.

mod inputplumber;
mod lease;

use std::collections::HashMap;
use std::env;
use std::process::ExitCode;

use futures_util::StreamExt;
use inputplumber::{InputPlumber, Pad};
use lease::EventKind;
use tokio::signal::unix::{signal, SignalKind};
use zbus::Connection;

const DEFAULT_SOCKET_PATH: &str = "/tmp/gamescope-lease.sock";
const DEFAULT_DEVICE_NAME: &str = "AYANEO Pocket DS";

fn rescale(pad: Pad, x: f32, y: f32) -> (f64, f64) {
    let x = match pad {
        Pad::Left => x as f64 / 0.5,
        Pad::Right => (x as f64 - 0.5) / 0.5,
    };
    (x.clamp(0.0, 1.0), (y as f64).clamp(0.0, 1.0))
}

fn pad_for_x(x: f32) -> Pad {
    if x < 0.5 {
        Pad::Left
    } else {
        Pad::Right
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    env_logger::init();

    let socket_path =
        env::var("GAMESCOPE_LEASE_SOCK").unwrap_or_else(|_| DEFAULT_SOCKET_PATH.to_string());
    let device_name =
        env::var("INPUTPLUMBER_DEVICE_NAME").unwrap_or_else(|_| DEFAULT_DEVICE_NAME.to_string());

    match run(&socket_path, &device_name).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(Fatal(msg)) => {
            log::error!("{msg}");
            ExitCode::FAILURE
        }
        Err(Retryable(msg)) => {
            // Expected/transient: the bottom screen is in use by something
            // else, or gamescope hasn't created the socket yet. Exit clean
            // so `Restart=always` in the systemd unit just tries again.
            log::info!("{msg}");
            ExitCode::SUCCESS
        }
    }
}

enum RunError {
    Fatal(String),
    Retryable(String),
}
use RunError::{Fatal, Retryable};

async fn run(socket_path: &str, device_name: &str) -> Result<(), RunError> {
    let conn = Connection::system()
        .await
        .map_err(|e| Fatal(format!("could not connect to the system D-Bus: {e}")))?;

    let mut ip = InputPlumber::find(&conn, device_name).await.map_err(|e| {
        Retryable(format!(
            "InputPlumber device '{device_name}' not ready: {e}"
        ))
    })?;
    log::info!(
        "forwarding bottom-screen touch to InputPlumber composite device '{}'",
        ip.device_name()
    );

    serve(&conn, device_name, &mut ip, socket_path).await
}

/// Attaches the `touchpad` target device for the duration of the session,
/// same as `controller-type` toggles targets at runtime rather than baking
/// them into the composite device's YAML, and detaches it again on the way
/// out so an idle companion doesn't leave a dangling touchpad target
/// (including on error exits: whatever `serve_touches` returns, we still
/// attempt the detach before propagating it).
async fn serve(
    conn: &Connection,
    device_name: &str,
    ip: &mut InputPlumber<'_>,
    socket_path: &str,
) -> Result<(), RunError> {
    ip.ensure_touchpad_target()
        .await
        .map_err(|e| Retryable(format!("could not attach touchpad target device: {e}")))?;

    let result = serve_touches(conn, device_name, ip, socket_path).await;

    if let Err(e) = ip.release_touchpad_target().await {
        log::warn!("could not detach touchpad target device: {e}");
    }

    result
}

/// Re-resolves the composite device by name and re-attaches `touchpad`.
/// Called when we've learned (via an `InterfacesAdded`/`InterfacesRemoved`
/// signal, not polling) that it's worth checking: our object path was
/// removed, or a new `CompositeDevice*` path showed up. Best-effort:
/// failures are logged, not propagated, so a transient D-Bus hiccup doesn't
/// tear down an otherwise-working touch session.
async fn reresolve_touchpad_target(
    conn: &Connection,
    device_name: &str,
    ip: &mut InputPlumber<'_>,
) {
    match InputPlumber::find(conn, device_name).await {
        Ok(fresh) => {
            let path_changed = fresh.object_path() != ip.object_path();
            *ip = fresh;
            match ip.ensure_touchpad_target().await {
                Ok(()) if path_changed => {
                    log::info!("re-attached touchpad target after composite device changed")
                }
                Ok(()) => {}
                Err(e) => log::warn!("could not attach touchpad target: {e}"),
            }
        }
        Err(e) => log::warn!("could not re-resolve composite device '{device_name}': {e}"),
    }
}

async fn serve_touches(
    conn: &Connection,
    device_name: &str,
    ip: &mut InputPlumber<'_>,
    socket_path: &str,
) -> Result<(), RunError> {
    let socket_path_owned = socket_path.to_string();
    let handshaken = tokio::task::spawn_blocking(move || lease::connect(&socket_path_owned))
        .await
        .map_err(|e| Fatal(format!("lease handshake task panicked: {e}")))?
        .map_err(|e| match e {
            lease::LeaseError::Busy => {
                Retryable(format!("gamescope lease socket at {socket_path} is busy"))
            }
            other => Retryable(format!(
                "could not connect to gamescope lease socket at {socket_path}: {other}"
            )),
        })?;
    log::info!("holding DRM lease companion socket at {socket_path}");

    let mut events = lease::EventStream::new(handshaken)
        .map_err(|e| Fatal(format!("could not set up lease event stream: {e}")))?;
    let mut sigterm = signal(SignalKind::terminate())
        .map_err(|e| Fatal(format!("could not install SIGTERM handler: {e}")))?;

    // Watch InputPlumber's own ObjectManager instead of polling: it emits
    // InterfacesRemoved/Added when a composite device is torn down and
    // recreated (e.g. a source device dropping out and back), which is the
    // only thing left that can invalidate our object path now that
    // `controller-type` preserves the `touchpad` target across controller
    // type switches (see armada/system_files/usr/libexec/armada/controller-type).
    let object_manager = zbus::fdo::ObjectManagerProxy::builder(conn)
        .destination(inputplumber::BUS_NAME)
        .map_err(|e| Fatal(format!("could not build ObjectManager proxy: {e}")))?
        .path(inputplumber::BUS_PREFIX)
        .map_err(|e| Fatal(format!("could not build ObjectManager proxy: {e}")))?
        .build()
        .await
        .map_err(|e| Fatal(format!("could not watch InputPlumber's ObjectManager: {e}")))?;
    let mut devices_removed = object_manager
        .receive_interfaces_removed()
        .await
        .map_err(|e| {
            Fatal(format!(
                "could not watch for removed InputPlumber devices: {e}"
            ))
        })?;
    let mut devices_added = object_manager
        .receive_interfaces_added()
        .await
        .map_err(|e| {
            Fatal(format!(
                "could not watch for added InputPlumber devices: {e}"
            ))
        })?;

    // Which pad (if any) each active touch id landed on, so a drag that
    // crosses the center line stays pinned to the pad it started on instead
    // of jumping to the other one mid-gesture.
    let mut active_touches: HashMap<i32, Pad> = HashMap::new();

    loop {
        let event = tokio::select! {
            _ = sigterm.recv() => {
                log::info!("received SIGTERM; releasing active touches");
                release_all(ip, &mut active_touches).await;
                return Ok(());
            }
            _ = tokio::signal::ctrl_c() => {
                log::info!("received interrupt; releasing active touches");
                release_all(ip, &mut active_touches).await;
                return Ok(());
            }
            Some(signal) = devices_removed.next() => {
                let is_ours = signal
                    .args()
                    .map(|args| args.object_path().as_str() == ip.object_path().as_str())
                    .unwrap_or(false);
                if is_ours {
                    log::info!("InputPlumber composite device '{device_name}' disappeared; re-resolving");
                    reresolve_touchpad_target(conn, device_name, ip).await;
                }
                continue;
            }
            Some(signal) = devices_added.next() => {
                let is_composite_device = signal
                    .args()
                    .map(|args| args.object_path().as_str().contains("/CompositeDevice"))
                    .unwrap_or(false);
                if is_composite_device {
                    reresolve_touchpad_target(conn, device_name, ip).await;
                }
                continue;
            }
            event = events.next_event() => {
                event.map_err(|e| Retryable(format!("lease socket read failed: {e}")))?
            }
        };
        let Some(event) = event else {
            return Err(Retryable(
                "gamescope lease broker closed the companion socket".to_string(),
            ));
        };

        match event.kind {
            EventKind::Down => {
                let pad = pad_for_x(event.x);
                active_touches.insert(event.touch_id, pad);
                let (x, y) = rescale(pad, event.x, event.y);
                log_dbus_err(ip.touch_motion(pad, x, y).await, "touch motion");
                log_dbus_err(ip.touch_button(pad, true).await, "touch down");
            }
            EventKind::Motion => {
                let pad = match active_touches.get(&event.touch_id) {
                    Some(pad) => *pad,
                    None => {
                        // Missed the Down for this id; recover instead of
                        // dropping the gesture on the floor.
                        let pad = pad_for_x(event.x);
                        active_touches.insert(event.touch_id, pad);
                        log_dbus_err(ip.touch_button(pad, true).await, "touch down (recovered)");
                        pad
                    }
                };
                let (x, y) = rescale(pad, event.x, event.y);
                log_dbus_err(ip.touch_motion(pad, x, y).await, "touch motion");
            }
            EventKind::Up => {
                if let Some(pad) = active_touches.remove(&event.touch_id) {
                    log_dbus_err(ip.touch_button(pad, false).await, "touch up");
                }
            }
            EventKind::Suspend => {
                log::info!("lease suspended by broker; releasing active touches");
                release_all(ip, &mut active_touches).await;
                if let Err(e) = events.ack_suspend().await {
                    return Err(Retryable(format!("could not ack suspend: {e}")));
                }
            }
            EventKind::Resume => {
                log::info!("lease resumed by broker");
            }
            EventKind::Unknown(kind) => {
                log::warn!("ignoring unknown lease event kind {kind}");
            }
        }
    }
}

async fn release_all(ip: &InputPlumber<'_>, active_touches: &mut HashMap<i32, Pad>) {
    for (_, pad) in active_touches.drain() {
        log_dbus_err(ip.touch_button(pad, false).await, "touch up");
    }
}

fn log_dbus_err(result: zbus::Result<()>, what: &str) {
    if let Err(e) = result {
        log::warn!("failed to send {what} to InputPlumber: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_at_the_center() {
        assert_eq!(pad_for_x(0.0), Pad::Left);
        assert_eq!(pad_for_x(0.49), Pad::Left);
        assert_eq!(pad_for_x(0.5), Pad::Right);
        assert_eq!(pad_for_x(1.0), Pad::Right);
    }

    #[test]
    fn rescales_each_half_to_full_range() {
        let (x, y) = rescale(Pad::Left, 0.0, 0.5);
        assert_eq!((x, y), (0.0, 0.5));
        let (x, _) = rescale(Pad::Left, 0.25, 0.0);
        assert_eq!(x, 0.5);
        let (x, _) = rescale(Pad::Left, 0.5, 0.0);
        assert_eq!(x, 1.0);

        let (x, _) = rescale(Pad::Right, 0.5, 0.0);
        assert_eq!(x, 0.0);
        let (x, _) = rescale(Pad::Right, 0.75, 0.0);
        assert_eq!(x, 0.5);
        let (x, _) = rescale(Pad::Right, 1.0, 0.0);
        assert_eq!(x, 1.0);
    }

    #[test]
    fn clamps_out_of_range_coordinates() {
        // A drag that overshoots past the panel edge, or crosses back over
        // the center line while pinned to the pad it started on.
        let (x, _) = rescale(Pad::Left, 0.9, 0.0);
        assert_eq!(x, 1.0);
        let (x, _) = rescale(Pad::Right, 0.1, 0.0);
        assert_eq!(x, 0.0);
    }
}
