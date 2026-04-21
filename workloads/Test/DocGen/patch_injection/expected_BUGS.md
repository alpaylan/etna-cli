# patch_injection — Injected Bugs

Total mutations: 1

## Bug Index

| # | Variant | Name | Location | Injection | Fix Commit |
|---|---------|------|----------|-----------|------------|
| 1 | `ac_patched_0000000_1` | `ac_patched` | `patches/ac_patched.patch` | `patch` | `ac00000000000000000000000000000000000000` |

## Property Mapping

| Variant | Property | Witness(es) |
|---------|----------|-------------|
| `ac_patched_0000000_1` | `Matches` | `b"aaab"` |

## Framework Coverage

| Property | proptest | quickcheck | crabcheck | hegel |
|----------|---------:|-----------:|----------:|------:|
| `Matches` | ✓ | ✓ | ✓ | ✓ |

## Bug Details

### 1. ac_patched

- **Variant**: `ac_patched_0000000_1`
- **Location**: `patches/ac_patched.patch`
- **Property**: `Matches`
- **Witness(es)**:
  - `b"aaab"`
- **Source**: internal fuzzer finding, no upstream PR — fix edge case in automaton construction
  > Discovered by in-house fuzzer; reported to upstream but not yet accepted.
- **Fix commit**: `ac00000000000000000000000000000000000000` — fix edge case in automaton construction
- **Invariant violated**: Matches found by the automaton must be a subset of byte-level search matches.
- **How the mutation triggers**: Patch relaxes a bounds check that upstream fix later tightened.
