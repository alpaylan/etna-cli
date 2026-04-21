# tinyvec_like — Injected Bugs

Test fixture mirroring the tinyvec injection shape: two marauders-based bugs with full source provenance.

Total mutations: 2

## Bug Index

| # | Variant | Name | Location | Injection | Fix Commit |
|---|---------|------|----------|-----------|------------|
| 1 | `debug_alternate_empty_a711c72_1` | `debug_alternate_empty` | `src/arrayvec.rs:1839` | `marauders` | `a711c72eef6d555ebc7bbbe78bf5039e72f790ac` |
| 2 | `swap_remove_last_71ad62a_1` | `swap_remove_last` | `src/arrayvec.rs:1168` | `marauders` | `71ad62a90f2ff95dae4e43d646a55b0329b1eedc` |

## Property Mapping

| Variant | Property | Witness(es) |
|---------|----------|-------------|
| `debug_alternate_empty_a711c72_1` | `ArrayvecDebugMatchesSlice` | `witness_arrayvec_debug_matches_slice_case_empty`, `witness_arrayvec_debug_matches_slice_case_three_elements` |
| `swap_remove_last_71ad62a_1` | `SwapRemoveLastReturnsTail` | `witness_swap_remove_last_returns_tail_case_single`, `witness_swap_remove_last_returns_tail_case_four_elements` |

## Framework Coverage

| Property | proptest | quickcheck | crabcheck | hegel |
|----------|---------:|-----------:|----------:|------:|
| `ArrayvecDebugMatchesSlice` | ✓ | ✓ | ✓ | ✓ |
| `SwapRemoveLastReturnsTail` | ✓ | ✓ | ✓ | ✓ |

## Bug Details

### 1. debug_alternate_empty

- **Variant**: `debug_alternate_empty_a711c72_1`
- **Location**: `src/arrayvec.rs:1839` (inside `impl<A: Array> Debug for ArrayVec<A>`)
- **Property**: `ArrayvecDebugMatchesSlice`
- **Witness(es)**:
  - `witness_arrayvec_debug_matches_slice_case_empty` — exposes the stray comma on empty input
  - `witness_arrayvec_debug_matches_slice_case_three_elements`
- **Source**: [#147](https://github.com/Lokathor/tinyvec/pull/147) — fix Debug alternate mode for empty containers
  > Debug impl was emitting a stray comma/newline for empty ArrayVec.
- **Fix commit**: `a711c72eef6d555ebc7bbbe78bf5039e72f790ac` — fix Debug alternate mode for empty containers
- **Invariant violated**: ArrayVec's Debug output must match the underlying slice's Debug output in both plain and alternate modes.
- **How the mutation triggers**: The mutation reinstates the pre-fix manual impl that unconditionally emits a leading newline and trailing comma.

### 2. swap_remove_last

- **Variant**: `swap_remove_last_71ad62a_1`
- **Location**: `src/arrayvec.rs:1168` (inside `ArrayVec::swap_remove`)
- **Property**: `SwapRemoveLastReturnsTail`
- **Witness(es)**:
  - `witness_swap_remove_last_returns_tail_case_single`
  - `witness_swap_remove_last_returns_tail_case_four_elements` — hits the tail-index boundary on a populated vec
- **Source**: [#132](https://github.com/Lokathor/tinyvec/pull/132), [#131](https://github.com/Lokathor/tinyvec/issues/131) — Fix ArrayishVec::swap_remove for last element
  > swap_remove panicked when called on the last element because pop shrank the vec before the index-write.
- **Fix commit**: `71ad62a90f2ff95dae4e43d646a55b0329b1eedc` — Fix ArrayishVec::swap_remove for last element
- **Invariant violated**: swap_remove(len - 1) must return the tail element without panicking, matching Vec::swap_remove.
- **How the mutation triggers**: The pre-fix body pops first, then indexes self[index]; when index == len - 1 the index is now out of range.

## Dropped Candidates

- `cafebabe1234567890abcdef1234567890abcdef` (style: rustfmt pass) — Whitespace-only lint fix; no invariant bug to inject.
