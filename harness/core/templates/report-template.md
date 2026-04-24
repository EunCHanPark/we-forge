# Report Template — 단계별 Handoff 보고서

## Purpose
각 역할(Coder, Security Reviewer, Tester)이 단계 완료 후 남기는 짧은 보고서 템플릿.
파일명: `<plan-id>-<role>-report.md` (예: `EP-001-coder-report.md`)

## Template

```md
# <Plan ID> — <Role> Report

## Verdict
- APPROVED | CHANGES_REQUESTED | BLOCKED | SKIPPED

## Reason
-

## Scope
- 이번 단계에서 확인한 범위

## Checked
- [x] 항목 1
- [x] 항목 2

## Passed
-

## Issues
- none

## Open Risks
- none

## Next Owner
- PM | Coder | Security Reviewer | Tester
```

## Rule
- 길게 쓰지 않는다.
- 판정 값은 정해진 네 가지 중 하나만 쓴다.
- `CHANGES_REQUESTED` 시: 구체적 파일:라인 + 수정 방법 포함.
- 확인한 것과 남은 위험을 분리해서 적는다.
- 다음 단계가 바로 이어받을 수 있게 쓴다.
