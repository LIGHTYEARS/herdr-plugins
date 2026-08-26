# herdr-gitui

A thin [herdr](https://herdr.dev) plugin that launches **gitui** (the TUI git client) beside
your current work to browse the git repository you're standing in.

- **Not bundled.** gitui must already be installed (it is launched on your `PATH`, or at
  `$HERDR_GITUI_BIN` / a few well-known locations). Install gitui from
  [github.com/gitui-org/gitui](https://github.com/gitui-org/gitui).
- **Points at your project.** It resolves the git project root of the currently focused pane
  and opens gitui there — so you see the whole repo, not just a subdirectory.
- **gitui takes 2/3 of the width.** When opened as a split, gitui is sized to two-thirds of the
  tab by default (override with `HERDR_GITUI_RATIO`).
- **Open-or-focus-or-toggle.** Invoke it repeatedly and it behaves how you'd expect (scoped to
  the current workspace): opens a split/tab, focuses it if you switched away, and toggles it
  off when you hit it again.
- **No event hooks.** gitui appears only on an explicit action; it never pops up on its own.

## Install

Clone/link the plugin locally, then enable it:

```bash
# from this repo root
herdr plugin link ./herdr-gitui
herdr plugin enable herdr-gitui
```

## Bind a key

Add a `plugin_action` keybinding to `~/.config/herdr/config.toml` and reload the config
(`herdr server reload-config`):

```toml
[[keys.command]]
key = "prefix+i"          # open gitui in a SPLIT pane, beside your work
type = "plugin_action"
command = "herdr-gitui.open-gitui"
description = "open gitui for current git project"

[[keys.command]]
key = "prefix+shift+i"    # open gitui in its own TAB
type = "plugin_action"
command = "herdr-gitui.open-gitui-tab"
description = "open gitui in a tab for current git project"
```

You can also invoke the actions ad-hoc:

```bash
herdr plugin action invoke open-gitui --plugin herdr-gitui
herdr plugin action invoke open-gitui-tab --plugin herdr-gitui
```

## How it works

| File | Role |
| --- | --- |
| `herdr-plugin.toml` | Manifest: declares the `gitui` pane and the `open-gitui` / `open-gitui-tab` actions. |
| `scripts/gitui-pane.sh` | The pane command. `cd`s to `$HERDR_GITUI_TARGET` (the project root passed by the launcher), resolves the gitui binary, and `exec`s it. |
| `scripts/open-gitui.sh` | Split-pane launcher: resolve project root → open-or-focus-or-toggle a split. |
| `scripts/open-gitui-tab.sh` | Tab launcher: resolve project root → open-or-switch-or-focus-or-toggle a tab. |
| `scripts/common.sh` | Shared helpers: active cwd / project root / workspace resolution, pane discovery via `jq`. |

### Why the project root is passed as env, not `--cwd`

Herdr launches a pane command whose argv is resolved relative to the **plugin root**. We
discovered (by testing against the live server) that passing `--cwd` to
`plugin pane open` overrides that resolution directory — so the manifest's relative
`scripts/gitui-pane.sh` path fails and the pane dies instantly. The correct mechanism is to
keep the pane command's cwd at the plugin root and hand the project root over as an
environment variable:

```bash
herdr plugin pane open \
  --plugin herdr-gitui --entrypoint gitui \
  --placement split --direction right \
  --env "HERDR_GITUI_TARGET=$root" --focus
```

`gitui-pane.sh` then `cd "$HERDR_GITUI_TARGET" && exec "$bin"`.

## Configuration

- `HERDR_GITUI_TARGET` — set automatically by the launchers; you can override per pane-open.
- `HERDR_GITUI_BIN` — absolute path to a `gitui` binary to use instead of `PATH` lookup.
- `HERDR_GITUI_RATIO` — fraction of the tab width that gitui occupies when opened as a split.
  Default `0.667` (2/3). E.g. `HERDR_GITUI_RATIO=0.5` for a 50/50 split.
- `jq` is required by the launchers (for `pane list` JSON parsing); installed by default on
  most systems.

### Making the split 2/3 wide

`plugin pane open` has no split-ratio knob, so the launcher opens the split, then reads the
current ratio via `pane layout` and moves the divider with `pane resize --direction left`
(which widens the pane on the right) until gitui reaches `GITUI_TARGET_FRACTION`. Split
entrypoints target the active pane with `--target-pane`, so gitui always splits beside your
current work (matching the workspace you're actually looking at).

## Platform notes

Tested on Linux against herdr 0.8.2 and gitui 0.28.x. The manifest is declared for
`linux` and `macos`. Windows is not currently handled.