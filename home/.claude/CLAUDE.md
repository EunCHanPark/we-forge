# Global Claude Code instructions for this user

These instructions auto-load in **every** Claude Code session, regardless of the
working directory. They are user-level (`~/.claude/CLAUDE.md`).

---

## we-forge — always available

This system has **we-forge** running as a 24/7 background pattern-learning
+ ECC marketplace orchestration daemon. It is **always loaded and accessible
from any directory**. Source: https://github.com/EunCHanPark/we-forge

### Available everywhere (no setup needed)

- **CLI**: `we-forgectl` is on `PATH` at `~/.local/bin/we-forgectl`
  - `we-forgectl status`             — running state, interval, next aligned tick
  - `we-forgectl set-interval <분>`  — change tick + telegram cadence (1-1440 min)
  - `we-forgectl logs`               — recent ticks
  - `we-forgectl ecc-trace --group`  — ECC marketplace skill usage histogram
  - `we-forgectl ecc-log <skill> <reason>` — record a manual ECC leverage
  - `we-forgectl tui`                — ratatui terminal UI
  - `we-forgectl dashboard`          — http://127.0.0.1:8765 KPI dashboard
- **Agent**: spawn via `Agent(subagent_type="we-forge")` for tick processing
- **Sub-agents**: monitor-sentinel, pattern-detector, skill-synthesizer, quality-auditor
- **Slash commands** (work in any cwd):
  - `/skill-report` — 6-section report (telemetry, top patterns, ECC matches, learned skills, decisions)
  - `/watch-and-learn` — manually trigger the synthesize-and-audit loop
  - `/dashboard [serve|tui|once]` — KPI dashboard
- **Telegram bot** (if user has `WE_FORGE_TELEGRAM_*` configured):
  `/status /skill_report /last_tick /ecc_trace /interval /set_interval <분> /help`
- **Data files** (always at these paths, regardless of cwd):
  - `~/.we-forge/config.json` — `{mode, interval_minutes, telegram_*, installed_at}`
  - `~/.we-forge/ecc-trace.jsonl` — every ECC marketplace skill leverage (ROI proof)
  - `~/.claude/learning/data/{events,patterns,promotion_queue,ledger}.jsonl`
  - `~/.claude/agent-memory/we-forge/MEMORY.md` — agent persistent memory across ticks

### When to actively involve we-forge in a session

The daemon collects events passively via Stop hook, but the live session model
(you) should **proactively** consult or invoke we-forge in these situations:

| Situation | Action |
|-----------|--------|
| User asks "any patterns I'm repeating?" / "what skills do I have?" | Run `/skill-report` |
| User asks about a workflow that might already be a skill | Check `we-forgectl ecc-trace --group` first |
| Before writing/proposing a new skill | Verify it doesn't duplicate by running `we-forgectl ecc-trace` |
| User asks "is we-forge running?" / "tick frequency?" | Run `we-forgectl status` |
| User reports unexpected we-forge behavior | Tail `we-forgectl logs` then read `~/.claude/learning/data/tick.log` |
| User wants ad-hoc pattern processing | `/watch-and-learn` (interactive trigger) |

### we-forge agent v2 — verdict vocabulary (when interpreting ledger.jsonl)

| Verdict | Meaning |
|---------|---------|
| `PASS` | Auditor approved, skill installed |
| `REVISE` | Auditor asked for rewrite (revise_count incremented) |
| `REJECT` | Auditor rejected; pattern poisoned |
| `ECC_MATCH` | Already covered by ECC marketplace skill (zero-spend) |
| `DROP` | Shell primitive / self-reference noise (zero-spend) |

**Zero-spend short-circuits** (DROP + ECC_MATCH) handle ~96% of queue entries.

---

## ECC alignment disclosure (mandatory standing protocol)

**Open every major work response with explicit ECC marketplace skill mapping.**
Format:

```
ECC 활용:
- <skill-name> → <reason for this work block>
- <skill-name> → <reason>
```

Then call `we-forgectl ecc-log <skill> "<reason>"` for each leveraged skill so
the ECC trace records ROI. This is the user's standing mandate — visibility
into ECC marketplace utilization is non-negotiable.

Reference: `~/.claude/projects/-Users-yukibana-we-forge/memory/ecc_alignment_protocol.md`

---

## Other persistent context

- **Project memory** (auto-loaded only when cwd is `~/we-forge`):
  `~/.claude/projects/-Users-yukibana-we-forge/memory/MEMORY.md`
- **we-forge agent's own memory** (loaded by we-forge agent on tick):
  `~/.claude/agent-memory/we-forge/MEMORY.md`
- **Session save/resume** (cross-session work continuity):
  `/save-session` at end-of-session, `/resume-session` at start-of-next
