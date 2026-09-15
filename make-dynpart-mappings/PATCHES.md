# Patches

Patches applied on top of BASE.env. Each entry's `source` is an upstream URL pinned
to a commit, or `armada` if it is original; a URL source with no `notes` is verbatim.
`notes` mean the file was modified.

- `patches/0001-skip-partitions-with-no-extents.patch`
  source: armada
  notes: Needed by 0002: a zero-target mapping still registers as ACTIVE, so creating one for slot 0's empty vendor_b makes 0002 skip slot 2's real definition as already-mapped.
- `patches/0002-map-partitions-defined-in-later-metadata-slots.patch`
  source: armada
  notes: The AYN Thor's super has three metadata slots with the current vendor_b (8 extents) in slot 2, but the google-sargo retrofit check broke out of the slot loop after slot 0 ("warn: This looks like metadata for retrofit partitions"), leaving only the stale _a partitions. Replaces that break with a per-partition check for an existing mapping, so duplicate slots still map once.
