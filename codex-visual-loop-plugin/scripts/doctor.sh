#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." >/dev/null 2>&1 && pwd)"
PLUGIN_DIR="${REPO_ROOT}/codex-visual-loop-plugin"
RUST_DEBUG_BIN="${PLUGIN_DIR}/target/debug/codex-visual-loop"
PY_CLI="${PLUGIN_DIR}/src/codex_visual_loop_plugin/cli.py"
CODEX_HOME="${CODEX_HOME:-${HOME}/.codex}"
CODEX_AUTO="${CODEX_HOME}/bin/codex-auto"

FAILURES=0
WARNINGS=0

pass() {
  printf '[doctor] PASS: %s\n' "$*"
}

warn() {
  printf '[doctor] WARN: %s\n' "$*"
  WARNINGS=$((WARNINGS + 1))
}

fail() {
  printf '[doctor] FAIL: %s\n' "$*" >&2
  FAILURES=$((FAILURES + 1))
}

check_cmd() {
  local cmd="$1"
  local required="${2:-required}"
  if command -v "${cmd}" >/dev/null 2>&1; then
    pass "command available: ${cmd}"
  else
    if [[ "${required}" == "required" ]]; then
      fail "missing required command: ${cmd}"
    else
      warn "missing optional command: ${cmd}"
    fi
  fi
}

check_cmd bash required
check_cmd python3 required
check_cmd make required
check_cmd codex optional
check_cmd osascript optional
check_cmd screencapture optional
check_cmd cargo optional

PY_USER_BIN="$(python3 - <<'PY'
import site
print(site.USER_BASE + '/bin')
PY
)"

if [[ ":$PATH:" == *":${PY_USER_BIN}:"* ]]; then
  pass "python user bin on PATH (${PY_USER_BIN})"
else
  warn "python user bin not on PATH (${PY_USER_BIN})"
fi

if [[ ":$PATH:" == *":${CODEX_HOME}/bin:"* ]]; then
  pass "${CODEX_HOME}/bin on PATH"
else
  warn "${CODEX_HOME}/bin not on PATH"
fi

if [[ -x "${CODEX_AUTO}" ]]; then
  pass "codex-auto wrapper installed (${CODEX_AUTO})"
else
  warn "codex-auto wrapper missing; run make bootstrap"
fi

if command -v codex-visual-loop >/dev/null 2>&1; then
  if codex-visual-loop commands >/dev/null 2>&1; then
    pass "codex-visual-loop command invocation works"
  else
    fail "codex-visual-loop command exists but failed to run 'commands'"
  fi
  if codex-visual-loop visual-loop-feedback --help >/dev/null 2>&1; then
    pass "visual-loop-feedback helper command is available"
  else
    fail "visual-loop-feedback helper command failed to render help"
  fi
elif [[ -x "${RUST_DEBUG_BIN}" ]]; then
  if "${RUST_DEBUG_BIN}" commands >/dev/null 2>&1; then
    pass "Rust debug binary invocation works"
  else
    fail "Rust debug binary exists but failed to run 'commands'"
  fi
elif [[ -f "${PY_CLI}" ]]; then
  if python3 "${PY_CLI}" commands >/dev/null 2>&1; then
    pass "Python CLI fallback invocation works"
  else
    fail "Python CLI fallback failed to run 'commands'"
  fi
else
  fail "no runnable codex-visual-loop entrypoint found"
fi

PEP_CHECK="$(python3 -m pip install --dry-run -e "${PLUGIN_DIR}" 2>&1 || true)"
if printf '%s' "${PEP_CHECK}" | grep -qi 'externally-managed-environment'; then
  warn "PEP668 detected; bootstrap will use --user --break-system-packages fallback"
else
  pass "pip editable install appears allowed (no PEP668 block detected in dry-run)"
fi

if command -v osascript >/dev/null 2>&1; then
  if osascript -e 'tell application "System Events" to get name of first process whose frontmost is true' >/dev/null 2>&1; then
    pass "Accessibility/System Events query succeeded"
  else
    warn "Accessibility query failed (grant Terminal access under Privacy & Security > Accessibility)"
  fi
fi

if command -v screencapture >/dev/null 2>&1; then
  TMP_CAPTURE="${TMPDIR:-/tmp}/cvlp-doctor-screen-$$.png"
  if screencapture -x "${TMP_CAPTURE}" >/dev/null 2>&1; then
    if [[ -s "${TMP_CAPTURE}" ]]; then
      pass "Screen capture command works"
    else
      warn "screencapture produced an empty file"
    fi
  else
    warn "screencapture failed (grant Terminal Screen Recording permission)"
  fi
  rm -f "${TMP_CAPTURE}" >/dev/null 2>&1 || true
fi

if [[ -n "${OMX_TEAM_WORKER:-}" ]]; then
  pass "OMX team worker context detected (${OMX_TEAM_WORKER})"
  if [[ -f "${REPO_ROOT}/codex-visual-loop-plugin/scripts/inbox_visual_loop.sh" ]]; then
    pass "inbox visual-loop helper script present"
  else
    fail "inbox visual-loop helper script missing"
  fi
fi

if [[ ${FAILURES} -gt 0 ]]; then
  printf '[doctor] Completed with %d failure(s), %d warning(s)\n' "${FAILURES}" "${WARNINGS}" >&2
  exit 1
fi

printf '[doctor] All required checks passed (%d warning(s))\n' "${WARNINGS}"
