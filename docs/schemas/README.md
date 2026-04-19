# Etna Contract Schemas

These JSON Schemas define the wire-format contracts between etna's driver
and workload capabilities. Any external producer or consumer that
participates in an etna pipeline must validate against them.

| Schema                              | Produced by                   | Consumed by                                   |
| ----------------------------------- | ----------------------------- | --------------------------------------------- |
| `input-stream.schema.json`          | `sample` capability           | `test` capability (and the driver in `Cross`) |
| `campaign-result.schema.json`       | `solve`, `test`, `shrink`     | the etna driver (logged to the metric store)  |

## Quick check (Python)

```sh
pip install jsonschema
python -c "
import json, jsonschema, sys
schema = json.load(open('docs/schemas/input-stream.schema.json'))
data   = json.load(open(sys.argv[1]))
jsonschema.validate(data, schema)
print('ok')
" /path/to/your/sampler/output.json
```

## Authoring a new capability

1. Pick which capability you are implementing (`solve`, `sample`, `test`, `shrink`).
2. Have your program emit JSON matching the schema for that capability on stdout.
3. Wire it up in the workload's `steps.json` under `capabilities.<name>`.
4. Validate the output against the schema as part of your CI before merging.
