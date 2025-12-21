#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
MANIFEST_PATH="${ROOT}/Cargo.toml"
RUST_BIN_RELEASE="${ROOT}/target/release/codex-visual-loop"

if [[ -f "${MANIFEST_PATH}" ]] && command -v cargo >/dev/null 2>&1; then
  echo "Building Rust backend (release)..."
  cargo build --release --manifest-path "${MANIFEST_PATH}"
else
  echo "Rust manifest not found at ${MANIFEST_PATH}; skipping Rust build"
fi

python3 -m pip install -e "${ROOT}" || \
python3 -m pip install --user --break-system-packages -e "${ROOT}"

echo "Installed codex-visual-loop-plugin wrapper entrypoints."
if [[ -x "${RUST_BIN_RELEASE}" ]]; then
  echo "Rust binary ready: ${RUST_BIN_RELEASE}"
  echo "Tip: export CODEX_VISUAL_LOOP_RUST_BIN='${RUST_BIN_RELEASE}'"
fi

echo "Run: codex-visual-loop --help"
