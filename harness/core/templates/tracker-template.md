# Tracker Template

## Purpose
`harness/plans/tracker.md`를 갱신할 때 사용하는 템플릿.

## Template

```md
# Tracker

## Current Phase
- phase:
- status: in_progress | completed | blocked

## Current Step
- owner: PM | Coder | Security Reviewer | Tester
- state:

## Current Work
- title:
- path: harness/plans/ongoing/<filename>.md

## Verification
- verdict: APPROVED | CHANGES_REQUESTED | BLOCKED | SKIPPED
- attempts: 0

## Next Owner
- owner:

## Next Action
- next:

## Issues
- none
```

## Rule
- 현재 상태만 남긴다.
- 상세 작업 내용은 `ongoing/` 문서에 둔다.
- 단계가 바뀌면 바로 갱신한다.
- 판정 값과 시도 횟수는 비워 두지 않는다.
