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

## Backlog

| EP | Title | Priority | Notes |
|----|-------|----------|-------|
| EP-HNS-002 | we-forgectl CI integration | LOW | Test harness for tick processing |
