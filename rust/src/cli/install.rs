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
