# systemd templates (Linux + WSL2)

Alternative to cron. Runs the we-forge tick as a user-scoped systemd timer.

## Install

```bash
# 1. Copy templates with $HOME substituted in
SYSTEMD_USER_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
mkdir -p "$SYSTEMD_USER_DIR"

sed "s|__HOME__|$HOME|g" we-forge-tick.service.template \
  > "$SYSTEMD_USER_DIR/we-forge-tick.service"
cp we-forge-tick.timer.template \
  "$SYSTEMD_USER_DIR/we-forge-tick.timer"

# 2. Reload systemd, enable, start
systemctl --user daemon-reload
systemctl --user enable --now we-forge-tick.timer
```

## Verify

```bash
# Show next firing
systemctl --user list-timers we-forge-tick.timer

# Last few runs
journalctl --user -u we-forge-tick.service -n 50 --no-pager

# Manual fire (does not affect timer schedule)
systemctl --user start we-forge-tick.service
```

## Uninstall

```bash
systemctl --user disable --now we-forge-tick.timer
rm "${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user/we-forge-tick."{service,timer}
systemctl --user daemon-reload
```

## WSL2 specifics

WSL2 user-mode systemd works only when:
- `systemd=true` is set under `[boot]` in `/etc/wsl.conf` (Windows side
  installs WSL2 with this enabled by default since wsl.exe v0.67+).
- The WSL2 distribution stays running. Closing all terminals stops WSL2
  after a few minutes — the timer pauses with it. Either:
  - Keep one terminal open
  - Or use Windows Task Scheduler (registered automatically by `install.ps1`)
    which calls `wsl.exe ~/.claude/learning/tick.sh` even when no WSL2
    terminal is open.

## Why prefer systemd over cron on Linux?

- `journalctl` per-unit logs (vs scattering output across mail/syslog)
- `Persistent=true` runs missed ticks after laptop wake/resume
- No need to remember a crontab line; the unit files self-document
- `systemctl --user` requires no root
