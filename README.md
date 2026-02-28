# codex-visual-feedback

Rust-first **Codex plugin** that gives AI agents eyes — capture screenshots, detect visual changes via pixel-level diff, and act on what's on screen.

> Main package: `codex-visual-loop-plugin/`

---

## What It Does

AI coding agents can read and write code, but they can't **see** the result. This plugin closes that gap:

```
capture → annotate → diff → act → observe → explain
```

1. **Capture** app windows as screenshots with rich metadata
2. **Diff** screenshots pixel-by-pixel, extracting changed regions via BFS flood-fill
3. **Annotate** screenshots with shapes, arrows, spotlight overlays
4. **Act** on apps natively — click, type, hotkey — no osascript wrappers needed
5. **Observe** before/after state around an action, producing a full diff packet
6. **Explain** an app in one shot — screenshot + AX tree → LLM report

---

## Demo: AI Debugs a Game by Watching the Screen

The `demo/` directory contains a hackathon demo: a Breakout game with 3 intentional visual bugs that the AI detects and fixes through the visual feedback loop.

```bash
# 1. Build the plugin
cd codex-visual-loop-plugin && cargo build --release && cd ..

# 2. Open the demo game
open -a "Google Chrome" demo/game.html

# 3. AI analyzes the running game
codex-visual-loop explain-app --process "Google Chrome" --json

# 4. AI observes a bug fix (press key "1" to fix wall collision)
codex-visual-loop observe --process "Google Chrome" --action "fix-wall" --duration 2 \
  --action-cmd 'codex-visual-loop act --process "Google Chrome" --hotkey "1"' --json

# 5. Compare before/after with pixel diff
codex-visual-loop loop current.png game-state --json
```

The 3 bugs:
- **Ball clips through right wall** — collision check off by ball radius
- **Paddle drawn 40px offset** — visual doesn't match hitbox
- **Ghost bricks** — score increments but bricks remain visible

Each fix produces a red bbox diff overlay showing exactly what changed.

See `demo/DEMO-GUIDE.md` for the full presenter script.

---

## Commands

| Command | Description |
|---|---|
| `capture` | Screenshot app window → PNG + JSON metadata sidecar |
| `annotate` | Overlay shapes/arrows/text/spotlight onto a PNG |
| `diff` | Pixel-level diff → changed-region bboxes + annotate spec |
| `loop` | Baseline/history diff loop with auto-annotated change boxes |
| `observe` | Before/after/action/diff observation packet |
| `act` | Native UI actions: click, type, hotkey, tab, enter |
| `ax-tree` | macOS Accessibility tree snapshot → JSON |
| `explain-app` | One-shot: capture + AX + LLM report |
| `visual-loop-feedback` | OMX inbox-aware team helper |

---

## Quick Start

```bash
# Install
make bootstrap

# Verify
make doctor
make happy-path APP="Google Chrome"

# Run commands
codex-visual-loop capture --process "Google Chrome" --json
codex-visual-loop act --process "Google Chrome" --hotkey "cmd+l" --text "https://example.com" --enter --json
codex-visual-loop observe --process "Google Chrome" --action "navigate" --duration 3 --json
codex-visual-loop ax-tree --process "Google Chrome" --depth 3 --json
codex-visual-loop explain-app --process "Google Chrome" --json
codex-visual-loop diff baseline.png current.png --json-out report.json --annotate-spec-out change-spec.json
```

---

## Architecture

- **Rust CLI** (`src-rs/main.rs`): all core commands — capture, annotate, diff, loop, observe, act, ax-tree, explain-app
- **Python router** (`cli.py`): dispatches to Rust binary (with `cargo run` fallback) or Python for `visual-loop-feedback`
- **Pure Rust drawing**: thick lines, arrowheads, discs, bitmap font, alpha blending — no external image processing deps
- **BFS diff engine**: per-pixel max-channel diff → connected-component flood-fill → bounding boxes sorted by size

### Capture Resilience

`capture` handles noisy app UIs gracefully:

- Enumerates all app windows, selects the largest **usable** candidate (≥220×140px, ≥40k px²)
- Falls back: window → full-screen → generated placeholder
- Use `--strict` to fail fast on fallback
- Metadata includes `capture_mode`, `fallback_used`, `warnings`, `window_probe`

---

## Artifacts & Environment

Default output root: `.codex-visual-loop/`

| Variable | Purpose |
|---|---|
| `CVLP_OUT_DIR` | Override artifact output root |
| `CVLP_LOOP_DIR` | Override loop storage directory |

---

## Package Structure

```
codex-visual-loop-plugin/
├── Cargo.toml
├── src-rs/main.rs          # Rust CLI (~4500 lines)
├── src/.../cli.py           # Python router
├── src/.../visual_loop_feedback.py
├── commands/                # Command documentation
├── docs/                    # Plugin documentation
├── manifest.json
└── install.sh
demo/
├── game.html               # Breakout demo (hackathon)
├── index.html              # Dashboard demo (backup)
├── run-game-demo.sh        # Automated demo script
└── DEMO-GUIDE.md           # Presenter cheat sheet
tests/
├── test_codex_visual_loop_plugin.py
└── test_visual_reasoning_loop.py
```

---

## Requirements

- macOS (screencapture + osascript + Accessibility API)
- Screen Recording + Accessibility permissions for terminal
- Rust toolchain (`cargo`, `rustc`)
- `ffmpeg`, `ImageMagick` (`magick`), `jq`, `codex` CLI

---

## License

MIT (`LICENSE`)
