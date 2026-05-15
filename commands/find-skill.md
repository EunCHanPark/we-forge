---
description: Find the best-fit ECC marketplace skill for a natural-language request via the skill-finder agent (LLM semantic matcher). Use this when `we-forgectl skill-suggest` returned weak / no-match results, when the prompt is heavily Korean / morphologically rich, or when you want an LLM second opinion on which skill applies.
---

You are executing **/find-skill**, the LLM fallback for ECC skill
resolution when the BM25 matcher has hit its limits.

## Argument

One required positional argument: the prompt to find a skill for.
Quote-wrap if it has spaces.

Examples:

- `/find-skill "접속시 검정화면으로 아무것도 보이지 않음"`
- `/find-skill "deploy verification after a Vite migration"`
- `/find-skill "데이터가 실데이터인지 확인하고 싶음"`

## Flow

1. Gather the BM25 baseline (top-20) for context:

   ```bash
   we-forgectl skill-suggest --top 20 "<prompt>"
   ```

   This is cheap and gives the agent a strong pre-filtered pool to
   reason over. Capture its full stdout.

2. Build the JSON input expected by the `skill-finder` agent:

   ```json
   {
     "prompt": "<the user's prompt>",
     "candidates": [
       {"slug": "...", "score": ..., "description": "..."},
       ...
     ],
     "context": "user invoked /find-skill — return your verdict in
                 the contract format defined in agents/skill-finder.md"
   }
   ```

   If the BM25 baseline returned 0 candidates (totally cold prompt),
   pass `"candidates": []` and tell the agent it should read the
   ecc-index.json directly to pick from the full pool.

3. Dispatch the agent:

   ```
   Agent({
     subagent_type: "skill-finder",
     description:   "resolve user request to ECC skill",
     prompt:        "<the JSON input from step 2>"
   })
   ```

4. Present the agent's verdict to the user verbatim (it's already in
   the plain-text contract format). Then, if `decision: match`, offer
   to invoke the top pick:

   > "위 1번 스킬을 바로 호출할까요? (`Skill(<slug>)`)"

   If the user agrees, fire the Skill tool with the picked slug. Do
   not invoke it without confirmation — this is a recommendation
   workflow, not auto-execute.

5. Optionally log the verdict to `~/.we-forge/skill-finder.jsonl` for
   later analysis (one row per /find-skill invocation):

   ```bash
   echo '{"ts": "<iso>", "prompt": "...", "verdict": "..."}' \
     >> ~/.we-forge/skill-finder.jsonl
   ```

## Rules

- **Don't bypass BM25.** Always run step 1 first — even if the user
  said skill-suggest failed. The candidate pool is what makes the
  agent fast (otherwise it has to scan all 254 skills).
- **One agent call.** Don't loop. If the agent says `no_match`, accept
  it — that's a useful signal too.
- **No auto-invocation.** Always confirm before firing `Skill()` on
  the top pick.
- **Cheap.** This whole flow is ~$0.001 per call (Haiku via the agent
  contract); fine for occasional use but don't put it in a hot loop.

## When NOT to use

- The hook's BM25 already injected good candidates (score ≥ 10) →
  just use those.
- The prompt is < 15 chars (matches trivial-skip threshold) — almost
  certainly not worth a skill lookup.
- The user explicitly already chose a skill — don't second-guess.

## Examples

User: `/find-skill "접속시 검정화면으로 아무것도 보이지 않음"`
You: run BM25 (returns 0 / weak), dispatch agent, present verdict —
agent should return `browser-qa` or `canary-watch` with rationale
about blank-screen-after-deploy.

User: `/find-skill "Stripe 구독 환불"`
You: BM25 already strong (`customer-billing-ops` ~26) — but if user
invoked /find-skill explicitly, still run the flow; agent will likely
confirm.
