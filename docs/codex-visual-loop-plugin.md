# Codex Visual Loop Plugin

This repository's primary deliverable is now the standalone package:

- `codex-visual-loop-plugin`

## Core commands

- `capture` → resilient capture metadata sidecars + explicit fallback modes
- `diff` → changed-region bounding boxes + annotate spec outputs
- `loop` → baseline/history diff loop with annotated artifacts
- `observe` → action observation packet flow (explain-ready JSON packet)
- `ax-tree` → accessibility tree dump
- `explain-app` → one-shot capture + AX packet + codex markdown report
- `visual-loop-feedback` → OMX inbox-driven safe feedback actions

## Entrypoint

`codex-visual-loop` (Rust CLI binary)

Primary implementation source:

`codex-visual-loop-plugin/src-rs/main.rs`

## Notes

- Command names and behavior are preserved for `capture/annotate/diff/loop/observe/ax-tree`; wrapper helper `visual-loop-feedback` adds team inbox orchestration.
- `capture` failure contract is explicit for agent branching:
  - `capture_mode`: `window | screen | fallback`
  - `fallback_used`: boolean
  - `warnings`: list of fallback explanations
  - `--strict`: fail fast when fallback image generation is used
- `observe` + `ax-tree` can be used as a screenshot→LLM “explain app state” packet.
- Output roots remain `.codex-visual-loop/` with `CVLP_OUT_DIR` / `CVLP_LOOP_DIR` overrides.
- Recommended smooth flow:
  - `make bootstrap`
  - `make doctor`
  - `make happy-path APP="Google Chrome"`
  - use `codex-auto` when you explicitly want command-approval bypass.
