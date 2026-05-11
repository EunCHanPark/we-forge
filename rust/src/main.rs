//! we-forgectl — we-forge service & operations control
//!
//! Rust port of the Python single-file CLI. See `Cargo.toml` for the ECC
//! marketplace skill alignment that drives the architecture.
//!
//! Binary entry point. Parses CLI args via clap, dispatches to the
//! appropriate subcommand handler in `cli::`, and propagates errors via
//! `anyhow::Result`.

mod cli;
mod core;
mod daemon;
mod service;
mod tui;

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "we-forgectl",
    version,
    about = "we-forge service & operations control"
)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Register the launchd / systemd / Task Scheduler service.
    Install {
        /// Opt-in Telegram notifier (forces daemon mode).
        #[arg(long)]
        enable_telegram: bool,
        /// Long-running daemon mode (KeepAlive=true / Restart=always).
        #[arg(long)]
        daemon: bool,
    },

    /// Remove the service (with safety backup).
    Uninstall {
        /// Also move ~/.we-forge and learning/data to backup.
        #[arg(long)]
        deep: bool,
    },

    /// Start the service.
    Start,
    /// Stop the service.
    Stop,
    /// Restart the service.
    Restart,
    /// Show service status.
    Status,

    /// Set unified tick + telegram cadence (minutes, 1-1440).
    SetInterval {
        /// Cadence in minutes (1-1440).
        minutes: u32,
    },

    /// Long-running loop (called by service manager).
    Daemon,
    /// Run a single tick and exit (called by scheduled mode).
    RunOnce,

    /// rich-style terminal UI (ratatui).
    Tui,
    /// Launch the web dashboard (delegates to dashboard.py --serve).
    Dashboard,

    /// Send a Telegram test message.
    NotifyTest,
    /// Diagnose dependencies and service state.
    Doctor,

    /// Tail recent service logs.
    Logs {
        #[arg(short, default_value_t = 30)]
        n: usize,
    },

    /// Record one ECC marketplace skill usage.
    EccLog {
        /// ECC skill slug (e.g. autonomous-agent-harness)
        skill: String,
        /// Short reason (why this skill was leveraged)
        #[arg(default_value = "")]
        reason: String,
        /// Who logged it (cli/agent/tick/bot)
        #[arg(long, default_value = "cli")]
        invoker: String,
    },

    /// Show ECC skill usage history.
    EccTrace {
        #[arg(long, default_value_t = 20)]
        last: usize,
        /// Display totals per skill instead of timeline.
        #[arg(long)]
        group: bool,
    },

    /// List active Claude Code sessions detected via transcript activity + heartbeats.
    Sessions {
        /// Window in minutes to consider a session active.
        #[arg(long, default_value_t = 60)]
        window: i64,
    },

    /// Register the current session (heartbeat fallback when transcript not detected).
    Ping {
        /// Optional label shown in `we-forgectl sessions`.
        #[arg(default_value = "")]
        label: String,
    },

    /// Cross-validate patterns / ledger / rejected entries to surface pipeline gaps.
    Audit {
        /// Show top-N qualifying patterns.
        #[arg(long, default_value_t = 30)]
        top: usize,
    },

    /// Inspect ECC_MATCH match_score distribution; flag low-confidence matches.
    EccQuality {
        /// Score below this is treated as a REVISE downgrade candidate.
        #[arg(long, default_value_t = 3)]
        threshold: i64,
    },

    /// Suggest ECC marketplace skills for a prompt (used by UserPromptSubmit hook).
    SkillSuggest {
        /// Prompt text (positional). Pass empty/short prompts → silent skip.
        #[arg(default_value = "")]
        prompt: String,
        /// Top-N suggestions to emit.
        #[arg(long, default_value_t = 3)]
        top: usize,
        /// Emit a system-reminder block (for UserPromptSubmit hook injection).
        #[arg(long)]
        inject: bool,
        /// Append the result to ~/.we-forge/skill-suggestions.jsonl + turns.jsonl.
        #[arg(long)]
        log: bool,
        /// Claude Code session id (passed by hook for join with turns.jsonl).
        #[arg(long, default_value = "")]
        session_id: String,
    },

    /// Show skill-suggest hit rate (suggested vs invoked) over a time window.
    SkillHits {
        /// Window in hours.
        #[arg(long, default_value_t = 24)]
        hours: i64,
    },
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = Cli::parse();

    match args.command {
        Cmd::Install {
            enable_telegram,
            daemon,
        } => cli::install::run(enable_telegram, daemon),
        Cmd::Uninstall { deep } => cli::uninstall::run(deep),
        Cmd::Start => cli::lifecycle::start(),
        Cmd::Stop => cli::lifecycle::stop(),
        Cmd::Restart => cli::lifecycle::restart(),
        Cmd::Status => cli::status::run(),
        Cmd::SetInterval { minutes } => cli::set_interval::run(minutes),
        Cmd::Daemon => {
            // Build a tokio runtime here so the rest of the CLI stays sync.
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?
                .block_on(daemon::run())
        }
        Cmd::RunOnce => daemon::tick::run_once(),
        Cmd::Tui => tui::run(),
        Cmd::Dashboard => cli::dashboard::run(),
        Cmd::NotifyTest => cli::notify_test::run(),
        Cmd::Doctor => cli::doctor::run(),
        Cmd::Logs { n } => cli::logs::run(n),
        Cmd::EccLog {
            skill,
            reason,
            invoker,
        } => cli::ecc::log(&skill, &reason, &invoker),
        Cmd::EccTrace { last, group } => cli::ecc::trace(last, group),
        Cmd::Sessions { window } => cli::sessions::run(window),
        Cmd::Ping { label } => cli::ping::run(&label),
        Cmd::Audit { top } => cli::audit::run(top),
        Cmd::EccQuality { threshold } => cli::ecc_quality::run(threshold),
        Cmd::SkillSuggest {
            prompt,
            top,
            inject,
            log,
            session_id,
        } => cli::skill_suggest::run(&prompt, top, inject, log, &session_id),
        Cmd::SkillHits { hours } => cli::skill_hits::run(hours),
    }
}
