#!/usr/bin/env bash
# AP0DL crash-recovery watchdog.
#
# Watches the OpenCode TUI process for this project. If the TUI disappears
# before /tmp/ap0dl-goal-complete exists, it resumes the exact same session and
# points the recovery agent at docs/goal-checklist.md, so completed checkpoints
# are not redone.
#
# Start detached:
#   setsid nohup scripts/ap0dl-watchdog.sh >/tmp/ap0dl-watchdog-launch.log 2>&1 &
#
# Stop/disarm:
#   touch /tmp/ap0dl-goal-complete
#   kill "$(cat /tmp/ap0dl-watchdog/pid 2>/dev/null)" 2>/dev/null || true
set -uo pipefail

PROJECT="${AP0DL_PROJECT:-/root/ap0dl}"
SESSION="${OPENCODE_SESSION_ID:-}"
STATE_DIR="${AP0DL_WATCHDOG_STATE:-/tmp/ap0dl-watchdog}"
PID_FILE="$STATE_DIR/pid"
HEARTBEAT="$STATE_DIR/heartbeat"
LOG="$STATE_DIR/watchdog.log"
DONE_MARKER="${AP0DL_GOAL_MARKER:-/tmp/ap0dl-goal-complete}"
MAX_ATTEMPTS="${AP0DL_WATCHDOG_MAX_ATTEMPTS:-5}"
CHECK_SECONDS="${AP0DL_WATCHDOG_CHECK_SECONDS:-15}"

if [[ -z "$SESSION" ]]; then
  echo "OPENCODE_SESSION_ID is required" >&2
  exit 64
fi
if [[ ! -d "$PROJECT/docs" ]]; then
  echo "project not found: $PROJECT" >&2
  exit 66
fi

mkdir -p "$STATE_DIR"
if [[ -f "$PID_FILE" ]] && kill -0 "$(cat "$PID_FILE" 2>/dev/null)" 2>/dev/null; then
  echo "watchdog already running: $(cat "$PID_FILE")"
  exit 0
fi
echo $$ > "$PID_FILE"
trap 'rm -f "$PID_FILE"' EXIT INT TERM

log() {
  printf '%s %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$*" >> "$LOG"
}

tui_running() {
  pgrep -x opencode >/dev/null 2>&1
}

log "watchdog armed session=$SESSION project=$PROJECT pid=$$"
attempts=0

while true; do
  if [[ -f "$DONE_MARKER" ]]; then
    log "completion marker found; watchdog disarming"
    exit 0
  fi

  if tui_running; then
    date -u +%Y-%m-%dT%H:%M:%SZ > "$HEARTBEAT"
    attempts=0
    sleep "$CHECK_SECONDS"
    continue
  fi

  # Give the terminal a few seconds to come back (reload, restart, etc.)
  sleep 8
  if [[ -f "$DONE_MARKER" ]]; then
    log "completion marker found during grace period; disarming"
    exit 0
  fi
  if tui_running; then
    log "OpenCode TUI returned during grace period"
    continue
  fi

  if (( attempts >= MAX_ATTEMPTS )); then
    log "recovery attempts exhausted ($attempts); leaving state for manual resume"
    exit 75
  fi
  attempts=$((attempts + 1))
  log "OpenCode TUI is gone; recovery attempt $attempts/$MAX_ATTEMPTS"

  prompt="Watchdog recovery for the AP0DL app goal. Resume this exact session and finish the work without asking for input. First read docs/goal-checklist.md and docs/evidence/ if present, then inspect git status and the latest diff. Continue from the first checklist item that is [~] or [ ], never redo an [x] item. Implement and verify the AP0DL app: rebrand/copy, minimalist dashboard and card fixes, circular continue-watching progress with minutes left and <95% completion, Aniyomi/Netflix-style player gestures with skip intro/outro, AniTrendz trending, AniList schedule, team credits and the 294M token counter. Keep Rust provider/application logic and Kotlin UI/player glue only. Build, test on the emulator with screenshots, update docs/goal-checklist.md and docs/evidence/, commit and push to origin/main, then touch /tmp/ap0dl-goal-complete so this watchdog disarms. Do not commit the test media folders."

  cd "$PROJECT" || exit 66
  timeout 7200 opencode run --session "$SESSION" --auto --agent build "$prompt" >> "$LOG" 2>&1
  status=$?
  log "recovery run exited with status $status"

  if [[ -f "$DONE_MARKER" ]]; then
    log "completion marker written; watchdog disarming"
    exit 0
  fi
  sleep $((30 * attempts))
done