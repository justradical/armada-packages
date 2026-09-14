# Patches

Patches applied on top of BASE.env. Each entry's `source` is an upstream URL pinned
to a commit, or `armada` if it is original; a URL source with no `notes` is verbatim.
`notes` mean the file was modified.

- `patches/0001-skip-partitions-with-no-extents.patch`
  source: armada
  notes: On a Virtual A/B device (AYN Thor, freshly EDL'd) slot 0 defines vendor_b with zero extents. The tool created that empty mapping first, so slot 2's real 8-extent vendor_b failed DM_DEVICE_CREATE silently, leaving msm-firmware-loader an empty device to mount. Skips zero-extent definitions and reports create failures.
