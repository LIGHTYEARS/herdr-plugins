# herdr-plugins

Local herdr plugins:

| Plugin | Description |
| --- | --- |
| [`herdr-gitui`](herdr-gitui/) | Open **gitui** in a herdr split/tab to browse the current git project. |
| [`herdr-sidebar`](herdr-sidebar/) | VS Code-style file explorer, search, and source-control sidebar. |

`herdr-sidebar/` is copied from [alexarthurs/herdr-sidebar](https://github.com/alexarthurs/herdr-sidebar),
upstream commit `1a5d37ef84edc91e5b3d3d4e39daa32952e6ecf2`
(`plugins/herdr-sidebar/`). Its MIT license is preserved in
[`herdr-sidebar/LICENSE`](herdr-sidebar/LICENSE). For local development, run
`cargo build --release` inside `herdr-sidebar/`, then link that directory.

## Link a plugin

```bash
herdr plugin link ./<plugin-dir>
```