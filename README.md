# herdr-plugins

Local herdr plugins:

| Plugin | Description |
| --- | --- |
| [`herdr-gitui`](herdr-gitui/) | Open **gitui** in a herdr split/tab to browse the current git project. |
| [`herdr-sidebar`](herdr-sidebar/) | VS Code-style file explorer, search, and source-control sidebar. |

`herdr-sidebar/` is copied from [alexarthurs/herdr-sidebar](https://github.com/alexarthurs/herdr-sidebar),
upstream commit `1a5d37ef84edc91e5b3d3d4e39daa32952e6ecf2`
(`plugins/herdr-sidebar/`). Its MIT license is preserved in
[`herdr-sidebar/LICENSE`](herdr-sidebar/LICENSE).

## Install herdr-sidebar

Install this fork (including the **Δ Workspace** overview) from GitHub:

```sh
herdr plugin install LIGHTYEARS/herdr-plugins/herdr-sidebar
```

For local development, build and link this checkout instead:

```sh
cd herdr-sidebar
cargo build --release --locked
herdr plugin link .
```

See the [sidebar README](herdr-sidebar/README.md#install-from-this-repository) for requirements
and the difference from the upstream plugin.