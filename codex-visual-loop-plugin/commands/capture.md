# capture

Capture a target app window and emit a metadata sidecar JSON.

```bash
codex-visual-loop capture --process "Promptlight" --json
```

Common options:

- `--out <path>` output PNG
- `--process <name>` target app process
- `--step <name>` workflow label (`before`, `after`, etc.)
- `--note <text>` free-form metadata note
- `--sidecar <path>` custom metadata JSON path
- `--no-sidecar` disable metadata file write
- `--strict` fail if capture falls back to placeholder output

Behavior notes:

- On macOS, `capture` enumerates app windows and selects the **largest usable** window (instead of blindly using `window 1`).
- Tiny utility windows are guarded: if selected bounds are too small for reliable reasoning, it falls back to full-screen capture and records warnings.
- Metadata includes `window_probe` (`selection_mode`, `candidate_count`, `usable_count`, `usable`) for debugging selection decisions.
