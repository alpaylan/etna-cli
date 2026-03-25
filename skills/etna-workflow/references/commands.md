# ETNA Commands

Use this reference for repeatable command patterns.

## Inspect Tests

```bash
ls -1 <experiment>/tests
sed -n '1,200p' <experiment>/tests/<test-file>.json
```

## Run Tests

```bash
cargo run -- experiment run --name <experiment-name> --tests <test-name>
cargo run -- experiment run --name <experiment-name> --tests <test-name> --parallel
cargo run -- experiment run --name <experiment-name> --tests <test-name> --short-circuit
```

If the standalone CLI is installed, equivalent commands are:

```bash
etna experiment run --name <experiment-name> --tests <test-name>
```

## Safety Pattern for Reruns

1. Archive current store first.
2. Apply targeted filtering (for the exact workload/strategy subset).
3. Rerun only the targeted test file.
4. Re-check counts after run completion.

Archive example:

```bash
cp <experiment>/store.jsonl <experiment>/archive/store.backup.<timestamp>.jsonl
```

## Common Query Snippets

Count rows by workload/strategy:

```bash
python3 - <<'PY'
import json
from collections import Counter
c=Counter()
for line in open('rust-strategy-triad/store.jsonl'):
    d=json.loads(line)['data']
    c[(d.get('workload'), d.get('strategy'))]+=1
print(sum(c.values()))
for k,v in sorted(c.items()):
    print(k, v)
PY
```

Prefer `scripts/summarize_store.py` for final reporting.
