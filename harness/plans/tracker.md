# we-forge — Plan Tracker

## Status Legend
- `[TODO]` Not started
- `[WIP]` In progress
- `[DONE]` Completed
- `[DEFER]` Deferred / blocked

## Active EPs

| EP | Title | Status | Notes |
|----|-------|--------|-------|
| EP-HNS-001 | Harness initial scaffold | [DONE] | Flat layout applied 2026-04-25 |
| EP-SES-001 | Session detection (auto + manual) | [DONE] | sessions/ping commands, heartbeat tracking (2026-04-25) |
| EP-INS-001 | install.sh PATH auto-registration | [DONE] | v0.4.6 — auto-register ~/.local/bin on user shell rc |
| EP-INS-002 | install.sh bootstrap completeness | [DONE] | 2026-04-26 — sessionstart hook + ping-forge/dashboard commands deployed |
| EP-WIN-001 | Windows install.ps1 critical fix | [DONE] | v0.4.5 — critical install fix + doctor PATH + 409 backoff |
| EP-TG-001 | Telegram 409 Conflict backoff | [DONE] | 60s backoff on poll conflict (commit 87c7bb9) |
| EP-V42-001 | v0.4.2 cross-PC propagation | [DONE] | we-forge auto-discovery for multi-PC setups |
| EP-V41-001 | v0.4.1 ratatui upgrade | [DONE] | ratatui 0.30 (drops vulnerable lru < 0.16.3) |
| EP-PNG-001 | Manual ping + /ping-forge | [DONE] | 2026-04-25 — heartbeat-based session registration |
| EP-PNG-002 | Attach announcement banner | [DONE] | 2026-04-26 — prominent banner on auto + manual attach |
| EP-ECC-001 | ECC alignment disclosure protocol | [DONE] | 2026-04-26 — SessionStart reminder + simplified format |
| EP-DOC-001 | Documentation update for sessions/ping | [DONE] | 2026-04-26 — README, CHANGELOG, DOCS-KO, CLAUDE.md |
| EP-RVC-001 | promotion_queue revise_count auto-reject | [DONE] | 2026-04-26 — cap at 3, prevents infinite REVISE loops |
| EP-DDP-001 | Pattern-detector dedupe accuracy | [DONE] | 2026-04-26 — improved heuristics + match scoring |
| EP-DDP-002 | ECC marketplace keyword index | [DONE] | 2026-04-26 — build_ecc_index.py, install + 24h refresh in tick.sh |
| EP-MET-001 | ecc-trace ROI metric enrichment | [DONE] | 2026-04-26 — match_method/match_score/decision_latency_ms (optional) |
| EP-ENV-001 | tick.sh env export to we-forge agent | [DONE] | 2026-04-26 — CLAUDE_LEARNING_DATA/CLAUDE_LEARNED_SKILLS/WE_FORGE_HOME |
| EP-PROT-001 | Unified work protocol (advisor → ECC) | [DONE] | 2026-04-26 — advisor first/last + ECC alignment in middle (CLAUDE.md+memory+SessionStart hook) |
| EP-AUD-001 | we-forgectl audit tool | [DONE] | 2026-04-26 — cross-references patterns.jsonl + ledger.jsonl + rejected.txt |
| EP-GAP-B  | no-ledger gap investigation (B task) | [DONE] | 2026-04-26 — confirmed pipeline integrity; 6 entries were REJECTED (in rejected.txt), not gaps |
| EP-MAT-001 | ECC_MATCH ledger traceability (C task) | [DONE] | 2026-04-26 — agent spec mandates ecc_skill/ecc_source/match_score on every ECC_MATCH |
| EP-SEQ-001 | Multi-step sequence pattern detection (A task) | [DONE] | 2026-04-26 — sequence_normalize.py with N=2..4, MIN_SUPPORT=3, self-loop collapse, shadow mode SEQ_CANDIDATE verdict |
| EP-BF-001 | Historical ECC_MATCH backfill | [DONE] | 2026-04-26 — backfill_ecc_match.py: 271/271 records gain ecc_source + match_score |
| EP-MON-001 | match_score quality monitor | [DONE] | 2026-04-26 — `we-forgectl ecc-quality` flags entries below threshold |
| EP-DSH-001 | Dashboard sequence metrics | [DONE] | 2026-04-26 — top_sequences + totals.sequences in compute_kpis(); --once renders sequences section |
| EP-PORT-001 | Cross-PC portability fixes | [DONE] | 2026-04-26 — install.sh deploys dashboard.py + hardcoded path removed from we-forgectl + CLAUDE.md template path generalized |
| EP-SUG-001 | skill-suggest era (UserPromptSubmit hook auto-injection) | [DONE] | 2026-04-27 — replaces advisor-strict mandate; IDF-weighted ECC skill suggestion via hook |
| EP-SUG-002 | skill-suggest announce + use protocol | [DONE] | 2026-04-30 — silent compliance reverted; one-line announcement of suggestion outcome restores user observability |
| EP-PARITY-001 | Rust CLI ↔ docs parity for missing subcommands | [DONE] | 2026-05-11 — sessions/ping/audit/ecc-quality/skill-suggest/skill-hits ported to Rust (v0.4.7); install.ps1 fetches dashboard.py + dashboard/ping-forge slash commands; install.sh + DOCS-KO scheduler text reconciled with auto-register behavior |
| EP-PARITY-002 | Service-manager re-install on Rust-binary upgrade | [WIP] | 2026-05-12 — root cause: after `we-forgectl` swapped from Python script → Mach-O binary, existing machines' launchd plist still had `ProgramArguments=[/usr/bin/env, python3, we-forgectl, daemon]` → `python3 <binary>` SyntaxError → daemon stayed dead with launchd throttle-retrying. Runtime hotfix applied on yukibana's Mac (bootout + `we-forgectl install --enable-telegram` regenerates plist to `[<binary>, daemon]`). PENDING: install.sh / install.ps1 should detect a stale interpreter-prefixed service def and always regenerate on upgrade; verify systemd `ExecStart=` + Windows Task Scheduler action. Details: `harness/plans/ongoing/EP-PARITY-002.md`. Follow-up to EP-PARITY-001. |
| EP-LOOP-P0 | Learning-loop security hardening (P0-1, P0-2) | [DONE] | 2026-05-12 — quality-auditor: `redact.sh --self-test` preflight before any rubric (broken filter ⇒ hold draft, never auto-PASS); semantic-intent rubric 7 (REJECTs network-execution/persistence/shell-bootstrap/credential-access/lateral-movement/obfuscated-execution intents the regex rubric misses). |
| EP-LOOP-P1-3 | pattern-detector pre-built skill index | [DONE] | 2026-05-12 — `learning/build-skill-index.sh` → `skill-index.jsonl` (4 dedupe sources, _meta header); tick.sh rebuilds >24h; pattern-detector reads one file (falls back to 4-glob if stale). install.sh deploys + seeds it. |
| EP-LOOP-P1-1 | Orchestrator decomposition (notifier + memory-manager) | [DONE] | 2026-05-12 — split Telegram → `agents/notifier.md` (Bash only) and persistent memory → `agents/memory-manager.md` (Read/Write only) out of `we-forge.md`; orchestrator now owns only control flow / verdicts / queue / ledger. queue-manager + ecc-router deferred. |
| EP-LOOP-P1-2 | 3-tier agent memory + migration | [DONE] | 2026-05-12 — `agent-memory/we-forge/MEMORY.md` → `hot.md` (raw, ~7d, ≤10KB) / `lessons.md` (compressed, ≤5KB) / `pointers.md` (JSON lookups); `learning/migrate-memory.sh` one-time split (backup + `.legacy` rename, idempotent); install.sh runs it on upgrade. |
| EP-LOOP-ECC1 | ecc_recs authoritative for ECC_MATCH | [DONE] | 2026-05-12 — orchestrator step 6: a slug already in `ecc_recs` (loaded from `pointers.md`) stays an ECC_MATCH regardless of pattern-detector score — fixes the scored matcher under-counting short single-token slugs (`tmux`/`codex` → only +2 vs `dmux-workflows`, below threshold 3). memory-manager `load` now returns `ecc_recs`. Followup: special-case single-token-slug ↔ ECC name/desc at the detector; memory-manager should prune stale `ecc_recs`. |
| EP-LOOP-TO | CLAUDE_TICK_TIMEOUT 600→900s | [DONE] | 2026-05-12 — the P1-1 split added 3 extra sub-agent round-trips/tick (memory-manager load+record, notifier) which pushed slow ticks near the old 600s cap. |

## Backlog

| EP | Title | Priority | Notes |
|----|-------|----------|-------|
| EP-HNS-002 | we-forgectl CI integration | LOW | Test harness for tick processing |
| EP-LOOP-P1-1b | Orchestrator decomposition phase 2 (queue-manager / ecc-router / lifecycle-manager) | LOW | After notifier+memory-manager stabilize (~1 week). |
| EP-LOOP-P2 | Learning-loop quality pass (P2-1 command-AST canonicalization, P2-2 skill utility scoring & archive, P2-3 synthesizer trivial-draft early-reject) | MED | Per the P0/P1 work order: revisit after ~1 week of operation data. |
| EP-LOOP-P3 | Learning-loop polish (P3-1 auditor dedup false-positive, P3-2 monitor-sentinel graceful degradation, P3-3 ECC-alignment output condensation, P3-4 tick_id idempotency key) | LOW | Time-permitting. |
