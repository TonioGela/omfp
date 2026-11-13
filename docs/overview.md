# omfp overview

omfp is a GUI client for the [omp](https://github.com/can1357/oh-my-pi) coding agent (oh-my-pi), in the browser and as
a desktop app on Linux and macOS. A daemon runs one omp process per thread, each in a workspace folder whose entries
are your repositories, and serves a web UI over WebSocket; threads keep running when the window closes. The agent can
build widgets for the UI itself, and you can talk to it with live voice.

## Features

- Threads grouped by workspace folder; omp gets every repository of the folder as a root, with its `AGENTS.md`.
- Transcript with streaming Markdown, thinking blocks and tool cards for `bash`, `read`, `edit`/`write`, `grep`,
  `glob`, `todo`, `web_search`, `task` and more; approvals and `ask` forms answered in place; queued follow-ups and
  steering.
- Views as layout trees in `.omfp/views.json` (or TSX views and init files), written by the agent, with `:widget add`
  in the window; widgets are React components the
  agent writes and checks without a browser (`omfp/sdk`), next to built-ins for changes and diffs, files (CodeMirror
  with vim keys), subagents, todos and the session tree.
- Live voice through omp's live session, with a transcript and mute.
- Keyboard first: vim-style modes, a `:` command line, `?` help, per-workspace key bindings an agent can add.
- Desktop app (Tauri: WebKitGTK on Linux, WKWebView on macOS) sharing a background daemon with the browser, with a tray
  icon (a menu bar item on macOS); system notifications and in-window notices when a thread needs you.
- Themes from tokens: built-in light and dark, matugen output or your own CSS, with a contrast guard for unreadable
  roles. Narrow windows and phones get a strip and a bottom bar.

## Run

Everything runs inside the Nix dev shell:

```sh
nix develop
just serve      # build the web UI and run the daemon with your own ~/.omp; open the printed URL
just desktop    # the desktop window on your local daemon (started in the background if none runs)
```

`just --list` shows the other recipes (tests, lints, fixtures); [AGENTS.md](../AGENTS.md) is the contributor guide.

## Agents outside omfp (MCP)

Threads run by omfp get its agent tools (`omfp_call` for every protocol method, `omfp_thread_wait`, the UI mount and
component tools) directly. Any other agent, such as an omp session in a terminal or another MCP client, reaches the
same tools through the daemon's MCP server at `/mcp` (Streamable HTTP, with the daemon's token as bearer):

```sh
omfp mcp config > ~/.omp/agent/mcp.json   # or merge its "omfp" entry into an existing mcp.json
export OMFP_TOKEN="$(cat ~/.local/share/omfp/token)"   # where omp runs, e.g. in your shell profile
```

prints the entry for the running local daemon (`--daemon <url>` for another one). omp then offers the tools as
`xd://mcp__omfp_call` and so on; `"timeout": 0` in the entry lets `omfp_thread_wait` wait longer than omp's 30 s
default. The entry's header is `"Bearer ${OMFP_TOKEN}"`: omp reads the token from the environment, so no file holds
it. The token grants full access to the daemon (including shell commands): `--literal-token` writes it into the entry
instead, but never put such an entry in a project's `.omp/mcp.json`, which may be committed. Threads and workspaces
such a client creates show its name as their origin, and its actions appear in the `omfp:activity` widget. See
[08-agent-control](specs/08-agent-control.md#mcp-server-m10) for the details.

## Install

`nix build .#omfp` (or the flake's `packages.<system>.omfp` in your configuration) builds `bin/omfp` with the desktop
window, the web UI and the pinned omp, on `x86_64-linux`, `aarch64-linux` and `aarch64-darwin`; `.#omfp-headless` is
the daemon alone, for servers. On macOS the package also holds `Applications/omfp.app`: in `environment.systemPackages`
of nix-darwin it shows up in `/Applications/Nix Apps`, in `home.packages` of home-manager in
`~/Applications/Home Manager Apps`, so Spotlight and Launchpad find it. The app is ad-hoc signed: macOS asks for the
microphone again after each update. The daemon keeps its data in `~/.local/share/omfp` on Linux and
`~/Library/Application Support/omfp` on macOS (`XDG_DATA_HOME` wins on both); the config and theme files are in
`~/.config/omfp` (the keys: [Settings](../README.md#settings) in the README).

## Status

Pre-1.0 and under active development: the done milestones and the next ones are in the
[roadmap](specs/07-roadmap.md). The specifications are in [docs/specs](specs/README.md).

## Licence

GPL-3.0-or-later, see [LICENSE](../LICENSE). The bundled fonts are under the SIL Open Font License
(`ui/packages/kit/fonts/`); the logo in [docs/brand](brand/README.md) is drawn from Commit Mono's outlines.
