---
description: Run the pattern-learning orchestration loop. Dispatches pattern-detector, then skill-synthesizer+quality-auditor per candidate via agent teams when available, falling back to sequential Agent calls otherwise. Clears processed entries from the promotion queue.
---

You are executing **/watch-and-learn**, the orchestrator for the 24/7
pattern-learning system.

<!-- Future-expansion trigger: split ingestion into source-specific miner agents (bash/transcript/stophook) in their own team when events.jsonl > 10 GB OR ledger shows a per-source accuracy regression (e.g., transcript-source PASS rate drops >20% vs rolling 30d baseline). -->

## Preflight

1. Read `~/.claude/learning/data/promotion_queue.jsonl`. If empty, print
   `watch-and-learn: queue empty` and stop. **Spend no tokens on agent
   dispatches when there is nothing to process.**
2. Check whether agent-team APIs are available. The flag
   `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1` is set by `tick.sh` (and is
   already enabled in the user's `~/.claude/settings.json`). If `TeamCreate`
   is in your tool list, take the **team path**. Otherwise take the
   **sequential path**. The system must be correct either way.

## Stage 1 — reduce the queue (always sequential)

Dispatch `pattern-detector` once and wait for its JSON output. This stage
is strictly sequential because every downstream stage consumes its result.

```
Agent({
  subagent_type: "pattern-detector",
  description: "reduce promotion queue",
  prompt: "Process ~/.claude/learning/data/promotion_queue.jsonl against
~/.claude/skills/learned/. Emit the trimmed candidate JSON array on stdout."
})
```

Parse the returned JSON array. If it's empty, print
`watch-and-learn: no new candidates` and stop.

### Budget cap

Read `CLAUDE_TICK_MAX_CANDIDATES` from the environment (default: `5`). If
the reduced candidate list is longer, take the first `N` entries (highest
`total_count` first — pattern-detector already ranks them) and leave the
rest in `promotion_queue.jsonl` for the next tick. Print
`watch-and-learn: capped candidates=<N> deferred=<M>` when capping occurs.

Rationale: bounds the worst-case API spend per tick to `N` synthesizer +
`N` auditor dispatches. A runaway noise pattern cannot cascade into a
triple-digit dispatch on a single cron firing.

## Stage 2 — synthesize + audit per candidate (parallel when possible)

### Team path (preferred: agent-teams flag enabled)

Fan candidates out in parallel. For each candidate `C` (slug `<slug>`):

1. `TeamCreate({team_name: "learning-batch-<slug>"})` — one team per
   candidate isolates draft writes.
2. Dispatch **synthesizer and auditor into the same team** so they share
   context (the auditor waits for a synthesizer SendMessage signal when
   the draft is ready on disk):

   ```
   Agent({
     subagent_type: "skill-synthesizer",
     team_name: "learning-batch-<slug>",
     description: "draft SKILL.md for <slug>",
     prompt: <candidate JSON>
   })
   Agent({
     subagent_type: "quality-auditor",
     team_name: "learning-batch-<slug>",
     description: "audit <slug>",
     prompt: "Audit ~/.claude/skills/learned/pending/<slug>/ once
synthesizer signals the draft is ready."
   })
   ```

3. Collect the auditor's one-line verdict:
   `<slug>  PASS|REVISE|REJECT  <rationale>`.
4. `TeamDelete({team_name: "learning-batch-<slug>"})`.

Fire teams for all candidates in a **single message** containing multiple
tool-use blocks so the harness runs them concurrently. Different teams
cannot interfere because each pending `<slug>` directory is distinct.

### Sequential fallback (team APIs unavailable)

For each candidate, sequentially:

1. Dispatch `skill-synthesizer` with the candidate JSON; await completion.
2. Dispatch `quality-auditor` with the slug; await completion.
3. Record the one-line verdict.

Slower but produces identical end state.

## Stage 3 — update the queue

For each candidate's verdict, apply to
`~/.claude/learning/data/promotion_queue.jsonl`:

- **PASS** — remove the entry. (Auditor already moved pending → learned.)
- **REJECT** — remove the entry. (Auditor already appended to
  `rejected.txt` and removed the pending dir.)
- **REVISE** — rewrite the entry with `revise_count += 1`. All other
  fields unchanged.

Write the queue atomically (write `promotion_queue.jsonl.tmp`, then `mv`
it over). Never edit in place while agents may still be reading.

## Stage 4 — summary

Print one line per candidate:

```
<slug>  PASS    pattern="<pattern>"  sessions=<n>
<slug>  REVISE  pattern="<pattern>"  revise_count=<n>
<slug>  REJECT  pattern="<pattern>"  reason="<rationale>"
```

Then a total:

```
watch-and-learn: processed=<N> pass=<p> revise=<r> reject=<j>
```

## Rules

- **Zero-spend when idle.** Empty queue → print one line and stop.
- **Respect agent boundaries.** Do not read drafts yourself; the auditor
  is the sole judge. Do not synthesize inline; go through skill-synthesizer
  so its scoped Write permissions are enforced.
- **Idempotence.** If re-invoked mid-batch (cron double-fire that escaped
  the lock), already-promoted candidates must be no-ops.
- **No secrets in logs.** Any rationale you print refers only to
  canonicalized `pattern` strings — never to `raw` event content.
