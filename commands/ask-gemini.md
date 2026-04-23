---
description: Delegate a one-shot question to the Gemini CLI and return its answer verbatim. Use when Gemini is better suited than Claude for the request (long-context analysis >200K tokens, Google API-specific questions, cost-sensitive tasks). Claude Code remains the orchestrator; Gemini runs as a one-shot sub-process.
---

# /ask-gemini

Delegate the user's question to the Gemini CLI and return its answer without
reinterpretation.

## Arguments

`$ARGUMENTS`

## Flow

1. **Preflight.** Run `command -v gemini` via the Bash tool.
   - If `gemini` is not on PATH, print exactly:
     ```
     Gemini CLI not installed. Install it and retry.
     ```
     and stop. Do not fall back to Claude.

2. **Credential guard.** Inspect `$ARGUMENTS` for credential-shaped tokens:
   - `sk-[A-Za-z0-9_-]{16,}`
   - `ghp_[A-Za-z0-9]{20,}` / `ghs_` / `gho_`
   - `AIzaSy[A-Za-z0-9_-]{33}` (Google API keys — particularly relevant here)
   - `AKIA[0-9A-Z]{16}` (AWS access keys)
   - Any `api[_-]?key|secret|password|token|bearer\s*[:=]\s*\S+`

   If any match, print:
   ```
   Blocked: question appears to contain a credential. Rephrase without it.
   ```
   and stop.

3. **Invoke Gemini.** Via the Bash tool, run:
   ```bash
   gemini "$ARGUMENTS"
   ```
   (The Bash tool handles shell-quoting of `$ARGUMENTS` automatically.)

4. **Return verbatim.** Prefix the reply with a single line header and the
   full Gemini output underneath:
   ```
   **Gemini says:**
   <gemini stdout verbatim>
   ```

5. **Do not summarize or rewrite** unless the user follows up asking you to.

## Rules

- **Single-shot only.** Never loop. Never call `/ask-gemini` recursively.
- **Never expose `$ARGUMENTS` before the credential check.** The guard
  runs before any external process sees the text.
- **No interpretation layer.** Claude's job here is routing, not answering.
- **Respect budget.** If Gemini errors or hits its rate limit, report the
  error verbatim and stop — do not retry with a different prompt.

## Why this exists

Claude Code is your main session. When a request genuinely fits Gemini
better (e.g., "read this 500K-token log and summarize errors", where
Gemini's extended context wins), use this command. Otherwise stay with
Claude — round-trip through another LLM adds latency and duplicates
cost unnecessarily.
