# omfp overview

omfp is a GUI client for the [omp](https://github.com/can1357/oh-my-pi) coding agent (oh-my-pi), in the browser and as
a Linux desktop app. A daemon runs one omp process per thread, each in a workspace folder whose entries are your
repositories, and serves a web UI over WebSocket; threads keep running when the window closes. The agent can build
widgets for the UI itself, and you can talk to it with live voice.

## Features

- Threads grouped by workspace folder; omp gets every repository of the folder as a root, with its `AGENTS.md`.
- Transcript with streaming Markdown, thinking blocks and tool cards for `bash`, `read`, `edit`/`write`, `grep`,
  `glob`, `todo`, `web_search`, `task` and more; approvals and `ask` forms answered in place; queued follow-ups and
  steering.
- Views as layout trees in `.omfp/views.json`, editable from the UI or by the agent; widgets are React components the
  agent writes and checks without a browser (`omfp/sdk`), next to built-ins for changes and diffs, files (CodeMirror
  with vim keys), subagents, todos and the session tree.
- Live voice through omp's live session, with a transcript and mute.
- Keyboard first: vim-style modes, a `:` command line, `?` help, per-workspace key bindings an agent can add.
- Desktop app (Tauri, WebKitGTK) sharing a background daemon with the browser, with a tray icon; system notifications
  and in-window notices when a thread needs you.
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

## Status

Pre-1.0 and under active development: the done milestones and the next ones are in the
[roadmap](specs/07-roadmap.md). The specifications are in [docs/specs](specs/README.md).

## Licence

GPL-3.0-or-later, see [LICENSE](../LICENSE). The bundled fonts are under the SIL Open Font License
(`ui/packages/kit/fonts/`); the logo in [docs/brand](brand/README.md) is drawn from Commit Mono's outlines.
