# diff

Compare baseline/current screenshots and emit changed-region boxes + optional spec.

```bash
codex-visual-loop diff baseline.png current.png --json-out report.json --annotate-spec-out change-spec.json
```

Common options:

- `--diff-out <path>` diff PNG output
- `--annotated-out <path>` current image with change boxes
- `--resize` resize current to baseline dimensions
- `--bbox-threshold <n>` pixel threshold (default: `24`)
- `--bbox-min-area <n>` min changed pixels per region (default: `64`)
- `--bbox-pad <n>` bbox padding (default: `2`)
- `--max-boxes <n>` max regions (default: `16`)
