# Pipeline Workflow — we-forge

## 1. Purpose
이 문서는 작업 흐름의 기본 순서를 정의한다.
세부 역할 규칙은 `harness/core/roles/` 문서에서 따로 다룬다.

## 2. Default Order

```
PM → Coder → Security Reviewer → Tester → PM
```

## 3. Optional Stages
- **Designer**: UI, 정보 구조, 사용자 흐름이 바뀌는 작업에서만 사용.
  들어가면 순서: `PM → Designer → Coder → Security Reviewer → Tester → PM`
- optional 단계가 필요 없으면 report에 `SKIPPED`로 남긴다.

## 4. Handoff Rule
- 앞 단계가 끝나기 전에는 다음 단계로 넘어가지 않는다.
- 각 단계는 자기 역할의 기준으로만 판단한다.
- 각 단계는 handoff report를 `harness/plans/ongoing/` 파일에 남긴다.
- 판정 값은 `APPROVED`, `CHANGES_REQUESTED`, `BLOCKED`, `SKIPPED`만 사용한다.
- 다음 단계는 `APPROVED` 또는 `SKIPPED`일 때만 진행한다.
- `CHANGES_REQUESTED` 또는 `BLOCKED`가 나오면 시도 횟수를 올린다.

## 5. Output Rule
- PM: 범위, 완료 기준, 관련 기능(F###) 명시
- Coder: 구현 결과, 셀프 체크 결과 (lint, type, test)
- Security Reviewer: agents.md 규칙 위반 여부, 신뢰 경계 점검
- Tester: 테스트 결과, 커버리지, 판정

## 6. 2-stage Review 파이프라인 (에이전트 작업 단위)

§2의 PM→Coder→Security→Tester→PM이 *역할* 흐름이라면, 이 절은 각 역할의 *단일 작업 단위* 내부에서 에이전트가 따르는 리뷰 체인을 정의한다.

```
advisor → Claude code → /everything-claude-code:code-review
```

| 단계 | 도구 (예시: Claude Code) | 역할 | 트리거 |
|------|-------------------------|------|--------|
| 1. advisor | `advisor()` 콜 (또는 플랫폼 상응 도구) | 상위 리뷰어에게 전체 맥락을 넘겨 접근 방향·스코프·리스크를 검증 | 실질적 작업 시작 전 (R0.5) / 접근 방식 변경 / 작업 완료 선언 전 |
| 2. Claude code (또는 사용 중인 에이전트 세션) | 본 세션 내 편집 | advisor 결과를 반영하여 실제 코드·문서 변경을 적용 | 상시 |
| 3. /everything-claude-code:code-review | 슬래시 커맨드 (또는 플랫폼 상응 리뷰 게이트) | 변경된 워킹트리에 대한 독립 리뷰 게이트 | 커밋/머지 직전 또는 일괄 변경 후 |

**"2-stage review" 명칭 근거**: advisor(사전 리뷰) + code-review(사후 리뷰) = 2단계 리뷰 샌드위치, 그 사이에 에이전트 구현.

**적용 범위**: 모든 실질적 작업 (코드 작성, 문서 수정, 마이그레이션, 배포 결정).
**적용 제외**: orientation (파일 탐색, `git status` 등 read-only 조사).
**근거**: `harness/docs/agents.md` R0.5, B7.

## 6-bis. Agent Teams 하이브리드 확장 (선택적)

§6의 3단계 체인은 **single session 기본값**이다. 특정 조건에서 `advisor`가 team을 권고하면 아래의 하이브리드 (a)+(c) 배치로 확장한다.

### 6-bis.1 배치 개요

```
(c) Team research  ──┐
                     ├─→ advisor  ──→  [single agent session | (a) Team implement]  ──→  code-review
(optional, 리서치용)   ┘
```

- **(c) Research/Review 단계** — advisor 호출 **전** 또는 **과정**에서 team spawn 허용. 팀의 synthesis 결과를 advisor 입력으로 넘긴다.
- **(a) 구현 단계 대안** — 구현 스텝에서 single session 대신 team **선택 가능**. 조건 충족 시만. **기본값은 여전히 single session**.

### 6-bis.2 Team 적합/부적합 판단

| Team 적합 (spawn 권장) | 단일 세션 유지 (spawn 금지) |
|------------------------|------------------------------|
| cross-layer 변경 (front/back/tests 각자 소유) | 같은 파일 공동 편집 (overwrite 리스크) |
| 독립 모듈 병렬 리팩터 | 순차 의존성이 강한 작업 |
| 새 기능 여러 조각 동시 착수 | routine/소규모 작업 (토큰 비용 낭비, B3) |
| 경쟁 가설 조사 (competing hypotheses) | 단일 파일 버그 수정 |
| PR 멀티렌즈 리뷰 (security + performance + tests) | 단일 PR의 linear 적용 |

### 6-bis.3 결정 흐름

1. advisor가 task의 team 적합성을 판정한다 (§6-bis.2 기준).
2. "적합"이면 lead가 team spawn 도구(예: Claude Code `TeamCreate`) 또는 natural-language 요청으로 팀 생성.
3. 팀 작업 진행 (R13 규칙 준수 — 크기, cleanup, risky 작업 격리).
4. lead가 teammates의 결과를 synthesis.
5. code-review 게이트로 진입 (§6 3단계).

### 6-bis.4 연동 근거

- 상세 스폰 규칙: `harness/docs/agents.md` R0.6, R13.1~R13.6
- 플랫폼 제약: `harness/core/platforms/claude-code.md` §5 (활성화·표시 모드·제약)

## 7. 앱별 특수 규칙 (app-level에서 확장)
- 배포가 포함된 plan: Coder 단계에서 dry-run diff 먼저 확인
- DB 마이그레이션이 포함된 plan: Tester가 스테이징에서 up/down 검증
- 외부 서비스 연동이 포함된 plan: Security Reviewer가 API Key 노출 여부 추가 점검

## 8. Completion Rule
- 마지막 PM 판정이 `APPROVED`일 때만 완료.
- 완료된 ongoing plan → `harness/plans/completed/`로 이동.
- `tracker.md` 갱신 필수.
