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
