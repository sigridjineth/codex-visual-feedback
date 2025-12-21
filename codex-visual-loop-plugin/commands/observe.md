# observe

Generate one action observation packet with before/after capture, clip, and diff payload.

```bash
codex-visual-loop observe --process "Promptlight" --action "submit" --action-cmd 'echo noop' --json
```

Common options:

- `--action <label>` action name in packet metadata
- `--action-cmd "<shell command>"` command executed between captures
- `--duration <seconds>` clip wait duration
- `--out-dir <path>` output directory
- `--summary-mode scene|fps|keyframes`
- `--summary-max <n>`
- `--summary-sheet`
- `--summary-gif`
- `--no-summary`

Notes:

- `--duration` now waits up to the full requested value (capped at 30s), so `--duration 3` truly observes for ~3 seconds.
- `observe` reuses resilient `capture` behavior, including largest-usable-window selection and tiny-window fallback guardrails.
