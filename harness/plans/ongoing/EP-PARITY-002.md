# EP-PARITY-002 — Service-manager re-install on Rust-binary upgrade

**Status:** [WIP] — runtime hotfix applied on one machine; installer fix pending
**Opened:** 2026-05-12
**Follows:** EP-PARITY-001 (Rust CLI port, v0.4.7)

## Problem

When `we-forgectl` was swapped from a Python script to a compiled Rust binary,
machines that already had the daemon installed kept their **old launchd plist**,
which invokes the CLI through a Python interpreter:

```xml
<key>ProgramArguments</key>
<array>
  <string>/usr/bin/env</string>
  <string>python3</string>
  <string>/Users/<user>/.local/bin/we-forgectl</string>
  <string>daemon</string>
</array>
```

`python3 <Mach-O binary>` → `SyntaxError: Non-UTF-8 code starting with '\xcf' …`
→ the daemon never starts, launchd throttle-retries forever. Symptoms:

- `we-forgectl status` → `status: stopped` (even though mode is `daemon`)
- `~/Library/Logs/we-forge/daemon.log` fills with the SyntaxError
- pattern-learning ticks stop running; Telegram goes silent

Same hazard class on the other platforms:
- **systemd**: `ExecStart=/usr/bin/python3 .../we-forgectl daemon`
- **Windows Task Scheduler**: action runs `python … we-forgectl daemon`

## Hotfix (done, per-machine, manual)

```bash
# macOS — bootout the broken job, regenerate the plist via the Rust installer
launchctl bootout gui/$(id -u)/com.we-forge.daemon 2>/dev/null
we-forgectl install --enable-telegram      # or: we-forgectl install --daemon

# verify
we-forgectl status                          # → running
ps aux | grep "we-forgectl daemon" | grep -v grep   # no python3 wrapper
```

The Rust installer's `service::launchd::generate_plist()` already emits
`ProgramArguments=["<exe>", "daemon"]` (no interpreter), so a re-install fixes it.
(`we-forgectl install` overwrites `config.json`'s `telegram_enabled` with the
flag value — pass `--enable-telegram` if Telegram was in use; the token/chat_id
stay in config and are reused.)

## Remaining work

1. **install.sh** — on (re-)install, detect a stale service definition (plist /
   unit / scheduled-task action whose program path is an interpreter rather than
   the `we-forgectl` binary) and always regenerate it. Cheapest robust approach:
   unconditionally run `we-forgectl install …` (which regenerates + re-bootstraps)
   rather than skipping when "already installed".
2. **install.ps1** — same for the Windows Task Scheduler action.
3. **systemd path** — verify `service::systemd` regenerates the unit file and runs
   `systemctl --user daemon-reload` + `restart` on re-install; add the stale-check
   if missing.
4. (optional) **`we-forgectl doctor`** — flag a service definition whose program
   path doesn't match the current `we-forgectl` binary, with a one-line fix hint.

## Acceptance

- Upgrading from a Python-era install to the Rust binary, then re-running the
  platform installer, leaves `we-forgectl status` = `running` with the binary
  invoked directly (no `python3`/`python` wrapper) on macOS, Linux, and Windows.
- `we-forgectl doctor` (if extended) reports OK after the upgrade path.
