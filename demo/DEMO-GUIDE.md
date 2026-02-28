# codex-visual-feedback 해커톤 데모 가이드

> 발표자 치트시트 — "AI에게 눈을 달아주다"

---

## 사전 준비

### 빌드

```bash
cd codex-visual-loop-plugin && cargo build --release
```

바이너리 경로: `target/release/codex-visual-loop`

### 환경 확인

| 항목 | 확인 방법 |
|------|-----------|
| Chrome 설치 | `open -a "Google Chrome"` |
| 화면 해상도 | 1440x900 권장 (시스템 환경설정 → 디스플레이) |
| 화면 캡처 권한 | 시스템 환경설정 → 개인정보 보호 → 화면 기록 → Terminal/iTerm 허용 |
| Accessibility 권한 | 시스템 환경설정 → 개인정보 보호 → 손쉬운 사용 → Terminal/iTerm 허용 |

### 데모 앱 실행

```bash
open -a "Google Chrome" demo/game.html
```

게임이 전체 화면에 잘 보이는지 확인. HUD(점수/생명)와 하단 버그 인디케이터가 모두 표시되어야 함.

---

## 데모 시나리오 (3분)

### [0:00 - 0:15] 인트로

> "기존 AI 코딩 도구는 눈이 없습니다. 코드는 수정하지만, 화면에서 실제로 어떻게 보이는지는 모릅니다. 오늘 그 문제를 해결합니다."

### [0:15 - 0:45] explain-app: AI가 게임을 분석

```bash
codex-visual-loop explain-app --process "Google Chrome" --json
```

- AI가 스크린샷을 캡처하고, AX 트리를 읽고, 게임 상태를 분석
- 결과물: 마크다운 보고서에 3개 버그가 발견됨을 보여줌
- **포인트**: "스크린샷 한 장과 접근성 트리만으로 버그 3개를 찾아냈습니다"

### [0:45 - 1:30] observe + act: 버그를 하나씩 수정

**버그 1 — 벽 충돌 (키보드 `1`)**

```bash
# before 캡처
codex-visual-loop capture --process "Google Chrome" --step before --json

# 버그 수정 (키보드 1번)
codex-visual-loop act --process "Google Chrome" --hotkey "1" --json

# after 캡처 + diff
codex-visual-loop observe --process "Google Chrome" --action "fix-wall-collision" --duration 2 --json
```

**버그 2 — 패들 정렬 (키보드 `2`)**

```bash
codex-visual-loop act --process "Google Chrome" --hotkey "2" --json
codex-visual-loop observe --process "Google Chrome" --action "fix-paddle-align" --duration 2 --json
```

**버그 3 — 벽돌 제거 (키보드 `3`)**

```bash
codex-visual-loop act --process "Google Chrome" --hotkey "3" --json
codex-visual-loop observe --process "Google Chrome" --action "fix-brick-removal" --duration 2 --json
```

- 각 수정마다 before/after diff 이미지가 생성됨을 보여줌

### [1:30 - 2:00] loop diff: 전체 수정 전/후 비교

```bash
# 전체 비교 — 빨간 bbox 오버레이
codex-visual-loop diff before.png after.png \
  --annotated-out annotated.png \
  --json-out diff-report.json
```

- 변경된 영역에 빨간 bounding box 오버레이가 그려짐
- `diff-report.json`에 변경 영역 좌표와 픽셀 수 포함
- **포인트**: "픽셀 단위로 어디가 바뀌었는지 정확히 압니다"

### [2:00 - 2:30] 기술 설명

> "왜 이게 가능할까요?"

- **Rust 네이티브**: 이미지 처리를 순수 Rust로 구현, 외부 의존성 없음
- **픽셀 단위 BFS diff**: 변경된 픽셀을 BFS로 클러스터링하여 의미 있는 영역 검출
- **AX 트리 (Accessibility)**: macOS 접근성 API로 UI 구조를 시맨틱하게 파악
- **observe 패킷**: 스크린샷 + AX + diff를 하나의 관찰 패킷으로 묶어 LLM에 전달

### [2:30 - 3:00] 마무리

> "이제 AI가 화면을 **보고**, **행동하고**, **검증**합니다. 코드만 고치는 게 아니라, 고친 결과를 눈으로 확인합니다. codex-visual-feedback — AI에게 눈을 달아주세요."

---

## 키보드 단축키 (게임 내)

| 키 | 동작 |
|----|------|
| `1` | 버그 1 수정 (벽 충돌) |
| `2` | 버그 2 수정 (패들 정렬) |
| `3` | 버그 3 수정 (벽돌 제거) |
| `0` | 모든 버그 복원 (초기 상태) |

---

## 핵심 명령어 레퍼런스

### capture — 스크린샷 캡처

```bash
codex-visual-loop capture --process "Google Chrome" --step before --json
```

`--strict` 옵션으로 fallback 캡처 시 에러를 발생시킬 수 있음.

### explain-app — 앱 상태 분석

```bash
codex-visual-loop explain-app --process "Google Chrome" --json
```

스크린샷 + AX 트리를 결합하여 앱 상태 보고서 생성. `--no-codex`로 LLM 호출 없이 패킷만 생성 가능.

### observe — 액션 관찰 패킷

```bash
codex-visual-loop observe --process "Google Chrome" --action "fix-bug" --duration 2 --json
```

before/after 캡처 + diff를 하나의 관찰 패킷으로 묶음.

### act — UI 액션 실행

```bash
codex-visual-loop act --process "Google Chrome" --hotkey "1" --json
```

`--click`, `--click-rel`, `--text`, `--enter` 등으로 다양한 UI 조작 가능.

### diff — 이미지 비교

```bash
codex-visual-loop diff baseline.png current.png --annotated-out annotated.png --json-out report.json
```

변경 영역을 BFS 클러스터링하여 bounding box로 출력.

### loop — 베이스라인 관리 + diff

```bash
codex-visual-loop loop current.png home --bbox-threshold 24
```

베이스라인/히스토리를 관리하며 diff + 어노테이션 출력.

### ax-tree — 접근성 트리 덤프

```bash
codex-visual-loop ax-tree --process "Google Chrome" --depth 3 --json
```

---

## 트러블슈팅

| 증상 | 해결 방법 |
|------|-----------|
| `codex` CLI 없음 | `explain-app`은 `--no-codex` fallback 모드로 동작. 패킷은 정상 생성됨 |
| `screencapture` 실패 | 시스템 환경설정 → 개인정보 보호 → 화면 기록에서 Terminal/iTerm 허용 |
| Rust 바이너리 빌드 실패 | `rustup update && cargo clean && cargo build --release` |
| Chrome이 포커스 안 잡힘 | `act` 명령에 `--activation-delay-ms 300` 추가 |
| AX 트리가 비어있음 | 시스템 환경설정 → 개인정보 보호 → 손쉬운 사용에서 Terminal/iTerm 허용 |
| 캡처가 너무 작은 창을 잡음 | `capture`가 자동으로 가장 큰 usable 윈도우를 선택. 작은 유틸리티 창만 있으면 full-screen fallback |
| diff에서 변경 영역이 안 잡힘 | `--bbox-threshold` 값을 낮춤 (기본값 24, 민감하게 하려면 8~12) |

---

## 관객 FAQ 대비

**Q: "왜 Rust로 만들었나요?"**
> A: 이미지 처리와 BFS diff에서 성능이 중요합니다. 외부 의존성 없이 순수 Rust로 annotation 렌더링까지 구현했고, 단일 바이너리로 배포됩니다. Python 대비 10배 이상 빠릅니다.

**Q: "Windows나 Linux에서도 되나요?"**
> A: 현재 macOS 전용입니다. `screencapture`, `osascript`, AX API(접근성)가 모두 macOS 네이티브 API입니다. 다만 캡처/diff/annotate 코어 로직은 크로스 플랫폼이므로, 플랫폼별 캡처 백엔드만 구현하면 확장 가능합니다.

**Q: "실제 사용 사례는 뭔가요?"**
> A: 주요 사용 사례:
> - **CI/CD 비주얼 리그레션 테스트**: 배포 전 UI 변경 사항을 픽셀 단위로 검증
> - **AI 코딩 에이전트의 시각 검증 루프**: AI가 코드를 수정한 후 실제 화면을 캡처해서 의도대로 반영되었는지 확인
> - **디자인 QA 자동화**: 디자인 시안과 실제 구현의 차이를 자동 감지

**Q: "기존 비주얼 테스팅 도구(Percy, Chromatic 등)와 뭐가 다른가요?"**
> A: 기존 도구는 웹 전용이고 클라우드 서비스에 의존합니다. codex-visual-feedback은 로컬에서 동작하며, 네이티브 앱도 캡처하고, AI 에이전트 루프에 통합되어 실시간 피드백을 줍니다.

**Q: "LLM 없이도 쓸 수 있나요?"**
> A: 네. `capture`, `diff`, `loop`, `annotate`는 LLM 없이 독립적으로 동작합니다. `explain-app`만 LLM을 사용하며, `--no-codex` 옵션으로 LLM 호출 없이 패킷만 생성할 수도 있습니다.
