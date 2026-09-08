# armada-bottom-touchpads

Prototype companion for armada's gamescope DRM leasing protocol. It leases
the bottom/secondary screen purely for touch input and forwards it to
InputPlumber as a pair of Steam Deck style touchpads, split down the
center — instead of running a full nested desktop session on that panel
(what `armada-run-bottom` / `bottom-screen-session` do today).

## Why

On devices with a second panel (currently AYANEO Pocket DS), the main
gamescope session leases that panel's DRM connector out to a companion
process over a Unix socket (`GAMESCOPE_LEASE_SOCK`, see gamescope patches
0012/0014/0015/0016/0017 in `../gamescope/patches` and
`../../armada/system_files/usr/bin/armada-run-bottom`). Today the only
companion is a nested gamescope + Plasma Mobile session. This program is an
alternative, much lighter companion: it never renders anything to the
leased panel, it just reads the panel's touch digitizer over the same
socket and turns it into input, so the panel can act as a pair of
touchpads for games instead of a second desktop.

## Protocol

Connects to the Unix socket at `$GAMESCOPE_LEASE_SOCK` (default
`/tmp/gamescope-lease.sock`):

1. On connect, the broker sends one control byte plus (via `SCM_RIGHTS`) a
   dup of the DRM lease fd in the same `sendmsg(2)`:
   - `'L'` — lease granted. We immediately close the fd: we don't render to
     the leased connector, so we have no use for it beyond completing the
     handshake. The panel is left showing whatever gamescope's own
     lease-release blanking (patch 0016) left it at.
   - `'B'` — busy. Something else already holds the lease (most likely
     `armada-bottom-screen.service`). We exit and let systemd retry later.
2. We send `'I'` (request touch forwarding) then `'Y'` (yield the lease to
   `wp_drm_lease_v1` protocol clients, e.g. a VR headset, if one ever asks
   for it — we have nothing worth holding onto against a real client).
3. The broker then streams 20-byte little-endian `DrmLeaseEvent` records:
   `{ u32 type; i32 touch_id; u32 time_ms; f32 x; f32 y; }`, with `x`/`y`
   normalized 0.0-1.0 across the *whole* panel. `type` is one of
   Down=1, Motion=2, Up=3, Suspend=4, Resume=5.
4. On `Suspend` we release any active touches (so InputPlumber doesn't see
   a stuck contact) and reply with a single `'A'` byte; on `Resume` we just
   resume normal handling.

See [`src/lease.rs`](src/lease.rs) for the implementation, including a test
that round-trips this whole handshake against a mock broker.

## Touch splitting

The panel is split vertically down the center:

- `x < 0.5` → left touchpad, rescaled to `x / 0.5`
- `x >= 0.5` → right touchpad, rescaled to `(x - 0.5) / 0.5`
- `y` is passed through unchanged (each pad spans the full panel height)

A touch is assigned to a pad on `Down` based on where it started, and stays
pinned to that pad for the rest of the gesture (coordinates are clamped to
`0.0..=1.0`, not reassigned) so a drag that crosses the center line doesn't
jump to the other pad mid-motion.

## InputPlumber integration

Rather than faking a hidraw/evdev device for InputPlumber to discover
(every existing touchpad driver in InputPlumber — see
`../../InputPlumber/src/drivers/steam_deck` and `.../gpd_win_mini` — is tied
to a specific piece of hardware's report format), this talks directly to
InputPlumber's system D-Bus API, the same way `../../armada/system_files/usr/libexec/armada/controller-type`
already does for switching controller emulation types at runtime:

1. Enumerates `org.shadowblip.InputPlumber`'s composite devices via
   `org.freedesktop.DBus.ObjectManager` at `/org/shadowblip/InputPlumber`,
   and finds the one named `$INPUTPLUMBER_DEVICE_NAME` (default
   `AYANEO Pocket DS`, matching the `name:` field in
   `../inputplumber/patches/0003-feat-Hardware-Support-Add-AYANEO-Pocket-DS.patch`).
2. Reads its current `TargetDevices` and, if `touchpad` isn't already one
   of them, calls `SetTargetDevices` with the current set plus `touchpad`
   appended — mirroring `controller-type`'s `extra_targets()`/`apply_type()`
   pattern of preserving whatever's already attached (the active controller
   type, keyboard, mouse) rather than clobbering it. `touchpad` is detached
   again the same way on clean shutdown (SIGTERM/Ctrl-C). This is a runtime
   toggle, not a static YAML change: the composite device's `target_devices:`
   list in the AYANEO Pocket DS config is left exactly as it was, matching
   how `deck-uhid` and friends are toggled today rather than baked in.
3. Calls `org.shadowblip.Input.CompositeDevice.SendEvent` on it with
   capability strings `Touchpad:LeftPad:Touch:Motion` /
   `Touchpad:RightPad:Touch:Motion` (value: a 2-element array of doubles)
   and `Touchpad:{Left,Right}Pad:Touch:Button:Touch` (value: bool), which
   InputPlumber fans out to that device's currently attached targets.

Calling `SendEvent` requires polkit action `org.shadowblip.Input.*`, which
InputPlumber's shipped rules
(`../../InputPlumber/rootfs/usr/share/polkit-1/rules.d/org.shadowblip.InputPlumber.rules`)
already grant to members of the `wheel` or `inputplumber` group — the
normal deck user on these images. `SetTargetDevices` is covered by that same
rule, and separately granted unconditionally to root by
`../../armada/system_files/usr/share/polkit-1/rules.d/50-armada-inputplumber.rules`
(for `controller-type`, which runs as root via `armada-control`). Either
way, no new polkit policy is needed here.

### Staying attached: reverts and re-resolution (no polling)

`touchpad` is attached once at startup and detached again on every exit path
— clean shutdown (`SIGTERM`/Ctrl-C), the lease socket closing, a read error,
or a D-Bus error — since `serve()` always runs the detach after
`serve_touches()` returns, whatever it returned. The only case that can't be
caught is the process being killed outright (`SIGKILL`, a crash, a power
loss), same as any other userspace cleanup.

While running, two things outside this program's control could otherwise
drop `touchpad` from the composite device or invalidate our handle to it,
without telling us directly:

- **`armada-control`'s `controller-type`** (`../../armada/system_files/usr/libexec/armada/controller-type`)
  calls `SetTargetDevices` with its own target list whenever the user
  switches controller type. Its `extra_targets()` helper already re-adds
  whatever non-controller-type targets were attached (previously just
  `keyboard`/`mouse`) so a controller type switch doesn't lose them —
  `touchpad` has been added to that list, so this case is fixed at the
  source and doesn't need handling on our side at all.
- **InputPlumber re-enumerating the composite device** (e.g. a source
  device, like the AYANEO controller's rumble hidraw node, dropping out and
  back) tears down and recreates it under a new `CompositeDeviceN` object
  path, invalidating our cached D-Bus proxy outright. This one really can
  happen without any signal from us, so it needs handling here.

For that, instead of polling, we watch InputPlumber's own
`org.freedesktop.DBus.ObjectManager` at `/org/shadowblip/InputPlumber` (which
InputPlumber registers and its `ObjectServer` automatically emits
`InterfacesAdded`/`InterfacesRemoved` for on every composite device
create/destroy — confirmed against zbus 5.12's `ObjectServer::at`/`remove`).
`serve_touches` subscribes to both signal streams once and reacts instead of
ticking a timer:

- `InterfacesRemoved` for our current object path → the device we're bound
  to is gone; re-resolve by name.
- `InterfacesAdded` for any new `/CompositeDevice*` path → something may
  have just come back; re-resolve by name (a no-op if it's not ours or
  `touchpad` is already attached).

Re-resolution reuses the same `ObjectManager.GetManagedObjects` lookup as
startup. Both paths are best-effort and only logged on failure, so a
transient D-Bus hiccup doesn't tear down an otherwise-working touch session.
(`org.shadowblip.Input.CompositeDevice`'s `TargetDevices` property doesn't
itself emit `PropertiesChanged` today — the emission call in InputPlumber is
present but commented out — so it can't be watched directly; that's the
other reason the `controller-type` fix above matters, rather than trying to
observe target-list changes as they happen.)

## Known limitations (prototype)

- No click/press support — the panel is a touchscreen, not a clickable
  touchpad, so only `Touch::Button::Touch` (contact) is sent, never
  `Touch::Button::Press`.
- If InputPlumber isn't running, or the named composite device isn't active
  yet, at startup we exit and rely on `Restart=always` rather than retrying
  in a loop. A `SendEvent` touch-motion/button failure mid-session is logged
  and skipped rather than triggering an immediate retry — if the underlying
  cause was the composite device being re-enumerated, the
  `InterfacesAdded`/`Removed` handling above will already be re-resolving it
  in the background, so the next touch event self-heals.
## Packaging

Packaged the same way as `../armada-rgb` (Cargo-based) and `../armada-splash`
(systemd units under `system/usr/...`, `LICENSE.md`/`README.md` in `%files`):
`armada-bottom-touchpads.spec` builds and installs the binary, and
`system/usr/lib/systemd/user/armada-bottom-touchpads.service` — modeled on
`../../armada/system_files/usr/lib/systemd/user/armada-bottom-screen.service`,
including the `Conflicts=` against it since both are mutually exclusive
holders of the same gamescope lease socket — is installed as a systemd user
unit. `build.sh` produces the RPM the same way every other package here does;
see the repo's top-level `Justfile` (`just artifacts armada-bottom-touchpads`)
and `.github/workflows/armada-bottom-touchpads.yml`.

## Building

```bash
cargo build --release
```

## Testing

```bash
cargo test
```

`lease::tests::round_trips_against_a_mock_broker` exercises the real
handshake (including the raw `recvmsg`/`SCM_RIGHTS` code) end to end
against an in-process mock broker speaking the same protocol gamescope
does. The InputPlumber D-Bus side isn't covered by automated tests here —
it needs a real system bus and a running InputPlumber — so exercise it
manually against real hardware (or a `inputplumber` dev instance) with
`INSECURE_DISABLE_POLKIT=1 inputplumber` and `GAMESCOPE_LEASE_SOCK`
pointed at a mock socket like the one in the test above.

## Running

```bash
GAMESCOPE_LEASE_SOCK=/tmp/gamescope-lease.sock \
INPUTPLUMBER_DEVICE_NAME="AYANEO Pocket DS" \
RUST_LOG=info \
./target/release/armada-bottom-touchpads
```
