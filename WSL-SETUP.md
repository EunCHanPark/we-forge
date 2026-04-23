# we-forge on Windows Server (via WSL2)

This guide walks through installing we-forge on a Windows Server host by running
it inside a WSL2 Ubuntu distro. The scheduler is **cron inside WSL2**, not the
Windows Task Scheduler (our scripts are POSIX bash).

## Prerequisites

- Windows Server 2019 (build 18362+) or Windows Server 2022
- Local administrator access on the Windows side
- Network access (to install packages and `git clone`)
- A claude.ai account for `claude auth login` inside WSL2

## 1. Install WSL2 and Ubuntu

Open **PowerShell as Administrator** on the Windows side and run:

```powershell
wsl --install -d Ubuntu-22.04
# reboot if prompted
```

After reboot, Windows launches the Ubuntu initial-setup console. Pick a
username/password — this is the Linux user inside WSL. All we-forge paths
below (`~/.claude/`, `~/.zsh_history`, etc.) refer to that Linux user's
home directory, not the Windows account.

Verify:

```powershell
wsl -l -v
# expect: Ubuntu-22.04   Running   2
```

## 2. Enable systemd and cron (persistent scheduling)

By default WSL2 Ubuntu does not run systemd, and cron is stopped at boot.
Enable both so the hourly tick fires unattended.

Inside WSL2:

```bash
# Enable systemd
sudo tee /etc/wsl.conf >/dev/null <<'EOF'
[boot]
systemd=true
EOF
```

Back in **PowerShell** (Windows side):

```powershell
wsl --shutdown
# next wsl launch boots with systemd
```

Re-enter WSL and install cron:

```bash
sudo apt-get update
sudo apt-get install -y cron jq python3 git curl
sudo systemctl enable --now cron
sudo systemctl status cron | head -5   # confirm "active (running)"
```

## 3. Install Claude Code CLI

Inside WSL2, follow the official Linux install for Claude Code. One common
path (verify against https://docs.claude.com for the current instruction):

```bash
curl -fsSL https://claude.ai/install.sh | bash
# then:
source ~/.bashrc
claude --version
claude auth login    # opens a browser link; complete on the Windows side
```

## 4. Clone we-forge and install

```bash
cd ~
git clone https://github.com/EunCHanPark/we-forge.git
cd we-forge
./install.sh --test       # redact self-test + tick dry-run fixture
./install.sh              # copies files under ~/.claude/; jq-merges Stop hook
```

`install.sh` auto-detects WSL and prints the Linux/cron next-steps block
(not the macOS LaunchAgent one).

## 5. Register the hourly cron entry

```bash
crontab -e
# paste this exact line:
0 * * * * /bin/bash -lc '~/.claude/learning/tick.sh >> ~/.claude/learning/data/tick.log 2>&1'
```

Verify:

```bash
crontab -l | grep tick.sh
```

## 6. Keep WSL running

Scheduled tasks only fire while WSL is active. Three options from easiest
to most robust:

**Option A — Always-on WSL (easiest)**
Windows Server typically stays logged in. As long as any WSL process is
running, the distro stays up. Launch one persistent process:

```powershell
# in PowerShell (not elevated):
Start-Process wsl -ArgumentList "bash -c 'tail -f /dev/null'" -WindowStyle Hidden
```

Add this command to a Startup task in Windows Task Scheduler so it runs
on login.

**Option B — Windows Task Scheduler kicks WSL on a timer**
Create a Windows scheduled task (not a we-forge task; just a WSL wake-up)
that runs `wsl -e true` every 30 minutes. Prevents WSL from idling out.

**Option C — Windows service wrapper**
Use `wsl-ssh-agent`, `nssm`, or similar to install a Windows service that
keeps a WSL shell attached. Overkill for most setups.

Pick whichever fits your operational model. **Option A is sufficient for
a personal Windows Server.**

## 7. Verify end-to-end

Inside WSL, after the next `:00`:

```bash
tail -n 20 ~/.claude/learning/data/tick.log
# expected line shape:
#   [2026-04-23T14:00:00Z] tick begin
#   normalize: events=N dropped_secret=0 patterns=M queue_len=0 added=0
#   [2026-04-23T14:00:01Z] promotion queue empty; no claude invocation
#   [2026-04-23T14:00:01Z] tick end
```

Run `/skill-report` inside a manual `claude` session for a fuller status
dashboard.

## Known friction points on WSL

| Issue | Fix |
|---|---|
| `sudo service cron start` works but dies on next WSL restart | Enable systemd in `/etc/wsl.conf` (step 2) |
| `claude auth login` can't open a browser from the server | Copy the login URL to a workstation browser; paste the code back in WSL |
| Windows Server headless (no GUI) | Everything here is CLI-only; no GUI needed |
| `~/.bash_history` is empty right after install | Expected. we-forge captures new commands going forward, not historical ones |
| WSL IP changes on reboot | Not relevant for we-forge — no network listeners exposed |
| `~/.zsh_history` not present inside WSL | Expected. Ubuntu default shell is bash. we-forge auto-falls back to `~/.bash_history`, which IS populated on WSL |

## Data locality

Everything we-forge reads and writes is inside the WSL2 filesystem:

- `~/.claude/learning/data/*` — event stream, patterns, queue, ledger
- `~/.claude/agents/*` — sub-agent definitions
- `~/.claude/skills/learned/*` — promoted skills
- `~/.claude/agent-memory/we-forge/` — we-forge's cross-run memory

None of this crosses the WSL2 ↔ Windows boundary. If you reinstall the
WSL distro, you lose all accumulated learnings. Back up via
`wsl --export Ubuntu-22.04 .\backup.tar` before any distro-level changes.

## Cross-host learning (optional, not implemented)

The current system is single-host: what the Mac learns stays on the Mac,
what the Windows Server learns stays there. If you want unified learning
across machines, a shared `events.jsonl` over syncthing / S3 + a single
head-node running the promotion pipeline is the minimum design. Not in
scope for this install.
