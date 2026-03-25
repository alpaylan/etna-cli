# store.jsonl Notes

`store.jsonl` is newline-delimited JSON. In this ETNA setup each line typically has:

- top level: `{"data": {...}, "hash": ...}`
- core fields in `data`:
  - `experiment`, `language`, `workload`
  - `strategy`, `property`, `mutations`, `trial`
  - `status` (`failed`, `passed`, `timed_out`)
  - `tests`, `discards`, `time` (often `<number>ns`)
  - `counterexample` (strategy-dependent richness)
  - `timestamp` (ISO-8601)

## Dedup Guidance

Reruns append more rows. For analysis, deduplicate by latest timestamp over:

- `(workload, mutations, property, strategy, trial)`

Use raw rows for auditability, deduped rows for comparisons.

## Metric Guidance

1. Failure rate: `failed / total`
2. Timeout rate: `timed_out / total`
3. Pass rate: `passed / total`
4. Kill-on-completed: `failed / (failed + passed)`

Report all four for fair cross-strategy comparisons.
