---
description: Show a status report for the 24/7 pattern-learning system — telemetry counts, patterns in flight, learned skills, and recent auditor decisions. Dispatches monitor-sentinel for the heavy lifting.
---

You are executing **/skill-report**, the read-only viewer for the
pattern-learning system.

## Flow

1. Dispatch `monitor-sentinel`:

   ```
   Agent({
     subagent_type: "monitor-sentinel",
     description: "summarize learning telemetry",
     prompt: "Produce the Telemetry, Top un-promoted patterns, Queue,
Learned skills, and Logs sections per your agent contract."
   })
   ```

2. Directly list the learned skills and recent decisions (cheap; no
   additional agent call needed):

   ```bash
   ls -1 ~/.claude/skills/learned/
   tail -n 20 ~/.claude/learning/data/ledger.jsonl
   ```

3. Assemble the final report with four sections **in this order**:

   ### Telemetry
   monitor-sentinel's Telemetry + Queue sections, verbatim.

   ### Patterns in flight
   monitor-sentinel's "Top un-promoted patterns" section, verbatim.

   ### Learned skills
   For each directory under `~/.claude/skills/learned/` (excluding
   `pending/`), print:
   ```
   <slug>  <description from frontmatter, first 80 chars>
   ```

   ### Recent decisions
   Last 10 rows from `ledger.jsonl`, newest first:
   ```
   <ts>  <decision>  <slug>  <rationale, truncated to 80 chars>
   ```

## Rules

- **Read-only.** No Write or Edit calls.
- **Do not dispatch skill-synthesizer or quality-auditor** — this command
  is a viewer, not an orchestrator. `/watch-and-learn` is the orchestrator.
- **No secrets.** If monitor-sentinel returns anything that looks like a
  raw event with credential-shaped content, omit it.
