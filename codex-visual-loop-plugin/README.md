# codex-visual-loop-plugin

A standalone Rust Codex plugin package for macOS visual automation loops.

## Features

- Resilient capture metadata JSON sidecars + strict failure mode controls (`capture`)
- annotation spec compatibility (`rect`/`arrow`/`text`/`spotlight`, semantic fields, rel units) (`annotate`)
- Diff-to-bbox and annotate-spec output (`diff`)
- Baseline/history loop with annotated outputs (`loop`)
- Action observation packet flow for screenshot→LLM explain pipelines (`observe`)
- AX tree dump (`ax-tree`)
- Native UI action command for click/type/hotkey (`act`)
- One-shot explain-app packet/report command (`explain-app`)
- OMX inbox-aware visual-loop feedback helper (`visual-loop-feedback`)

## Capture fallback contract

`capture` emits explicit fallback diagnostics for agent loops:

- `capture_mode` (`window`, `screen`, `fallback`)
- `fallback_used`
- `warnings`
- `window_probe` (selected window index/mode, candidate counts, usable threshold result)

Window selection prefers the largest usable app window and avoids tiny utility-window captures (useful for iTerm2/OMX sessions).

Use `--strict` to fail with non-zero status when output is a generated fallback capture.

## Explain-app workflow

```bash
codex-visual-loop capture --process "Safari" --step before --json
codex-visual-loop observe --process "Safari" --action "explain-app-state" --duration 0 --json > observe.json
codex-visual-loop ax-tree --process "Safari" --depth 3 --json > ax.json
```

Use `observe.json` + `ax.json` as a Codex context packet for detailed app-state explanation and next-step planning.

Or run:

```bash
codex-visual-loop explain-app --process "Safari" --json
```

## CLI

```bash
# run without install
cargo run --manifest-path Cargo.toml -- commands

# smooth setup
make bootstrap
make doctor

# once installed on PATH
codex-visual-loop commands
codex-visual-loop manifest
codex-visual-loop capture --help
codex-visual-loop annotate --help
codex-visual-loop diff --help
codex-visual-loop loop --help
codex-visual-loop observe --help
codex-visual-loop ax-tree --help
codex-visual-loop act --help
codex-visual-loop visual-loop-feedback --help
codex-auto
```

## Environment

- `CVLP_OUT_DIR` default artifact root (default: `.codex-visual-loop`)
- `CVLP_LOOP_DIR` optional loop storage override

## Layout

- `manifest.json` plugin manifest
- `Cargo.toml` Rust package metadata
- `src-rs/main.rs` Rust CLI implementation
- `commands/` command docs (including `visual-loop-feedback`)
- `docs/` plugin docs
