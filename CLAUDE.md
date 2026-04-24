# CLAUDE.md — we-forge

## Read Order (세션 시작 시 반드시)

1. `harness/core/docs/index.md` — 공통 규칙 진입점
2. `harness/core/workflows/pipeline.md` — 작업 흐름 (PM→Coder→Security→Tester→PM)
3. `harness/core/platforms/claude-code.md` — Claude Code 특화 규칙
4. `harness/docs/agents.md` — 에이전트 로스터 + R13 Agent Teams
5. `harness/plans/tracker.md` — 현재 EP 상태

## Project Identity

**we-forge**: 24/7 background pattern-learning + ECC-marketplace orchestration layer for Claude Code.

Single-daemon flat layout — no `apps/` subdivision.

## Dependency Chain

```
hooks → events.jsonl → pattern-detector → ECC dedup → synthesize → ledger.jsonl
```

## Working Rules

- 공통 규칙: `harness/core/` 따름
- 프로젝트 규칙: `harness/docs/` 따름
- 상태 변경: `harness/plans/tracker.md` + `harness/plans/ongoing/` 갱신
- Risky 작업 전: `python3 harness/scripts/git_checkpoint.py <name>`
- 하네스 파일은 사용자 요청 없이 수정하지 않는다

## Advisor 사용 규칙

- 모든 실질적 작업 전에 반드시 advisor()를 호출한다.
- 파일 탐색/조사(orientation)는 advisor 없이 가능하지만, 코드 작성/수정/커밋/설계 결정 전에는 advisor를 먼저 호출한다.
- 작업 완료 선언 전에도 advisor를 호출하여 검증한다.
