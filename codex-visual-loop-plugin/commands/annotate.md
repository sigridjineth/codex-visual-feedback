# annotate

Render a JSON annotation spec (supports semantic fields + `defaults.units: "rel"`).

```bash
codex-visual-loop annotate input.png output.png --spec spec.json
```

Common options:

- `--meta-out <path>` custom metadata sidecar output path
- `--no-meta` disable metadata sidecar generation
- `--spec-help` print supported spec schema and exit
