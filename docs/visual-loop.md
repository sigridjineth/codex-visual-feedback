# Visual Loop: codex-visual-loop-plugin

## Goal

Keep visual state in an agent loop by comparing a current screenshot against a baseline and producing diff metrics + change-region bounding boxes.

Also support robust capture + explain workflows where packets are handed to Codex for interpretation.

## Primary command

- `codex-visual-loop diff`

## Usage

```bash
codex-visual-loop diff baseline.png current.png \
  --diff-out .codex-visual-loop/loop/diffs/home-20260228.png \
  --json-out .codex-visual-loop/loop/reports/home-20260228.json \
  --annotate-spec-out .codex-visual-loop/loop/reports/home-20260228-change-spec.json
```

## Loop integration

`codex-visual-loop loop` manages:

- baselines
- latest/history snapshots
- diffs
- reports
- optional `diff-annotated` artifacts

Default loop root: `.codex-visual-loop/loop` (or `CVLP_LOOP_DIR`).

## Capture resiliency contract

Use `capture --json` metadata to handle fallback states explicitly:

- `capture_mode`: `window`, `screen`, or `fallback`
- `fallback_used`: `true` when placeholder capture is generated
- `warnings`: human-readable fallback causes

Use `--strict` when fallback captures should stop the loop immediately.

## Explain-app packet flow

```bash
codex-visual-loop capture --process "Finder" --step before --json
codex-visual-loop observe --process "Finder" --action "explain-app-state" --duration 0 --json > observe.json
codex-visual-loop ax-tree --process "Finder" --depth 3 --json > ax.json
```

Feed `observe.json` + `ax.json` into Codex for detailed UI explanation and next-action planning.

Or use one-shot mode:

```bash
codex-visual-loop explain-app --process "Finder" --json
```

## Annotation compatibility notes


- shape types: `rect`, `arrow`, `text`, `spotlight`
- relative units via `defaults.units: "rel"`
- semantic debugging fields (`severity`, `issue`, `hypothesis`, `next_action`, `verify`)
- compatibility defaults/fields (`auto_fit`, `anchor`, `from`, `to`, `anchor_pos`, `anchor_offset`)
