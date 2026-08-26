#!/usr/bin/env bash
# Idempotent launcher for gitui in its own TAB — used by the `open-gitui-tab` action and a
# herdr keybinding. "Open-or-switch-or-toggle", scoped across the tabs of the current workspace:
#   - no gitui pane in this workspace        -> open a new tab (focused)
#   - gitui in ANOTHER tab of this workspace -> switch to that tab (no duplicate)
#   - gitui in the current tab, not focused  -> focus it in place
#   - the focused pane IS gitui              -> close it ("toggle off")
set -uo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
# shellcheck source=common.sh
source "$DIR/common.sh"

open_tab() {
  local root="$1" ws="${2:-}"
  local envarg
  envarg="$(safe_env_value "HERDR_GITUI_TARGET=$root")" || exit 1
  local args=(--plugin herdr-gitui --entrypoint gitui --placement tab)
  [ -n "$ws" ] && args+=(--workspace "$ws")
  args+=(--env "$envarg" --focus)
  exec "$herdr_bin" plugin pane open "${args[@]}"
}

root="$(resolve_project_root "$(resolve_active_cwd || printf '%s' "${HOME:-.}")")"

ws="$(resolve_active_workspace || true)"
cur_tab="${HERDR_ACTIVE_TAB_ID:-${HERDR_TAB_ID:-}}"
decision="OPEN"
if [ -n "$ws" ]; then
  panes="$(find_gitui_panes "$ws")"
  if [ -n "$panes" ]; then
    # Prefer to toggle-off a focused gitui pane first; otherwise either switch to its
    # tab (if in another tab) or focus it in place.
    rec="$(printf '%s' "$panes" | jq -s 'sort_by(.focused) | reverse | .[0]' 2>/dev/null || printf '%s' "$panes" | jq -s '.[0]' 2>/dev/null)"
    pid="$(printf '%s' "$rec" | jq -r '.pane_id // empty' 2>/dev/null || true)"
    tid="$(printf '%s' "$rec" | jq -r '.tab_id // empty' 2>/dev/null || true)"
    focused="$(printf '%s' "$rec" | jq -r '.focused // false' 2>/dev/null || true)"
    if [ -n "$pid" ]; then
      if [ "$focused" = "true" ]; then
        decision="CLOSE $pid"
      elif [ -n "$tid" ] && [ "$tid" != "$cur_tab" ]; then
        decision="SWITCHTAB $tid"
      else
        decision="FOCUS $pid"
      fi
    fi
  fi
fi

case "$decision" in
  "SWITCHTAB "*)
    tid="${decision#SWITCHTAB }"
    # If the target tab vanished between snapshot and now, fall back to opening a fresh tab.
    "$herdr_bin" tab focus "$tid" || open_tab "$root" "$ws"
    ;;
  "FOCUS "*)
    pid="${decision#FOCUS }"
    exec "$herdr_bin" plugin pane focus "$pid"
    ;;
  "CLOSE "*)
    pid="${decision#CLOSE }"
    exec "$herdr_bin" plugin pane close "$pid"
    ;;
  *)
    open_tab "$root" "$ws"
    ;;
esac