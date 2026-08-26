#!/usr/bin/env bash
# Idempotent launcher for gitui in a SPLIT pane — used by the `open-gitui` action and a
# herdr keybinding (e.g. `prefix+i`). "Launch-or-focus-or-toggle", scoped to the current
# workspace:
#   - no gitui pane in this workspace -> open a split (focused)
#   - a gitui pane exists but isn't focused -> focus it
#   - the focused pane IS gitui -> close it ("toggle off")
#
# IMPORTANT: the git project root is passed to the pane via `--env HERDR_GITUI_TARGET=<dir>`,
# NOT `--cwd`. We verified (against the live herdr server) that `--cwd` overrides the
# working directory herdr uses to resolve the manifest pane command, which breaks the
# relative `scripts/gitui-pane.sh` path. Keeping the pane command cwd at the plugin root
# and letting gitui-pane.sh `cd` itself is the correct mechanism.
set -uo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
# shellcheck source=common.sh
source "$DIR/common.sh"

open_split() {
  local root="$1" tgt="${2:-}"
  local envarg out gpid
  envarg="$(safe_env_value "HERDR_GITUI_TARGET=$root")" || exit 1
  local args=(--plugin herdr-gitui --entrypoint gitui --placement split --direction right)
  [ -n "$tgt" ] && args+=(--target-pane "$tgt")
  args+=(--env "$envarg" --focus)
  out="$("$herdr_bin" plugin pane open "${args[@]}" 2>/dev/null || true)"
  gpid="$(printf '%s' "$out" | jq -r '.result.plugin_pane.pane.pane_id // empty' 2>/dev/null || true)"
  if [ -n "$gpid" ]; then
    # Let gitui take GITUI_TARGET_FRACTION (default 2/3) of the tab width.
    resize_gitui_to_fraction "$gpid"
  fi
}

root="$(resolve_project_root "$(resolve_active_cwd || printf '%s' "${HOME:-.}")")"

ws="$(resolve_active_workspace || true)"
decision="OPEN"
if [ -n "$ws" ]; then
  panes="$(find_gitui_panes "$ws")"
  if [ -n "$panes" ]; then
    # Prefer to toggle-off a focused gitui pane first; otherwise focus any gitui pane.
    focused_pid="$(printf '%s' "$panes" | jq -r 'select(.focused==true) | .pane_id // empty' 2>/dev/null | head -1)"
    if [ -n "$focused_pid" ]; then
      decision="CLOSE $focused_pid"
    else
      other_pid="$(printf '%s' "$panes" | jq -r '.pane_id // empty' 2>/dev/null | head -1)"
      if [ -n "$other_pid" ]; then
        decision="FOCUS $other_pid"
      fi
    fi
  fi
fi

case "$decision" in
  "FOCUS "*)
    pid="${decision#FOCUS }"
    exec "$herdr_bin" plugin pane focus "$pid"
    ;;
  "CLOSE "*)
    pid="${decision#CLOSE }"
    exec "$herdr_bin" plugin pane close "$pid"
    ;;
  *)
    # Split the ACTIVE pane (so gitui opens beside the current work).
    open_split "$root" "$(resolve_active_pane || true)"
    ;;
esac