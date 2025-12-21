# loop

Maintain baseline/latest/history and generate diff + annotated outputs.

```bash
codex-visual-loop loop current.png home --bbox-threshold 24
```

Common options:

- `--loop-dir <path>` override loop storage root
- `--resize` resize current to baseline dimensions
- `--update-baseline` replace baseline after comparison
- `--no-annotated` skip annotated image/spec artifacts
- `--bbox-threshold <n>`
- `--bbox-min-area <n>`
- `--bbox-pad <n>`
- `--max-boxes <n>`
