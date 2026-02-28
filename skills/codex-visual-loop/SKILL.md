---
name: codex-visual-loop
description: Codex-first visual reasoning workflow wrapper for the standalone codex-visual-loop-plugin CLI.
---

# Codex Visual Loop Skill

Use this skill when the user asks to inspect or reason about a macOS app UI using screenshots, diffs, annotations, observation packets, or AX tree data.

## Requirement

This skill expects the Rust plugin CLI command to be available:

- `codex-visual-loop`

Install if needed:

```bash
make bootstrap
make doctor
# or local run without install:
cargo run --manifest-path codex-visual-loop-plugin/Cargo.toml -- commands
```

If you want Codex command-approval prompts bypassed for this session:

```bash
codex-auto
```

## Command mapping

- Capture window + metadata sidecar:
  - `codex-visual-loop capture --process "<App>" --json`
  - strict mode: `codex-visual-loop capture --process "<App>" --strict --json`
- Annotate image:
  - `codex-visual-loop annotate <input.png> <output.png> --spec <spec.json>`
- Compare baseline/current + changed regions:
  - `codex-visual-loop diff <baseline.png> <current.png> --json-out <report.json> --annotate-spec-out <change-spec.json>`
- Visual loop with baselines/history:
  - `codex-visual-loop loop <current.png> <baseline-name>`
- Observation packet (before/after + clip + diff):
  - `codex-visual-loop observe --process "<App>" --action "<label>" --json`
  - click/keystroke action example:
    - `codex-visual-loop observe --process "Google Chrome" --action "search" --duration 3 --action-cmd 'osascript -e "tell application \"Google Chrome\" to activate" -e "delay 0.2" -e "tell application \"System Events\" to keystroke \"l\" using command down" -e "tell application \"System Events\" to keystroke \"codex visual loop\"" -e "key code 36"'`
- Native action command (click/type/hotkey):
  - `codex-visual-loop act --process "Google Chrome" --hotkey cmd+l --text "openai.com" --enter --json`
  - `codex-visual-loop act --process "iTerm2" --click-rel 120,80 --text "omx --xhigh --madmax" --enter --json`
- Accessibility tree:
  - `codex-visual-loop ax-tree --process "<App>" --depth 3 --json`
- Explain app state (capture + AX + Codex report):
  - `codex-visual-loop explain-app --process "<App>" --json`
  - fallback only: `codex-visual-loop explain-app --process "<App>" --no-codex --json`
- OMX team inbox-aware helper (dry-run by default):
  - `codex-visual-loop visual-loop-feedback --json`
  - execute: `codex-visual-loop visual-loop-feedback --execute --json`
  - compat wrapper: `codex-visual-loop-inbox --json`

## Command behavior contract

Preserve command names exactly:

- `capture`
- `annotate`
- `diff`
- `loop`
- `observe`
- `ax-tree`
- `act`

## Minimal Codex workflow

1. Capture current screen state (`capture`).
2. If validating changes, run `diff` or `loop`.
3. If guiding edits, generate annotation artifacts (`annotate`).
4. For state-transition bugs, run `observe`.
5. If element identity is ambiguous, run `ax-tree`.

## Resilient capture guidance

Use capture metadata to branch behavior safely:

- `capture_mode`: `window` | `screen` | `fallback`
- `fallback_used`: whether placeholder image generation was required
- `warnings`: explicit fallback reasons (permissions/tools/window issues)
- `window_probe`: window-selection diagnostics (`selection_mode`, `candidate_count`, `usable_count`, `usable`)

Use `--strict` when fallback captures should fail immediately.

For apps with many tiny utility windows (e.g. iTerm2), `capture`/`observe` already prefer the largest usable window and fall back to screen capture when only undersized bounds are available.

## Explain-app pipeline

When a user asks for a detailed app explanation, prefer this packet flow:

1. `codex-visual-loop capture --process "<App>" --step before --json`
2. `codex-visual-loop observe --process "<App>" --action "explain-app-state" --duration 0 --json > observe.json`
3. `codex-visual-loop ax-tree --process "<App>" --depth 3 --json > ax.json`
4. Feed `observe.json` + `ax.json` into Codex and request:
   - UI summary
   - anomalies/regressions
   - likely root causes
   - next safe actions

## annotate spec tips


- shapes: `rect`, `arrow`, `text`, `spotlight`
- relative units: `defaults.units: "rel"`
- compatibility defaults/fields: `auto_fit`, `anchor`, `from`, `to`, `anchor_pos`, `anchor_offset`
- semantic debugging fields: `severity`, `issue`, `hypothesis`, `next_action`, `verify`

## Safety guidance

- Treat UI automation actions as potentially destructive.
- Prefer dry observation first (`capture`, `diff`, `ax-tree`) before action loops.
- For sensitive apps/workflows, require explicit user confirmation before running `observe --action-cmd ...`.

## OMX team inbox mode

`visual-loop-feedback` resolves worker inbox using `OMX_TEAM_WORKER` and `OMX_TEAM_STATE_ROOT` (with identity/config fallback), infers process/action hints, and plans safe visual-loop actions.

Safety defaults:

- dry-run unless `--execute`
- ignores `--action-cmd` unless `--allow-action-cmd`
- `observe` defaults to `--duration 0` for low-risk feedback
- reads worker inbox from `<team_state_root>/team/<team>/workers/<worker>/inbox.md`
