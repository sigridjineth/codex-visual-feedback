#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." >/dev/null 2>&1 && pwd)"
APP="${APP:-Finder}"
DEPTH="${AX_DEPTH:-4}"
OUT_DIR="${REPO_ROOT}/.codex-visual-loop/happy"
mkdir -p "${OUT_DIR}"

log() {
  printf '[happy-path] %s\n' "$*"
}

run_cli() {
  if command -v codex-visual-loop >/dev/null 2>&1; then
    codex-visual-loop "$@"
  else
    python3 -m codex_visual_loop_plugin.cli "$@"
  fi
}

log "Running bootstrap"
"${SCRIPT_DIR}/bootstrap.sh"

log "Running doctor"
"${SCRIPT_DIR}/doctor.sh"

log "Running make verify"
make -C "${REPO_ROOT}" verify

log "Running make test"
make -C "${REPO_ROOT}" test

if [[ -n "${OMX_TEAM_WORKER:-}" ]]; then
  log "OMX worker detected: running inbox-aware helper first"
  run_cli visual-loop-feedback --json --execute --ax-depth "${DEPTH}" > "${OUT_DIR}/inbox-${OMX_TEAM_WORKER//\//-}.json"
fi

TS="$(date +%Y%m%d-%H%M%S)"
CAPTURE_JSON="$(run_cli capture --process "${APP}" --strict --json)"
AX_JSON="$(run_cli ax-tree --process "${APP}" --depth "${DEPTH}" --json)"

PACKET_PATH="${OUT_DIR}/happy-path-${APP// /-}-${TS}.json"

CAPTURE_JSON="${CAPTURE_JSON}" AX_JSON="${AX_JSON}" APP="${APP}" PACKET_PATH="${PACKET_PATH}" python3 - <<'PY'
import json
import os
from pathlib import Path

capture = json.loads(os.environ['CAPTURE_JSON'])
ax = json.loads(os.environ['AX_JSON'])
packet = {
    "flow": "happy-path",
    "app": os.environ["APP"],
    "capture": capture,
    "ax_tree": ax,
}
out = Path(os.environ["PACKET_PATH"])
out.parent.mkdir(parents=True, exist_ok=True)
out.write_text(json.dumps(packet, ensure_ascii=False, indent=2), encoding='utf-8')
print(str(out))
PY

log "Happy-path automation completed"
log "Packet: ${PACKET_PATH}"
