# Claude Code Platform Guide — we-forge

## 1. Purpose
Claude Code가 이 저장소에서 작업을 시작할 때 따르는 진입 규칙.

## 2. Read Order (세션 시작 시)
1. `CLAUDE.md` (루트) — 읽기 순서 확인
2. `harness/core/docs/index.md` — 공통 규칙 진입점
3. `harness/core/workflows/pipeline.md` — 작업 흐름
4. `harness/docs/index.md` — 앱별 규칙
5. `harness/plans/tracker.md` — 현재 상태

## 3. Working Rule
- 공통 규칙은 `harness/core/`를 따른다.
- 도메인 규칙은 `harness/docs/`를 따른다.
- 상태 변경이 있으면 `tracker.md`와 `ongoing/` plan을 갱신한다.
- risky 변경 전 git checkpoint 생성 (`harness/scripts/git_checkpoint.py`).
- 하네스 파일을 사용자 요청 없이 수정하지 않는다.

## 4. File Lookup Guide

| 필요한 것 | 위치 |
|----------|------|
| 최상위 원칙 | `harness/core/docs/constitution.md` |
| 인프라/서비스 구조 | `harness/core/docs/repository-architecture.md` |
| 역할 정의 | `harness/core/roles/` |
| 에이전트 규칙 (트리거형) | `harness/docs/agents.md` |
| 현재 작업 상태 | `harness/plans/tracker.md` |
| 품질 현황 | `harness/docs/quality-score.md` |
| 기술 부채 | `harness/plans/tech-debt-tracker.md` |
| 실패 사례 | `harness/references/failure-cases.md` |
| 신뢰 경계 다이어그램 | `harness/references/trust-boundaries.md` |

## 5. Agent Teams (experimental)

> **공식 문서**: <https://code.claude.com/docs/en/agent-teams>
> **스폰 규칙**: `harness/docs/agents.md` R13 (상세)
> **파이프라인 배치**: `harness/core/workflows/pipeline.md` §6-bis

### 5.1 활성화 상태

`.claude/settings.json`에 다음 두 항목을 설정한다:

```json
"env": { "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS": "1" },
"teammateMode": "in-process"
```

**요구 버전**: Claude Code v2.1.32+ (`claude --version`으로 확인).

### 5.2 표시 모드 — 플랫폼 제약

- **`in-process` 기본** — 공식 문서: split-pane 모드는 `Windows Terminal`, `VS Code integrated terminal`, `Ghostty`에서 지원되지 않는다. **tmux / iTerm2**만 split-pane 가능.
- **권장**: Windows 환경(Windows Terminal, Git Bash)은 `in-process` 고정. macOS/Linux에서 tmux 또는 iTerm2가 있으면 split-pane 선택 가능.
- **조작 (in-process)**: lead 터미널에서 Shift+Down으로 teammate 순환, 직접 메시지 입력.

### 5.3 맥락 상속

- teammate는 본 파일(`platforms/claude-code.md`)과 루트 `CLAUDE.md`를 **자동으로 읽는다** — lead와 동일한 프로젝트 컨텍스트 로드.
- **상속되지 않음**: lead의 대화 이력. teammate는 spawn prompt + 프로젝트 파일만 본다 → spawn prompt에 task-specific 맥락을 충분히 담아야 한다 (R13 상세).
- MCP 서버 / skills: project + user 설정에서 로드 (lead와 동일).

### 5.4 플러그인 스킬과의 구분

- **Everything Claude Code `team-builder` 스킬** (설치되어 있는 경우): "Interactive agent picker for composing and dispatching parallel teams" — 내부적으로 `Task(subagent_type=...)` multi-tool 병렬 호출을 구성. **sub-agent 병렬** 메커니즘.
- **Claude Code native agent teams**: `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS` 플래그 + `TeamCreate` 도구 + 공유 task list + 직접 teammate 메시징. **별도 프로세스 Claude Code 세션** 메커니즘.
- **둘은 서로 다른 것**. `harness/docs/agents.md` R0.6의 선택 트리를 따라 분기.

### 5.5 Known Limitations (하네스 영향)

공식 문서의 limitations 중 본 하네스에 영향이 큰 4건:

| 제약 | 하네스 대응 |
|------|-------------|
| `/resume`로 in-process teammate 복원 불가 | Long-running team 금지, 세션 내 완결 (R13.4) |
| One team per session | cleanup 전까지 신규 team spawn 금지 (R13.5) |
| teammate nested team 불가 | team spawn은 **lead 전용** (R13.1) |
| permissions는 spawn 시점에 lead 모드 상속 | risky 작업(인프라 apply, git push, destructive)은 lead 세션에서만 실행 (R13.3) |
