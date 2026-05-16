//! CLI subcommand router. Each `pub mod` below lives in `src/cli/<name>.rs`
//! and exports a `run(...)` entry point that `src/main.rs` dispatches to.
//!
//! Split from a 2,072-line monolith on 2026-05-16 (repo-scan R3 finding).
//!
//! Shared imports live at this level so each submodule's `use super::*;`
//! continues to resolve `config`, `ecc_core`, `paths`, `service`, `Result`,
//! `now_iso`, and `BTreeMap` exactly as it did pre-split.
//!
//! ECC alignment per submodule:
//!   - install/uninstall  → enterprise-agent-ops + safety-guard
//!   - lifecycle          → enterprise-agent-ops
//!   - status / doctor    → continuous-agent-loop (observability)
//!   - dashboard          → dashboard-builder (delegated to dashboard.py)
//!   - notify_test        → messages-ops
//!   - ecc                → architecture-decision-records

use crate::core::{config, ecc as ecc_core, now_iso, paths};
use crate::service;
use anyhow::Result;
use std::collections::BTreeMap;

pub mod install;
pub mod uninstall;
pub mod lifecycle;
pub mod status;
pub mod set_interval;
pub mod set_workflow_suggest;
pub mod dashboard;
pub mod notify_test;
pub mod doctor;
pub mod logs;
pub mod ecc;
pub mod session_util;
pub mod sessions;
pub mod ping;
pub mod audit;
pub mod ecc_quality;
pub mod skill_suggest;
pub mod skill_hits;
pub mod synonym_candidates;
pub mod synonym_coverage;
pub mod skill_regressions;

