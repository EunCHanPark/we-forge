#!/usr/bin/env bash
# install.sh — idempotent installer for the 24/7 pattern-learning system.
#
# Usage:
#   # Local (after `git clone`):
#   ./install.sh                    # install into ~/.claude/ (default)
#   ./install.sh --test             # redact self-test + tick.sh dry-run fixture
#   ./install.sh --dry-run          # show what would happen, write nothing
#   ./install.sh --branch <name>    # use a specific branch when curl-piped
#
#   # Remote (one-line, no clone needed):
#   curl -fsSL https://raw.githubusercontent.com/EunCHanPark/we-forge/main/install.sh | bash
#   curl -fsSL https://raw.githubusercontent.com/EunCHanPark/we-forge/main/install.sh | bash -s -- --dry-run
#
# Env:
#   CLAUDE_HOME       override install prefix (default: $HOME/.claude)
#   WE_FORGE_REPO     override clone URL when curl-piped (default: github EunCHanPark/we-forge)
#   WE_FORGE_BRANCH   override branch when curl-piped (default: main)
#
# Side effects:
#   - clone the repo into a tmp dir IF run via curl-pipe (no local checkout)
#   - mkdir the ECC dir tree under $CLAUDE_HOME
#   - copy agents/, commands/, hooks/, learning/, dashboard/ into place
#   - seed empty data files in $CLAUDE_HOME/learning/data/
#   - jq-merge the Stop-hook entry into $CLAUDE_HOME/settings.json
#     (existing entries preserved; previous file backed up to settings.json.bak.<ISO>)
#   - print the crontab/launchd/systemd line; does NOT modify the user's scheduler

set -euo pipefail

# ---------------------------------------------------------------------------
# Curl-pipe self-bootstrap: if BASH_SOURCE is empty, /dev/fd/*, or otherwise
# unreadable as a file, we were piped via `curl ... | bash`. In that case
# clone the repo into a tmp dir and re-exec ourselves from there.
# ---------------------------------------------------------------------------
_BOOTSTRAP_SOURCE="${BASH_SOURCE[0]:-}"
if [ -z "$_BOOTSTRAP_SOURCE" ] || [ ! -r "$_BOOTSTRAP_SOURCE" ] || \
   [[ "$_BOOTSTRAP_SOURCE" == /dev/fd/* ]] || [[ "$_BOOTSTRAP_SOURCE" == /proc/self/fd/* ]]; then
  WE_FORGE_REPO="${WE_FORGE_REPO:-https://github.com/EunCHanPark/we-forge.git}"
  WE_FORGE_BRANCH="${WE_FORGE_BRANCH:-main}"
  # Allow overriding branch on the command line BEFORE we re-exec
  for arg in "$@"; do
    case "$arg" in
      --branch=*) WE_FORGE_BRANCH="${arg#--branch=}" ;;
    esac
  done
  for tool in git bash; do
    command -v "$tool" >/dev/null || { echo "missing: $tool" >&2; exit 1; }
  done
  _TMP="$(mktemp -d "${TMPDIR:-/tmp}/we-forge-install-XXXXXX")"
  echo "==> bootstrapping: cloning $WE_FORGE_REPO (branch $WE_FORGE_BRANCH) into $_TMP"
  git clone --depth 1 --branch "$WE_FORGE_BRANCH" "$WE_FORGE_REPO" "$_TMP" >/dev/null
  chmod +x "$_TMP/install.sh" "$_TMP/verify.sh" "$_TMP/learning"/*.sh "$_TMP/hooks"/*.sh 2>/dev/null || true
  exec bash "$_TMP/install.sh" "$@"
fi

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CLAUDE_HOME="${CLAUDE_HOME:-$HOME/.claude}"

DRY_RUN=0
TEST_MODE=0

NO_SERVICE=0
ENABLE_TELEGRAM=0
DAEMON_MODE=0
for arg in "$@"; do
  case "$arg" in
    --dry-run)         DRY_RUN=1 ;;
    --test)            TEST_MODE=1 ;;
    --no-service)      NO_SERVICE=1 ;;
    --enable-telegram) ENABLE_TELEGRAM=1 ;;
    --daemon)          DAEMON_MODE=1 ;;
    --branch=*) ;;  # already consumed by curl-pipe self-bootstrap above
    -h|--help)
      sed -n '1,/^$/p' "$0" | sed -n 's/^# \{0,1\}//p'
      exit 0
      ;;
    *)
      echo "unknown flag: $arg" >&2
      exit 2
      ;;
  esac
done

_say()  { printf '==> %s\n' "$*"; }
_warn() { printf 'warn: %s\n' "$*" >&2; }
_die()  { printf 'error: %s\n' "$*" >&2; exit 1; }

# Host detection for per-platform post-install guidance.
_is_wsl()   { grep -qi microsoft /proc/version 2>/dev/null || [ -n "${WSL_DISTRO_NAME:-}" ]; }
_is_macos() { [ "$(uname -s)" = "Darwin" ]; }
_is_linux() { [ "$(uname -s)" = "Linux" ] && ! _is_wsl; }

_run() {
  if [ "$DRY_RUN" = "1" ]; then
    printf '  DRY: %s\n' "$*"
  else
    eval "$@"
  fi
}

_copy() {
  local src="$1" dst="$2"
  [ -f "$src" ] || _die "missing source: $src"
  _run "mkdir -p \"$(dirname "$dst")\""
  _run "cp -f \"$src\" \"$dst\""
  _run "chmod +r \"$dst\""
  case "$src" in
    *.sh|*.py) _run "chmod +x \"$dst\"" ;;
  esac
}

# -----------------------------------------------------------------------
run_test_mode() {
  _say "test: redact self-test"
  bash "$REPO_DIR/learning/redact.sh" --self-test || _die "redact self-test failed"

  _say "test: tick.sh dry-run with 3-distinct-sessions fixture"
  local T
  T="$(mktemp -d)"
  mkdir -p "$T/claude/learning/data" "$T/claude/skills/learned"
  cat > "$T/claude/learning/data/events.jsonl" <<'EOF'
{"ts":"2026-04-23T12:00:00Z","session_id":"sess-A","source":"bash","raw":"git status","normalized":null}
{"ts":"2026-04-23T12:05:00Z","session_id":"sess-B","source":"bash","raw":"git status","normalized":null}
{"ts":"2026-04-23T12:10:00Z","session_id":"sess-C","source":"bash","raw":"git status","normalized":null}
{"ts":"2026-04-23T12:15:00Z","session_id":"sess-A","source":"bash","raw":"ANTHROPIC_API_KEY=sk-ant-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA","normalized":null}
EOF
  touch "$T/empty_history"
  CLAUDE_HOME="$T/claude" \
    CLAUDE_LEARNING_DATA="$T/claude/learning/data" \
    CLAUDE_LEARNING_DIR="$T/claude/learning" \
    CLAUDE_LEARNED_SKILLS="$T/claude/skills/learned" \
    CLAUDE_REDACT_LIB="$REPO_DIR/learning/redact.sh" \
    CLAUDE_NORMALIZE_PY="$REPO_DIR/learning/normalize.py" \
    BASH_HISTFILE_OVERRIDE="$T/empty_history" \
    CLAUDE_DRY_RUN=1 \
    bash "$REPO_DIR/learning/tick.sh"

  if ! grep -q '"pattern": "git status"' "$T/claude/learning/data/promotion_queue.jsonl" 2>/dev/null; then
    _die "tick.sh did not promote 'git status' to the queue"
  fi
  # events.jsonl is append-only (stop-telemetry's redact filter is the sole
  # gate on what gets written). normalize.py does not scrub events.jsonl in
  # place — doing so would race with stop-telemetry's O_APPEND fd and lose
  # in-flight events. The real invariant is that no secret propagates
  # DOWNSTREAM (patterns.jsonl, promotion_queue.jsonl, learned/).
  if grep -q 'sk-ant' "$T/claude/learning/data/patterns.jsonl" 2>/dev/null; then
    _die "secret leaked into patterns.jsonl"
  fi
  if grep -q 'sk-ant' "$T/claude/learning/data/promotion_queue.jsonl" 2>/dev/null; then
    _die "secret leaked into promotion_queue.jsonl"
  fi

  _say "test: PASS"
  echo "(tmpdir left at $T for inspection; safe to remove manually)"
  exit 0
}

[ "$TEST_MODE" = "1" ] && run_test_mode

# -----------------------------------------------------------------------
command -v jq >/dev/null 2>&1 || _die "jq is required; install with: brew install jq (or apt-get install jq)"
command -v python3 >/dev/null 2>&1 || _die "python3 is required"

_say "installing into $CLAUDE_HOME (dry-run=$DRY_RUN)"

_run "mkdir -p \"$CLAUDE_HOME/agents\" \"$CLAUDE_HOME/commands\" \"$CLAUDE_HOME/hooks\" \"$CLAUDE_HOME/learning/data\" \"$CLAUDE_HOME/skills/learned\""

_copy "$REPO_DIR/learning/redact.sh"    "$CLAUDE_HOME/learning/redact.sh"
_copy "$REPO_DIR/learning/normalize.py" "$CLAUDE_HOME/learning/normalize.py"
_copy "$REPO_DIR/learning/tick.sh"      "$CLAUDE_HOME/learning/tick.sh"

_copy "$REPO_DIR/hooks/stop-telemetry.sh" "$CLAUDE_HOME/hooks/stop-telemetry.sh"

for a in monitor-sentinel pattern-detector skill-synthesizer quality-auditor we-forge; do
  _copy "$REPO_DIR/agents/$a.md" "$CLAUDE_HOME/agents/$a.md"
done

for c in watch-and-learn skill-report ask-codex ask-gemini; do
  _copy "$REPO_DIR/commands/$c.md" "$CLAUDE_HOME/commands/$c.md"
done

for f in events.jsonl patterns.jsonl promotion_queue.jsonl ledger.jsonl rejected.txt; do
  _run "touch \"$CLAUDE_HOME/learning/data/$f\""
done
if [ ! -f "$CLAUDE_HOME/learning/data/state.json" ]; then
  _run "printf '%s' '{}' > \"$CLAUDE_HOME/learning/data/state.json\""
fi

# Settings merge (jq)
SETTINGS="$CLAUDE_HOME/settings.json"
SNIPPET="$REPO_DIR/learning/settings.snippet.json"
BACKUP="$SETTINGS.bak.$(date -u +%Y%m%dT%H%M%SZ)"

if [ ! -f "$SETTINGS" ]; then
  _say "no existing settings.json — creating fresh from snippet"
  _run "cp \"$SNIPPET\" \"$SETTINGS\""
else
  _say "backing up settings.json → $BACKUP"
  _run "cp -f \"$SETTINGS\" \"$BACKUP\""

  _say "merging Stop and SubagentStop telemetry entries via jq"
  # Shared merge snippet: for a given .hooks.<event> array, append the
  # telemetry-hook command to the empty-matcher group if not already there,
  # or create that group if no empty-matcher group exists.
  MERGE_EXPR='
    def merge_telemetry(cmd):
      if (length == 0) then
        [{matcher:"", hooks:[{type:"command", command:cmd}]}]
      else
        ( map(
            if (.matcher == "" or .matcher == null)
            then .hooks = (
              (.hooks // [])
              | if (map(.command) | index(cmd))
                then .
                else . + [{type:"command", command:cmd}]
                end
            )
            else .
            end
          )
        ) as $arr
        | if ($arr | any(.matcher == "" or .matcher == null))
          then $arr
          else $arr + [{matcher:"", hooks:[{type:"command", command:cmd}]}]
          end
      end;
    .hooks //= {} |
    .hooks.SessionStart //= [] |
    .hooks.SessionStart |= merge_telemetry("~/.claude/hooks/sessionstart-we-forge.sh") |
    .hooks.Stop //= [] |
    .hooks.Stop |= merge_telemetry("~/.claude/hooks/stop-telemetry.sh") |
    .hooks.SubagentStop //= [] |
    .hooks.SubagentStop |= merge_telemetry("~/.claude/hooks/stop-telemetry.sh")
  '
  if [ "$DRY_RUN" = "1" ]; then
    echo "  DRY: jq merge preview:"
    jq "$MERGE_EXPR" "$SETTINGS"
  else
    TMP="$(mktemp)"
    jq "$MERGE_EXPR" "$SETTINGS" > "$TMP"
    mv "$TMP" "$SETTINGS"
  fi
fi

# ---------------------------------------------------------------------------
# Global CLAUDE.md install (idempotent via marker block)
#
# ~/.claude/CLAUDE.md is auto-loaded by Claude Code in EVERY session, regardless
# of cwd. We install our we-forge global instructions as a marker-bounded block
# so users with existing personal CLAUDE.md content don't lose it.
#
# - If user has no CLAUDE.md → copy template as-is
# - If user has CLAUDE.md but no marker → append we-forge block
# - If marker exists → replace block (idempotent; supports re-install upgrades)
# ---------------------------------------------------------------------------
GLOBAL_CLAUDE_MD="$CLAUDE_HOME/CLAUDE.md"
TEMPLATE="$REPO_DIR/home/.claude/CLAUDE.md"
MARKER_START="<!-- WE-FORGE-GLOBAL-START -->"
MARKER_END="<!-- WE-FORGE-GLOBAL-END -->"

if [ ! -f "$TEMPLATE" ]; then
  _warn "global CLAUDE.md template not found at $TEMPLATE — skipping"
elif [ ! -f "$GLOBAL_CLAUDE_MD" ]; then
  _say "creating global $GLOBAL_CLAUDE_MD (with we-forge marker block)"
  if [ "$DRY_RUN" = "1" ]; then
    echo "  DRY: would write $GLOBAL_CLAUDE_MD with marker-bounded we-forge block"
  else
    {
      printf '%s\n' "$MARKER_START"
      cat "$TEMPLATE"
      printf '%s\n' "$MARKER_END"
    } > "$GLOBAL_CLAUDE_MD"
  fi
elif grep -qF "$MARKER_START" "$GLOBAL_CLAUDE_MD" 2>/dev/null; then
  _say "updating we-forge marker block in $GLOBAL_CLAUDE_MD (preserving user content)"
  if [ "$DRY_RUN" = "1" ]; then
    echo "  DRY: would replace block between $MARKER_START and $MARKER_END"
  else
    BACKUP="$GLOBAL_CLAUDE_MD.bak.$(date -u +%Y%m%dT%H%M%SZ)"
    cp -f "$GLOBAL_CLAUDE_MD" "$BACKUP"
    awk -v start="$MARKER_START" -v end="$MARKER_END" -v tpl="$TEMPLATE" '
      $0 == start { in_block=1; print start; while ((getline line < tpl) > 0) print line; print end; next }
      $0 == end   { in_block=0; next }
      !in_block   { print }
    ' "$GLOBAL_CLAUDE_MD" > "$GLOBAL_CLAUDE_MD.tmp"
    mv "$GLOBAL_CLAUDE_MD.tmp" "$GLOBAL_CLAUDE_MD"
  fi
else
  _say "appending we-forge marker block to existing $GLOBAL_CLAUDE_MD (preserving user content)"
  if [ "$DRY_RUN" = "1" ]; then
    echo "  DRY: would append marker-bounded we-forge block to bottom of file"
  else
    BACKUP="$GLOBAL_CLAUDE_MD.bak.$(date -u +%Y%m%dT%H%M%SZ)"
    cp -f "$GLOBAL_CLAUDE_MD" "$BACKUP"
    {
      printf '\n\n'
      printf '%s\n' "$MARKER_START"
      cat "$TEMPLATE"
      printf '%s\n' "$MARKER_END"
    } >> "$GLOBAL_CLAUDE_MD"
  fi
fi

_say "install complete."

if ! find "$CLAUDE_HOME/projects" -maxdepth 3 -name '*.jsonl' 2>/dev/null | head -1 | grep -q .; then
  _warn "no transcript .jsonl files found under $CLAUDE_HOME/projects/ yet."
  _warn "This is fine on a fresh install — the Stop hook will start logging on the first Claude session."
fi

# --- per-OS post-install guidance ---
# All paths in the generated plist / cron line derive from $CLAUDE_HOME and $HOME,
# so the script works on any user account (no "yukibana" hardcoding).

cat <<EON

Next steps:

  1. Review the merged settings.json:
       jq .hooks.Stop "$SETTINGS"

  2. Verify (any OS):
       bash $CLAUDE_HOME/learning/redact.sh --self-test
       echo '{"session_id":"t","transcript_path":"/dev/null","stop_hook_active":false,"cwd":"/tmp"}' \\
         | $CLAUDE_HOME/hooks/stop-telemetry.sh; echo "exit=\$?"

  3. Full end-to-end test:
       $REPO_DIR/install.sh --test

EON

# --- step 4: install we-forgectl + register service automatically ---

WFCTL_SRC="$REPO_DIR/scripts/we-forgectl"
WFCTL_DEST="$HOME/.local/bin/we-forgectl"

if [ -f "$WFCTL_SRC" ]; then
  _say "installing we-forgectl to $WFCTL_DEST"
  _run "mkdir -p \"$HOME/.local/bin\""
  _run "cp -f \"$WFCTL_SRC\" \"$WFCTL_DEST\""
  _run "chmod +x \"$WFCTL_DEST\""

  # Add ~/.local/bin to PATH for this session if missing
  case ":$PATH:" in
    *":$HOME/.local/bin:"*) ;;
    *)
      _warn "$HOME/.local/bin is not on your PATH. Add to ~/.zshrc or ~/.bashrc:"
      _warn "  export PATH=\"\$HOME/.local/bin:\$PATH\""
      ;;
  esac
else
  _warn "scripts/we-forgectl not found in repo; skipping service registration"
  NO_SERVICE=1
fi

if [ "$NO_SERVICE" = "1" ]; then
  cat <<EON

  4. (skipped — --no-service) register the service manually:
       we-forgectl install
       we-forgectl status

EON
else
  WFCTL_INSTALL_FLAGS=""
  [ "$ENABLE_TELEGRAM" = "1" ] && WFCTL_INSTALL_FLAGS="$WFCTL_INSTALL_FLAGS --enable-telegram"
  [ "$DAEMON_MODE" = "1" ]     && WFCTL_INSTALL_FLAGS="$WFCTL_INSTALL_FLAGS --daemon"

  if [ "$DRY_RUN" = "1" ]; then
    _say "DRY: would run: we-forgectl install$WFCTL_INSTALL_FLAGS"
  else
    _say "registering service via we-forgectl"
    "$WFCTL_DEST" install $WFCTL_INSTALL_FLAGS || _warn "we-forgectl install reported issues; run 'we-forgectl doctor' to diagnose"
  fi

  cat <<EON

  4. Service registered automatically. Useful commands:
       we-forgectl status        # service state
       we-forgectl tui           # rich-powered control TUI
       we-forgectl dashboard     # open web dashboard
       we-forgectl logs          # tail recent ticks
       we-forgectl uninstall     # one-line removal (with safety backup)

  5. Optional — Telegram notifier:
       export WE_FORGE_TELEGRAM_TOKEN=...    # from @BotFather
       export WE_FORGE_TELEGRAM_CHAT_ID=...
       we-forgectl install --enable-telegram
       we-forgectl notify-test

EON
fi
