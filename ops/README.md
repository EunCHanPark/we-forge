# ops/ — runtime supervisor

Production-grade dead-man supervisor for the we-forge daemon. Addresses the
gap that `launchd KeepAlive=true` only restarts crashed processes — *hung*
daemons (alive but stuck in an infinite loop) need an external supervisor to
detect and kill them.

Source: ECC security guide §"Kill Switches" recommendation. Implemented for
this environment 2026-05-15.

## Files

- `supervisor.sh` — bash script. Reads `~/Library/Logs/we-forge/daemon.log`,
  detects two failure modes:
  1. **Tick hung** — `tick begin` older than 30 min with no matching `tick end`
  2. **Tick missed** — last `tick begin` older than 6.5 h (interval 6 h + 30 min margin),
     no new tick scheduled
- `com.we-forge.supervisor.plist` — launchd agent, fires `supervisor.sh` every
  300 s (5 min).

## Install

```bash
cp ops/supervisor.sh ~/.we-forge/supervisor.sh
chmod +x ~/.we-forge/supervisor.sh
cp ops/com.we-forge.supervisor.plist ~/Library/LaunchAgents/
launchctl load ~/Library/LaunchAgents/com.we-forge.supervisor.plist
launchctl list | grep we-forge.supervisor   # verify PID
```

## Behavior

- **ok** verdict → quiet (no log spam), only touches `~/.we-forge/supervisor-last-check.txt`
- **hung** / **missed** verdict → logs to `~/Library/Logs/we-forge/supervisor.log`,
  sends SIGTERM to daemon process group, escalates to SIGKILL after 5 s,
  launchd `KeepAlive` restarts the daemon

## Tunables (inside `supervisor.sh`)

| Variable | Default | Meaning |
|---|---|---|
| `TICK_HUNG_THRESHOLD` | 1800 s (30 min) | Max time a single tick may run before declared hung |
| `INTERVAL_SEC` | 21600 s (6 h) | Aligned daemon tick interval |
| `NO_TICK_THRESHOLD` | 23400 s (6.5 h) | Max gap between tick begins before declared missed |

Tuned against observed tick duration of 7 min for typical empty-queue ticks
(2026-05-15 sample). Raise `TICK_HUNG_THRESHOLD` if real ticks regularly run
longer than 30 min (e.g., very large queue, slow Telegram round-trip).

## Uninstall

```bash
launchctl unload ~/Library/LaunchAgents/com.we-forge.supervisor.plist
rm ~/Library/LaunchAgents/com.we-forge.supervisor.plist
rm ~/.we-forge/supervisor.sh
```

The supervisor never touches `~/.we-forge/config.json` or daemon settings —
it only reads logs and signals the daemon process.
