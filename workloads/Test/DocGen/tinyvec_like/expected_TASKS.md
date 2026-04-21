# tinyvec_like — ETNA Tasks

Total tasks: 8

## Task Index

| Task | Variant | Framework | Property | Witness |
|------|---------|-----------|----------|---------|
| 001 | `debug_alternate_empty_a711c72_1` | proptest | `ArrayvecDebugMatchesSlice` | `witness_arrayvec_debug_matches_slice_case_empty` |
| 002 | `debug_alternate_empty_a711c72_1` | quickcheck | `ArrayvecDebugMatchesSlice` | `witness_arrayvec_debug_matches_slice_case_empty` |
| 003 | `debug_alternate_empty_a711c72_1` | crabcheck | `ArrayvecDebugMatchesSlice` | `witness_arrayvec_debug_matches_slice_case_empty` |
| 004 | `debug_alternate_empty_a711c72_1` | hegel | `ArrayvecDebugMatchesSlice` | `witness_arrayvec_debug_matches_slice_case_empty` |
| 005 | `swap_remove_last_71ad62a_1` | proptest | `SwapRemoveLastReturnsTail` | `witness_swap_remove_last_returns_tail_case_single` |
| 006 | `swap_remove_last_71ad62a_1` | quickcheck | `SwapRemoveLastReturnsTail` | `witness_swap_remove_last_returns_tail_case_single` |
| 007 | `swap_remove_last_71ad62a_1` | crabcheck | `SwapRemoveLastReturnsTail` | `witness_swap_remove_last_returns_tail_case_single` |
| 008 | `swap_remove_last_71ad62a_1` | hegel | `SwapRemoveLastReturnsTail` | `witness_swap_remove_last_returns_tail_case_single` |

## Witness Catalog

- `witness_arrayvec_debug_matches_slice_case_empty` — exposes the stray comma on empty input
- `witness_arrayvec_debug_matches_slice_case_three_elements` — base passes, variant fails
- `witness_swap_remove_last_returns_tail_case_single` — base passes, variant fails
- `witness_swap_remove_last_returns_tail_case_four_elements` — hits the tail-index boundary on a populated vec
