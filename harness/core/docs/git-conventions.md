# Git 컨벤션 — we-forge

## 커밋 메시지

Conventional Commits: `<type>(<scope>): <description>`

| Type | 용도 |
|------|------|
| `feat` | 새 기능 |
| `fix` | 버그 수정 |
| `refactor` | 동작 변경 없는 코드 개선 |
| `docs` | 문서 변경 |
| `test` | 테스트 추가/수정 |
| `chore` | 빌드, CI, 의존성 |
| `style` | 포맷팅 (동작 무관) |
| `harness` | 하네스 구조/설정 변경 |
| `infra` | 배포/컨테이너/CI 매니페스트 |
| `migration` | DB 마이그레이션 |

```
feat(auth): add Google OAuth 2FA
fix(api): retry after connection timeout
harness(eval): adjust scoring rule
infra(k8s): adjust deployment resource limit
migration(app): 004_add_subscription_tier
docs(exec-plan): move EP-002 to completed
```

### 규칙
- 한 커밋에 하나의 논리적 변경 (structural change와 behavioral change 분리, Tidy First)
- 제목 72자 이내, 명령형
- 본문은 "왜"를 설명

## 브랜치 전략

```
main ─── feat/* ─── fix/* ─── refactor/* ─── harness/* ─── infra/*
```

- `main`: 항상 배포 가능 상태
- `feat/*`: 새 기능 개발
- `fix/*`: 버그 수정
- `refactor/*`: 코드 개선 (동작 변경 없음)
- `harness/*`: 하네스/docs 구조 변경
- `infra/*`: 배포/CI 변경

## 에이전트 브랜치

- Claude Code가 자동 생성하는 브랜치: `claude/*` (예: `claude/harness-v1`)
- PR 머지 후 브랜치 삭제 원칙

## 커밋 전 체크리스트

```
[ ] 린트 에러 없음
[ ] 타입 체크 통과
[ ] 테스트 통과
[ ] secrets/ 파일 포함되지 않음
[ ] .env 파일 포함되지 않음
```

## 태그 (릴리즈)

```
v1.0.0 → v1.1.0 → v1.1.1  (semantic versioning)
```

- 프로덕션 배포 시 태그 필수
- 태그 메시지에 주요 변경 사항 기록
