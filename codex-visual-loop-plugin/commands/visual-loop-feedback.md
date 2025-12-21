# visual-loop-feedback

Read an OMX team worker `inbox.md`, infer safe visual-loop actions, and optionally execute them.

```bash
# Dry-run plan from OMX_TEAM_WORKER + OMX_TEAM_STATE_ROOT
codex-visual-loop visual-loop-feedback --json

# Execute safe capture/ax-tree/observe flow
codex-visual-loop visual-loop-feedback --execute --json
```

Safety defaults:

- Runs in dry-run mode unless `--execute` is provided.
- Ignores inferred/provided `--action-cmd` unless `--allow-action-cmd` is set.
- Uses `observe --duration 0` by default for quick, low-risk feedback loops.

Common options:

- `--inbox <path>` explicit inbox path
- `--team-state-root <path>` explicit team state root
- `--process <name>` override process inference
- `--action <label>` override observe action label
- `--actions capture,ax-tree,observe` action subset/order
- `--ax-depth <n>` AX tree depth
- `--observe-duration <sec>` override observe duration
- `--execute` execute planned actions
- `--json` emit structured output
