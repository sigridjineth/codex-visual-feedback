#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
PLUGIN_ROOT="$(cd -- "${SCRIPT_DIR}/.." >/dev/null 2>&1 && pwd)"

PYTHONPATH="${PLUGIN_ROOT}/src${PYTHONPATH:+:${PYTHONPATH}}" \
  python3 -m codex_visual_loop_plugin.visual_loop_feedback "$@"
