# Governance — 문서 경계와 변경 규칙

## 1. Purpose
이 문서는 repository-level 문서와 app-level 문서의 경계를 정의한다.
변경은 작고 분명해야 하며, 이유를 설명할 수 있어야 한다.

## 2. Document Boundary

| 레이어 | 위치 | 다루는 것 |
|--------|------|----------|
| Repository Core | `harness/core/` | 모든 앱에 공통으로 적용되는 원칙, 역할, 워크플로우 |
| App-Level | `harness/` | 도메인, 기능, UI, 에이전트 규칙, 품질 상태 |
| Plans | `harness/plans/` | 현재 작업 상태, 실행 계획 |
| References | `harness/references/` | 외부 참조, 과거 결정, 실패 사례 |

- repository-level 문서는 공통 규칙만. 앱 고유 내용은 넣지 않는다.
- app-level 문서는 repository-level 규칙보다 느슨한 기준을 두지 않는다.
- 문서가 커지면 더 작은 문서로 나눈다.

## 3. Ownership

| 역할 | 변경 가능 범위 |
|------|---------------|
| PM | 범위, 완료 기준, 문서 정합성 |
| Coder | 구현 문맥에서 필요한 문서 변경 제안 |
| Security Reviewer | 보안 기준 위반 발견 시 문서 수정 요청 |
| Tester | 테스트 기준 위반 발견 시 문서 수정 요청 |
| 에이전트 | constitution.md 이하 문서를 사용자 요청 없이 수정 금지 |

## 4. Change Rules
- 문서 변경은 문제와 이유가 분명할 때만 한다.
- 한 번의 변경에는 한 가지 목적만 담는다.
- repository-level 문서를 바꾸면 관련 app 문서도 함께 점검한다.
- 같은 이유로 수정이 반복되면 repository-level 규칙으로 올릴지 검토한다.
- 하네스 구조 변경 이력: `harness/references/harness-evolution.md`

## 5. Review Rule
- 문서와 구현이 다르면 먼저 문서가 맞는지 확인한다.
- 문서가 더 이상 맞지 않으면 범위를 줄이거나 문서를 고친다.
- 에이전트가 같은 실수를 2회 반복하면 `agents.md`에 규칙 추가 제안.
