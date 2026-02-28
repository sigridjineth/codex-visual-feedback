# observe

Generate one action observation packet with before/after capture, clip, and diff payload.

```bash
codex-visual-loop observe --process "Google Chrome" --action "submit" --action-cmd 'echo noop' --json
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

Action examples (click / keystroke):

```bash
# Browser keystroke (Cmd+L, type, Enter)
codex-visual-loop observe \
  --process "Google Chrome" \
  --action "open-openai" \
  --duration 3 \
  --action-cmd 'osascript -e "tell application \"Google Chrome\" to activate" -e "delay 0.2" -e "tell application \"System Events\" to keystroke \"l\" using command down" -e "delay 0.1" -e "tell application \"System Events\" to keystroke \"https://openai.com\"" -e "key code 36"'

# Coordinate click
codex-visual-loop observe \
  --process "Google Chrome" \
  --action "click-top-right" \
  --duration 3 \
  --action-cmd 'osascript -e "tell application \"Google Chrome\" to activate" -e "delay 0.2" -e "tell application \"System Events\" to click at {1680, 78}"'
```
