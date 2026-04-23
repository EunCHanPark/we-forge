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
    }
}
