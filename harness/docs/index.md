# we-forge — Harness Docs Index

## Project Identity

24/7 background pattern-learning + ECC-marketplace orchestration layer for Claude Code.

Single-daemon project. Flat layout: `harness/` at repo root (no `apps/` subdivision).

## Dependency Chain

```
hooks → events.jsonl → pattern-detector → ECC dedup → synthesize → ledger.jsonl
```

## Key Components

| Path | Role |
|------|------|
| `agents/` | Sub-agent definitions (monitor-sentinel, pattern-detector, skill-synthesizer, quality-auditor) |
| `hooks/` | Claude Code hook scripts (Stop hook feeds events.jsonl) |
| `scripts/` | CLI tools (we-forgectl, install scripts) |
| `rust/` | Native components |
| `learning/` | Runtime data (events, patterns, ledger) |
| `dashboard/` | KPI dashboard server |

## Read Order

1. `harness/core/docs/index.md` — common rules
2. `harness/core/workflows/pipeline.md` — PM→Coder→Security→Tester→PM flow
3. `harness/core/platforms/claude-code.md` — Claude Code specifics
4. `harness/docs/agents.md` — agent roster + R13 Agent Teams rules
5. `harness/plans/tracker.md` — current EP status
