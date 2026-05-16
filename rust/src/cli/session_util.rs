use super::paths;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Decode Claude Code's encoded project dir name back to a filesystem path.
/// Claude Code prepends '-' and replaces every '/' and '.' with '-'.
/// Greedy resolution using filesystem existence checks.
pub fn decode_project_path(encoded: &str) -> String {
    let stripped = encoded.trim_start_matches('-');
    // Fast-path 1: pure slash substitution
    let simple: String = std::iter::once('/').chain(stripped.replace('-', "/").chars()).collect();
    if PathBuf::from(&simple).exists() {
        return simple;
    }
    // Fast-path 2: '--' as '/.'
    let with_dot: String = std::iter::once('/')
        .chain(stripped.replace("--", "/.").replace('-', "/").chars())
        .collect();
    if PathBuf::from(&with_dot).exists() {
        return with_dot;
    }
    // Greedy walk
    let raw: Vec<&str> = stripped.split('-').collect();
    let mut segments: Vec<String> = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        if raw[i].is_empty() && i + 1 < raw.len() {
            segments.push(format!(".{}", raw[i + 1]));
            i += 2;
        } else {
            if !raw[i].is_empty() {
                segments.push(raw[i].to_string());
            }
            i += 1;
        }
    }
    if segments.is_empty() {
        return if !with_dot.is_empty() { with_dot } else { simple };
    }
    let mut path = format!("/{}", segments[0]);
    for seg in &segments[1..] {
        let slash_try = format!("{}/{}", path, seg);
        let dash_try  = format!("{}-{}", path, seg);
        if PathBuf::from(&slash_try).exists() {
            path = slash_try;
        } else if PathBuf::from(&dash_try).exists() {
            path = dash_try;
        } else {
            path = slash_try;
        }
    }
    path
}

pub struct ActiveRow {
    pub mtime_secs: u64,
    pub label: String,
    pub path: String,
}

/// Read heartbeat files newer than `cutoff_secs` (epoch seconds), pruning expired ones.
pub fn read_heartbeats(cutoff_secs: u64) -> Vec<ActiveRow> {
    let dir = paths::heartbeats_dir();
    let mut rows = Vec::new();
    let entries = match fs::read_dir(&dir) {
        Ok(e)  => e,
        Err(_) => return rows,
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let txt = match fs::read_to_string(&p) {
            Ok(s)  => s,
            Err(_) => continue,
        };
        let v: serde_json::Value = match serde_json::from_str(&txt) {
            Ok(v)  => v,
            Err(_) => continue,
        };
        let epoch = v.get("epoch").and_then(|x| x.as_f64())
            .unwrap_or_else(|| {
                fs::metadata(&p).ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as f64)
                    .unwrap_or(0.0)
            }) as u64;
        if epoch < cutoff_secs {
            let _ = fs::remove_file(&p);
            continue;
        }
        let cwd = v.get("cwd").and_then(|x| x.as_str()).unwrap_or("?").to_string();
        let label = v.get("label").and_then(|x| x.as_str()).filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                let pid = v.get("pid").and_then(|x| x.as_i64()).unwrap_or(0);
                format!("pid={pid}")
            });
        rows.push(ActiveRow { mtime_secs: epoch, label, path: cwd });
    }
    rows
}

/// Format active sessions block: transcript scan + heartbeat fallback.
pub fn format_active(window_min: i64, max_show: usize) -> String {
    use std::collections::HashSet;
    let now_secs = SystemTime::now().duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs()).unwrap_or(0);
    let cutoff = now_secs.saturating_sub((window_min.max(0) as u64) * 60);

    let mut rows: Vec<ActiveRow> = Vec::new();
    let projects = paths::claude_home().join("projects");
    if let Ok(it) = fs::read_dir(&projects) {
        for proj in it.flatten() {
            let pp = proj.path();
            if !pp.is_dir() { continue; }
            let name = match pp.file_name().and_then(|s| s.to_str()) {
                Some(n) => n.to_string(),
                None    => continue,
            };
            let decoded = decode_project_path(&name);
            if let Ok(files) = fs::read_dir(&pp) {
                for tx in files.flatten() {
                    let txp = tx.path();
                    if txp.extension().and_then(|s| s.to_str()) != Some("jsonl") { continue; }
                    let mtime = match fs::metadata(&txp).and_then(|m| m.modified()) {
                        Ok(t)  => t.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
                        Err(_) => continue,
                    };
                    if mtime < cutoff { continue; }
                    let stem = txp.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                    let sid: String = stem.chars().take(8).collect();
                    rows.push(ActiveRow { mtime_secs: mtime, label: sid, path: decoded.clone() });
                }
            }
        }
    }

    let mut seen: HashSet<String> = rows.iter().map(|r| r.path.clone()).collect();
    for r in read_heartbeats(cutoff) {
        if !seen.contains(&r.path) {
            seen.insert(r.path.clone());
            rows.push(ActiveRow {
                mtime_secs: r.mtime_secs,
                label: format!("ping:{}", r.label),
                path: r.path,
            });
        }
    }

    rows.sort_by(|a, b| b.mtime_secs.cmp(&a.mtime_secs));

    if rows.is_empty() {
        return format!(
            "active sessions (last {window_min}min): (none — all idle)\n  tip: run  ! we-forgectl ping  from inside a session to register it"
        );
    }
    let total = rows.len();
    let mut lines = vec![format!("active sessions (last {window_min}min, {total} total):")];
    for r in rows.iter().take(max_show) {
        let age_min = (now_secs.saturating_sub(r.mtime_secs)) / 60;
        let mark = if age_min < 5 { "⚡" } else if age_min < 30 { "🕐" } else { "💤" };
        let hh_mm = format_hh_mm_local(r.mtime_secs);
        let short = if r.path.chars().count() <= 45 {
            r.path.clone()
        } else {
            let tail: String = r.path.chars().rev().take(44).collect::<String>().chars().rev().collect();
            format!("…{tail}")
        };
        lines.push(format!("  {mark} {}  {hh_mm} ({age_min}m)  {short}", r.label));
    }
    if total > max_show {
        lines.push(format!("  … ({} more)", total - max_show));
    }
    lines.push("  (run  ! we-forgectl ping  to register a session not shown above)".to_string());
    lines.join("\n")
}

fn format_hh_mm_local(epoch_secs: u64) -> String {
    use chrono::TimeZone;
    match chrono::Local.timestamp_opt(epoch_secs as i64, 0).single() {
        Some(dt) => dt.format("%H:%M").to_string(),
        None     => "??:??".to_string(),
    }
}
