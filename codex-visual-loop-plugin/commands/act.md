# act

Perform native macOS UI actions against a target app process.

```bash
codex-visual-loop act --process "Google Chrome" --hotkey "cmd+l" --text "https://openai.com" --enter --json
```

Common options:

- `--process <name>` target app process (default: frontmost app)
- `--click <x,y>` absolute screen click
- `--click-rel <x,y>` click relative to selected app window origin
- `--text <value>` text input (`--text -` reads stdin)
- `--hotkey <combo>` hotkey combo (`cmd+shift+p`, `ctrl+alt+k`)
- `--tab <n>` press tab key N times
- `--enter` press return key
- `--no-activate` skip app activation
- `--activation-delay-ms <ms>` delay after activation (default: 120)
- `--dry-run` validate/plan only
- `--json` print structured execution payload

Examples:

```bash
# click only
codex-visual-loop act --process "Google Chrome" --click 1680,78 --json

# window-relative click + type
codex-visual-loop act --process "iTerm2" --click-rel 120,80 --text "omx --xhigh --madmax" --enter --json

# hotkey + text flow
codex-visual-loop act --process "Google Chrome" --hotkey cmd+l --text "codex visual loop" --enter
```
