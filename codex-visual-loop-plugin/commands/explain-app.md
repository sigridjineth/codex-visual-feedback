# explain-app

Build a screenshot + AX observation packet and optionally run `codex exec` to generate a detailed markdown explanation report.

## Usage

```bash
codex-visual-loop explain-app --process "Slack" --json
```

## Key flags

- `--process <App>`: target app (default: frontmost app)
- `--ax-depth <n>`: AX traversal depth (default: 4)
- `--out-dir <dir>`: output root (default: `.codex-visual-loop`)
- `--prompt <text>`: extra prompt instruction
- `--report <path>`: custom markdown report path
- `--packet-out <path>`: custom packet JSON output path
- `--prompt-out <path>`: custom prompt text output path
- `--codex-bin <path>`: override Codex executable
- `--model <name>`: optional model override for `codex exec`
- `--codex-timeout <sec>`: codex run timeout (default: 300)
- `--no-codex`: skip codex execution and write fallback report
- `--strict-llm`: fail if codex execution fails
- `--json`: emit full result payload to stdout

## Output

Writes packet/prompt/report artifacts under `<out-dir>/explain/` and returns paths in JSON.
