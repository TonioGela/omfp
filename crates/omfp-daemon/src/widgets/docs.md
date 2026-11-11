# omfp widgets

A widget is a piece of the omfp window written in React. omfp shows the widgets a workspace's **views manifest**
lists, reloads a widget as soon as one of its files changes, and shows any error in the widget's place.

## Files

- `<workspace>/.omfp/views.json` — the views of the window (schema: `omfp://schemas/views.json`). Without it the window
  shows the default view:
  `["row", { "sizes": [3, 1] }, ["col", {}, "omfp:header", "omfp:chat"], ["tabs", {}, "omfp:workspace", "omfp:changes"]]`.
- `<workspace>/.omfp/widgets/<name>/index.tsx` (or `index.ts`, `index.jsx`, `index.js`) — a workspace widget,
  referenced as `./widgets/<name>`. Its default export is a React component; its props are the layout node's props.
  Other files of the folder are imported with relative paths (`./chart.tsx` or `./chart`).
- `<data-dir>/widgets/<name>/` — the user library, referenced as `user:<name>` in every workspace.
- A workspace widget named like a built-in (`.omfp/widgets/chat/` for `omfp:chat`) replaces the built-in in that
  workspace. The built-ins' sources can be copied as a starting point (`:widget copy omfp:workspace` in the window).
- `<workspace>/.omfp/keymap.json` — this workspace's key bindings (schema: `omfp://schemas/keymap.json`, see Keys).

```json
{
  "$schema": "omfp://schemas/views.json",
  "default": "work",
  "views": {
    "work": {
      "title": "Work",
      "layout": ["row", { "sizes": [3, 1] },
        ["col", {}, "omfp:header", "omfp:chat"],
        ["tabs", {}, "omfp:workspace", ["./widgets/tests", { "command": "make test" }]]]
    }
  }
}
```

Layout nodes: a reference (`"omfp:chat"`), `[reference, props]`, or `[container, props, ...children]` where the
container is `row`, `col`, `tabs` or `stack` and `props.sizes` (optional) gives one positive flex weight per child.
Built-in widgets: `omfp:header`, `omfp:chat` (transcript, interactions, queue and composer), `omfp:transcript`,
`omfp:interactions`, `omfp:composer`, `omfp:workspace`, `omfp:changes` (changed files and diffs of the workspace's
repositories), `omfp:subagents` (the selected thread's subagents, their transcripts, steer and cancel), `omfp:todos`
(its todo list and goal), `omfp:tree` (its session tree: go to an entry, fork from it, label it), `omfp:activity`
(`{ threadId? }`: the activity log of what agents did through omfp, `activity.list`), `omfp:file`
(`{ path, line?, diff? }`: a file, viewed, edited with vim bindings, diffed against
`HEAD`). Files opened by the user open as tabs after the children of the `tabs` container with `"id": "main"`, never
written to `views.json`. Props every widget accepts: `instance` (a string naming the instance for `useStored` and `useJob`;
default: the reference) and `title` (a tab's label; default: the built-in's name, else the reference).

## Mounts, panels and the library

An agent shows UI to the user without editing `views.json` by hand:

- `omfp_ui_mount { id, code | widget, props?, view?, where?, panel? }` mounts a widget: `code` (a component's source,
  written to `.omfp/widgets/<id>/index.tsx`) or `widget` (a reference: `user:<name>`, `./widgets/<name>`). It goes
  into the view `view` (default: the default view; a new id creates a view showing only it, a dashboard) at `where`
  (`side-tab`, `main-tab`, `end`), or with `panel: true` floats over every view. The node gets `instance: <id>`, so
  `useStored` state follows the mount; mounting the same id again replaces it.
- `omfp_ui_update { id, props?, code? }` merges `props` into the mounted widget (`null` deletes one) or replaces its
  inline code; windows re-render it in place, its state kept. Drive a mounted widget (a progress, a status) this way.
- `omfp_ui_remove { id }` removes it (a view it filled goes too); its folder stays.
- Floating panels are `"panels": { "<id>": <node> }` in `views.json`: the `title` prop labels the panel, the user moves,
  resizes, minimises and closes it (closing removes it from the file).
- The component library is the user library: `omfp_component_save { name, widget | code, replace? }` saves a
  workspace widget or code as `user:<name>`, `omfp_component_list` lists it. Start a component's entry with a comment
  saying what it shows and which props it takes: the list shows it.
- Widgets reach the same through `useCall()`: `views.mount`, `views.mount_update`, `views.unmount`, `widgets.save`,
  `widgets.library`.

## Keys

`<workspace>/.omfp/keymap.json` binds keys to commands in the windows showing this workspace, over the default keys and
the user's `[keymap]` config: entries apply in file order, an entry replaces the binding with the same mode and keys,
and this file wins. omfp only reads it; while it is invalid, the window shows its error and keeps the last valid
bindings. A binding runs a built-in command (`:help` lists the keys and commands) or one a widget registers with
`useCommand` (it runs while the widget is shown), so a widget can ship a command and the key that runs it:

```json
{
  "$schema": "omfp://schemas/keymap.json",
  "bindings": [
    { "keys": "Space T", "command": "tests.run" },
    { "keys": "g l", "command": "view.switch", "args": ["logs"] },
    { "mode": "insert", "keys": "Ctrl-t", "command": "tests.run" },
    { "keys": "Space x", "command": "" }
  ]
}
```

`mode` is `normal` (the default: the focus is not in a text field), `insert` (a text field has it) or `cmdline` (the `:`
line is open). `keys` are tokens separated by spaces: a printable key is its character (`j`, `G`, `:`), other keys
their name (`Space`, `Esc`, `Enter`, `Tab`), prefixed by `Ctrl-`, `Alt-`, `Meta-` (`g g`, `Space t`, `Ctrl-k`). `args`
are passed to the command's `run(args)`; `"command": ""` removes a binding.

## Imports

Only these specifiers resolve; everything else must be a relative file of the widget's folder:

- `react`, `react/jsx-runtime`, `react-dom` — the window's own React 19 (hooks work across widgets and the shell).
- `omfp/sdk` — hooks over the daemon (below).
- `omfp/kit` — the shell's components (below), styled by the theme.

JSX needs no import. TypeScript types are stripped, not checked. CSS files cannot be imported: use the kit, inline
`style`, or the theme's custom properties (`var(--omfp-primary)`, `var(--omfp-surface-container)`,
`var(--omfp-on-surface)`, `var(--omfp-outline)`, `var(--omfp-s2)`, `var(--omfp-fs-sm)`, `var(--omfp-font-code)`, …).

```tsx
import { Button, Card, Log, Progress } from "omfp/kit";
import { useJob } from "omfp/sdk";

export default function Tests({ command = "make test" }: { command?: string }) {
  const job = useJob(command);
  const done = job.stdout.filter((line) => line.startsWith("ok")).length;
  const running = job.status === "running";
  return (
    <Card title={`Tests ${job.status} (run ${job.runs}${job.exitCode === null ? "" : `, exit ${job.exitCode}`})`}>
      <Progress label="Tests" value={done} max={20} />
      <Log lines={job.lines.slice(-10)} />
      <Button onPress={running ? job.kill : job.restart}>{running ? "Stop" : "Run again"}</Button>
    </Card>
  );
}
```

## omfp/sdk

| Export | What it gives |
| --- | --- |
| `useWidget()` | `{ instance, reference, workspaceId }` of the calling widget. |
| `useStored(key, initial)` | `[value, set, loaded]`: JSON state the daemon keeps per workspace, widget instance and key, shared live by every window and kept across reloads and restarts. `set(value)` or `set((previous) => next)`; `set(null)` deletes the key. `loaded` is `false` while the state has not arrived yet (`value` is then `initial`). |
| `useJob(command)` | `{ status, lines, stdout, stderr, exitCode, signal, runs, startedAt, endedAt, state, restart, kill }` for `sh -c <command>` run by the daemon in the workspace folder. `status` is `"pending"` (no job yet, or a start or restart on its way), `"running"`, `"succeeded"` (exit code 0) or `"failed"` (another code, or a signal). The first mount starts the job; reloads, other windows and moving the widget follow the same job (stored as `job:<command>`: `{ id, runs }`). `lines` are stdout and stderr interleaved, `stdout` and `stderr` each one's lines; `runs` counts the jobs started (a restart adds one), `startedAt`/`endedAt` (ms) date the current one; `restart()` kills a running job and starts a new one; `state` is the raw `job/<id>` state. `omfp_widget_check` shows each job's status and last lines. |
| `useToolRun(toolCallId)` | A tool run of the selected thread, or `useToolRun({ match: /nextest/, threadId? })` for the latest run whose command matches: `{ toolCallId, toolName, status, output, lines, isError }` or `undefined`. While a `bash` command runs, `output` is the latest tail omp streams. |
| `useCall()` | `call(method, params)`: any daemon protocol method, e.g. `call("thread.prompt", { threadId, text })`. |
| `useTopic(topic)` | Follows a topic (`index`, `thread/<id>`, `workspace/<id>`, `views/<id>`, `job/<id>`) while mounted; returns its subscription state. |
| `useIndex()` | Workspaces, threads, theme and settings (the `index` topic). |
| `useThreads()` | Thread summaries by id. |
| `useThreadSummary(threadId)` | One thread's summary (title, process state, workspace). |
| `useThread(threadId)` | `{ state, closed }`: the thread's full state (transcript, tools, queue, pending interactions). |
| `useWorkspaces()` | Workspaces by id. |
| `useWorkspace(workspaceId)` | The workspace and its repositories' git status. |
| `useViews(workspaceId)` | The workspace's views manifest, keymap (`keymap`, `keymapError`), widget folders and stored state (the `views/<id>` topic). |
| `useConnection()` | The connection status. |
| `useWindowState(key)` | Window-local state; `SELECTED_THREAD` is the thread this window shows. |
| `useCurrentWorkspaceId()` | The workspace this window works in: the selected thread's, else the draft thread's. |
| `useThreadActions()` | `select`, `rename`, `archive`, `stop`, `restart`, … for threads. |
| `useCommand(command)` | Registers a `:` command or key binding target while mounted. |
| `useAnnounce()` | `announce(message)` for screen readers. |
| `useNotify()` | `notify({ title, body, threadId? })`: tells the user something without covering anything. While the window has focus it is a notice at the right end of the mode line (`◆ title body`, `g n` or a click opens it, gone after 8 s); while it has not, a system notification (when the user allowed them; `[ui] notifications = "never"` shows neither). With `threadId`, opening it selects that thread, visiting the thread clears it, and nothing is shown about the thread the user is on. Use it for results worth a look (a build turned red, a long job finished), not for every update. |
| `useSettings()` | The daemon's UI settings. |
| `useFile(path, { follow? })` | `{ status, file, error, reload, save }`: a file of the current workspace (`workspaces.read_file`: `file.content`, `file.hash`, `file.head`, …). `status` is `"loading"`, `"ready"`, `"missing"` or `"error"`. With `follow` (default `true`) it is checked every 2 s and on every git status change. `save(content, expectedHash?)` writes it only if the file still has that hash (default: the one read) and resolves to `{ ok: true, file }` or `{ ok: false, conflict, error }`. |
| `useLive()` | `[live, snapshot]`: this window's live voice session (M6). `live.start(threadId)` opens the microphone and a WebRTC call relayed to omp's realtime voice (`thread.live_connect`), or, without WebRTC on the daemon's machine (`#local=1`, the desktop app), omp's own session (`thread.live_start`, the spectrum then draws omp's levels), `live.end()`, `live.endThread(threadId)` (any window's session), `live.toggleMute()`, `live.source()` (the audio for `Spectrum`), `live.support()` (`"webrtc"`, `"host"`, `"insecure"`, `"unsupported"`); `snapshot.local` is the session (`idle`, `starting`, `running`, `failed`). The thread's `state.live` has omp's phase and transcript. |
| `useOpenFile()` | `openFile(path, { line?, diff? })`: opens a file of the current workspace in a tab next to the chat (the view's `tabs` with `"id": "main"`; keep that slot in your views), else as a tab of a floating panel; `diff: true` shows it against `HEAD`. A bare or partial name (`theme.ts`, `src/theme.ts`) that does not exist as given opens the file it names, or lets the user pick when several match. |
| `useMissingFile(path, missing, { line?, diff? })` | For a widget showing a file: while `missing`, looks for the file `path` names (`workspaces.find_file`); one match opens in its place, several open the window's picker. Gives `"finding"`, `"opened"`, `"choosing"` or `"none"`. |
| `SELECTED_THREAD` | Window key of the selected thread id (`useWindowState(SELECTED_THREAD)`). |

The SDK also exports the view models and strings the built-in widgets use; read a copied built-in to see them.

## omfp/kit

| Export | What it is |
| --- | --- |
| `Card` | A bordered surface: `title`, `actions`, `tone` (`default`, `error`, `warning`, `success`). |
| `Button` | A button: `onPress`, `variant` (`primary`, `secondary`, `quiet`, `danger`), `size` (`sm`, `md`), `isDisabled`. |
| `IconButton` | An icon-only button: `label`, `icon`, `onPress`. |
| `Progress` | A progress bar: `label` (required), `value` (omit for indeterminate), `max` (default 100), `hideLabel`. |
| `Spectrum` | The live voice spectrum: `source` (`useLive()[0].source()`), `status`, `label`; a static meter below motion `full`. |
| `LivePanel` | The live voice panel: `status`, `statusLabel`, `spectrum`, `lines`, `message`, `hints`, `actions`. |
| `Log` | Monospace lines that follow the end: `lines`. |
| `Badge` | A small status label: `tone` (`default`, `primary`, `error`, `warning`, `success`). |
| `Chip` | A compact labelled item: `tone`, `onRemove`. |
| `Markdown` | Renders Markdown text (no raw HTML): `text`. |
| `Code` | Code: `code`, `block`, `language` (a name or file extension; highlights a block). |
| `Diff` | A unified patch (`git diff` output) as numbered lines: `patch`, `language` (highlighting). |
| `languageOfPath` | `languageOfPath("src/lib.rs")` → `"rust"`: the language to pass to `Code` or `Diff`. |
| `CodeEditor` | CodeMirror with vim bindings: `value`, `path` (picks the language), `readOnly`, `line`, `onSave(text)` (`:w`, Ctrl-S), `onDirtyChange`, `onRequestEdit` (`i` while read-only). Loaded on first use. |
| `CodeDiff` | Two texts compared with CodeMirror: `original`, `value`, `path`, `layout` (`"unified"` or `"split"`). |
| `PathLink` | An inline button opening a file path: `reference` (`{ path, line }`), `onOpen` (e.g. from `useOpenFile()`). |
| `Tabs` | Tabs with `TabList`, `Tab` and `TabPanel`. |
| `ListBox` | A selectable list with `ListBoxItem`. |
| `Menu` | A menu with `MenuItem` and `MenuTrigger`. |
| `Select` | A drop-down: `label`, `items`, `selectedKey`, `onSelectionChange`. |
| `TextField` | A text input: `label`, `value`, `onChange`. |
| `Checkbox` | A checkbox: `isSelected`, `onChange`. |
| `Disclosure` | An expandable section. |
| `Toolbar` | A row of controls. |
