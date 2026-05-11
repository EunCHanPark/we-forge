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
        let interval_sec = config::interval_seconds(&cfg);
        let next = config::next_aligned_tick_time(interval_sec, chrono::Local::now());
        println!("  status:   {}", s);
        println!("  mode:     {}", if cfg.mode.is_empty() { "scheduled".into() } else { cfg.mode });
        println!("  interval: {} min  ({}s — learning + telegram, aligned to 00:00)",
                 interval_sec / 60, interval_sec);
        println!("  next tick: {}", next.format("%Y-%m-%d %H:%M"));
        println!("  telegram: {}", if cfg.telegram_enabled { "enabled" } else { "disabled" });
        println!("  os:       {:?}{}",
            crate::core::Os::detect(),
            if crate::core::Os::is_wsl() { " (WSL)" } else { "" });
        println!("  config:   {}", paths::config_file().display());
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// set-interval — unified tick + telegram cadence (minutes)
// ---------------------------------------------------------------------------

pub mod set_interval {
    use super::*;
    use std::fs;

    pub fn run(minutes: u32) -> Result<()> {
        if minutes < 1 || minutes > 1440 {
            eprintln!("  FAIL interval must be 1-1440 minutes");
            return Err(anyhow::anyhow!("invalid interval"));
        }
        let mut cfg = config::with_env_overrides(config::load());
        let old = if cfg.interval_minutes == 0 {
            config::DEFAULT_INTERVAL_MIN
        } else {
            cfg.interval_minutes
        };
        cfg.interval_minutes = minutes;
        config::save(&cfg)?;
        println!("  OK interval: {} min → {} min", old, minutes);
        println!("  config:   {}", paths::config_file().display());
        println!("  effect:   tick every {} min · telegram notify every {} min", minutes, minutes);
        println!("  applies:  next iteration of running daemon (no restart needed — hot-reload)");

        // Reset Telegram throttle state so the new cadence applies cleanly.
        let last_sent = paths::we_forge_home().join("last_telegram_sent_at");
        let pending   = paths::we_forge_home().join("telegram_pending.jsonl");
        let _ = fs::remove_file(&last_sent);
        let _ = fs::remove_file(&pending);
        println!("  reset:    telegram throttle state (next tick fires fresh)");

        let _ = ecc_core::log(
            "enterprise-agent-ops",
            &format!("interval changed via set-interval ({old}→{minutes} min)"),
            "cli",
        );
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
        // On Windows, binaries live as tool.exe / tool.cmd / tool.bat on PATH.
        // Searching for the bare name misses them and produces false FAIL
        // reports in `doctor` even when the tool is installed.
        let exts: &[&str] = if cfg!(windows) {
            &["", ".exe", ".cmd", ".bat"]
        } else {
            &[""]
        };
        std::env::var("PATH").ok().and_then(|paths| {
            paths
                .split(if cfg!(windows) { ';' } else { ':' })
                .flat_map(|d| {
                    exts.iter().map(move |e| {
                        if e.is_empty() {
                            Path::new(d).join(tool)
                        } else {
                            Path::new(d).join(format!("{tool}{e}"))
                        }
                    })
                })
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

// ---------------------------------------------------------------------------
// session / heartbeat helpers (shared by sessions + ping)
// ---------------------------------------------------------------------------

pub mod session_util {
    use super::paths;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Decode Claude Code's encoded project dir name back to a filesystem path.
    /// Claude Code prepends '-' and replaces every '/' and '.' with '-'.
    /// Greedy resolution using filesystem existence checks.
    pub fn decode_project_path(encoded: &str) -> String {
        let stripped = encoded.trim_start_matches('-');
        // Fast-path 1: pure slash substitution
        let simple: String = std::iter::once('/').chain(stripped.replace('-', "/").chars()).collect();
        if PathBuf::from(&simple).exists() {
            return simple;
        }
        // Fast-path 2: '--' as '/.'
        let with_dot: String = std::iter::once('/')
            .chain(stripped.replace("--", "/.").replace('-', "/").chars())
            .collect();
        if PathBuf::from(&with_dot).exists() {
            return with_dot;
        }
        // Greedy walk
        let raw: Vec<&str> = stripped.split('-').collect();
        let mut segments: Vec<String> = Vec::new();
        let mut i = 0;
        while i < raw.len() {
            if raw[i].is_empty() && i + 1 < raw.len() {
                segments.push(format!(".{}", raw[i + 1]));
                i += 2;
            } else {
                if !raw[i].is_empty() {
                    segments.push(raw[i].to_string());
                }
                i += 1;
            }
        }
        if segments.is_empty() {
            return if !with_dot.is_empty() { with_dot } else { simple };
        }
        let mut path = format!("/{}", segments[0]);
        for seg in &segments[1..] {
            let slash_try = format!("{}/{}", path, seg);
            let dash_try  = format!("{}-{}", path, seg);
            if PathBuf::from(&slash_try).exists() {
                path = slash_try;
            } else if PathBuf::from(&dash_try).exists() {
                path = dash_try;
            } else {
                path = slash_try;
            }
        }
        path
    }

    pub struct ActiveRow {
        pub mtime_secs: u64,
        pub label: String,
        pub path: String,
    }

    /// Read heartbeat files newer than `cutoff_secs` (epoch seconds), pruning expired ones.
    pub fn read_heartbeats(cutoff_secs: u64) -> Vec<ActiveRow> {
        let dir = paths::heartbeats_dir();
        let mut rows = Vec::new();
        let entries = match fs::read_dir(&dir) {
            Ok(e)  => e,
            Err(_) => return rows,
        };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let txt = match fs::read_to_string(&p) {
                Ok(s)  => s,
                Err(_) => continue,
            };
            let v: serde_json::Value = match serde_json::from_str(&txt) {
                Ok(v)  => v,
                Err(_) => continue,
            };
            let epoch = v.get("epoch").and_then(|x| x.as_f64())
                .unwrap_or_else(|| {
                    fs::metadata(&p).ok()
                        .and_then(|m| m.modified().ok())
                        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                        .map(|d| d.as_secs() as f64)
                        .unwrap_or(0.0)
                }) as u64;
            if epoch < cutoff_secs {
                let _ = fs::remove_file(&p);
                continue;
            }
            let cwd = v.get("cwd").and_then(|x| x.as_str()).unwrap_or("?").to_string();
            let label = v.get("label").and_then(|x| x.as_str()).filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .unwrap_or_else(|| {
                    let pid = v.get("pid").and_then(|x| x.as_i64()).unwrap_or(0);
                    format!("pid={pid}")
                });
            rows.push(ActiveRow { mtime_secs: epoch, label, path: cwd });
        }
        rows
    }

    /// Format active sessions block: transcript scan + heartbeat fallback.
    pub fn format_active(window_min: i64, max_show: usize) -> String {
        use std::collections::HashSet;
        let now_secs = SystemTime::now().duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs()).unwrap_or(0);
        let cutoff = now_secs.saturating_sub((window_min.max(0) as u64) * 60);

        let mut rows: Vec<ActiveRow> = Vec::new();
        let projects = paths::claude_home().join("projects");
        if let Ok(it) = fs::read_dir(&projects) {
            for proj in it.flatten() {
                let pp = proj.path();
                if !pp.is_dir() { continue; }
                let name = match pp.file_name().and_then(|s| s.to_str()) {
                    Some(n) => n.to_string(),
                    None    => continue,
                };
                let decoded = decode_project_path(&name);
                if let Ok(files) = fs::read_dir(&pp) {
                    for tx in files.flatten() {
                        let txp = tx.path();
                        if txp.extension().and_then(|s| s.to_str()) != Some("jsonl") { continue; }
                        let mtime = match fs::metadata(&txp).and_then(|m| m.modified()) {
                            Ok(t)  => t.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
                            Err(_) => continue,
                        };
                        if mtime < cutoff { continue; }
                        let stem = txp.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                        let sid: String = stem.chars().take(8).collect();
                        rows.push(ActiveRow { mtime_secs: mtime, label: sid, path: decoded.clone() });
                    }
                }
            }
        }

        let mut seen: HashSet<String> = rows.iter().map(|r| r.path.clone()).collect();
        for r in read_heartbeats(cutoff) {
            if !seen.contains(&r.path) {
                seen.insert(r.path.clone());
                rows.push(ActiveRow {
                    mtime_secs: r.mtime_secs,
                    label: format!("ping:{}", r.label),
                    path: r.path,
                });
            }
        }

        rows.sort_by(|a, b| b.mtime_secs.cmp(&a.mtime_secs));

        if rows.is_empty() {
            return format!(
                "active sessions (last {window_min}min): (none — all idle)\n  tip: run  ! we-forgectl ping  from inside a session to register it"
            );
        }
        let total = rows.len();
        let mut lines = vec![format!("active sessions (last {window_min}min, {total} total):")];
        for r in rows.iter().take(max_show) {
            let age_min = (now_secs.saturating_sub(r.mtime_secs)) / 60;
            let mark = if age_min < 5 { "⚡" } else if age_min < 30 { "🕐" } else { "💤" };
            let hh_mm = format_hh_mm_local(r.mtime_secs);
            let short = if r.path.chars().count() <= 45 {
                r.path.clone()
            } else {
                let tail: String = r.path.chars().rev().take(44).collect::<String>().chars().rev().collect();
                format!("…{tail}")
            };
            lines.push(format!("  {mark} {}  {hh_mm} ({age_min}m)  {short}", r.label));
        }
        if total > max_show {
            lines.push(format!("  … ({} more)", total - max_show));
        }
        lines.push("  (run  ! we-forgectl ping  to register a session not shown above)".to_string());
        lines.join("\n")
    }

    fn format_hh_mm_local(epoch_secs: u64) -> String {
        use chrono::TimeZone;
        match chrono::Local.timestamp_opt(epoch_secs as i64, 0).single() {
            Some(dt) => dt.format("%H:%M").to_string(),
            None     => "??:??".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// sessions
// ---------------------------------------------------------------------------

pub mod sessions {
    use super::*;

    pub fn run(window_min: i64) -> Result<()> {
        println!("{}", session_util::format_active(window_min, 20));
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// ping — register the current session via heartbeat file
// ---------------------------------------------------------------------------

pub mod ping {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    pub fn run(label: &str) -> Result<()> {
        let dir = paths::heartbeats_dir();
        fs::create_dir_all(&dir)?;
        let cwd = std::env::current_dir()?;
        let cwd_str = cwd.to_string_lossy().to_string();
        let pid = std::process::id();
        let epoch = SystemTime::now().duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs_f64()).unwrap_or(0.0);
        let record = serde_json::json!({
            "ts":    now_iso(),
            "epoch": epoch,
            "cwd":   cwd_str,
            "pid":   pid,
            "label": label,
        });
        let path = dir.join(format!("{pid}.json"));
        fs::write(&path, record.to_string())?;
        println!("================================================================");
        println!("✅  we-forge attached to this session");
        println!("    cwd:   {cwd_str}");
        println!("    pid:   {pid}");
        if !label.is_empty() {
            println!("    label: {label}");
        }
        println!("================================================================");
        println!("  we-forgectl sessions  — to verify this session is visible");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// audit — cross-validate patterns / ledger / rejected entries
// ---------------------------------------------------------------------------

pub mod audit {
    use super::*;
    use std::collections::{BTreeMap, HashMap, HashSet};
    use std::fs;

    fn slugify(s: &str) -> String {
        let mut out = String::new();
        let mut prev_dash = true;
        for c in s.chars().flat_map(|c| c.to_lowercase()) {
            if c.is_ascii_alphanumeric() {
                out.push(c);
                prev_dash = false;
            } else if !prev_dash {
                out.push('-');
                prev_dash = true;
            }
        }
        let trimmed = out.trim_matches('-').to_string();
        let truncated: String = trimmed.chars().take(60).collect();
        let final_s = truncated.trim_matches('-').to_string();
        if final_s.is_empty() { "pattern".into() } else { final_s }
    }

    pub fn run(top_n: usize) -> Result<()> {
        let learn = paths::learning_data_dir();
        let pat_path = learn.join("patterns.jsonl");
        let led_path = learn.join("ledger.jsonl");
        let rej_path = learn.join("rejected.txt");
        if !pat_path.exists() {
            println!("  WARN no patterns yet: {}", pat_path.display());
            return Ok(());
        }
        let rejected: HashSet<String> = if rej_path.exists() {
            fs::read_to_string(&rej_path).unwrap_or_default()
                .lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect()
        } else {
            HashSet::new()
        };

        let mut patterns: Vec<serde_json::Value> = Vec::new();
        if let Ok(text) = fs::read_to_string(&pat_path) {
            for line in text.lines() {
                if line.trim().is_empty() { continue; }
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                    patterns.push(v);
                }
            }
        }

        let mut slug_to_decisions: HashMap<String, Vec<serde_json::Value>> = HashMap::new();
        if led_path.exists() {
            if let Ok(text) = fs::read_to_string(&led_path) {
                for line in text.lines() {
                    if line.trim().is_empty() { continue; }
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                        if let Some(s) = v.get("slug").and_then(|x| x.as_str()) {
                            slug_to_decisions.entry(s.to_string()).or_default().push(v);
                        }
                    }
                }
            }
        }

        struct Row { count: i64, verdict: String, pattern: String, reason: String }
        let mut rows: Vec<Row> = Vec::new();
        for p in &patterns {
            let cnt = p.get("count").and_then(|x| x.as_i64()).unwrap_or(0);
            let sids = p.get("sample_session_ids").and_then(|x| x.as_array()).map(|a| a.len()).unwrap_or(0);
            if cnt < 3 || sids < 3 { continue; }
            let pat = p.get("pattern").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let slug = slugify(&pat);
            let (verdict, reason) = if let Some(decs) = slug_to_decisions.get(&slug) {
                let last = decs.last().unwrap();
                let v = last.get("decision").and_then(|x| x.as_str()).unwrap_or("?").to_string();
                let r: String = last.get("reason").and_then(|x| x.as_str()).unwrap_or("").chars().take(60).collect();
                (v, r)
            } else if rejected.contains(&pat) {
                ("REJECTED".into(), "in rejected.txt (skipped from queue)".into())
            } else {
                ("no-ledger".into(), "(never reached ledger — investigate)".into())
            };
            let pat_short: String = pat.chars().take(60).collect();
            rows.push(Row { count: cnt, verdict, pattern: pat_short, reason });
        }
        rows.sort_by(|a, b| b.count.cmp(&a.count));

        println!("==> audit: top {top_n} patterns (count >= 3, distinct sessions >= 3)");
        println!("{:>6}  {:<12}  {:<60}  reason", "count", "verdict", "pattern");
        println!("{}", "-".repeat(110));
        for r in rows.iter().take(top_n) {
            println!("{:>6}  {:<12}  {:<60}  {}", r.count, r.verdict, r.pattern, r.reason);
        }
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for r in &rows { *counts.entry(r.verdict.clone()).or_insert(0) += 1; }
        println!();
        println!("verdict breakdown (qualifying patterns): {:?}", counts);
        println!();
        println!("Interpretation:");
        println!("  - High count + ECC_MATCH: verify the matched skill genuinely fits.");
        println!("  - High count + DROP:      verify the primitive filter wasn't over-eager.");
        println!("  - High count + no-ledger: pipeline gap — should have been processed.");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// ecc-quality — ECC_MATCH match_score distribution + flagged candidates
// ---------------------------------------------------------------------------

pub mod ecc_quality {
    use super::*;
    use std::collections::BTreeMap;
    use std::fs;

    pub fn run(score_threshold: i64) -> Result<()> {
        let led_path = paths::learning_data_dir().join("ledger.jsonl");
        if !led_path.exists() {
            println!("  WARN no ledger: {}", led_path.display());
            return Ok(());
        }
        // Score key: i64 score, or i64::MIN for "unknown"
        let mut score_dist: BTreeMap<i64, usize> = BTreeMap::new();
        let mut unknown: usize = 0;
        let mut skill_counts: BTreeMap<String, usize> = BTreeMap::new();
        let mut flagged: Vec<serde_json::Value> = Vec::new();

        if let Ok(text) = fs::read_to_string(&led_path) {
            for line in text.lines() {
                if line.trim().is_empty() { continue; }
                let d: serde_json::Value = match serde_json::from_str(line) {
                    Ok(v)  => v,
                    Err(_) => continue,
                };
                if d.get("decision").and_then(|x| x.as_str()) != Some("ECC_MATCH") { continue; }
                match d.get("match_score").and_then(|x| x.as_i64()) {
                    Some(s) => {
                        *score_dist.entry(s).or_insert(0) += 1;
                        let skill = d.get("ecc_skill").and_then(|x| x.as_str()).unwrap_or("?").to_string();
                        *skill_counts.entry(skill).or_insert(0) += 1;
                        if s < score_threshold { flagged.push(d); }
                    }
                    None => {
                        unknown += 1;
                    }
                }
            }
        }

        println!("==> ECC_MATCH match_score distribution (threshold={score_threshold})");
        if unknown > 0 {
            println!("  score {:<8} {:>4}", "unknown", unknown);
        }
        for (s, n) in &score_dist {
            let marker = if *s < score_threshold { " ⚠️" } else { "" };
            println!("  score {:<8} {:>4}{marker}", s, n);
        }
        println!();
        println!("==> top matched ECC skills (top 10)");
        let mut pairs: Vec<(String, usize)> = skill_counts.into_iter().collect();
        pairs.sort_by(|a, b| b.1.cmp(&a.1));
        for (skill, n) in pairs.iter().take(10) {
            println!("  {:>4}  {}", n, skill);
        }
        println!();
        if !flagged.is_empty() {
            println!("==> ⚠️  {} entries below score_threshold (REVISE downgrade candidates)", flagged.len());
            for d in flagged.iter().take(15) {
                let slug = d.get("slug").and_then(|x| x.as_str()).unwrap_or("");
                let score = d.get("match_score").and_then(|x| x.as_i64())
                    .map(|s| s.to_string()).unwrap_or_else(|| "?".into());
                let ecc_skill = d.get("ecc_skill").and_then(|x| x.as_str()).unwrap_or("?");
                println!("  {:<35} score={} ecc_skill={}", slug, score, ecc_skill);
            }
        } else {
            println!("==> all ECC_MATCH entries score >= {score_threshold} ✓");
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// skill-suggest — match user prompt against ECC marketplace skills (BM25-lite)
//
// Reads ~/.we-forge/ecc-index.json (built by learning/build_ecc_index.py).
// If the index is missing, all modes silently emit nothing — the
// UserPromptSubmit hook treats empty stdout as "no suggestion" and proceeds.
// ---------------------------------------------------------------------------

pub mod skill_suggest {
    use super::*;
    use std::collections::{BTreeMap, HashSet};
    use std::fs;

    const STOPWORDS: &[&str] = &[
        "the","and","for","with","this","that","from","into","when","where","which",
        "after","before","are","or","of","to","in","on","at","be","by","as","it",
        "if","you","your","use","used",
    ];

    fn tokenize(text: &str) -> Vec<String> {
        // Equivalent of Python regex r"[A-Za-z][A-Za-z0-9_-]{2,}", lowercase.
        // Light suffix strip for plurals/-ing. Order preserved, deduped.
        let stops: HashSet<&str> = STOPWORDS.iter().copied().collect();
        let mut seen: HashSet<String> = HashSet::new();
        let mut out: Vec<String> = Vec::new();

        let bytes = text.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            let c = bytes[i];
            if c.is_ascii_alphabetic() {
                let start = i;
                i += 1;
                while i < bytes.len() {
                    let b = bytes[i];
                    if b.is_ascii_alphanumeric() || b == b'_' || b == b'-' { i += 1; } else { break; }
                }
                let raw = &text[start..i];
                let t: String = raw.to_ascii_lowercase();
                if t.len() < 3 || stops.contains(t.as_str()) { continue; }
                let pushed: String = if t.len() > 5 && t.ends_with("ing") {
                    t[..t.len()-3].to_string()
                } else if t.len() > 5 && t.ends_with("ies") {
                    let mut s = t[..t.len()-3].to_string(); s.push('y'); s
                } else if t.len() > 5 && t.ends_with("es") {
                    t[..t.len()-2].to_string()
                } else if t.len() > 4 && t.ends_with('s') && !t.ends_with("ss") {
                    t[..t.len()-1].to_string()
                } else {
                    t
                };
                if !seen.contains(&pushed) {
                    seen.insert(pushed.clone());
                    out.push(pushed);
                }
            } else {
                i += 1;
            }
        }
        out
    }

    fn split_slug_parts(slug: &str) -> Vec<String> {
        slug.split(|c: char| c == '-' || c == '_')
            .filter(|p| p.len() >= 3)
            .map(|p| p.to_ascii_lowercase())
            .collect()
    }

    struct Ranked {
        namespaced_slug: String,
        slug: String,
        description: String,
        score: f64,
        overlap: Vec<String>,
    }

    fn rank(prompt: &str, top_n: usize, min_score: f64) -> Vec<Ranked> {
        let path = paths::ecc_index_file();
        if !path.exists() { return vec![]; }
        let txt = match fs::read_to_string(&path) { Ok(s) => s, Err(_) => return vec![] };
        let idx: serde_json::Value = match serde_json::from_str(&txt) { Ok(v) => v, Err(_) => return vec![] };
        let skills = match idx.get("skills").and_then(|x| x.as_array()) {
            Some(a) => a, None => return vec![],
        };
        let idf = match idx.get("idf").and_then(|x| x.as_object()) {
            Some(o) => o, None => return vec![],
        };
        let prompt_tokens: HashSet<String> = tokenize(prompt).into_iter().collect();
        if prompt_tokens.is_empty() { return vec![]; }

        let threshold = if min_score <= 0.0 { 3.0 } else { min_score };
        let slug_boost = 3.0_f64;
        let prefix_boost = 4.0_f64;

        struct Entry { score: f64, namespaced: String, slug: String, desc: String, overlap: Vec<String> }
        let mut by_ns: BTreeMap<String, Entry> = BTreeMap::new();

        for s in skills {
            if !s.get("suggestable").and_then(|x| x.as_bool()).unwrap_or(false) { continue; }
            let skill_tokens: HashSet<String> = s.get("tokens").and_then(|x| x.as_array())
                .map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default();
            if skill_tokens.is_empty() { continue; }
            let overlap: Vec<String> = prompt_tokens.intersection(&skill_tokens).cloned().collect();
            if overlap.is_empty() { continue; }
            let slug = s.get("slug").and_then(|x| x.as_str()).unwrap_or("").to_ascii_lowercase();
            let slug_first = slug.split_once('-').map(|(a, _)| a.to_string()).unwrap_or_else(|| slug.clone());
            let mut slug_token_set: HashSet<String> = split_slug_parts(&slug).into_iter().collect();
            // Also include name parts
            if let Some(name) = s.get("name").and_then(|x| x.as_str()) {
                for p in split_slug_parts(&name.to_ascii_lowercase()) {
                    slug_token_set.insert(p);
                }
            }
            let mut score = 0.0_f64;
            for t in &overlap {
                let mut w = idf.get(t).and_then(|v| v.as_f64()).unwrap_or(0.0);
                if t == &slug_first { w *= prefix_boost; }
                else if slug_token_set.contains(t) { w *= slug_boost; }
                score += w;
            }
            if score < threshold { continue; }
            let ns = s.get("namespaced_slug").and_then(|x| x.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| slug.clone());
            if ns.is_empty() { continue; }
            let desc: String = s.get("description").and_then(|x| x.as_str()).unwrap_or("").chars().take(140).collect();
            let mut sorted_overlap = overlap.clone();
            sorted_overlap.sort();
            let entry = Entry { score, namespaced: ns.clone(), slug: slug.clone(), desc, overlap: sorted_overlap };
            match by_ns.get(&ns) {
                Some(prev) if prev.score >= score => {}
                _ => { by_ns.insert(ns, entry); }
            }
        }

        let mut all: Vec<Entry> = by_ns.into_values().collect();
        all.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        all.into_iter().take(top_n).map(|e| Ranked {
            namespaced_slug: e.namespaced,
            slug: e.slug,
            description: e.desc,
            score: (e.score * 100.0).round() / 100.0,
            overlap: e.overlap,
        }).collect()
    }

    fn short_hash(s: &str) -> String {
        // Cheap non-crypto rolling hash → 8 hex chars (FNV-1a).
        let mut h: u64 = 0xcbf29ce484222325;
        for &b in s.as_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        format!("{:016x}", h).chars().take(8).collect()
    }

    fn log_turn(prompt: &str, session_id: &str) {
        let _ = std::fs::create_dir_all(paths::we_forge_home());
        let rec = serde_json::json!({
            "ts": now_iso(),
            "session_id": session_id,
            "prompt_len": prompt.chars().count(),
        });
        let _ = std::fs::OpenOptions::new().create(true).append(true)
            .open(paths::turns_log())
            .and_then(|mut f| {
                use std::io::Write;
                writeln!(f, "{}", rec)
            });
    }

    fn log_suggestion(prompt: &str, suggestions: &[Ranked], session_id: &str) {
        let _ = std::fs::create_dir_all(paths::we_forge_home());
        let suggested: Vec<&str> = suggestions.iter().map(|s| s.namespaced_slug.as_str()).collect();
        let scores: Vec<f64> = suggestions.iter().map(|s| s.score).collect();
        let preview: String = prompt.chars().take(120).collect();
        let rec = serde_json::json!({
            "ts": now_iso(),
            "session_id": session_id,
            "prompt_hash": short_hash(prompt),
            "prompt_preview": preview,
            "suggested": suggested,
            "scores": scores,
        });
        let _ = std::fs::OpenOptions::new().create(true).append(true)
            .open(paths::suggest_log())
            .and_then(|mut f| {
                use std::io::Write;
                writeln!(f, "{}", rec)
            });
    }

    pub fn run(prompt: &str, top_n: usize, inject: bool, log: bool, session_id: &str) -> Result<()> {
        let prompt = prompt.trim();
        if prompt.is_empty() { return Ok(()); }
        // Quick-path: silent skip on trivial prompts.
        if prompt.chars().count() < 15 {
            if log { log_turn(prompt, session_id); }
            return Ok(());
        }
        let suggestions = rank(prompt, top_n.max(1), 0.0);
        if log {
            log_turn(prompt, session_id);
            if !suggestions.is_empty() {
                log_suggestion(prompt, &suggestions, session_id);
            }
        }
        if suggestions.is_empty() { return Ok(()); }

        if inject {
            let mut lines = vec![
                "<system-reminder>".to_string(),
                format!("we-forge skill-suggest matched these ECC skills against the user's prompt (top {}, IDF-weighted):", suggestions.len()),
                "".to_string(),
            ];
            for (i, s) in suggestions.iter().enumerate() {
                lines.push(format!("{}. `{}` (score {}) — {}", i + 1, s.namespaced_slug, s.score, s.description));
            }
            lines.push("".to_string());
            lines.push("If any of these match the user's intent, invoke via the Skill tool BEFORE writing code. If none apply, proceed normally — do not announce that you are skipping suggestions.".to_string());
            lines.push("</system-reminder>".to_string());
            println!("{}", lines.join("\n"));
            return Ok(());
        }

        println!("skill-suggest: top {} for prompt ({} chars)", suggestions.len(), prompt.chars().count());
        for (i, s) in suggestions.iter().enumerate() {
            let desc_short: String = s.description.chars().take(70).collect();
            println!("  {}. {:55} score={:5}  {}", i + 1, s.namespaced_slug, s.score, desc_short);
            let ov: Vec<String> = s.overlap.iter().take(8).cloned().collect();
            println!("     overlap: {}", ov.join(", "));
            let _ = &s.slug;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// skill-hits — hit rate (suggested vs invoked) over a time window
// ---------------------------------------------------------------------------

pub mod skill_hits {
    use super::*;
    use std::collections::{BTreeMap, HashSet};
    use std::fs;

    pub fn run(window_hours: i64) -> Result<()> {
        let cutoff = chrono::Utc::now() - chrono::Duration::hours(window_hours.max(0));
        let cutoff_iso = cutoff.format("%Y-%m-%dT%H:%M:%SZ").to_string();

        let mut suggestions: Vec<serde_json::Value> = Vec::new();
        let sug_path = paths::suggest_log();
        if let Ok(text) = fs::read_to_string(&sug_path) {
            for line in text.lines() {
                if line.trim().is_empty() { continue; }
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                    if v.get("ts").and_then(|x| x.as_str()).unwrap_or("") >= cutoff_iso.as_str() {
                        suggestions.push(v);
                    }
                }
            }
        }

        let mut invoked: HashSet<String> = HashSet::new();
        let trace_path = paths::ecc_trace_file();
        if let Ok(text) = fs::read_to_string(&trace_path) {
            for line in text.lines() {
                if line.trim().is_empty() { continue; }
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                    if v.get("ts").and_then(|x| x.as_str()).unwrap_or("") >= cutoff_iso.as_str() {
                        if let Some(sk) = v.get("skill").and_then(|x| x.as_str()) {
                            let sk = sk.trim().to_string();
                            if sk.is_empty() { continue; }
                            invoked.insert(sk.clone());
                            if let Some((_, after)) = sk.split_once(':') {
                                invoked.insert(after.to_string());
                            }
                        }
                    }
                }
            }
        }

        let mut hits = 0_usize;
        let mut miss_counts: BTreeMap<String, usize> = BTreeMap::new();
        for sg in &suggestions {
            let suggested = match sg.get("suggested").and_then(|x| x.as_array()) {
                Some(a) if !a.is_empty() => a,
                _ => continue,
            };
            let mut any_hit = false;
            for ns_v in suggested {
                let ns = match ns_v.as_str() { Some(s) => s, None => continue };
                let bare = ns.split_once(':').map(|(_, a)| a).unwrap_or(ns);
                if invoked.contains(ns) || invoked.contains(bare) {
                    any_hit = true;
                    break;
                }
            }
            if any_hit {
                hits += 1;
            } else {
                for ns_v in suggested {
                    if let Some(ns) = ns_v.as_str() {
                        *miss_counts.entry(ns.to_string()).or_insert(0) += 1;
                    }
                }
            }
        }
        let n_sugg = suggestions.len();
        let rate = if n_sugg > 0 { hits as f64 / n_sugg as f64 * 100.0 } else { 0.0 };

        println!("==> skill-suggest hit rate (last {window_hours}h)");
        println!("  suggestions:  {n_sugg}");
        println!("  hits:         {hits}");
        println!("  hit rate:     {:.1}%", rate);
        if !miss_counts.is_empty() {
            println!();
            println!("  top misses (suggested but never invoked):");
            let mut top: Vec<(String, usize)> = miss_counts.into_iter().collect();
            top.sort_by(|a, b| b.1.cmp(&a.1));
            for (slug, n) in top.iter().take(5) {
                println!("    {:3}  {}", n, slug);
            }
        }
        Ok(())
    }
}
