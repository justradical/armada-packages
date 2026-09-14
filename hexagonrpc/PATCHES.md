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
