//! Long-running daemon loop + tick subprocess wrapper.
//!
//! ECC alignment:
//!   - daemon::run         → autonomous-agent-harness + continuous-agent-loop
//!   - daemon::tick        → continuous-agent-loop
//!   - daemon::telegram    → messages-ops
//!
//! NOTE: This is the v0.3.0 scaffold. The async loop, tick subprocess, and
//! Telegram client are stubs that compile but don't yet match the Python
//! implementation feature-for-feature. Next session will port the full
//! daemon_loop() from scripts/we-forgectl.

use crate::core::{ecc, now_iso, paths};
use anyhow::Result;

/// Async daemon entry point. Currently a placeholder that just logs.
pub async fn run() -> Result<()> {
    println!("[{}] we-forge daemon (Rust scaffold) starting", now_iso());
    let _ = ecc::log(
        "autonomous-agent-harness",
        "daemon started (Rust scaffold v0.3.0-dev)",
        "daemon",
    );
    println!("[{}] TODO: port daemon_loop() from scripts/we-forgectl (Python)", now_iso());
    println!("[{}] meanwhile use the Python version for production", now_iso());
    let _ = paths::daemon_pid(); // touch to keep import live
    Ok(())
}

pub mod tick {
    use super::*;
    use std::process::Command;

    /// Run tick.sh once. Same behavior as `bash ~/.claude/learning/tick.sh`.
    pub fn run_once() -> Result<()> {
        let script = paths::claude_home().join("learning/tick.sh");
        if !script.exists() {
            return Err(anyhow::anyhow!("tick.sh not found: {}", script.display()));
        }
        let _ = ecc::log("continuous-agent-loop", "tick run via we-forgectl run-once", "cli");
        let status = Command::new("bash").arg(&script).status()?;
        if !status.success() {
            return Err(anyhow::anyhow!("tick.sh exited with {}", status.code().unwrap_or(-1)));
        }
        Ok(())
    }
}

pub mod telegram {
    //! Telegram Bot API client — stub for now. Full impl ports from
    //! scripts/we-forgectl::TelegramNotifier in next session (uses reqwest).
}
