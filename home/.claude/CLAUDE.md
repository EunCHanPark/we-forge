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
  - `we-forgectl status`             — running state, interval, next tick, active sessions
  - `we-forgectl sessions [--window N]` — list active Claude Code sessions (last N min)
  - `we-forgectl ping [label]`       — register current session (heartbeat fallback)
  - `we-forgectl set-interval <분>`  — change tick + telegram cadence (1-1440 min)
  - `we-forgectl logs`               — recent ticks
  - `we-forgectl ecc-trace --group`  — ECC marketplace skill usage histogram
  - `we-forgectl ecc-log <skill> <reason>` — record a manual ECC leverage
  - `we-forgectl tui`                — ratatui terminal UI
  - `we-forgectl dashboard`          — http://127.0.0.1:8765 KPI dashboard
- **Agent**: spawn via `Agent(subagent_type="we-forge")` for tick processing
- **Sub-agents**: monitor-sentinel, pattern-detector, skill-synthesizer, quality-auditor
- **Slash commands** (work in any cwd):
  - `/ping-forge` — register this session with we-forge (manual heartbeat ping)
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

## Unified work protocol (skill-suggest era + announce, 2026-04-30)

The advisor-strict 4-step protocol (advisor pre → ECC disclosure → work →
advisor post) was **retired 2026-04-27** in favor of automatic ECC-skill
suggestion via the `UserPromptSubmit` hook. The "silent compliance" default
(2026-04-27) was further superseded **2026-04-30** by **announce + use** so
users can observe whether the mechanism is firing.

```
[automatic, every user prompt]
  UserPromptSubmit hook fires:
    we-forgectl skill-suggest --inject --log <prompt>
  → if any ECC marketplace skill scores above threshold (BM25-lite IDF +
    slug-prefix 4x boost), top-3 are injected as <system-reminder>.

[Claude's response loop — announce + use]
  1. Read injected suggestions if any.
  2. If a suggestion fits the user's intent:
       a. One-line announce: "💡 skill-suggest: `<name>` 사용합니다."
       b. Invoke via Skill() BEFORE writing code (real Skill() invocation,
          not just `we-forgectl ecc-log`).
  3. If suggestions came in but none match:
       One-line announce: "skill-suggest: N개 후보 주입됐으나 무관
       (<short-names>) — 일반 진행"
  4. If no suggestions fired (empty injection): silent skip OK.

[advisor — optional, no longer mandatory]
  Call Agent(subagent_type="Plan") only when:
    - the work is multi-file architectural (not single-file edit), OR
    - it's a hard-to-reverse change (DB migration, API contract, deletion).

[telemetry]
  - Suggestions logged to ~/.we-forge/skill-suggestions.jsonl
  - Skill invocations recorded by `we-forgectl ecc-log` (existing)
  - Hit rate visible in `we-forgectl status` (suggested vs invoked, last 24h)
  - Detail: `we-forgectl skill-hits --hours N`
```

**Why announce**: silent compliance hid whether the hook was working,
making users assume it failed. Two short lines per suggestion event
restore observability without returning to the heavy "ECC 활용:" tables
of the advisor-strict era.

Reference (per-PC, auto-generated by Claude Code):
`~/.claude/projects/<dash-encoded-cwd>/memory/ecc_alignment_protocol.md`
where `<dash-encoded-cwd>` is the current working directory with `/` → `-`
(Claude Code creates this directory automatically on first session in the project).

---

## Other persistent context

- **Project memory** (auto-loaded only when cwd is `~/we-forge`):
  `~/.claude/projects/<dash-encoded-cwd>/memory/MEMORY.md`
- **we-forge agent's own memory** (loaded by we-forge agent on tick):
  `~/.claude/agent-memory/we-forge/MEMORY.md`
- **Session save/resume** (cross-session work continuity):
  `/save-session` at end-of-session, `/resume-session` at start-of-next
