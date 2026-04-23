//! All CLI subcommand handlers, organized as nested submodules so
//! `src/main.rs` can dispatch via `cli::install::run(...)` etc.
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

// ---------------------------------------------------------------------------
// install
// ---------------------------------------------------------------------------

pub mod install {
    use super::*;

    pub fn run(enable_telegram: bool, mut daemon: bool) -> Result<()> {
        if enable_telegram { daemon = true; }
        println!("==> we-forgectl install (daemon={daemon}, telegram={enable_telegram})");

        let m = service::manager();
        let _ = m.migrate_legacy();

        let mut cfg = config::with_env_overrides(config::load());
        cfg.mode             = if daemon { "daemon".into() } else { "scheduled".into() };
        cfg.installed_at     = now_iso();
        cfg.telegram_enabled = enable_telegram;

        if enable_telegram {
            if cfg.telegram_token.is_empty() || cfg.telegram_chat_id.is_empty() {
                eprintln!("  FAIL telegram token+chat_id required (set WE_FORGE_TELEGRAM_TOKEN / WE_FORGE_TELEGRAM_CHAT_ID)");
                return Err(anyhow::anyhow!("missing telegram credentials"));
            }
        }
        config::save(&cfg)?;
        println!("  OK config: {}", paths::config_file().display());

        m.install(daemon)?;
        println!("  OK service installed (mode={})", cfg.mode);

        std::thread::sleep(std::time::Duration::from_millis(1500));
        let s = m.status();
        println!("  status: {}", s);

        let _ = ecc_core::log("enterprise-agent-ops", "install via we-forgectl", "cli");
        let _ = ecc_core::log(
            if daemon { "autonomous-agent-harness" } else { "continuous-agent-loop" },
            if daemon { "daemon mode (KeepAlive)" } else { "scheduled mode (hourly)" },
            "cli",
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// uninstall — safety-guard pattern (backup-before-destroy)
// ---------------------------------------------------------------------------

pub mod uninstall {
    use super::*;
    use std::fs;

    pub fn run(deep: bool) -> Result<()> {
        println!("==> we-forgectl uninstall (safety-guard pattern)");

        fs::create_dir_all(paths::backup_dir())?;
        let snap = paths::backup_dir().join(now_iso());
        fs::create_dir_all(&snap)?;
        if paths::config_file().exists() {
            let _ = fs::copy(paths::config_file(), snap.join("config.json"));
        }
        let claude_settings = paths::claude_home().join("settings.json");
        if claude_settings.exists() {
            let _ = fs::copy(&claude_settings, snap.join("settings.json"));
        }
        println!("  OK backup: {}", snap.display());

        let m = service::manager();
        let _ = m.stop();
        m.uninstall()?;
        println!("  OK service uninstalled");

        if deep {
            let we_forge_home = paths::we_forge_home();
            if we_forge_home.exists() {
                let dest = snap.join("we-forge-home");
                let _ = fs::rename(&we_forge_home, &dest);
                println!("  OK ~/.we-forge moved to {}", dest.display());
            }
            let learning_data = paths::claude_home().join("learning/data");
            if learning_data.exists() {
                let dest = snap.join("learning-data");
                let _ = fs::rename(&learning_data, &dest);
                println!("  OK ~/.claude/learning/data moved to {}", dest.display());
            }
        }

        let _ = ecc_core::log("safety-guard", "uninstall via we-forgectl (backup created)", "cli");
        println!("\n==> uninstalled. backup at: {}", snap.display());
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// lifecycle (start / stop / restart)
// ---------------------------------------------------------------------------

pub mod lifecycle {
    use super::*;

    pub fn start()   -> Result<()> { service::manager().start() }
    pub fn stop()    -> Result<()> { service::manager().stop() }
    pub fn restart() -> Result<()> { service::manager().restart() }
}

// ---------------------------------------------------------------------------
// status
// ---------------------------------------------------------------------------

pub mod status {
    use super::*;

    pub fn run() -> Result<()> {
        let m   = service::manager();
        let s   = m.status();
        let cfg = config::with_env_overrides(config::load());
        println!("  status:   {}", s);
        println!("  mode:     {}", if cfg.mode.is_empty() { "scheduled".into() } else { cfg.mode });
        println!("  telegram: {}", if cfg.telegram_enabled { "enabled" } else { "disabled" });
        println!("  os:       {:?}{}",
            crate::core::Os::detect(),
            if crate::core::Os::is_wsl() { " (WSL)" } else { "" });
        println!("  config:   {}", paths::config_file().display());
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// dashboard (shells out to Python dashboard.py)
// ---------------------------------------------------------------------------

pub mod dashboard {
    use super::*;
    use std::process::Command;

    pub fn run() -> Result<()> {
        let candidates = [
            std::env::current_exe()?.parent().map(|p| p.join("../dashboard/dashboard.py")),
            Some(dirs::home_dir().unwrap().join("we-forge/dashboard/dashboard.py")),
        ];
        for path in candidates.iter().flatten() {
            if path.exists() {
                let _ = ecc_core::log("dashboard-builder", "dashboard.py launched via we-forgectl", "cli");
                let status = Command::new("python3").arg(path).arg("--serve").status()?;
                std::process::exit(status.code().unwrap_or(0));
            }
        }
        eprintln!("  FAIL dashboard.py not found");
        Err(anyhow::anyhow!("dashboard.py not found"))
    }
}

// ---------------------------------------------------------------------------
// notify-test — Telegram (full impl in daemon::telegram next session)
// ---------------------------------------------------------------------------

pub mod notify_test {
    use super::*;
    use crate::daemon::telegram::TelegramNotifier;
    use crate::core::now_iso;

    pub fn run() -> Result<()> {
        let cfg = config::with_env_overrides(config::load());
        if !cfg.telegram_enabled || cfg.telegram_token.is_empty() || cfg.telegram_chat_id.is_empty() {
            eprintln!("  FAIL telegram not enabled or credentials missing");
            return Err(anyhow::anyhow!("telegram not configured"));
        }
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let ok = rt.block_on(async {
            let n = TelegramNotifier::new(&cfg.telegram_token, &cfg.telegram_chat_id);
            n.send(&format!("we-forge notify-test (Rust) at {}", now_iso()), false).await
        });
        let _ = ecc_core::log("messages-ops", "notify-test (Rust) sent", "cli");
        if ok {
            println!("  OK test message sent");
            Ok(())
        } else {
            Err(anyhow::anyhow!("send failed — check token/chat_id"))
        }
    }
}

// ---------------------------------------------------------------------------
// doctor
// ---------------------------------------------------------------------------

pub mod doctor {
    use super::*;
    use std::path::Path;

    fn which(tool: &str) -> Option<std::path::PathBuf> {
        std::env::var("PATH").ok().and_then(|paths| {
            paths.split(if cfg!(windows) { ';' } else { ':' })
                .map(|d| Path::new(d).join(tool))
                .find(|p| p.is_file())
        })
    }

    pub fn run() -> Result<()> {
        println!("==> doctor");
        let mut issues = 0;
        for tool in ["python3", "bash", "jq"] {
            if let Some(p) = which(tool) {
                println!("  OK {tool}: {}", p.display());
            } else {
                println!("  FAIL {tool} not in PATH");
                issues += 1;
            }
        }
        for (path, label) in [
            (paths::claude_home().join("learning/tick.sh"), "tick.sh"),
            (paths::claude_home().join("agents/we-forge.md"), "we-forge agent"),
            (paths::claude_home().join("settings.json"), "settings.json"),
        ] {
            if path.exists() {
                println!("  OK {label}: {}", path.display());
            } else {
                println!("  FAIL {label} missing: {}", path.display());
                issues += 1;
            }
        }
        let s = service::manager().status();
        println!("  service: {s}");
        if issues == 0 {
            println!("\nall checks passed.");
            Ok(())
        } else {
            eprintln!("\n{issues} issue(s)");
            Err(anyhow::anyhow!("{issues} doctor issue(s)"))
        }
    }
}

// ---------------------------------------------------------------------------
// logs
// ---------------------------------------------------------------------------

pub mod logs {
    use super::*;
    use std::fs;

    pub fn run(n: usize) -> Result<()> {
        let candidates = [
            paths::macos_log_dir().join("daemon.log"),
            paths::linux_state_dir().join("daemon.log"),
            paths::claude_home().join("learning/data/tick.log"),
        ];
        for path in candidates {
            if path.exists() {
                println!("==> tail -n {n} {}", path.display());
                let text = fs::read_to_string(&path)?;
                let lines: Vec<&str> = text.lines().collect();
                let start = lines.len().saturating_sub(n);
                for line in &lines[start..] {
                    println!("  {line}");
                }
                return Ok(());
            }
        }
        eprintln!("  WARN no log files found yet");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// ECC trace ledger
// ---------------------------------------------------------------------------

pub mod ecc {
    use super::*;

    pub fn log(skill: &str, reason: &str, invoker: &str) -> Result<()> {
        if skill.is_empty() {
            return Err(anyhow::anyhow!("skill name required"));
        }
        ecc_core::log(skill, reason, invoker)?;
        println!("  OK logged: {skill}");
        Ok(())
    }

    pub fn trace(last_n: usize, group: bool) -> Result<()> {
        let entries = ecc_core::read_all();
        if entries.is_empty() {
            println!("  WARN no ECC trace yet: {}", paths::ecc_trace_file().display());
            println!("  the we-forge agent (or CLI) calls 'we-forgectl ecc-log <skill> <reason>'");
            return Ok(());
        }
        if group {
            let mut counter: BTreeMap<String, usize> = BTreeMap::new();
            for e in &entries {
                *counter.entry(e.skill.clone()).or_insert(0) += 1;
            }
            let mut sorted: Vec<_> = counter.into_iter().collect();
            sorted.sort_by(|a, b| b.1.cmp(&a.1));
            println!("==> ECC skill usage (totals across {} records)", entries.len());
            for (skill, n) in sorted {
                println!("  {n:>4}  {skill}");
            }
            return Ok(());
        }
        println!("==> ECC trace (last {last_n} of {})", entries.len());
        let start = entries.len().saturating_sub(last_n);
        for e in &entries[start..] {
            println!("  {}  [{}]  {}  {}",
                e.ts,
                e.invoker,
                e.skill,
                e.reason.chars().take(80).collect::<String>(),
            );
        }
        Ok(())
    }
}
