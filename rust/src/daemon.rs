//! Long-running async daemon loop + tick subprocess + Telegram client.
//!
//! ECC alignment:
//!   - daemon::run         → autonomous-agent-harness + continuous-agent-loop
//!   - daemon::tick        → continuous-agent-loop
//!   - daemon::telegram    → messages-ops + rust-patterns
//!
//! Design notes:
//!   - tokio::select! runs tick interval + telegram polling in parallel,
//!     so a long tick (claude --agent up to 10min) never blocks the bot
//!   - tick spawned via tokio::process::Command (non-blocking)
//!   - println! works for daemon.log because launchd redirects stdout to
//!     a file with line-buffering when stdout is detected as non-tty
//!   - Telegram TelegramNotifier matches Python's plain-text default +
//!     html opt-in + auto-fallback on parse error

use crate::core::{config, ecc, now_iso, paths};
use anyhow::Result;
use serde::Deserialize;
use std::time::Duration;

const DAEMON_INTERVAL_SECS: u64 = 300; // 5 min

/// Async daemon entry point. Runs forever (or until Ctrl-C).
pub async fn run() -> Result<()> {
    let pid = std::process::id();
    println!("[{}] we-forge daemon starting (pid={pid}, interval={DAEMON_INTERVAL_SECS}s)",
             now_iso());

    let cfg = config::with_env_overrides(config::load());
    let notifier = if cfg.telegram_enabled
        && !cfg.telegram_token.is_empty()
        && !cfg.telegram_chat_id.is_empty()
    {
        println!("[{}] Telegram notifier enabled", now_iso());
        let n = telegram::TelegramNotifier::new(&cfg.telegram_token, &cfg.telegram_chat_id);
        let _ = n.send("we-forge daemon started (Rust)", false).await;
        let _ = ecc::log("messages-ops", "Telegram notifier active (Rust daemon)", "daemon");
        Some(n)
    } else {
        None
    };

    let _ = ecc::log("autonomous-agent-harness",
                     "daemon loop started (tokio::select! parallel tick + telegram)",
                     "daemon");

    // Initialize last_tick to NOW so first iteration polls Telegram (if enabled)
    // immediately instead of running a 10-min-blocking tick on startup.
    let mut last_tick_at = std::time::Instant::now();

    // Tick scheduling via interval (next firing in DAEMON_INTERVAL_SECS).
    let mut tick_interval = tokio::time::interval(Duration::from_secs(DAEMON_INTERVAL_SECS));
    tick_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    tick_interval.tick().await; // burn the immediate-fire tick

    // Telegram poll offset
    let mut update_offset: i64 = 0;

    // Ctrl-C handler: graceful shutdown — pinned future so we can poll
    // repeatedly via &mut in select!
    let sigterm_fut = tokio::signal::ctrl_c();
    tokio::pin!(sigterm_fut);

    loop {
        tokio::select! {
            _ = tick_interval.tick() => {
                println!("[{}] tick begin", now_iso());
                tokio::spawn(async {
                    let rc = tick::run_once_async().await;
                    println!("[{}] tick end (rc={})", now_iso(), rc);
                });
                last_tick_at = std::time::Instant::now();
            }

            updates = telegram::poll_if_enabled(&notifier, update_offset) => {
                if let Some((new_offset, batch)) = updates {
                    update_offset = new_offset;
                    if let Some(n) = notifier.as_ref() {
                        for upd in batch {
                            if let Some(msg) = upd.message {
                                let chat_id = msg.chat.id.to_string();
                                if chat_id != n.chat_id {
                                    continue; // not our chat
                                }
                                if let Some(text) = msg.text {
                                    if text.starts_with('/') {
                                        println!("[{}] handling command: {}", now_iso(), text);
                                        let reply = n.handle_command(&text);
                                        let _ = n.send(&reply, false).await;
                                    }
                                }
                            }
                        }
                    }
                }
            }

            _ = &mut sigterm_fut => {
                println!("[{}] daemon stopped (SIGINT/SIGTERM)", now_iso());
                if let Some(n) = notifier.as_ref() {
                    let _ = n.send("we-forge daemon stopped", false).await;
                }
                let _ = std::fs::remove_file(paths::daemon_pid());
                let _ = last_tick_at; // suppress unused warn
                return Ok(());
            }
        }
    }
}

// ----------------------------------------------------------------------------
// Tick subprocess
// ----------------------------------------------------------------------------

pub mod tick {
    use super::*;

    /// Sync entry — for `we-forgectl run-once` CLI subcommand.
    pub fn run_once() -> Result<()> {
        let script = paths::claude_home().join("learning/tick.sh");
        if !script.exists() {
            return Err(anyhow::anyhow!("tick.sh not found: {}", script.display()));
        }
        let _ = ecc::log("continuous-agent-loop", "tick run via we-forgectl run-once", "cli");
        let status = std::process::Command::new("bash").arg(&script).status()?;
        if !status.success() {
            return Err(anyhow::anyhow!("tick.sh exited with {}", status.code().unwrap_or(-1)));
        }
        Ok(())
    }

    /// Async entry — for daemon loop.
    pub async fn run_once_async() -> i32 {
        let script = paths::claude_home().join("learning/tick.sh");
        if !script.exists() {
            eprintln!("[{}] tick.sh not found: {}", now_iso(), script.display());
            return 127;
        }
        let _ = ecc::log("continuous-agent-loop", "tick run by daemon", "daemon");
        match tokio::process::Command::new("bash")
            .arg(&script)
            .status()
            .await
        {
            Ok(s) => s.code().unwrap_or(-1),
            Err(e) => {
                eprintln!("[{}] tick subprocess failed: {}", now_iso(), e);
                -1
            }
        }
    }
}

// ----------------------------------------------------------------------------
// Telegram client (reqwest-based, matches Python TelegramNotifier behavior)
// ----------------------------------------------------------------------------

pub mod telegram {
    use super::*;

    pub struct TelegramNotifier {
        pub api:      String,
        pub chat_id:  String,
        client:       reqwest::Client,
    }

    #[derive(Debug, Deserialize)]
    pub struct Update {
        pub update_id: i64,
        #[serde(default)]
        pub message:   Option<Message>,
    }

    #[derive(Debug, Deserialize)]
    pub struct Message {
        pub chat: Chat,
        #[serde(default)]
        pub text: Option<String>,
    }

    #[derive(Debug, Deserialize)]
    pub struct Chat {
        pub id: i64,
    }

    #[derive(Debug, Deserialize)]
    struct UpdatesResponse {
        ok: bool,
        #[serde(default)]
        result: Vec<Update>,
    }

    impl TelegramNotifier {
        pub fn new(token: &str, chat_id: &str) -> Self {
            Self {
                api: format!("https://api.telegram.org/bot{}", token),
                chat_id: chat_id.to_string(),
                client: reqwest::Client::builder()
                    .timeout(Duration::from_secs(35))
                    .build()
                    .expect("build reqwest client"),
            }
        }

        /// Send a message. Default plain text (always works).
        /// html=true for HTML parse_mode with auto-fallback to plain on 400.
        pub async fn send(&self, text: &str, html: bool) -> bool {
            let truncated: String = text.chars().take(4000).collect();
            let mut params = vec![
                ("chat_id", self.chat_id.as_str()),
                ("text",    truncated.as_str()),
            ];
            if html {
                params.push(("parse_mode", "HTML"));
            }
            let url = format!("{}/sendMessage", self.api);
            match self.client.post(&url).form(&params).send().await {
                Ok(resp) if resp.status().is_success() => true,
                Ok(resp) => {
                    if html {
                        // Retry plain
                        let plain = vec![
                            ("chat_id", self.chat_id.as_str()),
                            ("text",    truncated.as_str()),
                        ];
                        match self.client.post(&url).form(&plain).send().await {
                            Ok(r2) => r2.status().is_success(),
                            Err(_) => false,
                        }
                    } else {
                        eprintln!("[{}] telegram send failed: HTTP {}",
                                  now_iso(), resp.status());
                        false
                    }
                }
                Err(e) => {
                    eprintln!("[{}] telegram send failed: {}", now_iso(), e);
                    false
                }
            }
        }

        /// Long-poll getUpdates. Returns (new_offset, updates).
        pub async fn poll(&self, offset: i64) -> Option<(i64, Vec<Update>)> {
            let timeout_secs = 25;
            let url = format!(
                "{}/getUpdates?offset={}&timeout={}&allowed_updates=%5B%22message%22%5D",
                self.api, offset + 1, timeout_secs
            );
            match self.client.get(&url).send().await {
                Ok(resp) => {
                    let body: UpdatesResponse = match resp.json().await {
                        Ok(b) => b,
                        Err(e) => {
                            eprintln!("[{}] telegram poll decode: {}", now_iso(), e);
                            return None;
                        }
                    };
                    if !body.ok {
                        return None;
                    }
                    let new_offset = body.result.iter()
                        .map(|u| u.update_id)
                        .max()
                        .unwrap_or(offset);
                    Some((new_offset, body.result))
                }
                Err(e) => {
                    eprintln!("[{}] telegram poll: {}", now_iso(), e);
                    None
                }
            }
        }

        /// Dispatch /commands — matches Python word-for-word.
        pub fn handle_command(&self, raw: &str) -> String {
            let cmd = raw.trim().to_lowercase();
            let cmd = cmd.split('@').next().unwrap_or(&cmd);
            match cmd {
                "/help" | "/start" => help_text(),
                "/status" | "/health" => self.cmd_status(),
                "/skill_report" | "/report" => self.cmd_skill_report(),
                "/last_tick" | "/last" => cmd_last_tick(),
                "/dashboard" | "/dash" => cmd_dashboard(),
                "/ecc_trace" | "/ecc" => cmd_ecc_trace(),
                _ => format!("unknown command: {}\nsend /help for the full list", cmd),
            }
        }

        fn cmd_status(&self) -> String {
            let s = crate::service::manager().status();
            let cfg = config::with_env_overrides(config::load());
            format!(
                "we-forge status: {}\nmode: {}\ntelegram: {}",
                s,
                if cfg.mode.is_empty() { "scheduled".into() } else { cfg.mode },
                if cfg.telegram_enabled { "enabled" } else { "disabled" },
            )
        }

        fn cmd_skill_report(&self) -> String {
            // Shell out to dashboard.py --once and capture stdout (simplest port).
            let candidates = [
                std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("../dashboard/dashboard.py"))),
                Some(dirs::home_dir().unwrap().join("we-forge/dashboard/dashboard.py")),
            ];
            for path in candidates.iter().flatten() {
                if path.exists() {
                    if let Ok(out) = std::process::Command::new("python3")
                        .arg(path).arg("--once").output()
                    {
                        if out.status.success() {
                            let s = String::from_utf8_lossy(&out.stdout).to_string();
                            return s.chars().take(3500).collect();
                        }
                    }
                }
            }
            "skill_report failed: dashboard.py not found".to_string()
        }
    }

    fn help_text() -> String {
        "we-forge 봇 명령어 안내\n\
         ══════════════════════════════\n\n\
         ▸ /status\n  서비스 가동 상태 확인\n  (running/stopped, scheduled/daemon, telegram on/off)\n\n\
         ▸ /skill_report\n  학습 KPI 요약\n  - 누적 events / patterns / queue / ledger\n  - ECC_MATCH 비율 (마켓플레이스 활용도)\n  - TOP 5 자주 하는 패턴\n\n\
         ▸ /last_tick\n  최근 tick(학습 사이클) 로그 마지막 15줄\n\n\
         ▸ /ecc_trace\n  ECC 마켓플레이스 스킬 사용 통계\n\n\
         ▸ /dashboard\n  웹 대시보드 접속 안내\n\n\
         ▸ /help\n  이 도움말\n\n\
         ──────────────────────────────\n\
         we-forge 는 무엇인가\n  · Claude Code 위에서 24/7 패턴 학습 데몬\n  · 사용자 작업을 관찰하고 ECC 마켓플레이스 스킬과 매칭\n  · 매칭 시 추천, 미매칭 시 신규 합성".to_string()
    }

    fn cmd_last_tick() -> String {
        let log = paths::claude_home().join("learning/data/tick.log");
        if !log.exists() {
            return "no tick.log yet".to_string();
        }
        match std::fs::read_to_string(&log) {
            Ok(text) => {
                let lines: Vec<&str> = text.lines().collect();
                let start = lines.len().saturating_sub(15);
                let tail = lines[start..].join("\n");
                format!("last tick log:\n{tail}")
            }
            Err(e) => format!("read failed: {e}"),
        }
    }

    fn cmd_dashboard() -> String {
        "dashboard: we-forgectl dashboard (web on http://127.0.0.1:8765)\nor: we-forgectl tui (terminal)".to_string()
    }

    fn cmd_ecc_trace() -> String {
        let entries = ecc::read_all();
        if entries.is_empty() {
            return "no ECC trace yet — agent will populate as it leverages skills".to_string();
        }
        let mut counter: std::collections::BTreeMap<String, usize> = Default::default();
        for e in &entries { *counter.entry(e.skill.clone()).or_insert(0) += 1; }
        let mut sorted: Vec<_> = counter.into_iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(&a.1));
        let mut s = format!("ECC skill usage ({} records)\n══════════════════════════════\n", entries.len());
        for (skill, n) in sorted.iter().take(15) {
            s.push_str(&format!("  {n:>3}  {skill}\n"));
        }
        s
    }

    /// Helper for tokio::select! — only polls if notifier is Some.
    /// Returns None to make the future never resolve when notifier is None.
    pub async fn poll_if_enabled(
        notifier: &Option<TelegramNotifier>,
        offset: i64,
    ) -> Option<(i64, Vec<Update>)> {
        match notifier.as_ref() {
            Some(n) => n.poll(offset).await,
            None    => {
                // Sleep forever — let other branches of select! drive
                tokio::time::sleep(Duration::from_secs(3600)).await;
                None
            }
        }
    }
}
