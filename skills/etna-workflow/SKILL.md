---
name: etna-workflow
description: Run and analyze ETNA experiments end-to-end. Use when working with ETNA test JSON files, running `etna experiment run` or `cargo run -- experiment run`, splitting tests by workload, interpreting `store.jsonl`, and comparing strategies while handling reruns and duplicates safely.
---

# Etna Workflow

Use this skill to make ETNA experiment work reproducible and comparable across runs.

## Quick Start

1. Identify the experiment directory (contains `tests/`, `store.jsonl`, and optional `archive/`).
2. Validate available tests before running: list files in `tests/` and inspect the target JSON.
3. Run experiments with explicit test names and optional `--parallel`/`--short-circuit`.
4. Analyze results from `store.jsonl` using `scripts/summarize_store.py` to avoid rerun skew.

## Run Workflow

1. Inspect the selected test file and confirm workload/scope.
2. Run only the intended test subset.
3. If rerunning a subset, archive `store.jsonl` before edits or filtering.
4. Summarize with deduped metrics and report both raw and dedup row counts.

Use command patterns like:

```bash
cargo run -- experiment run --name <experiment-name> --tests <test-name> --parallel
# or
etna experiment run --name <experiment-name> --tests <test-name> --parallel
```

## Store Analysis Rules

1. Treat `store.jsonl` as append-only unless explicitly cleaning/rebuilding.
2. Report raw row count and deduped logical row count.
3. Deduplicate by latest timestamp per logical trial key:
   - `(workload, mutations, property, strategy, trial)`
4. Track statuses separately: `failed`, `passed`, `timed_out`.
5. Do not fold `timed_out` into `failed` when computing timeout rate.
6. When comparing strategies, report both:
   - overall failure rate (`failed / total`)
   - kill-on-completed (`failed / (failed + passed)`)

Run:

```bash
python3 skills/etna-workflow/scripts/summarize_store.py --store <experiment>/store.jsonl --experiment <name>
```

## Test File Editing Rules

1. Preserve existing test/task shape unless asked to redesign generation.
2. For workload-only variants, filter `rust-3way.json` into dedicated files like `rust-3way-bst.json`, `rust-3way-rbt.json`, `rust-3way-stlc.json`.
3. Keep `trials`, `timeout`, `cross`, and per-task fields unchanged unless explicitly requested.
4. Validate output JSON parses and contains only intended workloads.

## References

1. Read `references/commands.md` for CLI patterns and safe run practices.
2. Read `references/store-schema.md` when interpreting statuses and metrics.

## Scripts

1. Use `scripts/summarize_store.py` for consistent deduped summaries.
2. Prefer this script over ad-hoc one-off analysis snippets when reporting results.
