# codex-visual-loop-plugin

Rust-first **Codex plugin** for macOS visual reasoning loops.

> Main package: `codex-visual-loop-plugin/`

---

## Overview

- Rust command surface:
- `commands`, `manifest`, `capture`, `annotate`, `diff`, `loop`, `observe`, `ax-tree`, `explain-app`
- Python-dispatched command:
- `visual-loop-feedback` (OMX inbox-aware team helper)

Core capabilities:

- Resilient capture with explicit failure modes (`capture_mode`, `fallback_used`, `warnings`, `--strict`)
- Annotation metadata with semantic fields + shape/spec compatibility
- Diff-to-bbox reports + annotate-ready specs
- Observation packet flow (before/after/action/clip/diff) for explain-to-LLM handoff
- AX tree snapshot output for UI grounding

---

## Runtime Architecture

- Console entrypoint: `codex-visual-loop` (Python package in `codex-visual-loop-plugin/src/codex_visual_loop_plugin`)
- Python router (`cli.py`) dispatches:
- `visual-loop-feedback` -> Python implementation (`visual_loop_feedback.py`)
- everything else -> Rust binary (`src-rs/main.rs`) with local binary preference and `cargo run` fallback
- `codex-visual-loop commands` reflects Rust command surface only

---

## Install

Recommended:

```bash
make bootstrap
```

Rust-local run (without global install):

```bash
cargo run --manifest-path codex-visual-loop-plugin/Cargo.toml -- commands
```

Seamless checks:

```bash
make doctor
make happy-path APP="Google Chrome"
```

Codex approval-step automation launcher:

```bash
codex-auto
```

---

## CLI quick start

```bash
codex-visual-loop commands
codex-visual-loop manifest

codex-visual-loop capture --process "Promptlight" --json
codex-visual-loop annotate input.png output.png --spec spec.json
codex-visual-loop diff baseline.png current.png --json-out report.json --annotate-spec-out change-spec.json
codex-visual-loop loop current.png home
codex-visual-loop observe --process "Promptlight" --action "submit" --json
codex-visual-loop observe --process "iTerm2" --action "watch-omx-xhigh-madmax" --duration 3 --json
codex-visual-loop ax-tree --process "Promptlight" --depth 3 --json
codex-visual-loop visual-loop-feedback --json
codex-visual-loop capture --process "Google Chrome" --strict --json
```

---

## Command Flow

- Typical flow:
- `capture` -> `annotate` / `diff` -> `loop` or `observe` -> `explain-app`
- `explain-app` can run Codex report generation when `codex` CLI is present
- use `--no-codex` to force packet/report output without Codex execution

---

## Resilient capture + explicit failure modes

`capture` first attempts app-window bounds via AX/AppleScript, then falls back to whole-screen capture, then to a generated placeholder image if capture tools/permissions are unavailable.

Window selection is hardened for noisy app UIs (like iTerm2 utility popups):

- enumerate app windows and prefer the largest **usable** candidate
- avoid tiny-window captures (for example 30x23pt utility fragments)
- fall back to full-screen capture when only undersized window bounds are available

Use these fields to branch agent behavior:

- `capture_mode`: `window` | `screen` | `fallback`
- `fallback_used`: boolean
- `warnings`: list of human-readable fallback reasons
- `window_probe`: selected index/mode + candidate counts + usable threshold result

Use `--strict` when fallback output should fail fast with non-zero status.

---

## Explain-app pipeline (screenshot → packet → Codex)

```bash
codex-visual-loop capture --process "Google Chrome" --step before --json
codex-visual-loop observe --process "Google Chrome" --action "explain-app-state" --duration 0 --json > observe.json
codex-visual-loop ax-tree --process "Google Chrome" --depth 3 --json > ax.json
```

Feed `observe.json` + `ax.json` to Codex for detailed UI explanation, change diagnosis, and next-action planning.

Or run the one-shot command:

```bash
codex-visual-loop explain-app --process "Google Chrome" --json
# fallback report only (no codex exec)
codex-visual-loop explain-app --process "Google Chrome" --no-codex --json
```

---

## annotation options in Rust


- shapes: `rect`, `arrow`, `text`, `spotlight` (plus `focus`/`dim` aliases)
- relative units (`defaults.units: "rel"`)
- semantic fields (`severity`, `issue`, `hypothesis`, `next_action`, `verify`)
- compatibility fields like `defaults.auto_fit`, `anchor`, `from`, `to`, `anchor_pos`, `anchor_offset`

Metadata sidecars preserve defaults/semantic context and normalized geometry for downstream reasoning.

---

## Codex usage model

Use both layers together:

- **Plugin (`codex-visual-loop`)**: executes commands.
- **Skill (`skills/codex-visual-loop/SKILL.md`)**: maps natural-language tasks to the command set.

OMX team worker flow:

- `visual-loop-feedback` reads `OMX_TEAM_WORKER` inbox path (same inbox/mailbox pattern as OMX workers)
- dry-run by default
- `--execute` runs safe feedback actions (`capture`, `ax-tree`, optional `observe`)

---

## Behavior Differences

- Rust `observe` builds an observation packet and clip artifact path for workflow continuity.
- Legacy shell flow (`scripts/observe_action_clip.sh` + `scripts/record_app_window.sh`) is the path for full ffmpeg recording behavior.
- For deterministic automation and metadata-first analysis, prefer Rust commands.
- For richer legacy recording pipelines, use script flows explicitly.

---

## Package structure

- `codex-visual-loop-plugin/Cargo.toml`
- `codex-visual-loop-plugin/src-rs/main.rs`
- `codex-visual-loop-plugin/src/codex_visual_loop_plugin/cli.py`
- `codex-visual-loop-plugin/src/codex_visual_loop_plugin/visual_loop_feedback.py`
- `codex-visual-loop-plugin/manifest.json`
- `codex-visual-loop-plugin/commands/*`
- `codex-visual-loop-plugin/docs/*`
- `tests/test_codex_visual_loop_plugin.py`
- `tests/test_visual_reasoning_loop.py`

---

## Artifacts and Environment

Default output root: `.codex-visual-loop/`

Environment variables:

- `CVLP_OUT_DIR` (preferred)
- `CVLP_LOOP_DIR` (loop-specific override)

---

## Requirements

- macOS (for native window/AX integrations)
- Screen Recording + Accessibility permissions for terminal
- Rust toolchain (`cargo`, `rustc`)
- Optional utilities by flow: `ffmpeg`, `ImageMagick` (`magick`), `jq`, `codex` CLI

---

## Verification Coverage

```bash
make test
python3 -m unittest discover -s tests -v
```

- `tests/test_codex_visual_loop_plugin.py`: plugin CLI dispatch, command wiring, packet/report paths
- `tests/test_visual_reasoning_loop.py`: visual loop script/flow behavior and integration helpers

---

## License

MIT (`LICENSE`)
