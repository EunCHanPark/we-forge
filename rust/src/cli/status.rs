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
