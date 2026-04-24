# Constitution — we-forge

> v2.0

## 1. Identity
24/7 background pattern-learning + ECC-marketplace orchestration layer for Claude Code

## 2. Core Values
1. 간결성 — 가장 단순한 구현이 기본이다. 복잡하면 쪼개고 제거한다.
2. 일치성 — 문서와 코드가 다르면 코드를 의심하되, 문서를 먼저 점검하며 문서, 코드, 테스트를 일치시킨다.
3. 관측 가능한 시스템 — 로그·메트릭 없이 프로덕션에 올리지 않는다.
4. 복구 가능한 데이터 — 백업과 롤백이 정의되지 않은 데이터는 프로덕션 불가.
5. 작고 잦은 변경 — 큰 변경보다 작은 커밋을 여러 번 한다.
6. 구조 변경과 기능 변경을 섞지 않는다.
7. 안정성 — 사용자 데이터는 보호 대상이다.

## 3. Non-Negotiables
1. 의존성은 단방향으로만 흐른다 (hooks → events.jsonl → pattern-detector → ECC dedup → synthesize → ledger.jsonl)
2. 인프라, 스키마, 설정은 레포에 코드로 존재한다. 서버 수동 변경은 레포로 돌아온다.
3. secrets/는 PLACEHOLDER만 커밋한다. 실제 시크릿은 수동 적용만.
4. 테스트 없이 완료하지 않는다.
5. 에이전트는 하네스 파일을 사용자 요청 없이 수정하지 않는다.
6. 파괴적 DB 변경(DROP, TRUNCATE)은 백업 확인 없이 실행하지 않는다.
7. 새 외부 의존성은 사용자 승인 없이 추가하지 않는다.
8. 문서 없이 기능을 추가하지 않는다.
9. 테스트를 속여서 통과시키지 않는다.

## 4. Principle Priority
Non-Negotiables는 순서 없이 전부 위반 불가.
Core Values가 충돌하면 상위 tier가 이긴다. 같은 tier 내에서는 판단한다.

- Tier 1 — 데이터: 안정성, 복구 가능한 데이터
- Tier 2 — 정확성: 일치성, 관측 가능한 시스템
- Tier 3 — 실천: 간결성, 작고 잦은 변경, 구조·기능 분리

한 줄 요약: 데이터 > 정확성 > 실천

## 5. Document Hierarchy
문서가 충돌하면 아래 순서를 따른다.
1. Constitution (이 문서)
2. CLAUDE.md — Constitution의 진입점, 작업 규칙
3. harness/core/ — 공통 규칙
4. harness/docs/ — 앱별 규칙
5. harness/plans/ — 실행 계획, tracker
6. harness/references/ — 과거 결정, 실패 사례

- 문서는 의도(spec)이고, 코드는 구현이다. 둘이 다르면 문서가 맞는지 먼저 확인한다.
- 외부 문서(라이브러리, 프레임워크)는 참고한다. 우리 문서와 다르면 우리 문서의 설계 결정을 따른다.

## 6. Roles
- Owner(사용자): 최종 판단자. 하네스 변경 권한, 승인, 방향 결정.
- PM: 범위를 정하고 완료 기준을 고정한다.
- Coder: 문서에 맞게 가장 단순한 구현을 만든다.
- Security Reviewer: 신뢰 경계 위반과 데이터 유출 가능성을 막는다.
- Tester: 문서 기준으로 동작을 검증한다.
- Designer: UI/흐름이 바뀌는 작업에서만 참여 (optional)

## 7. Definition of Done
1. 목적과 범위가 plan 문서에 적혀 있다.
2. 구현이 해당 문서와 모순되지 않는다.
3. 테스트가 존재하고 통과한다.
4. 테스트가 실제 동작을 검증한다 (속여서 통과한 것이 아니다).
5. tracker.md와 ongoing plan이 최신 상태다.
6. 새 외부 의존성이 있으면 사용자 승인을 받았다.

## 8. Boundaries
- 레포 안의 서비스 간 통신은 정의된 API로만 한다.
- 외부 제품은 완성·편입 시점까지 코드·API·데이터 의존성을 갖지 않는다.
- 외부 노출되는 엔드포인트는 trust-boundaries 문서에 명시된 것만 허용한다.

## 9. Writing Style
- 짧게 쓴다. 한 문서에 한 책임만 둔다.
- 에이전트가 파싱할 상태는 고정 필드로 남긴다 (verdict, attempts, phase).
- "왜"는 항상 근거를 남긴다.

## 10. Amendment
이 문서의 변경은 Owner만 결정한다.
