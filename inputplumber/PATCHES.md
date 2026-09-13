# Patches

Patches applied on top of BASE.env. Each entry's `source` is an upstream URL pinned
to a commit, or `armada` if it's original; a URL source with no `notes` is verbatim.
`notes` mean the file was modified.

- `patches/0001-fix-gamepad-honor-passthrough-config-skip-exclusive-grab.patch`
  source: armada
- `patches/0002-fix-force-feedback-reset-effects-when-replacing-targets.patch`
  source: armada
- `patches/0003-feat-Hardware-Support-Add-AYANEO-Pocket-DS.patch`
  source: armada
- `patches/0004-feat-Hardware-Support-Add-AYN-Thor-Lite.patch`
  source: armada
  notes: Matches the Thor Lite device-tree compatible and its Retroid-protocol MCU gamepad, reusing the existing Retroid Type 1 capability map.
- `patches/0005-feat-Hardware-Support-Qualcomm-SSC-sensors.patch`
  source: https://github.com/ShadowBlip/InputPlumber/pull/590
  notes: rebased on latest InputPlumber
- `patches/0005-add-fastrpc-config-to-devices.patch`
  source: armada
