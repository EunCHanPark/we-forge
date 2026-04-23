---
description: Delegate a one-shot question to the Codex CLI and return its answer verbatim. Use when OpenAI/Codex is better suited than Claude for the request (e.g., an OpenAI-tuned code completion, cost-sensitive quick lookup). Claude Code remains the orchestrator; Codex runs as a one-shot sub-process.
---

# /ask-codex

Delegate the user's question to the Codex CLI and return its answer without
reinterpretation.

## Arguments

`$ARGUMENTS`

## Flow

1. **Preflight.** Run `command -v codex` via the Bash tool.
   - If `codex` is not on PATH, print exactly:
     ```
     Codex CLI not installed. Install it and retry.
     ```
     and stop. Do not fall back to Claude.

2. **Credential guard.** Inspect `$ARGUMENTS` for credential-shaped tokens:
   - `sk-[A-Za-z0-9_-]{16,}`
   - `ghp_[A-Za-z0-9]{20,}` / `ghs_` / `gho_`
   - `AIzaSy[A-Za-z0-9_-]{33}` (Google API keys)
   - `AKIA[0-9A-Z]{16}` (AWS access keys)
   - Any `api[_-]?key|secret|password|token|bearer\s*[:=]\s*\S+`

   If any match, print:
   ```
   Blocked: question appears to contain a credential. Rephrase without it.
   ```
   and stop.

3. **Invoke Codex.** Via the Bash tool, run:
   ```bash
   codex "$ARGUMENTS"
   ```
   (The Bash tool handles shell-quoting of `$ARGUMENTS` automatically.)

4. **Return verbatim.** Prefix the reply with a single line header and the
   full Codex output underneath:
   ```
   **Codex says:**
   <codex stdout verbatim>
   ```

5. **Do not summarize or rewrite** unless the user follows up asking you to.

## Rules

- **Single-shot only.** Never loop. Never call `/ask-codex` recursively.
- **Never expose `$ARGUMENTS` before the credential check.** The guard
  runs before any external process sees the text.
- **No interpretation layer.** Claude's job here is routing, not answering.
- **Respect budget.** If Codex errors or exceeds its rate limit, report
  the error verbatim and stop — do not retry with a different prompt.
