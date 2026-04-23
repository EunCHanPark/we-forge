# ehforge-ultraplan

Isolated repository for running `/ultraplan` on the
"24/7 monitoring + repetitive pattern auto-learning system" spec
for the ECC + custom harness environment at `~/ehforge`.

This repo intentionally contains no source code — it exists only to
satisfy the `/ultraplan` command's GitHub remote requirement while
keeping the multi-project `ehforge` workspace (which includes personal
and sensitive subdirectories) out of any cloud bundle.

## Target spec

- `~/.claude/agents/` — 4 sub-agents:
  monitor-sentinel, pattern-detector, skill-synthesizer, quality-auditor
- `~/.claude/commands/` — 2 slash commands: `/watch-and-learn`, `/skill-report`
- Cron-driven hourly execution
- Stop hook for session-end event collection
- Source telemetry: bash history, claude logs, stop hook output
- Promotion rule: patterns repeated 3+ times become SKILL.md
- quality-auditor gate: PASS / REVISE / REJECT
- Approved skills land in `~/.claude/skills/learned/`
- Requires `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1`
- Must follow existing ECC SKILL.md and agents/ folder conventions
- Secret-bearing patterns (API keys, passwords) auto-excluded
