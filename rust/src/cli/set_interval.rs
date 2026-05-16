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
