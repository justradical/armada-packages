//! Talks to InputPlumber over its system D-Bus API to (1) make sure the
//! `touchpad` target device is attached to the composite device, the same
//! way `armada-control`'s `controller-type` helper toggles targets like
//! `deck-uhid` at runtime instead of baking them into the composite device's
//! YAML, and (2) inject native `Touchpad:LeftPad`/`Touchpad:RightPad`
//! capability events into it via `SendEvent`, without needing a real
//! hidraw/evdev source device.
//!
//! Access is gated by polkit action `org.shadowblip.Input.*`, which
//! InputPlumber's shipped rules grant to members of the `inputplumber` or
//! `wheel` group (see InputPlumber's
//! `rootfs/usr/share/polkit-1/rules.d/org.shadowblip.InputPlumber.rules`);
//! `SetTargetDevices` is additionally granted unconditionally to root by
//! `armada/system_files/usr/share/polkit-1/rules.d/50-armada-inputplumber.rules`.

use zbus::{
    proxy,
    zvariant::{OwnedObjectPath, Value},
    Connection,
};

pub(crate) const BUS_NAME: &str = "org.shadowblip.InputPlumber";
pub(crate) const BUS_PREFIX: &str = "/org/shadowblip/InputPlumber";
const TOUCHPAD_TARGET: &str = "touchpad";

#[proxy(
    default_service = "org.shadowblip.InputPlumber",
    interface = "org.shadowblip.Input.CompositeDevice"
)]
trait CompositeDevice {
    #[zbus(property)]
    fn name(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn target_devices(&self) -> zbus::Result<Vec<String>>;

    fn send_event(&self, event: &str, value: Value<'_>) -> zbus::Result<()>;

    fn set_target_devices(&self, target_device_types: Vec<String>) -> zbus::Result<()>;
}

/// Turns a target device object path (e.g.
/// `/org/shadowblip/InputPlumber/devices/target/touchpad3`) into the target
/// type name InputPlumber's `SetTargetDevices` expects (`touchpad`),
/// mirroring `controller-type`'s `extra_targets()`.
fn target_type_name(path: &str) -> &str {
    let segment = path.rsplit('/').next().unwrap_or(path);
    segment.trim_end_matches(|c: char| c.is_ascii_digit())
}

/// One of the Steam Deck style touchpads. `CenterPad` also exists in
/// InputPlumber's capability model but has no natural counterpart on a
/// screen split down the middle, so it's unused here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pad {
    Left,
    Right,
}

impl Pad {
    fn capability_name(self) -> &'static str {
        match self {
            Pad::Left => "LeftPad",
            Pad::Right => "RightPad",
        }
    }

    /// The multi-touch slot this pad's contact is reported on in
    /// InputPlumber's `touchpad` target device. Both `LeftPad` and
    /// `RightPad` route through that single shared virtual device (see
    /// `InputPlumber/src/input/target/touchpad.rs`), so without distinct
    /// slots a simultaneous touch on each pad would collide on the same
    /// one — the real Steam Deck driver hardcodes slot 0 for both
    /// (`driver.rs`, `index: 0, // TODO: Something else`) and so can't
    /// represent that case; giving each pad its own fixed slot here avoids
    /// inheriting that limitation.
    fn touch_slot(self) -> u8 {
        match self {
            Pad::Left => 0,
            Pad::Right => 1,
        }
    }
}

impl std::fmt::Display for Pad {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.capability_name())
    }
}

pub struct InputPlumber<'a> {
    device: CompositeDeviceProxy<'a>,
    device_name: String,
    object_path: OwnedObjectPath,
}

impl InputPlumber<'_> {
    /// Finds the composite device with the given name (as configured by its
    /// `name:` field, e.g. in a `devices/*.yaml` composite device config)
    /// among InputPlumber's currently active devices.
    pub async fn find(conn: &Connection, device_name: &str) -> zbus::Result<Self> {
        let object_manager = zbus::fdo::ObjectManagerProxy::builder(conn)
            .destination(BUS_NAME)?
            .path(BUS_PREFIX)?
            .build()
            .await?;
        let objects = object_manager.get_managed_objects().await?;

        for path in objects.keys() {
            if !path.as_str().contains("/CompositeDevice") {
                continue;
            }
            let device = CompositeDeviceProxy::builder(conn)
                .path(path.to_owned())?
                .build()
                .await?;
            let Ok(name) = device.name().await else {
                continue;
            };
            if name == device_name {
                log::info!("found InputPlumber composite device '{name}' at {path}");
                return Ok(Self {
                    device,
                    device_name: name,
                    object_path: path.to_owned(),
                });
            }
        }

        Err(zbus::Error::Failure(format!(
            "no active InputPlumber composite device named '{device_name}'"
        )))
    }

    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    /// The composite device's D-Bus object path, e.g.
    /// `/org/shadowblip/InputPlumber/CompositeDevice0`. InputPlumber
    /// re-enumerating the device (a source device dropping out and back)
    /// assigns a new one, invalidating this handle — callers watch for that
    /// via `InterfacesRemoved` on this path rather than polling.
    pub fn object_path(&self) -> &OwnedObjectPath {
        &self.object_path
    }

    async fn current_target_types(&self) -> zbus::Result<Vec<String>> {
        let paths = self.device.target_devices().await?;
        Ok(paths
            .iter()
            .map(|path| target_type_name(path).to_string())
            .collect())
    }

    /// Adds `touchpad` to the composite device's target devices if it isn't
    /// already there, preserving whatever's currently attached (the active
    /// controller type, keyboard, mouse, ...) exactly like `controller-type
    /// set` preserves keyboard/mouse when switching controller types.
    pub async fn ensure_touchpad_target(&self) -> zbus::Result<()> {
        let mut targets = self.current_target_types().await?;
        if targets.iter().any(|t| t == TOUCHPAD_TARGET) {
            return Ok(());
        }
        targets.push(TOUCHPAD_TARGET.to_string());
        log::info!("attaching '{TOUCHPAD_TARGET}' target device: {targets:?}");
        self.device.set_target_devices(targets).await
    }

    /// Detaches `touchpad` again, e.g. on clean shutdown, so a composite
    /// device we're no longer feeding doesn't keep an idle touchpad target
    /// attached.
    pub async fn release_touchpad_target(&self) -> zbus::Result<()> {
        let mut targets = self.current_target_types().await?;
        let before = targets.len();
        targets.retain(|t| t != TOUCHPAD_TARGET);
        if targets.len() == before {
            return Ok(());
        }
        log::info!("detaching '{TOUCHPAD_TARGET}' target device: {targets:?}");
        self.device.set_target_devices(targets).await
    }

    /// Reports a touch's position and contact state for `pad`. The
    /// `touchpad` target's motion handler needs this bundled into one
    /// `InputValue::Touch` — a plain (x, y) `Vector2` doesn't carry
    /// `is_touching`, so it's silently ignored — hence the 5-element
    /// `[index, is_touching, x, y, pressure]` encoding InputPlumber's
    /// `SendEvent` maps to `InputValue::Touch` (see InputPlumber patch
    /// 0004-feat-send_event-support-touch-values.patch).
    pub async fn touch_motion(
        &self,
        pad: Pad,
        is_touching: bool,
        x: f64,
        y: f64,
    ) -> zbus::Result<()> {
        let event = format!("Touchpad:{pad}:Touch:Motion");
        let index = pad.touch_slot() as f64;
        let is_touching = if is_touching { 1.0 } else { 0.0 };
        let pressure = 1.0;
        self.device
            .send_event(
                &event,
                Value::from(vec![index, is_touching, x, y, pressure]),
            )
            .await
    }

    /// Reports finger contact for `pad` as its own capability, matching
    /// the real Steam Deck driver (which sends this alongside, not instead
    /// of, touch motion). InputPlumber's `touchpad` target itself doesn't
    /// use it — it derives touch state from `touch_motion`'s embedded
    /// `is_touching` — but other targets (e.g. a gamepad's touch-sensor
    /// indicator) may.
    pub async fn touch_button(&self, pad: Pad, is_touching: bool) -> zbus::Result<()> {
        let event = format!("Touchpad:{pad}:Touch:Button:Touch");
        self.device
            .send_event(&event, Value::from(is_touching))
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_the_trailing_index_from_a_target_path() {
        assert_eq!(
            target_type_name("/org/shadowblip/InputPlumber/devices/target/touchpad3"),
            "touchpad"
        );
        assert_eq!(
            target_type_name("/org/shadowblip/InputPlumber/devices/target/deck-uhid0"),
            "deck-uhid"
        );
        assert_eq!(target_type_name("touchpad"), "touchpad");
    }
}
