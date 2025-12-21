# PR Changelog: codex-visual-loop-plugin Rust Migration

## Summary

Re-centered documentation and manifest metadata around a **Rust-first** standalone plugin package named **`codex-visual-loop-plugin`** while preserving user-facing command names.

## What Changed

### 1) Rust-first docs + skill contract, now including robust visual-loop details

Updated:

- root `README.md`
- `docs/codex-visual-loop-plugin.md`
- `docs/visual-loop.md`
- `codex-visual-loop-plugin/README.md`
- `skills/codex-visual-loop/SKILL.md`

Added documentation coverage for:

- resilient osascript/AX/window fallback behavior and explicit capture failure fields
- screenshot → observation packet → Codex explain-app workflow

### 2) Test coverage expanded for new flags/JSON behavior

Updated:

- `tests/test_codex_visual_loop_plugin.py`

New deterministic checks cover:

- `capture --strict` explicit failure behavior with fallback payload
- additional visual-loop-feedback flags and action-subset planning
- explain-pipeline packet shape in `observe --json`

### 3) Scope ownership respected

- No edits to core Rust implementation in this docs+tests task.

## Validation

Run:

- `make typecheck`
- `python3 -m ruff check tests/test_codex_visual_loop_plugin.py`
- `python3 -m unittest tests/test_codex_visual_loop_plugin.py -v`
