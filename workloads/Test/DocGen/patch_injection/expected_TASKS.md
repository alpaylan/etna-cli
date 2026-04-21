# patch_injection — ETNA Tasks

Total tasks: 4

## Task Index

| Task | Variant | Framework | Property | Witness |
|------|---------|-----------|----------|---------|
| 001 | `ac_patched_0000000_1` | proptest | `Matches` | `b"aaab"` |
| 002 | `ac_patched_0000000_1` | quickcheck | `Matches` | `b"aaab"` |
| 003 | `ac_patched_0000000_1` | crabcheck | `Matches` | `b"aaab"` |
| 004 | `ac_patched_0000000_1` | hegel | `Matches` | `b"aaab"` |

## Witness Catalog

- `b"aaab"` — base passes, variant fails
