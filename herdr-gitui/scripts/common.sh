#!/usr/bin/env bash
# Shared helpers for the herdr-gitui launchers.
# Expected to be `source`d by open-gitui.sh / open-gitui-tab.sh.
#
# Design notes (learned the same way the herdr-file-viewer plugin did):
#   * herdr actions run a `command`; there is no declarative "open this pane" knob,
#     so we shell out to the herdr CLI via $HERDR_BIN_PATH (herdr injects it; we fall
#     back to `herdr` on PATH).
#   * A pane command's default cwd is the PLUGIN ROOT, not the active pane's cwd. So
#     we resolve the git project root of the ACTIVE pane and pass it back into
#     `plugin pane open --cwd`, so gitui opens the repo the user is standing in.
#   * We stay as dumb as possible and degrade to plain "open" on any parse/failure.

set -uo pipefail

# Location of the herdr CLI.
herdr_bin="${HERDR_BIN_PATH:-herdr}"

# Active pane's working directory. Prefer the injected var; fall back to `pane current`.
resolve_active_cwd() {
  local cwd="${HERDR_ACTIVE_PANE_CWD:-}"
  if [ -n "$cwd" ] && [ -d "$cwd" ]; then
    printf '%s\n' "$cwd"
    return 0
  fi
  local cur
  cur="$("$herdr_bin" pane current 2>/dev/null || true)"
  if [ -n "$cur" ]; then
    cwd="$(printf '%s' "$cur" | jq -r '.result.pane.foreground_cwd // .result.pane.cwd // empty' 2>/dev/null || true)"
    if [ -n "$cwd" ] && [ "$cwd" != "null" ] && [ -d "$cwd" ]; then
      printf '%s\n' "$cwd"
      return 0
    fi
  fi
  return 1
}

# Resolve a working directory up to the git project root (gitui shows the whole repo).
# Falls back to the given dir if not inside a git work tree.
resolve_project_root() {
  local dir="$1"
  local root
  root="$(git -C "$dir" rev-parse --show-toplevel 2>/dev/null || true)"
  if [ -n "$root" ] && [ -d "$root" ]; then
    printf '%s\n' "$root"
  else
    printf '%s\n' "$dir"
  fi
}

# Active pane id (prefer injected var, else parse from `pane current`).
resolve_active_pane() {
  local pane="${HERDR_ACTIVE_PANE_ID:-${HERDR_PANE_ID:-}}"
  if [ -n "$pane" ]; then
    printf '%s\n' "$pane"
    return 0
  fi
  local cur
  cur="$("$herdr_bin" pane current 2>/dev/null || true)"
  if [ -n "$cur" ]; then
    pane="$(printf '%s' "$cur" | jq -r '.result.pane.pane_id // empty' 2>/dev/null || true)"
    if [ -n "$pane" ] && [ "$pane" != "null" ]; then
      printf '%s\n' "$pane"
      return 0
    fi
  fi
  return 1
}

# Active workspace id (prefer injected var, else parse from `pane current`).
resolve_active_workspace() {
  local ws="${HERDR_ACTIVE_WORKSPACE_ID:-${HERDR_WORKSPACE_ID:-}}"
  if [ -n "$ws" ]; then
    printf '%s\n' "$ws"
    return 0
  fi
  local cur
  cur="$("$herdr_bin" pane current 2>/dev/null || true)"
  if [ -n "$cur" ]; then
    ws="$(printf '%s' "$cur" | jq -r '.result.pane.workspace_id // empty' 2>/dev/null || true)"
    if [ -n "$ws" ] && [ "$ws" != "null" ]; then
      printf '%s\n' "$ws"
      return 0
    fi
  fi
  return 1
}

# Target width fraction (0..1) that gitui should occupy in a split pane.
# Default 0.667 = 2/3 of the tab width. Override per-open with HERDR_GITUI_RATIO.
GITUI_TARGET_FRACTION="${HERDR_GITUI_RATIO:-0.667}"

# Resize a just-opened gitui split pane so gitui occupies GITUI_TARGET_FRACTION of the
# width. Herdr's `plugin pane open --placement split` creates a 50/50 split with no
# ratio knob, so we read the current top-level split ratio and move the divider with
# `pane resize --direction left` (which expands the right pane), by however much is
# needed to reach the target. Best-effort: any failure falls through silently.
resize_gitui_to_fraction() {
  local pid="$1"
  local layout current target_left delta left_w total_w
  layout="$("$herdr_bin" pane layout --pane "$pid" 2>/dev/null || true)"
  if [ -z "$layout" ]; then return 0; fi
  # Top-level (first) split's ratio = left pane fraction when the pane was just split.
  current="$(printf '%s' "$layout" | jq -r '.result.layout.splits[0].ratio // empty' 2>/dev/null || true)"
  [ -z "$current" ] || [ "$current" = "null" ] && return 0
  target_left="$(awk -v f="$GITUI_TARGET_FRACTION" 'BEGIN{printf "%.4f", 1-f}' 2>/dev/null || printf '0.3333')"
  delta="$(awk -v c="$current" -v t="$target_left" 'BEGIN{printf "%.4f", c-t}' 2>/dev/null || printf '0.1667')"
  # Ignore tiny / non-positive adjustments.
  awk -v d="$delta" 'BEGIN{exit !(d>0.005)}' || return 0
  "$herdr_bin" pane resize --direction left --amount "$delta" --pane "$pid" >/dev/null 2>&1 || true
}

# Sanitize a path for embedding in a herdr `--env KEY=VALUE` argument.
# Options are passed as a single argv, so a newline would split it; reject control chars.
safe_env_value() {
  local v="$1"
  case "$v" in
    *$'\n'* | *$'\r'*) printf '%s\n' "unsafe path: newline in HERDR_GITUI_TARGET" >&2; return 1 ;;
  esac
  printf '%s\n' "$v"
}

# List the plugin's gitui panes (label == the pane title "Git") in the given workspace.
# Emits one JSON-encoded record per pane (pane_id, tab_id, focused) on separate lines.
find_gitui_panes() {
  local ws="$1"
  local list
  list="$("$herdr_bin" pane list 2>/dev/null || true)"
  if [ -z "$list" ]; then
    return 0
  fi
  printf '%s' "$list" | jq -r --arg ws "$ws" '
    .result.panes[] |
    select(.workspace_id == $ws and .label == "Git") |
    { pane_id: .pane_id, tab_id: .tab_id, focused: .focused }
  ' 2>/dev/null
}