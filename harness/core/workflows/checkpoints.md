# Checkpoints — 작업 상태 기록 기준

## 1. Purpose
작업이 중단된 후에도 다음 사람(또는 에이전트)이 바로 이어서 작업할 수 있게 하는 것이 목표다.

## 2. Where to Record
- 앱 전체 진행 상태: `harness/plans/tracker.md`
- 현재 진행 중인 작업: `harness/plans/ongoing/<plan-name>.md`
- 완료된 작업: `harness/plans/completed/`로 이동

## 3. What to Record
- 현재 작업 목적
- 현재 단계와 담당자
- 마지막으로 확인된 상태
- 판정 값 (verdict)
- 시도 횟수 (attempts)
- 다음 담당자
- 다음에 해야 할 한 가지 일
- 보류/실패 이유

## 4. Git Checkpoint (risky 작업 전)
큰 구현 변경, 파괴적 마이그레이션, 리팩토링 전에는 git checkpoint를 먼저 만든다.

```bash
python3 harness/scripts/git_checkpoint.py <checkpoint-name>
# 예: python3 harness/scripts/git_checkpoint.py before-db-migration-004
```

저장 위치: `refs/harness-checkpoints/<name>`
복원: `git checkout refs/harness-checkpoints/<name> -- .`

## 5. Rule
- 체크포인트는 짧게 쓴다. 현재 상태와 다음 행동이 바로 보여야 한다.
- 상태가 바뀌면 최신 내용으로 갱신한다.
- 판정 값과 시도 횟수는 고정 필드로 남긴다.
- 큰 작업은 더 작은 단위로 나눈다.
