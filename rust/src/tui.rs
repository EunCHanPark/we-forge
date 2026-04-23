//! ratatui-powered terminal UI — stub.
//!
//! ECC alignment: dashboard-builder (operator-question UI) +
//! enterprise-agent-ops (start/stop/restart actions).
//!
//! Full impl in next session: ports the cokacctl menu pattern
//! ([s] start [t] stop [r] restart [d] disable ...) using ratatui +
//! crossterm event loop. For now, prints the menu as text and exits.

use crate::core::{config, ecc};
use crate::service;
use anyhow::Result;

pub fn run() -> Result<()> {
    let _ = ecc::log("dashboard-builder", "TUI launched (Rust scaffold)", "cli");
    let cfg = config::with_env_overrides(config::load());
    let s   = service::manager().status();
    println!("we-forge control (Rust scaffold)");
    println!("  status:   {}", s);
    println!("  mode:     {}", if cfg.mode.is_empty() { "scheduled".into() } else { cfg.mode });
    println!("  telegram: {}", if cfg.telegram_enabled { "enabled" } else { "disabled" });
    println!();
    println!("  TODO: ratatui live UI — see scripts/we-forgectl tui (Python+rich) for now");
    println!();
    println!("  available subcommands:");
    println!("    we-forgectl install [--enable-telegram] [--daemon]");
    println!("    we-forgectl start | stop | restart | status");
    println!("    we-forgectl logs | doctor | dashboard");
    println!("    we-forgectl ecc-trace [--last N] [--group]");
    println!("    we-forgectl uninstall [--deep]");
    Ok(())
}
