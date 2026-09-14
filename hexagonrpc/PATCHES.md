# Patches

Patches applied on top of BASE.env. Each entry's `source` is an upstream URL pinned
to a commit, or `armada` if it is original; a URL source with no `notes` is verbatim.
`notes` mean the file was modified.

- `patches/0001-data-install-units-to-the-canonical-systemd-unit-dir.patch`
  source: armada
- `patches/0002-use-msm-firmware-loader-dir.patch`
  source: armada
- `patches/0003-run-hexagonrpcd-as-root.patch`
  source: armada
  notes: The fastrpc user only had udev-granted access to /dev/fastrpc-*, not to the root-only vendor/persist/dsp paths msm-firmware-loader stages under /run/msm-firmware-loader/hexagonrpc. Run as root instead of chasing that with more ACLs/udev rules.
- `patches/0004-bring-hexagonrpcd-back-after-resume.patch`
  source: armada
  notes: Conflicts=suspend.target stops the hexagonrpcd units on suspend and nothing starts them again, so they stay dead until reboot. Adds a oneshot ordered after suspend.target that restarts the enabled ones.
- `patches/0005-gate-hexagonrpcd-on-staged-hexagonfs-tree.patch`
  source: armada
  notes: ConditionPathExists for the README's HexagonFS paths, under msm-firmware-loader's staged prefix, so a device whose firmware partitions never mounted skips the unit instead of restart-looping and thrashing the FastRPC node. acdb is gated on rootpd only - sensorspd was verified to bring up the Sensor Core service without it (acdb removed, ADSP restarted to force a fresh PD init); sdsp is assumed to match sensorspd but is untested, no available device exposes /dev/fastrpc-sdsp. Paths are tied to the -R prefix from 0002.
