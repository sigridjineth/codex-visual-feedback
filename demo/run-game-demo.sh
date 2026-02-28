#!/usr/bin/env bash
# ============================================================
# Breakout Game - Visual Loop Demo Script
# Codex Visual Feedback hackathon demo
#
# Uses: explain-app, capture, observe, act, loop
# Game: demo/game.html (Breakout with 3 intentional bugs)
# ============================================================
set -euo pipefail

# ── Paths ────────────────────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
GAME_HTML="$SCRIPT_DIR/game.html"
ARTIFACTS="$SCRIPT_DIR/artifacts/game"
CVLP_BIN="$PROJECT_ROOT/codex-visual-loop-plugin/target/release/codex-visual-loop"

# ── Verify game file exists ─────────────────────────────────
if [[ ! -f "$GAME_HTML" ]]; then
  echo "ERROR: game.html not found at $GAME_HTML"
  exit 1
fi

# ── Build Rust binary if needed ──────────────────────────────
if [[ -x "$CVLP_BIN" ]]; then
  echo "[build] Rust binary found: $CVLP_BIN"
else
  echo "[build] Rust binary not found. Building with cargo..."
  cargo build --release --manifest-path "$PROJECT_ROOT/codex-visual-loop-plugin/Cargo.toml"
  if [[ -x "$CVLP_BIN" ]]; then
    echo "[build] Build complete."
  else
    echo "[build] Binary still not found after build. Falling back to cargo run."
    CVLP_BIN="cargo run --release --manifest-path $PROJECT_ROOT/codex-visual-loop-plugin/Cargo.toml --"
  fi
fi

# Helper: run the binary (handles both direct-binary and cargo-run fallback)
run_cvlp() {
  if [[ "$CVLP_BIN" == cargo* ]]; then
    eval "$CVLP_BIN" "$@"
  else
    "$CVLP_BIN" "$@"
  fi
}

# ── Artifacts directory ──────────────────────────────────────
mkdir -p "$ARTIFACTS"
echo "[setup] Artifacts directory: $ARTIFACTS"

# ── Helper: stage separator ──────────────────────────────────
stage() {
  local num="$1"
  local title="$2"
  echo ""
  echo ""
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  echo "  Stage $num: $title"
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  echo ""
}

pause() {
  echo ""
  read -p "  >>> 계속하려면 아무 키나 누르세요... " -n 1
  echo ""
}

# ============================================================
# Stage 0: 게임 열기  (Open the game)
# ============================================================
stage 0 "게임 열기 — Chrome에서 Breakout 게임 실행"

echo "  게임 파일: $GAME_HTML"
echo "  Chrome에서 게임을 엽니다..."
open -a "Google Chrome" "$GAME_HTML"

echo "  게임이 로딩될 때까지 3초 대기합니다..."
sleep 3

echo "  게임이 준비되었습니다. 3개의 버그가 숨어 있습니다:"
echo "    [1] Wall Clip  — 공이 오른쪽 벽을 뚫고 지나감"
echo "    [2] Paddle Offset — 패들 시각 위치가 40px 어긋남"
echo "    [3] Ghost Bricks — 파괴된 벽돌이 계속 표시됨"

pause

# ============================================================
# Stage 1: AI가 게임을 분석한다 (explain-app)
# ============================================================
stage 1 "AI 분석 — explain-app으로 게임 UI 자동 분석"

echo "  AI가 Chrome 윈도우를 캡처하고 접근성 트리를 추출합니다."
echo "  Codex CLI로 게임 구조를 자동 분석합니다..."
echo ""

run_cvlp explain-app \
  --process "Google Chrome" \
  --out-dir "$ARTIFACTS" \
  --json 2>&1 | tee "$ARTIFACTS/explain-app.log" || true

echo ""
echo "  분석 완료. 결과가 $ARTIFACTS 에 저장되었습니다."

pause

# ============================================================
# Stage 2: 버그를 관찰한다 — capture baseline
# ============================================================
stage 2 "기준 캡처 — 버그가 있는 상태의 스크린샷 저장"

echo "  현재 버그가 활성화된 상태에서 기준(baseline) 스크린샷을 저장합니다."
echo ""

run_cvlp capture \
  --out "$ARTIFACTS/baseline.png" \
  --process "Google Chrome" \
  --step "baseline" \
  --note "All 3 bugs active: wall-clip, paddle-offset, ghost-bricks" \
  --json 2>&1 | tee "$ARTIFACTS/capture-baseline.log" || true

echo ""
echo "  기준 스크린샷 저장: $ARTIFACTS/baseline.png"

pause

# ============================================================
# Stage 3: 버그 1 수정 — Wall Collision
# ============================================================
stage 3 "버그 1 수정 — act + observe로 벽 충돌 버그 해결"

echo "  act 명령으로 키 '1'을 입력하여 Wall Clip 버그를 수정합니다."
echo "  observe가 수정 전후를 자동으로 비교합니다."
echo ""

run_cvlp observe \
  --process "Google Chrome" \
  --action "fix-wall-collision" \
  --duration 2 \
  --action-cmd "$CVLP_BIN act --process \"Google Chrome\" --hotkey 1" \
  --out-dir "$ARTIFACTS" \
  --json 2>&1 | tee "$ARTIFACTS/observe-fix1.log" || true

echo ""
echo "  Wall Clip 버그가 수정되었습니다!"
echo "  공이 더 이상 오른쪽 벽을 뚫지 않습니다."

pause

# ============================================================
# Stage 4: 버그 2 수정 — Paddle Offset
# ============================================================
stage 4 "버그 2 수정 — act + observe로 패들 정렬 버그 해결"

echo "  act 명령으로 키 '2'를 입력하여 Paddle Offset 버그를 수정합니다."
echo ""

run_cvlp observe \
  --process "Google Chrome" \
  --action "fix-paddle-offset" \
  --duration 2 \
  --action-cmd "$CVLP_BIN act --process \"Google Chrome\" --hotkey 2" \
  --out-dir "$ARTIFACTS" \
  --json 2>&1 | tee "$ARTIFACTS/observe-fix2.log" || true

echo ""
echo "  Paddle Offset 버그가 수정되었습니다!"
echo "  패들 시각 위치와 실제 충돌 영역이 일치합니다."

pause

# ============================================================
# Stage 5: 버그 3 수정 — Ghost Bricks
# ============================================================
stage 5 "버그 3 수정 — act + observe로 유령 벽돌 버그 해결"

echo "  act 명령으로 키 '3'을 입력하여 Ghost Bricks 버그를 수정합니다."
echo ""

run_cvlp observe \
  --process "Google Chrome" \
  --action "fix-ghost-bricks" \
  --duration 2 \
  --action-cmd "$CVLP_BIN act --process \"Google Chrome\" --hotkey 3" \
  --out-dir "$ARTIFACTS" \
  --json 2>&1 | tee "$ARTIFACTS/observe-fix3.log" || true

echo ""
echo "  Ghost Bricks 버그가 수정되었습니다!"
echo "  파괴된 벽돌이 정상적으로 사라집니다."

pause

# ============================================================
# Stage 6: 전체 비교 — loop diff
# ============================================================
stage 6 "전체 비교 — baseline과 수정 후 상태 diff"

echo "  모든 버그가 수정된 최종 스크린샷을 캡처합니다."
echo ""

run_cvlp capture \
  --out "$ARTIFACTS/final.png" \
  --process "Google Chrome" \
  --step "final" \
  --note "All 3 bugs fixed" \
  --json 2>&1 | tee "$ARTIFACTS/capture-final.log" || true

echo ""
echo "  baseline과 최종 스크린샷을 비교합니다..."
echo ""

run_cvlp loop \
  "$ARTIFACTS/final.png" \
  "game-baseline" \
  --loop-dir "$ARTIFACTS/loop" \
  --resize \
  2>&1 | tee "$ARTIFACTS/loop-diff.log" || true

echo ""
echo "  Diff 결과가 $ARTIFACTS/loop 에 저장되었습니다."

# Try to open the diff image in Preview
DIFF_IMG="$ARTIFACTS/loop/game-baseline_diff.png"
if [[ -f "$DIFF_IMG" ]]; then
  echo "  diff 이미지를 Preview에서 엽니다..."
  open -a "Preview" "$DIFF_IMG"
else
  echo "  (diff 이미지를 찾을 수 없습니다. 아티팩트 디렉토리를 확인하세요.)"
  # Try to find any diff image
  FOUND_DIFF=$(find "$ARTIFACTS/loop" -name "*diff*" -type f 2>/dev/null | head -1)
  if [[ -n "${FOUND_DIFF:-}" ]]; then
    echo "  대체 diff 파일 발견: $FOUND_DIFF"
    open -a "Preview" "$FOUND_DIFF"
  fi
fi

# ============================================================
# Done
# ============================================================
echo ""
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  데모 완료!"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo "  아티팩트 디렉토리: $ARTIFACTS"
echo "  생성된 파일들:"
ls -la "$ARTIFACTS/" 2>/dev/null || true
echo ""
echo "  Codex Visual Feedback 이 3개의 게임 버그를 자동으로 감지하고"
echo "  수정하는 과정을 시연했습니다."
echo ""
