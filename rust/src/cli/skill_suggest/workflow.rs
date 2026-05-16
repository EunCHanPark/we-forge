//! Prompt-pattern → multi-agent workflow recommendations (opt-in via
//! `config.workflow_suggest_enabled`). Conservative on purpose — precision
//! over recall so the injection stays useful rather than noisy.

// -----------------------------------------------------------------------
// workflow_match — opt-in (cfg.workflow_suggest_enabled). Pattern-match
// the prompt to ECC multi-agent workflow skills (`/santa-method`,
// `/council`, `/multi-workflow`, `/gan-style-harness`, …). Returns a small
// ranked list of (slug, why). Conservative on purpose: precision >
// recall, so the injection stays useful rather than noisy.
// -----------------------------------------------------------------------
struct WfRule {
    slug: &'static str,   // namespaced ECC skill slug
    why:  &'static str,   // one-line rationale shown next to the recommendation
    // Each pattern is a list of substrings; ALL must appear (case-insensitive)
    // somewhere in the prompt for the rule to fire. ANY of the patterns can
    // trigger.
    any_of_all: &'static [&'static [&'static str]],
}

const WORKFLOW_RULES: &[WfRule] = &[
    // --- Convergence / consensus ----------------------------------------
    WfRule {
        slug: "everything-claude-code:santa-method",
        why:  "production-bound code / dual-reviewer convergence",
        any_of_all: &[
            &["production"], &["deploy"],
            &["push to main"], &["push to master"],
            &["release candidate"], &["before shipping"],
            &["before merging"], &["before merge"],
            &["compliance"], &["regulatory"],
            &["customer-facing"], &["pre-launch"],
            &["go live"], &["ready to ship"],
        ],
    },
    WfRule {
        slug: "everything-claude-code:council",
        why:  "ambiguous tradeoff / multiple valid paths — convene 4-voice council",
        any_of_all: &[
            &["should i"], &["should we"],
            &["trade-off"], &["tradeoff"],
            &[" vs "], &[" or "],
            &["which", "better"], &["which", "choose"],
            &["which", "should"],
            &["pros and cons"], &["decide between"], &["decide on"],
            &["go/no-go"], &["go-no-go"],
            &["pick between"], &["choose between"],
            &["second opinion"], &["dissent"],
        ],
    },

    // --- Multi-phase delivery ------------------------------------------
    WfRule {
        slug: "everything-claude-code:multi-workflow",
        why:  "multi-phase feature build (research → plan → execute → review)",
        any_of_all: &[
            &["new feature"], &["implement", "across"],
            &["build out", "feature"], &["multi-file"],
            &["refactor", "across"], &["end-to-end implementation"],
            &["full implementation"], &["complete implementation"],
            &["from", "to deployment"],
        ],
    },
    WfRule {
        slug: "everything-claude-code:gan-style-harness",
        why:  "long-running autonomous app build (generator/evaluator loop)",
        any_of_all: &[
            &["build", "app", "from"], &["from scratch"],
            &["prd"], &["from a one-liner"],
            &["autonomous", "build"], &["one-liner", "to"],
            &["scaffold", "entire"],
        ],
    },
    WfRule {
        slug: "everything-claude-code:multi-frontend",
        why:  "frontend-focused multi-model workflow (UI/UX/animation)",
        any_of_all: &[
            &["frontend", "feature"], &["ui", "polish"],
            &["component library"], &["design system"],
            &["ux", "iterate"], &["pixel-perfect"],
        ],
    },
    WfRule {
        slug: "everything-claude-code:multi-backend",
        why:  "backend-focused multi-model workflow (APIs/algorithms/data)",
        any_of_all: &[
            &["backend", "feature"], &["api", "design"],
            &["database", "schema"], &["service", "architecture"],
            &["microservice"], &["data pipeline"],
        ],
    },
    WfRule {
        slug: "everything-claude-code:team-builder",
        why:  "ad-hoc parallel team across mixed domains (interactive picker)",
        any_of_all: &[
            &["pick agents"], &["compose team"], &["choose agents"],
            &["parallel team"], &["dispatch", "agents"],
            &["agent team"], &["which agents"],
        ],
    },
    WfRule {
        slug: "everything-claude-code:dmux-workflows",
        why:  "multi-agent orchestration in tmux/dmux/cmux panes (multiple OS processes)",
        any_of_all: &[
            // Distinctive single tokens — safe alone
            &["dmux"], &["cmux"],
            // tmux is too generic — require a coordination cue alongside it
            &["tmux", "claude"], &["tmux", "agent"], &["tmux", "pane"],
            &["tmux", "session", "parallel"],
            // Parallelism + agent/claude/instance signals
            &["parallel", "agent"], &["parallel", "claude"], &["parallel", "instance"],
            &["multiple claude"], &["multiple", "instances"],
            &["run", "agents", "parallel"], &["run", "claude", "parallel"],
            &["agents", "in parallel"], &["claudes", "in parallel"],
            // Work splitting / coordination patterns
            &["split work"], &["divide and conquer"],
            &["pane", "agent"], &["pane", "claude"],
            &["fan out", "agent"], &["fan-out", "agent"],
            &["claude-teams"],
        ],
    },

    // --- Review / audit -------------------------------------------------
    WfRule {
        slug: "everything-claude-code:review-pr",
        why:  "PR review via specialized review agents",
        any_of_all: &[
            &["review pr"], &["review", "pull request"],
            &["pr review"], &["pr #"], &["review my pr"],
        ],
    },
    WfRule {
        slug: "everything-claude-code:code-review",
        why:  "comprehensive code review (uncommitted changes or PR)",
        any_of_all: &[
            &["code review"], &["review", "changes"],
            &["review my code"], &["review this code"],
            &["lgtm"],
        ],
    },
    WfRule {
        slug: "everything-claude-code:security-review",
        why:  "security-focused review pass",
        any_of_all: &[
            &["security review"], &["audit", "security"],
            &["vulnerab"], &["threat model"],
            &["secure code"], &["security audit"],
        ],
    },
    WfRule {
        slug: "everything-claude-code:harness-audit",
        why:  "deterministic harness audit + prioritized scorecard",
        any_of_all: &[
            &["audit my setup"], &["audit", "harness"],
            &["harness health"], &["harness audit"],
            &["audit", "config"],
        ],
    },

    // --- Planning / PRD -------------------------------------------------
    WfRule {
        slug: "everything-claude-code:prp-plan",
        why:  "feature implementation plan with codebase analysis",
        any_of_all: &[
            &["implementation plan"], &["plan", "implementation"],
            &["plan", "feature"], &["feature plan"],
            &["plan", "implement"], &["plan", "refactor"],
            &["break down", "task"], &["roadmap for"],
        ],
    },
    WfRule {
        slug: "everything-claude-code:prp-prd",
        why:  "interactive PRD generator (problem-first, hypothesis-driven)",
        any_of_all: &[
            &["prd"], &["product spec"], &["product requirements"],
            &["product brief"], &["write a spec"],
            &["draft a spec"],
        ],
    },
    WfRule {
        slug: "everything-claude-code:plan",
        why:  "step-by-step implementation plan (wait for CONFIRM)",
        any_of_all: &[
            &["plan", "before"], &["plan first"],
            &["plan this", "out"],
            &["explain", "approach"],
        ],
    },

    // --- Testing / verification ----------------------------------------
    WfRule {
        slug: "everything-claude-code:tdd-workflow",
        why:  "test-first development (write tests, then implement)",
        any_of_all: &[
            &["tdd"], &["test-driven"], &["test driven"],
            &["tests first"], &["test first"],
            &["write tests before"],
        ],
    },
    WfRule {
        slug: "everything-claude-code:e2e-testing",
        why:  "end-to-end test setup and runner",
        any_of_all: &[
            &["e2e test"], &["end-to-end test"], &["end to end test"],
            &["playwright"], &["cypress"],
        ],
    },
    WfRule {
        slug: "everything-claude-code:verification-loop",
        why:  "structured verification + remediation loop",
        any_of_all: &[
            &["verification loop"], &["verify", "rigorous"],
            &["validation loop"], &["verify the implementation"],
            &["pass all checks"],
        ],
    },
    WfRule {
        slug: "everything-claude-code:test-coverage",
        why:  "coverage analysis + missing-test generation",
        any_of_all: &[
            &["test coverage"], &["coverage", "gap"],
            &["coverage report"], &["missing tests"],
        ],
    },

    // --- Cleanup / safety ----------------------------------------------
    WfRule {
        slug: "everything-claude-code:refactor-clean",
        why:  "dead-code cleanup with per-step verification",
        any_of_all: &[
            &["dead code"], &["unused code"],
            &["clean up", "dead"], &["remove unused"],
            &["dead-code"], &["dead .md"],
        ],
    },
    WfRule {
        slug: "everything-claude-code:safety-guard",
        why:  "destructive-operation gate before agent action",
        any_of_all: &[
            &["rm -rf"], &["drop table"],
            &["delete the"], &["force push"],
            &["before i delete"], &["destructive"],
        ],
    },

    // --- Meta / tooling -------------------------------------------------
    WfRule {
        slug: "everything-claude-code:prompt-optimizer",
        why:  "rewrite user prompt for better ECC routing (advisory only)",
        any_of_all: &[
            &["optimize", "prompt"], &["improve", "prompt"],
            &["better prompt"], &["prompt engineering"],
            &["rewrite this prompt"],
        ],
    },
    WfRule {
        slug: "everything-claude-code:model-route",
        why:  "recommend model tier (Haiku vs Sonnet vs Opus) for this task",
        any_of_all: &[
            &["which model"], &["haiku", "sonnet"],
            &["sonnet", "opus"], &["model tier"],
            &["model selection"],
        ],
    },
    WfRule {
        slug: "everything-claude-code:agent-eval",
        why:  "head-to-head comparison of coding agents on custom tasks",
        any_of_all: &[
            &["compare agents"], &["benchmark agents"],
            &["claude", "aider"], &["claude", "codex"],
            &["aider", "codex"], &["agent benchmark"],
        ],
    },
    WfRule {
        slug: "everything-claude-code:code-tour",
        why:  "persona-targeted CodeTour walkthrough (.tour files)",
        any_of_all: &[
            &["onboarding tour"], &["code tour"],
            &["walkthrough", "codebase"], &["walkthrough", "code"],
            &["walk through", "code"], &["explain how", "works"],
            &["architecture walkthrough"], &["tour", "junior"],
            &["walkthrough", "junior"],
        ],
    },
    WfRule {
        slug: "everything-claude-code:codebase-onboarding",
        why:  "unfamiliar-codebase analysis → onboarding guide + CLAUDE.md starter",
        any_of_all: &[
            &["new repo"], &["unfamiliar codebase"],
            &["first time", "repo"], &["joining", "project"],
            &["onboard me"],
        ],
    },
];

pub(super) fn workflow_match(prompt: &str, max_n: usize) -> Vec<(&'static str, &'static str)> {
    let lc = prompt.to_ascii_lowercase();
    let mut hits: Vec<(&'static str, &'static str)> = Vec::new();
    for rule in WORKFLOW_RULES {
        let fired = rule.any_of_all.iter().any(|reqs|
            reqs.iter().all(|needle| lc.contains(&needle.to_ascii_lowercase()))
        );
        if fired {
            if !hits.iter().any(|(s, _)| *s == rule.slug) {
                hits.push((rule.slug, rule.why));
                if hits.len() >= max_n { break; }
            }
        }
    }
    hits
}
