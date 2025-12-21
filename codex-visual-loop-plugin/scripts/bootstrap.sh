#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." >/dev/null 2>&1 && pwd)"
PLUGIN_DIR="${REPO_ROOT}/codex-visual-loop-plugin"
SKILL_SRC="${REPO_ROOT}/skills/codex-visual-loop"
CODEX_HOME="${CODEX_HOME:-${HOME}/.codex}"
SKILL_DST="${CODEX_HOME}/skills/codex-visual-loop"
CODEX_BIN_DIR="${CODEX_HOME}/bin"
ZSHRC_PATH="${HOME}/.zshrc"

log() {
  printf '[bootstrap] %s\n' "$*"
}

require_cmd() {
  local cmd="$1"
  if ! command -v "${cmd}" >/dev/null 2>&1; then
    printf '[bootstrap] missing required command: %s\n' "${cmd}" >&2
    exit 1
  fi
}

ensure_path_block() {
  local py_user_bin="$1"
  local marker_start="# >>> codex-visual-loop bootstrap >>>"
  local marker_end="# <<< codex-visual-loop bootstrap <<<"
  local block
  block="${marker_start}\n# Added by codex-visual-loop bootstrap\nexport PATH=\"${CODEX_BIN_DIR}:${py_user_bin}:\$PATH\"\n${marker_end}"

  if [[ ! -f "${ZSHRC_PATH}" ]]; then
    printf '%b\n' "${block}" >"${ZSHRC_PATH}"
    return
  fi

  if grep -q "${marker_start}" "${ZSHRC_PATH}"; then
    python3 - <<PY
from pathlib import Path
path = Path(${ZSHRC_PATH@Q})
text = path.read_text(encoding='utf-8')
start = ${marker_start@Q}
end = ${marker_end@Q}
block = ${block@Q} + "\n"
if start in text and end in text:
    pre = text.split(start, 1)[0]
    post = text.split(end, 1)[1]
    if post.startswith("\n"):
        post = post[1:]
    path.write_text(pre + block + post, encoding='utf-8')
PY
  else
    printf '\n%b\n' "${block}" >>"${ZSHRC_PATH}"
  fi
}

install_codex_auto_wrapper() {
  mkdir -p "${CODEX_BIN_DIR}"
  cat >"${CODEX_BIN_DIR}/codex-auto" <<'WRAP'
#!/usr/bin/env bash
set -euo pipefail
exec codex --dangerously-bypass-approvals-and-sandbox "$@"
WRAP
  chmod +x "${CODEX_BIN_DIR}/codex-auto"

  cat >"${CODEX_BIN_DIR}/codex-visual-loop-inbox" <<'WRAP'
#!/usr/bin/env bash
set -euo pipefail
if command -v codex-visual-loop >/dev/null 2>&1; then
  exec codex-visual-loop visual-loop-feedback "$@"
fi
exec python3 -m codex_visual_loop_plugin.visual_loop_feedback "$@"
WRAP
  chmod +x "${CODEX_BIN_DIR}/codex-visual-loop-inbox"
}

require_cmd bash
require_cmd python3

if ! python3 -m pip --version >/dev/null 2>&1; then
  printf '[bootstrap] python3 -m pip is required but unavailable\n' >&2
  exit 1
fi

if [[ -f "${PLUGIN_DIR}/Cargo.toml" ]] && command -v cargo >/dev/null 2>&1; then
  log "Building Rust backend (release)"
  cargo build --release --manifest-path "${PLUGIN_DIR}/Cargo.toml"
else
  log "Skipping Rust release build (cargo or Cargo.toml missing)"
fi

log "Installing Python package in editable mode"
if ! python3 -m pip install -e "${PLUGIN_DIR}"; then
  python3 -m pip install --user --break-system-packages -e "${PLUGIN_DIR}"
fi

if [[ ! -d "${SKILL_SRC}" ]]; then
  printf '[bootstrap] skill source directory not found: %s\n' "${SKILL_SRC}" >&2
  exit 1
fi

log "Installing Codex skill wrapper to ${SKILL_DST}"
if [[ -z "${SKILL_DST}" || "${SKILL_DST}" == "/" ]]; then
  printf '[bootstrap] refusing to modify invalid skill destination: %s\n' "${SKILL_DST}" >&2
  exit 1
fi

rm -rf "${SKILL_DST}"
mkdir -p "${SKILL_DST}"
cp -R "${SKILL_SRC}/." "${SKILL_DST}/"

PY_USER_BIN="$(python3 - <<'PY'
import site
print(site.USER_BASE + '/bin')
PY
)"

install_codex_auto_wrapper
ensure_path_block "${PY_USER_BIN}"

log "Bootstrap complete"
log "Open a new shell (or source ~/.zshrc) to use codex-auto and codex-visual-loop"
