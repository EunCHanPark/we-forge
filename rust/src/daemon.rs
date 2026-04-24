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

// Daemon interval now resolved per-iteration via config::interval_seconds()
// for hot-reload. The Python CLI ships `set-interval` to mutate it.

/// Async daemon entry point. Runs forever (or until Ctrl-C).
pub async fn run() -> Result<()> {
    let pid = std::process::id();
    let cfg = config::with_env_overrides(config::load());
    let initial_secs = config::interval_seconds(&cfg);
    println!("[{}] we-forge daemon starting (pid={pid}, interval={initial_secs}s = {}min)",
             now_iso(), initial_secs / 60);

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

    // Tick scheduling — aligned to local 00:00 (midnight) every interval slots.
    // For interval=720min: slots at 00:00 and 12:00.
    // For interval=30min:  slots at 00:00, 00:30, ..., 23:30.
    let mut current_secs = initial_secs;
    let mut next_tick_at = config::next_aligned_tick_time(current_secs, chrono::Local::now());
    println!("[{}] next aligned tick: {}",
             now_iso(),
             next_tick_at.format("%Y-%m-%d %H:%M %Z"));

    // Telegram poll offset
    let mut update_offset: i64 = 0;

    // Ctrl-C handler: graceful shutdown — pinned future so we can poll
    // repeatedly via &mut in select!
    let sigterm_fut = tokio::signal::ctrl_c();
    tokio::pin!(sigterm_fut);

    loop {
        // Hot-reload interval from config (catches `set-interval` changes).
        let cfg_now = config::with_env_overrides(config::load());
        let want_secs = config::interval_seconds(&cfg_now);
        if want_secs != current_secs {
            current_secs = want_secs;
            next_tick_at = config::next_aligned_tick_time(current_secs, chrono::Local::now());
            println!("[{}] interval changed → {}min, next aligned tick: {}",
                     now_iso(), current_secs / 60,
                     next_tick_at.format("%H:%M"));
        }

        let now_local = chrono::Local::now();
        let until_next = (next_tick_at - now_local).num_seconds().max(1) as u64;
        let tick_sleep = tokio::time::sleep(Duration::from_secs(until_next.min(60)));
        tokio::pin!(tick_sleep);

        tokio::select! {
            _ = &mut tick_sleep => {
                if chrono::Local::now() >= next_tick_at {
                    println!("[{}] tick begin (aligned slot {})",
                             now_iso(), next_tick_at.format("%H:%M"));
                    tokio::spawn(async {
                        let rc = tick::run_once_async().await;
                        println!("[{}] tick end (rc={})", now_iso(), rc);
                    });
                    next_tick_at = config::next_aligned_tick_time(current_secs, chrono::Local::now());
                    println!("[{}] next aligned tick: {}",
                             now_iso(), next_tick_at.format("%H:%M"));
                }
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
            let trimmed = raw.trim();
            let mut parts = trimmed.splitn(2, char::is_whitespace);
            let head_raw = parts.next().unwrap_or("").to_lowercase();
            let head = head_raw.split('@').next().unwrap_or(&head_raw).to_string();
            let rest = parts.next().unwrap_or("").trim().to_string();
            match head.as_str() {
                "/help" | "/start" => help_text(),
                "/status" | "/health" => self.cmd_status(),
                "/skill_report" | "/report" => self.cmd_skill_report(),
                "/last_tick" | "/last" => cmd_last_tick(),
                "/dashboard" | "/dash" => cmd_dashboard(),
                "/ecc_trace" | "/ecc" => cmd_ecc_trace(),
                "/interval" => cmd_interval(),
                "/set_interval" | "/setinterval" => cmd_set_interval(&rest),
                _ => format!("unknown command: {}\nsend /help for the full list", head),
            }
        }

        fn cmd_status(&self) -> String {
            let s = crate::service::manager().status();
            let cfg = config::with_env_overrides(config::load());
            let sec = config::interval_seconds(&cfg);
            let next = config::next_aligned_tick_time(sec, chrono::Local::now());
            let head = format!(
                "we-forge status: {}\nmode: {}\ninterval: {} min  (next tick: {})\ntelegram: {}\n",
                s,
                if cfg.mode.is_empty() { "scheduled".into() } else { cfg.mode.clone() },
                sec / 60,
                next.format("%m/%d %H:%M"),
                if cfg.telegram_enabled { "enabled" } else { "disabled" },
            );
            format!("{}\n{}", head, format_active_sessions(60, 10))
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
         ▸ /interval\n  학습 + 알림 주기 조회 (현재 cadence)\n\n\
         ▸ /set_interval <분>\n  학습 + 알림 주기 설정 (1 ~ 1440)\n  - 예: /set_interval 30 → 30분마다\n  - 다음 daemon 사이클부터 자동 적용\n\n\
         ▸ /help\n  이 도움말\n\n\
         ──────────────────────────────\n\
         we-forge 는 무엇인가\n  · Claude Code 위에서 24/7 패턴 학습 데몬\n  · 사용자 작업을 관찰하고 ECC 마켓플레이스 스킬과 매칭\n  · 매칭 시 추천, 미매칭 시 신규 합성".to_string()
    }

    /// Recover original filesystem path from Claude Code's encoded project
    /// directory name. Tries combinations of '-'/'/'  separators and prefers
    /// ones that exist on disk to disambiguate dashes in directory names.
    fn decode_project_path(encoded: &str) -> String {
        let trimmed = encoded.trim_start_matches('-');
        let parts: Vec<&str> = trimmed.split('-').collect();
        let n = parts.len();
        if n <= 1 {
            return format!("/{}", trimmed);
        }
        // Iterate from "most dashes preserved" to "most slashes" so we find
        // the deepest-existing path first.
        for popcount in (0..=(n - 1)).rev() {
            let combos = 1u32 << (n - 1);
            for mask in 0..combos {
                if (mask as u32).count_ones() as usize != popcount {
                    continue;
                }
                let mut path = String::from("/");
                path.push_str(parts[0]);
                for i in 0..(n - 1) {
                    let sep = if (mask >> i) & 1 == 1 { '-' } else { '/' };
                    path.push(sep);
                    path.push_str(parts[i + 1]);
                }
                if std::path::Path::new(&path).exists() {
                    return path;
                }
            }
        }
        format!("/{}", parts.join("/"))
    }

    /// List Claude Code sessions with transcript activity in the window.
    fn format_active_sessions(window_min: u64, max_show: usize) -> String {
        let projects = paths::claude_home().join("projects");
        if !projects.exists() {
            return "active sessions: (no projects/ directory yet)".to_string();
        }
        let now = std::time::SystemTime::now();
        let cutoff = now
            .checked_sub(std::time::Duration::from_secs(window_min * 60))
            .unwrap_or(std::time::UNIX_EPOCH);

        let mut rows: Vec<(std::time::SystemTime, String, String)> = Vec::new();
        let entries = match std::fs::read_dir(&projects) {
            Ok(e) => e,
            Err(_) => return "active sessions: (cannot read projects/)".to_string(),
        };
        for entry in entries.flatten() {
            let proj_path = entry.path();
            if !proj_path.is_dir() {
                continue;
            }
            let encoded_name = match entry.file_name().to_str() {
                Some(s) => s.to_string(),
                None => continue,
            };
            let decoded = decode_project_path(&encoded_name);
            let txs = match std::fs::read_dir(&proj_path) {
                Ok(t) => t,
                Err(_) => continue,
            };
            for tx in txs.flatten() {
                let p = tx.path();
                if p.extension().and_then(|s| s.to_str()) != Some("jsonl") {
                    continue;
                }
                let mtime = match p.metadata().and_then(|m| m.modified()) {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                if mtime < cutoff {
                    continue;
                }
                let sid = p.file_stem().and_then(|s| s.to_str()).unwrap_or("?").to_string();
                rows.push((mtime, sid, decoded.clone()));
            }
        }
        rows.sort_by(|a, b| b.0.cmp(&a.0));

        if rows.is_empty() {
            return format!("active sessions (last {window_min}min): (none — all idle)");
        }
        let mut out = format!("active sessions (last {window_min}min, {} total):", rows.len());
        for (mtime, sid, path) in rows.iter().take(max_show) {
            let age_secs = now.duration_since(*mtime).map(|d| d.as_secs()).unwrap_or(0);
            let age_min = age_secs / 60;
            let mark = if age_min < 5 { "⚡" } else if age_min < 30 { "🕐" } else { "💤" };
            let dt: chrono::DateTime<chrono::Local> = (*mtime).into();
            let ts = dt.format("%H:%M");
            let short_path: String = if path.chars().count() <= 45 {
                path.clone()
            } else {
                let suffix: String = path.chars().rev().take(44).collect::<String>().chars().rev().collect();
                format!("…{}", suffix)
            };
            let sid_short = sid.chars().take(8).collect::<String>();
            out.push_str(&format!("\n  {mark} {sid_short} {ts} ({age_min}m) {short_path}"));
        }
        if rows.len() > max_show {
            out.push_str(&format!("\n  … ({} more)", rows.len() - max_show));
        }
        out
    }

    fn cmd_interval() -> String {
        let cfg = config::with_env_overrides(config::load());
        let sec = config::interval_seconds(&cfg);
        let line2 = if cfg.interval_minutes > 0 {
            format!("  설정값: {}분  (config.json)", cfg.interval_minutes)
        } else {
            format!("  설정값: 미지정 → 기본값 {}분 사용 중", config::DEFAULT_INTERVAL_MIN)
        };
        let next = config::next_aligned_tick_time(sec, chrono::Local::now());
        format!(
            "we-forge cadence\n\
             ══════════════════════════════\n\
               현재: {}분  ({}초)\n\
             {}\n\
               다음 발화: {}  (로컬 00:00 기준 정렬)\n\
               의미: tick(학습) + telegram 알림이 같은 주기로 발화\n\n\
             변경: /set_interval <분>  (예: /set_interval 30)\n\
             범위: 1 ~ 1440 (1분 ~ 24시간)\n\
             참고: 모든 슬롯은 자정(00:00) 기준 정렬됨",
            sec / 60, sec, line2,
            next.format("%Y-%m-%d %H:%M"),
        )
    }

    fn cmd_set_interval(arg: &str) -> String {
        if arg.is_empty() {
            return "사용법: /set_interval <분>\n\
                    예: /set_interval 30   (30분마다)\n    \
                        /set_interval 60   (1시간마다, 기본값)\n    \
                        /set_interval 5    (5분마다, 빈번)\n\
                    범위: 1 ~ 1440 (1분 ~ 24시간)\n\
                    현재값 조회: /interval".to_string();
        }
        let first = arg.split_whitespace().next().unwrap_or("");
        let minutes: u32 = match first.parse() {
            Ok(n) => n,
            Err(_) => return format!("잘못된 입력: '{}'\n정수(분)를 입력하세요. 예: /set_interval 30", first),
        };
        if minutes < 1 || minutes > 1440 {
            return format!("범위 초과: {}분\n허용 범위: 1 ~ 1440 (1분 ~ 24시간)", minutes);
        }
        let mut cfg = config::with_env_overrides(config::load());
        let old = if cfg.interval_minutes == 0 { config::DEFAULT_INTERVAL_MIN } else { cfg.interval_minutes };
        cfg.interval_minutes = minutes;
        if let Err(e) = config::save(&cfg) {
            return format!("config 저장 실패: {}", e);
        }
        let _ = std::fs::remove_file(paths::we_forge_home().join("last_telegram_sent_at"));
        let _ = std::fs::remove_file(paths::we_forge_home().join("telegram_pending.jsonl"));
        let _ = ecc::log("enterprise-agent-ops",
            &format!("interval changed via telegram bot ({}→{} min)", old, minutes), "bot");
        format!(
            "⚙️ interval 변경 완료\n\
             ══════════════════════════════\n  \
               이전: {}분\n  \
               현재: {}분\n  \
               적용: 다음 daemon iteration (최대 60초 내, 재시작 불필요)\n  \
               효과: tick {}분마다 · telegram 알림 {}분마다",
            old, minutes, minutes, minutes,
        )
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
