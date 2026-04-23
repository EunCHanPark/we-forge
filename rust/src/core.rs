//! Cross-cutting helpers: paths, config, OS detection, ECC trace ledger.
//!
//! Mirrors the data-layer functions in the Python single-file CLI so any user
//! of the Python version can switch to the Rust binary without losing state
//! (config.json and ecc-trace.jsonl are byte-compatible between versions).
//!
//! ECC alignment:
//!   - core::config  → safety-guard (atomic write, mode 0o600)
//!   - core::ecc     → architecture-decision-records (record-when-decided)
//!   - core::paths   → enterprise-agent-ops (lifecycle artifact locations)

use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Time
// ---------------------------------------------------------------------------

/// ISO-8601 UTC timestamp string (no fractional seconds).
pub fn now_iso() -> String {
    Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

// ---------------------------------------------------------------------------
// Platform detection
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Os {
    MacOS,
    Linux,
    Windows,
    Other,
}

impl Os {
    pub fn detect() -> Self {
        match std::env::consts::OS {
            "macos"   => Os::MacOS,
            "linux"   => Os::Linux,
            "windows" => Os::Windows,
            _         => Os::Other,
        }
    }

    pub fn is_wsl() -> bool {
        if Self::detect() != Os::Linux {
            return false;
        }
        fs::read_to_string("/proc/version")
            .map(|s| s.to_lowercase().contains("microsoft"))
            .unwrap_or(false)
    }
}

// ---------------------------------------------------------------------------
// Paths (~/.we-forge, ~/.claude, log dirs, service-file dirs)
// ---------------------------------------------------------------------------

pub mod paths {
    use super::*;

    pub fn we_forge_home() -> PathBuf {
        std::env::var_os("WE_FORGE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| dirs::home_dir().expect("home dir").join(".we-forge"))
    }

    pub fn claude_home() -> PathBuf {
        std::env::var_os("CLAUDE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| dirs::home_dir().expect("home dir").join(".claude"))
    }

    pub fn config_file()   -> PathBuf { we_forge_home().join("config.json") }
    pub fn ecc_trace_file() -> PathBuf { we_forge_home().join("ecc-trace.jsonl") }
    pub fn backup_dir()    -> PathBuf { we_forge_home().join("backup") }
    pub fn daemon_pid()    -> PathBuf { we_forge_home().join("daemon.pid") }

    pub fn macos_launch_agents() -> PathBuf {
        dirs::home_dir().expect("home dir").join("Library/LaunchAgents")
    }
    pub fn macos_log_dir() -> PathBuf {
        dirs::home_dir().expect("home dir").join("Library/Logs/we-forge")
    }
    pub fn linux_systemd_user_dir() -> PathBuf {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| dirs::home_dir().expect("home dir").join(".config"))
            .join("systemd/user")
    }
    pub fn linux_state_dir() -> PathBuf {
        std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| dirs::home_dir().expect("home dir").join(".local/state"))
            .join("we-forge")
    }
}

// ---------------------------------------------------------------------------
// Config (~/.we-forge/config.json)
// ---------------------------------------------------------------------------

pub mod config {
    use super::*;

    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    #[serde(default)]
    pub struct Config {
        pub mode:             String, // "scheduled" | "daemon"
        pub installed_at:     String, // ISO-8601 UTC
        pub telegram_enabled: bool,
        pub telegram_token:   String,
        pub telegram_chat_id: String,
    }

    pub fn load() -> Config {
        let path = paths::config_file();
        if !path.exists() {
            return Config::default();
        }
        let bytes = match fs::read_to_string(&path) {
            Ok(s)  => s,
            Err(_) => return Config::default(),
        };
        serde_json::from_str(&bytes).unwrap_or_default()
    }

    pub fn save(cfg: &Config) -> Result<()> {
        let dir = paths::we_forge_home();
        fs::create_dir_all(&dir).context("create ~/.we-forge")?;
        let path = paths::config_file();
        atomic_write(&path, serde_json::to_string_pretty(cfg)?.as_bytes(), 0o600)?;
        Ok(())
    }

    pub fn with_env_overrides(mut cfg: Config) -> Config {
        if let Ok(t) = std::env::var("WE_FORGE_TELEGRAM_TOKEN") {
            if !t.is_empty() { cfg.telegram_token = t; }
        }
        if let Ok(c) = std::env::var("WE_FORGE_TELEGRAM_CHAT_ID") {
            if !c.is_empty() { cfg.telegram_chat_id = c; }
        }
        cfg
    }
}

// ---------------------------------------------------------------------------
// ECC trace ledger (~/.we-forge/ecc-trace.jsonl, append-only)
// ---------------------------------------------------------------------------

pub mod ecc {
    use super::*;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct EccRecord {
        pub ts:      String,
        pub skill:   String,
        pub reason:  String,
        pub invoker: String,
    }

    pub fn log(skill: &str, reason: &str, invoker: &str) -> Result<()> {
        let path = paths::ecc_trace_file();
        fs::create_dir_all(path.parent().unwrap()).context("create ~/.we-forge")?;
        let rec = EccRecord {
            ts:      now_iso(),
            skill:   skill.trim().to_string(),
            reason:  reason.trim().chars().take(200).collect(),
            invoker: invoker.trim().to_string(),
        };
        let line = serde_json::to_string(&rec)? + "\n";
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .context("open ecc-trace.jsonl")?;
        f.write_all(line.as_bytes())?;
        Ok(())
    }

    pub fn read_all() -> Vec<EccRecord> {
        let path = paths::ecc_trace_file();
        if !path.exists() {
            return Vec::new();
        }
        let text = match fs::read_to_string(&path) {
            Ok(s)  => s,
            Err(_) => return Vec::new(),
        };
        text.lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Atomic file write (tmp + fsync + rename, mode applied at create_new)
// ---------------------------------------------------------------------------

pub fn atomic_write(path: &std::path::Path, content: &[u8], mode: u32) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("no parent dir for {}", path.display()))?;
    fs::create_dir_all(parent).context("create parent dir")?;

    let tmp = path.with_extension(format!(
        "{}.tmp",
        path.extension().and_then(|s| s.to_str()).unwrap_or("dat")
    ));
    // Clean up any stale tmp from a crashed previous attempt.
    let _ = fs::remove_file(&tmp);

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(mode)
            .open(&tmp)
            .with_context(|| format!("create_new {}", tmp.display()))?;
        f.write_all(content)?;
        let _ = f.sync_all();
    }
    #[cfg(not(unix))]
    {
        let _ = mode;
        let mut f = fs::File::create(&tmp)?;
        f.write_all(content)?;
        let _ = f.sync_all();
    }

    fs::rename(&tmp, path).context("rename tmp to final")?;
    Ok(())
}
