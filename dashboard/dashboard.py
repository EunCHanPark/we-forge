#!/usr/bin/env python3
"""dashboard.py — we-forge KPI dashboard (web + TUI in one).

Modes:
    --serve [--port 8765]   start a localhost HTTP server with auto-refresh
    --tui                   render a rich-powered terminal UI (requires `rich`)
    --once                  print KPIs to stdout once and exit (no deps)

Reads (read-only):
    ~/.claude/learning/data/events.jsonl
    ~/.claude/learning/data/patterns.jsonl
    ~/.claude/learning/data/promotion_queue.jsonl
    ~/.claude/learning/data/ledger.jsonl
    ~/.claude/skills/learned/*/SKILL.md
    ~/.claude/plugins/marketplaces/**/SKILL.md
    ~/.claude/homunculus/projects/*/instincts/personal/*.yaml

Writes: none. --serve binds to 127.0.0.1 only.

Why a single file: stdlib-only for the HTTP path; `rich` is imported lazily
only when --tui is requested. Keep zero-dep promise of the project.
"""

from __future__ import annotations

import argparse
import datetime as dt
import glob
import http.server
import json
import os
import re
import socketserver
import sys
import threading
import time
import webbrowser
from collections import Counter, defaultdict
from pathlib import Path
from typing import Any

# ----------------------------------------------------------------------------
# Paths
# ----------------------------------------------------------------------------

CLAUDE_HOME = Path(os.environ.get("CLAUDE_HOME", str(Path.home() / ".claude")))
DATA_DIR    = CLAUDE_HOME / "learning" / "data"
LEARNED_DIR = CLAUDE_HOME / "skills" / "learned"
MARKETPLACE_GLOB = str(CLAUDE_HOME / "plugins" / "marketplaces" / "**" / "SKILL.md")
INSTINCT_GLOB    = str(CLAUDE_HOME / "homunculus" / "projects" / "*" / "instincts" / "personal" / "*.yaml")

# ----------------------------------------------------------------------------
# Data loading
# ----------------------------------------------------------------------------

def _read_jsonl(path: Path) -> list[dict]:
    if not path.exists():
        return []
    out: list[dict] = []
    with path.open(encoding="utf-8", errors="replace") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            try:
                out.append(json.loads(line))
            except json.JSONDecodeError:
                pass  # skip malformed
    return out


def _parse_iso(s: str | None) -> dt.datetime | None:
    if not s:
        return None
    try:
        # Accept both `Z` and `+00:00`
        return dt.datetime.fromisoformat(s.replace("Z", "+00:00"))
    except (ValueError, AttributeError):
        return None


def _read_skill_frontmatter(path: Path) -> dict[str, str]:
    """Tiny YAML frontmatter reader — first 30 lines, name + description only."""
    try:
        with path.open(encoding="utf-8", errors="replace") as f:
            head = "".join(line for _, line in zip(range(30), f))
    except OSError:
        return {}
    m = re.search(r"^---\s*\n(.*?)\n---", head, flags=re.S | re.M)
    if not m:
        return {}
    out: dict[str, str] = {}
    for line in m.group(1).splitlines():
        kv = re.match(r"^(name|description|origin)\s*:\s*(.+?)\s*$", line)
        if kv:
            v = kv.group(2)
            # strip surrounding quotes
            if (v.startswith('"') and v.endswith('"')) or (v.startswith("'") and v.endswith("'")):
                v = v[1:-1]
            out[kv.group(1)] = v
    return out


# ----------------------------------------------------------------------------
# KPI computation
# ----------------------------------------------------------------------------

def compute_kpis() -> dict[str, Any]:
    """Single source of truth — both --serve and --tui consume this."""
    events   = _read_jsonl(DATA_DIR / "events.jsonl")
    patterns = _read_jsonl(DATA_DIR / "patterns.jsonl")
    queue    = _read_jsonl(DATA_DIR / "promotion_queue.jsonl")
    ledger   = _read_jsonl(DATA_DIR / "ledger.jsonl")

    now = dt.datetime.now(dt.timezone.utc)
    seven_days_ago    = now - dt.timedelta(days=7)
    fourteen_days_ago = now - dt.timedelta(days=14)

    # --- Events per day (last 7) ---
    events_per_day: dict[str, int] = defaultdict(int)
    for ev in events:
        t = _parse_iso(ev.get("ts"))
        if t and t >= seven_days_ago:
            events_per_day[t.date().isoformat()] += 1

    # --- Top patterns by total count ---
    pattern_counts: Counter[str] = Counter()
    for p in patterns:
        key = p.get("pattern", "")
        if not key:
            continue
        pattern_counts[key] += int(p.get("count", 0) or 0)
    top_patterns = pattern_counts.most_common(10)

    # --- Decision distribution ---
    decision_counts: Counter[str] = Counter()
    recent_decisions: list[dict] = []
    for row in ledger:
        d = (row.get("decision") or "").upper()
        if d:
            decision_counts[d] += 1
        recent_decisions.append(row)
    recent_decisions.sort(key=lambda r: r.get("ts", ""), reverse=True)
    recent_decisions = recent_decisions[:15]

    # --- Learned skills ---
    learned: list[dict] = []
    if LEARNED_DIR.exists():
        for sd in sorted(LEARNED_DIR.iterdir()):
            if sd.is_dir() and sd.name != "pending":
                fm = _read_skill_frontmatter(sd / "SKILL.md")
                if fm:
                    learned.append({
                        "slug": sd.name,
                        "name": fm.get("name", sd.name),
                        "description": fm.get("description", "")[:160],
                    })

    # --- Marketplace skills (count + name index for matching) ---
    marketplace_skills: list[dict] = []
    seen_slugs: set[str] = set()
    for path in glob.glob(MARKETPLACE_GLOB, recursive=True):
        if "/cache/" in path:
            continue  # de-dup against marketplaces tree
        fm = _read_skill_frontmatter(Path(path))
        name = fm.get("name") or Path(path).parent.name
        if name in seen_slugs:
            continue
        seen_slugs.add(name)
        marketplace_skills.append({
            "name": name,
            "description": fm.get("description", "")[:160],
        })
    marketplace_skills.sort(key=lambda s: s["name"])

    # --- Marketplace recommendations: top patterns -> matching marketplace skills ---
    recommendations: list[dict] = []
    placeholder_re = re.compile(r"<[^>]+>")
    for pat, count in top_patterns:
        head_token = placeholder_re.sub("", pat).split(":", 1)[0].strip().lower()
        match = None
        if head_token:
            for s in marketplace_skills:
                if head_token in s["name"].lower() or head_token in s["description"].lower():
                    match = s
                    break
        recommendations.append({
            "pattern": pat,
            "count": count,
            "match": match,
        })

    # --- ECC instincts ---
    instinct_count = len(glob.glob(INSTINCT_GLOB, recursive=True))

    # --- Dead skill candidates (PASS'd > 14d ago) ---
    pass_dates: dict[str, dt.datetime] = {}
    for row in ledger:
        if (row.get("decision") or "").upper() == "PASS":
            slug = row.get("slug", "")
            t = _parse_iso(row.get("ts"))
            if slug and t:
                pass_dates[slug] = t  # most recent PASS wins
    dead_candidates = [
        {"slug": s, "passed_at": d.isoformat()}
        for s, d in pass_dates.items()
        if d < fourteen_days_ago
    ]

    # --- ECC utilization KPI: ECC_MATCH / total decisions ---
    total_dec = sum(decision_counts.values()) or 1
    ecc_match_ratio = decision_counts.get("ECC_MATCH", 0) / total_dec

    # --- Sequence candidates (shadow-mode multi-step learning) ---
    sequences = _read_jsonl(DATA_DIR / "sequence_candidates.jsonl")
    top_sequences = sorted(
        sequences,
        key=lambda c: (-int(c.get("support", 0)), int(c.get("n", 0))),
    )[:10]

    return {
        "generated_at": now.isoformat(),
        "claude_home":  str(CLAUDE_HOME),
        "totals": {
            "events":        len(events),
            "patterns":      len(patterns),
            "queue":         len(queue),
            "ledger":        len(ledger),
            "learned":       len(learned),
            "marketplace":   len(marketplace_skills),
            "instincts":     instinct_count,
            "sequences":     len(sequences),
        },
        "events_per_day":   sorted(events_per_day.items()),
        "top_patterns":     top_patterns,
        "decisions":        dict(decision_counts),
        "ecc_match_ratio":  ecc_match_ratio,
        "recommendations":  recommendations,
        "learned":          learned,
        "recent_decisions": recent_decisions,
        "queue":            queue[:10],
        "dead_candidates":  dead_candidates,
        "top_sequences":    top_sequences,
    }


# ----------------------------------------------------------------------------
# Mode: --once (stdout)
# ----------------------------------------------------------------------------

def render_once() -> None:
    k = compute_kpis()
    t = k["totals"]
    print(f"we-forge dashboard ({k['generated_at']})")
    print(f"  CLAUDE_HOME = {k['claude_home']}")
    print()
    print(f"  events:       {t['events']:>6}")
    print(f"  patterns:     {t['patterns']:>6}")
    print(f"  queue:        {t['queue']:>6}")
    print(f"  ledger:       {t['ledger']:>6}")
    print(f"  learned:      {t['learned']:>6}")
    print(f"  marketplace:  {t['marketplace']:>6}")
    print(f"  ECC instincts:{t['instincts']:>6}")
    print(f"  sequences:    {t.get('sequences', 0):>6}  (shadow mode)")
    print()
    print(f"  ECC_MATCH ratio: {k['ecc_match_ratio']*100:.1f}% of decisions")
    print(f"  decisions: {k['decisions']}")
    print()
    print("Top 10 patterns:")
    for pat, count in k["top_patterns"]:
        print(f"  {count:>4}  {pat}")
    print()
    print("ECC marketplace recommendations:")
    for r in k["recommendations"]:
        if r["match"]:
            print(f"  {r['pattern']}  ->  /everything-claude-code:{r['match']['name']}")
        else:
            print(f"  {r['pattern']}  ->  (no match - candidate for synthesis)")
    if k["dead_candidates"]:
        print()
        print(f"Dead skill candidates ({len(k['dead_candidates'])}):")
        for c in k["dead_candidates"][:10]:
            print(f"  {c['slug']}  (PASS'd at {c['passed_at']})")
    if k.get("top_sequences"):
        print()
        print(f"Top sequence candidates (shadow mode, n={t.get('sequences',0)} total):")
        for c in k["top_sequences"]:
            seq = " -> ".join(c.get("sequence", []))
            if len(seq) > 90:
                seq = seq[:87] + "..."
            print(f"  support={c.get('support','?'):>3} n={c.get('n','?')} {seq}")


# ----------------------------------------------------------------------------
# Mode: --tui (rich)
# ----------------------------------------------------------------------------

def render_tui(refresh_seconds: float = 3.0) -> None:
    try:
        from rich.console import Console
        from rich.layout import Layout
        from rich.live import Live
        from rich.panel import Panel
        from rich.table import Table
        from rich.text import Text
    except ImportError:
        print("rich is not installed.")
        print("  pip install rich")
        print("  (or: pip3 install rich)")
        print("Falling back to --once mode:")
        print()
        render_once()
        return

    console = Console()

    def build_layout() -> Layout:
        k = compute_kpis()
        t = k["totals"]
        layout = Layout()
        layout.split_column(
            Layout(name="header", size=3),
            Layout(name="body"),
            Layout(name="footer", size=3),
        )
        layout["body"].split_row(Layout(name="left"), Layout(name="right"))
        layout["left"].split_column(Layout(name="totals", size=11), Layout(name="patterns"))
        layout["right"].split_column(Layout(name="recommendations"), Layout(name="decisions", size=14))

        # Header
        layout["header"].update(Panel(
            Text(f"we-forge dashboard  -  CLAUDE_HOME={k['claude_home']}  -  {k['generated_at']}",
                 justify="center", style="bold cyan"),
            border_style="cyan",
        ))

        # Totals
        tt = Table(show_header=False, box=None, padding=(0, 1))
        tt.add_column(style="dim")
        tt.add_column(justify="right", style="bold")
        tt.add_row("events",         f"{t['events']:,}")
        tt.add_row("patterns",       f"{t['patterns']:,}")
        tt.add_row("queue",          f"{t['queue']:,}")
        tt.add_row("ledger",         f"{t['ledger']:,}")
        tt.add_row("learned",        f"{t['learned']:,}")
        tt.add_row("marketplace",    f"{t['marketplace']:,}")
        tt.add_row("ECC instincts",  f"{t['instincts']:,}")
        ratio_pct = k["ecc_match_ratio"] * 100
        tt.add_row("ECC_MATCH ratio", f"{ratio_pct:5.1f} %")
        layout["totals"].update(Panel(tt, title="Totals", border_style="green"))

        # Top patterns
        pt = Table(show_header=True, header_style="bold")
        pt.add_column("count", justify="right", style="cyan")
        pt.add_column("pattern", overflow="fold")
        for pat, count in k["top_patterns"]:
            pt.add_row(str(count), pat)
        layout["patterns"].update(Panel(pt, title="Top patterns", border_style="cyan"))

        # Recommendations
        rt = Table(show_header=True, header_style="bold")
        rt.add_column("pattern", overflow="fold")
        rt.add_column("->  ECC marketplace skill", overflow="fold")
        for r in k["recommendations"]:
            mark = (f"[green]/everything-claude-code:{r['match']['name']}[/green]"
                    if r["match"]
                    else "[dim](no match - candidate for synthesis)[/dim]")
            rt.add_row(r["pattern"], mark)
        layout["recommendations"].update(Panel(rt, title="ECC marketplace recommendations",
                                               border_style="magenta"))

        # Decisions
        dt_ = Table(show_header=True, header_style="bold")
        dt_.add_column("decision")
        dt_.add_column("count", justify="right")
        for d in ("PASS", "REVISE", "REJECT", "ECC_MATCH"):
            dt_.add_row(d, str(k["decisions"].get(d, 0)))
        layout["decisions"].update(Panel(dt_, title="Auditor decisions", border_style="yellow"))

        # Footer
        layout["footer"].update(Panel(
            Text(f"refresh: every {refresh_seconds}s  -  Ctrl-C to quit", style="dim", justify="center"),
            border_style="dim",
        ))
        return layout

    with Live(build_layout(), console=console, refresh_per_second=2, screen=True) as live:
        try:
            while True:
                time.sleep(refresh_seconds)
                live.update(build_layout())
        except KeyboardInterrupt:
            pass


# ----------------------------------------------------------------------------
# Mode: --serve (HTTP)
# ----------------------------------------------------------------------------

HTML_PAGE = r"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>we-forge dashboard</title>
<meta http-equiv="refresh" content="10">
<script src="https://cdn.jsdelivr.net/npm/chart.js@4.4.0/dist/chart.umd.min.js"></script>
<style>
  :root {
    --bg: #0e0f12; --panel: #1a1c22; --text: #e6e6e6; --dim: #888;
    --accent: #4dd0e1; --good: #7cdc6e; --warn: #ffcc66; --bad: #ff6b6b;
    --border: #2a2c33;
  }
  * { box-sizing: border-box; }
  html,body { margin:0; background: var(--bg); color: var(--text);
              font-family: -apple-system, BlinkMacSystemFont, "SF Mono",
                          "Cascadia Mono", Menlo, Consolas, monospace; }
  header { padding: 14px 22px; border-bottom: 1px solid var(--border);
           display: flex; justify-content: space-between; align-items: center; }
  h1 { margin:0; font-size: 18px; color: var(--accent); }
  .meta { color: var(--dim); font-size: 12px; }
  main { padding: 18px; display: grid; gap: 14px;
         grid-template-columns: repeat(auto-fit, minmax(360px, 1fr)); }
  .panel { background: var(--panel); border: 1px solid var(--border);
           border-radius: 6px; padding: 14px; }
  .panel h2 { margin: 0 0 10px 0; font-size: 13px; color: var(--accent);
              text-transform: uppercase; letter-spacing: 0.06em; }
  .totals { display: grid; grid-template-columns: 1fr auto; gap: 6px 16px;
            font-size: 13px; }
  .totals .v { color: var(--good); text-align: right; font-variant-numeric: tabular-nums; }
  table { width: 100%; border-collapse: collapse; font-size: 12px; }
  th { text-align: left; color: var(--dim); font-weight: normal;
       border-bottom: 1px solid var(--border); padding: 6px 4px; }
  td { padding: 6px 4px; border-bottom: 1px solid var(--border); vertical-align: top; }
  tr:last-child td { border-bottom: none; }
  td.num { text-align: right; color: var(--accent); font-variant-numeric: tabular-nums; }
  .pill { display: inline-block; padding: 2px 8px; border-radius: 10px;
          font-size: 11px; }
  .pill.PASS { background: #1b3a1d; color: var(--good); }
  .pill.REVISE { background: #3a3a1b; color: var(--warn); }
  .pill.REJECT { background: #3a1b1b; color: var(--bad); }
  .pill.ECC_MATCH { background: #1b2a3a; color: var(--accent); }
  .ratio { font-size: 24px; color: var(--accent); margin-top: 6px; }
  .dim { color: var(--dim); }
  canvas { max-width: 100%; }
</style>
</head>
<body>
<header>
  <h1>we-forge dashboard</h1>
  <div class="meta">
    <span id="claude-home"></span>
    <span id="generated"></span>
    auto-refresh 10s
  </div>
</header>
<main>
  <section class="panel">
    <h2>Totals</h2>
    <div class="totals" id="totals"></div>
    <div class="ratio" id="ratio"></div>
    <div class="dim">ECC_MATCH ratio of total auditor decisions</div>
  </section>

  <section class="panel">
    <h2>Events per day (7d)</h2>
    <canvas id="chart-events" height="180"></canvas>
  </section>

  <section class="panel">
    <h2>Top patterns</h2>
    <table id="patterns"><thead>
      <tr><th>count</th><th>pattern</th></tr>
    </thead><tbody></tbody></table>
  </section>

  <section class="panel">
    <h2>ECC marketplace recommendations</h2>
    <table id="recs"><thead>
      <tr><th>pattern</th><th>match</th></tr>
    </thead><tbody></tbody></table>
  </section>

  <section class="panel">
    <h2>Auditor decisions</h2>
    <canvas id="chart-decisions" height="180"></canvas>
  </section>

  <section class="panel">
    <h2>Recent decisions</h2>
    <table id="recent"><thead>
      <tr><th>when</th><th>verdict</th><th>slug</th></tr>
    </thead><tbody></tbody></table>
  </section>

  <section class="panel">
    <h2>Learned skills</h2>
    <table id="learned"><thead>
      <tr><th>slug</th><th>description</th></tr>
    </thead><tbody></tbody></table>
  </section>

  <section class="panel">
    <h2>Dead skill candidates (>14d, never re-referenced)</h2>
    <table id="dead"><thead>
      <tr><th>slug</th><th>passed_at</th></tr>
    </thead><tbody></tbody></table>
  </section>
</main>
<script>
let chartEvents, chartDecisions;
async function load() {
  const r = await fetch("/api/kpis");
  const k = await r.json();
  document.getElementById("claude-home").textContent = "CLAUDE_HOME=" + k.claude_home;
  document.getElementById("generated").textContent  = k.generated_at;

  const tot = k.totals;
  const totEl = document.getElementById("totals");
  totEl.innerHTML = "";
  for (const [label, val] of Object.entries(tot)) {
    totEl.insertAdjacentHTML("beforeend", `<div>${label}</div><div class="v">${val.toLocaleString()}</div>`);
  }
  document.getElementById("ratio").textContent = (k.ecc_match_ratio * 100).toFixed(1) + " %";

  const labels = k.events_per_day.map(p => p[0]);
  const data   = k.events_per_day.map(p => p[1]);
  if (chartEvents) chartEvents.destroy();
  chartEvents = new Chart(document.getElementById("chart-events"), {
    type: "bar",
    data: { labels, datasets: [{ data, backgroundColor: "#4dd0e1" }] },
    options: { plugins: { legend: { display: false }},
               scales: { x: { ticks: { color: "#888" }},
                         y: { ticks: { color: "#888" }, beginAtZero: true }}}
  });

  const pt = document.querySelector("#patterns tbody"); pt.innerHTML = "";
  for (const [pat, count] of k.top_patterns) {
    pt.insertAdjacentHTML("beforeend",
      `<tr><td class="num">${count}</td><td>${escapeHtml(pat)}</td></tr>`);
  }

  const rt = document.querySelector("#recs tbody"); rt.innerHTML = "";
  for (const r of k.recommendations) {
    const m = r.match
      ? `<span style="color:#7cdc6e">/everything-claude-code:${escapeHtml(r.match.name)}</span>`
      : `<span class="dim">(no match - candidate for synthesis)</span>`;
    rt.insertAdjacentHTML("beforeend",
      `<tr><td>${escapeHtml(r.pattern)}</td><td>${m}</td></tr>`);
  }

  const decKeys = ["PASS", "REVISE", "REJECT", "ECC_MATCH"];
  const decData = decKeys.map(d => k.decisions[d] || 0);
  if (chartDecisions) chartDecisions.destroy();
  chartDecisions = new Chart(document.getElementById("chart-decisions"), {
    type: "doughnut",
    data: { labels: decKeys, datasets: [{
      data: decData,
      backgroundColor: ["#7cdc6e", "#ffcc66", "#ff6b6b", "#4dd0e1"]
    }]},
    options: { plugins: { legend: { position: "right",
                                    labels: { color: "#e6e6e6" }}}}
  });

  const rc = document.querySelector("#recent tbody"); rc.innerHTML = "";
  for (const row of k.recent_decisions) {
    rc.insertAdjacentHTML("beforeend",
      `<tr><td class="dim">${row.ts}</td>` +
      `<td><span class="pill ${row.decision}">${row.decision}</span></td>` +
      `<td>${escapeHtml(row.slug || "")}</td></tr>`);
  }

  const lt = document.querySelector("#learned tbody"); lt.innerHTML = "";
  if (k.learned.length === 0) {
    lt.innerHTML = `<tr><td colspan=2 class="dim">no learned skills yet</td></tr>`;
  } else {
    for (const s of k.learned) {
      lt.insertAdjacentHTML("beforeend",
        `<tr><td>${escapeHtml(s.slug)}</td><td>${escapeHtml(s.description)}</td></tr>`);
    }
  }

  const dt = document.querySelector("#dead tbody"); dt.innerHTML = "";
  if (k.dead_candidates.length === 0) {
    dt.innerHTML = `<tr><td colspan=2 class="dim">no dead-skill candidates</td></tr>`;
  } else {
    for (const c of k.dead_candidates) {
      dt.insertAdjacentHTML("beforeend",
        `<tr><td>${escapeHtml(c.slug)}</td><td class="dim">${c.passed_at}</td></tr>`);
    }
  }
}
function escapeHtml(s) {
  return String(s).replace(/[&<>"']/g, c =>
    ({ "&":"&amp;", "<":"&lt;", ">":"&gt;", '"':"&quot;", "'":"&#39;" }[c]));
}
load();
</script>
</body>
</html>
"""


class _Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self, format: str, *args: Any) -> None:
        pass  # suppress request log

    def do_GET(self) -> None:  # noqa: N802
        if self.path == "/" or self.path.startswith("/index"):
            body = HTML_PAGE.encode("utf-8")
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
        elif self.path.startswith("/api/kpis"):
            try:
                payload = json.dumps(compute_kpis(), default=str).encode("utf-8")
            except Exception as e:  # noqa: BLE001
                payload = json.dumps({"error": str(e)}).encode("utf-8")
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Cache-Control", "no-store")
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)
        else:
            self.send_response(404)
            self.end_headers()


def serve(port: int, open_browser: bool) -> None:
    # Bind to localhost only - never expose KPIs externally.
    addr = ("127.0.0.1", port)
    try:
        httpd = socketserver.ThreadingTCPServer(addr, _Handler)
    except OSError as e:
        print(f"could not bind to {addr}: {e}", file=sys.stderr)
        sys.exit(1)
    httpd.allow_reuse_address = True
    url = f"http://127.0.0.1:{port}/"
    print(f"we-forge dashboard serving at {url}  (Ctrl-C to stop)")
    if open_browser:
        threading.Thread(target=lambda: (time.sleep(0.5), webbrowser.open(url)),
                         daemon=True).start()
    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        print("\nshutting down...")
        httpd.server_close()


# ----------------------------------------------------------------------------
# Entry point
# ----------------------------------------------------------------------------

def main() -> None:
    ap = argparse.ArgumentParser(description="we-forge KPI dashboard")
    g = ap.add_mutually_exclusive_group()
    g.add_argument("--serve", action="store_true",
                   help="start localhost HTTP server (default mode)")
    g.add_argument("--tui",   action="store_true",
                   help="render rich-powered terminal UI")
    g.add_argument("--once",  action="store_true",
                   help="print KPIs to stdout once and exit")
    ap.add_argument("--port", type=int, default=8765,
                    help="port for --serve (default 8765)")
    ap.add_argument("--no-browser", action="store_true",
                    help="don't auto-open browser when --serve")
    ap.add_argument("--refresh", type=float, default=3.0,
                    help="refresh interval seconds for --tui (default 3.0)")
    args = ap.parse_args()

    if args.once:
        render_once()
    elif args.tui:
        render_tui(refresh_seconds=args.refresh)
    else:
        serve(args.port, open_browser=not args.no_browser)


if __name__ == "__main__":
    main()
