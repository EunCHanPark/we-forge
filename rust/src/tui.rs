//! ratatui-powered live terminal UI for we-forge service control.
//!
//! ECC alignment:
//!   - dashboard-builder    → operator-question UI (status / ECC / ticks)
//!   - enterprise-agent-ops → start/stop/restart/install/uninstall actions
//!   - frontend-design      → distinct visual hierarchy (panels + key hints)
//!
//! Inspired by cokacctl menu pattern: [s]tart [t]op [r]estart [d]isable etc.

use crate::core::{config, ecc, paths};
use crate::service;
use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Row, Table},
    Terminal,
};
use std::io::{self, Stdout};
use std::time::{Duration, Instant};

pub fn run() -> Result<()> {
    let _ = ecc::log("dashboard-builder", "TUI launched (Rust ratatui)", "cli");

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = run_app(&mut terminal);

    // Cleanup terminal regardless of error
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    res
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    let mut last_refresh = Instant::now();
    let mut status_msg: Option<(String, Color)> = None;

    loop {
        let cfg = config::with_env_overrides(config::load());
        let svc_status = service::manager().status();
        let ecc_entries = ecc::read_all();

        terminal.draw(|f| {
            let area = f.area();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),  // header
                    Constraint::Length(8),  // status panel
                    Constraint::Min(6),     // ecc trace panel
                    Constraint::Length(7),  // controls panel
                    Constraint::Length(3),  // footer
                ])
                .split(area);

            // ── Header ──
            let header = Paragraph::new(Line::from(vec![
                Span::styled("we-forge control", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::raw("    "),
                Span::styled("(Rust ratatui)", Style::default().fg(Color::DarkGray)),
            ]))
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Cyan)));
            f.render_widget(header, chunks[0]);

            // ── Status panel ──
            let status_color = match svc_status {
                service::Status::Running       => Color::Green,
                service::Status::Stopped       => Color::Yellow,
                service::Status::NotInstalled  => Color::Red,
            };
            let mode_str = if cfg.mode.is_empty() { "scheduled".to_string() } else { cfg.mode.clone() };
            let tg_str = if cfg.telegram_enabled { "enabled" } else { "disabled" };

            let status_lines = vec![
                Line::from(vec![Span::raw("  status:   "), Span::styled(svc_status.to_string(), Style::default().fg(status_color).add_modifier(Modifier::BOLD))]),
                Line::from(vec![Span::raw("  mode:     "), Span::raw(mode_str)]),
                Line::from(vec![Span::raw("  telegram: "), Span::raw(tg_str.to_string())]),
                Line::from(vec![Span::raw("  config:   "), Span::styled(paths::config_file().display().to_string(), Style::default().fg(Color::DarkGray))]),
                Line::from(vec![Span::raw("  ECC trace: "), Span::styled(format!("{} records", ecc_entries.len()), Style::default().fg(Color::Magenta))]),
            ];
            f.render_widget(
                Paragraph::new(status_lines)
                    .block(Block::default().borders(Borders::ALL).title(" Status ").border_style(Style::default().fg(Color::Green))),
                chunks[1],
            );

            // ── ECC trace panel (top 10 by frequency) ──
            let mut counter: std::collections::BTreeMap<String, usize> = Default::default();
            for e in &ecc_entries { *counter.entry(e.skill.clone()).or_insert(0) += 1; }
            let mut sorted: Vec<_> = counter.into_iter().collect();
            sorted.sort_by(|a, b| b.1.cmp(&a.1));
            let rows: Vec<Row> = sorted.iter().take(10)
                .map(|(skill, n)| Row::new(vec![
                    format!("{n:>4}"),
                    skill.clone(),
                ]))
                .collect();
            let ecc_table = Table::new(rows, [Constraint::Length(6), Constraint::Min(20)])
                .header(Row::new(vec!["count", "ECC marketplace skill"]).style(Style::default().add_modifier(Modifier::BOLD)))
                .block(Block::default().borders(Borders::ALL).title(" ECC skill usage ").border_style(Style::default().fg(Color::Magenta)));
            f.render_widget(ecc_table, chunks[2]);

            // ── Controls ──
            let controls = vec![
                Line::from(vec![
                    Span::styled("[s]", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)), Span::raw(" Start    "),
                    Span::styled("[t]", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)), Span::raw(" Stop     "),
                    Span::styled("[r]", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)), Span::raw(" Restart  "),
                    Span::styled("[d]", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)), Span::raw(" Run tick now"),
                ]),
                Line::from(vec![
                    Span::styled("[i]", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),   Span::raw(" Install  "),
                    Span::styled("[u]", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),    Span::raw(" Uninstall"),
                    Span::styled("[v]", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),   Span::raw(" Doctor   "),
                    Span::styled("[m]", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),   Span::raw(" Dashboard"),
                ]),
                Line::from(vec![
                    Span::styled("[n]", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)), Span::raw(" Notify-test  "),
                    Span::styled("[q]", Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)),Span::raw(" Quit"),
                ]),
                Line::from(""),
                Line::from(match &status_msg {
                    Some((msg, color)) => Span::styled(format!("→ {msg}"), Style::default().fg(*color)),
                    None => Span::styled("→ (waiting for input)", Style::default().fg(Color::DarkGray)),
                }),
            ];
            f.render_widget(
                Paragraph::new(controls)
                    .block(Block::default().borders(Borders::ALL).title(" Controls ").border_style(Style::default().fg(Color::Yellow))),
                chunks[3],
            );

            // ── Footer ──
            let footer = Paragraph::new(Line::from(vec![
                Span::styled("auto-refresh 2s · keys are non-blocking · q to quit",
                    Style::default().fg(Color::DarkGray)),
            ]))
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::DarkGray)));
            f.render_widget(footer, chunks[4]);
        })?;

        // Poll for input (200ms timeout for responsiveness, refresh every 2s)
        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press { continue; }
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                    KeyCode::Char('s') => {
                        status_msg = action(|| service::manager().start(), "started");
                    }
                    KeyCode::Char('t') => {
                        status_msg = action(|| service::manager().stop(), "stopped");
                    }
                    KeyCode::Char('r') => {
                        status_msg = action(|| service::manager().restart(), "restarted");
                    }
                    KeyCode::Char('d') => {
                        // Run a tick in background (non-blocking)
                        std::thread::spawn(|| {
                            let _ = crate::daemon::tick::run_once();
                        });
                        status_msg = Some(("tick started in background".into(), Color::Cyan));
                    }
                    KeyCode::Char('i') => {
                        status_msg = action(|| service::manager().install(false), "install attempted (scheduled)");
                    }
                    KeyCode::Char('u') => {
                        status_msg = Some(("uninstall: run `we-forgectl uninstall` from CLI for safety prompt".into(), Color::Red));
                    }
                    KeyCode::Char('v') => {
                        status_msg = Some(("doctor: run `we-forgectl doctor` from CLI for full output".into(), Color::Cyan));
                    }
                    KeyCode::Char('m') => {
                        status_msg = Some(("dashboard: run `we-forgectl dashboard` from CLI".into(), Color::Cyan));
                    }
                    KeyCode::Char('n') => {
                        status_msg = Some(("notify-test: run `we-forgectl notify-test` from CLI".into(), Color::Magenta));
                    }
                    _ => {}
                }
            }
        }

        // Throttle refresh
        if last_refresh.elapsed() >= Duration::from_secs(2) {
            last_refresh = Instant::now();
        }
    }
}

fn action<F: FnOnce() -> Result<()>>(f: F, label: &str) -> Option<(String, Color)> {
    match f() {
        Ok(())  => Some((format!("{} OK", label), Color::Green)),
        Err(e)  => Some((format!("{} FAILED: {}", label, e), Color::Red)),
    }
}
