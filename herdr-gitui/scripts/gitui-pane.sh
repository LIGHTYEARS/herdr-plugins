#!/usr/bin/env bash
# Pane entrypoint for herdr-gitui. herdr launches this as the [[panes]] command, whose
# manifest command (`scripts/gitui-pane.sh`) is resolved relative to the PLUGIN ROOT.
# The open-* launchers pass the git project root via the HERDR_GITUI_TARGET env var
# (NOT --cwd: we verified that --cwd overrides the manifest command's resolution
# directory, breaking the relative `scripts/gitui-pane.sh` path). We cd to the target
# and exec gitui there, so it opens the repository the user is standing in.
#
# gitui is NOT bundled. Resolve it by, in order:
#   1. $HERDR_GITUI_BIN   (explicit override)
#   2. `gitui` on PATH
#   3. a few well-known install locations
set -uo pipefail

resolve_gitui() {
  local bin="${HERDR_GITUI_BIN:-}"
  if [ -n "$bin" ] && [ -x "$bin" ]; then
    printf '%s\n' "$bin"
    return 0
  fi
  bin="$(command -v gitui 2>/dev/null || true)"
  if [ -n "$bin" ]; then
    printf '%s\n' "$bin"
    return 0
  fi
  for candidate in \
    /home/linuxbrew/.linuxbrew/bin/gitui \
    /usr/local/bin/gitui \
    /opt/homebrew/bin/gitui \
    "$HOME/.cargo/bin/gitui" \
    "$HOME/.local/bin/gitui"
  do
    if [ -x "$candidate" ]; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done
  return 1
}

# Move into the target project directory (fall back to plugin root).
target="${HERDR_GITUI_TARGET:-}"
if [ -z "$target" ] || [ ! -d "$target" ]; then
  target="${HERDR_PLUGIN_ROOT:-$(pwd)}"
fi
cd "$target" || exit 1

bin="$(resolve_gitui || true)"
if [ -z "$bin" ]; then
  printf '%s\n' \
    "gitui not found." \
    "Install gitui (https://github.com/gitui-org/gitui), then either put it on PATH" \
    "or set HERDR_GITUI_BIN to its absolute path." >&2
  sleep 10
  exit 1
fi

exec "$bin"