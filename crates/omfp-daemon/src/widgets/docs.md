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
repositories). Props every widget accepts: `instance` (a string naming the instance for `useStored` and `useJob`;
default: the reference) and `title` (a tab's label; default: the built-in's name, else the reference).

## Imports

Only these specifiers resolve; everything else must be a relative file of the widget's folder:

- `react`, `react/jsx-runtime`, `react-dom` — the window's own React 19 (hooks work across widgets and the shell).
- `omfp/sdk` — hooks over the daemon (below).
- `omfp/kit` — the shell's components (below), styled by the theme.

JSX needs no import. TypeScript types are stripped, not checked. CSS files cannot be imported: use the kit, inline
`style`, or the theme's custom properties (`var(--omfp-primary)`, `var(--omfp-surface-container)`,
`var(--omfp-on-surface)`, `var(--omfp-outline)`, `var(--omfp-space-2)`, `var(--omfp-font-code)`, …).

```tsx
import { Button, Card, Log, Progress } from "omfp/kit";
import { useJob } from "omfp/sdk";

export default function Tests({ command = "make test" }: { command?: string }) {
  const job = useJob(command);
  const done = job.lines.filter((line) => line.startsWith("ok")).length;
  return (
    <Card title={job.running ? "Tests running" : `Tests finished (exit ${job.exitCode})`}>
      <Progress label="Tests" value={done} max={20} />
      <Log lines={job.lines.slice(-10)} />
      <Button onPress={job.running ? job.kill : job.restart}>{job.running ? "Stop" : "Run again"}</Button>
    </Card>
  );
}
```

## omfp/sdk

| Export | What it gives |
| --- | --- |
| `useWidget()` | `{ instance, reference, workspaceId }` of the calling widget. |
| `useStored(key, initial)` | `[value, set, loaded]`: JSON state the daemon keeps per workspace, widget instance and key, shared live by every window and kept across reloads and restarts. `set(value)` or `set((previous) => next)`; `set(null)` deletes the key. `loaded` is `false` while the state has not arrived yet (`value` is then `initial`). |
| `useJob(command)` | `{ lines, running, exitCode, signal, restart, kill }` for `sh -c <command>` run by the daemon in the workspace folder. The first mount starts it; reloads, other windows and moving the widget follow the same job (its id is stored as `job:<command>`). `lines` are stdout and stderr lines; `restart()` kills a running job and starts a new one; `state` is the raw `job/<id>` state. |
| `useToolRun(toolCallId)` | A tool run of the selected thread, or `useToolRun({ match: /nextest/, threadId? })` for the latest run whose command matches: `{ toolCallId, toolName, status, output, lines, isError }` or `undefined`. While a `bash` command runs, `output` is the latest tail omp streams. |
| `useCall()` | `call(method, params)`: any daemon protocol method, e.g. `call("thread.prompt", { threadId, text })`. |
| `useTopic(topic)` | Follows a topic (`index`, `thread/<id>`, `workspace/<id>`, `views/<id>`, `job/<id>`) while mounted; returns its subscription state. |
| `useIndex()` | Workspaces, threads, theme and settings (the `index` topic). |
| `useThreads()` | Thread summaries by id. |
| `useThreadSummary(threadId)` | One thread's summary (title, process state, workspace). |
| `useThread(threadId)` | `{ state, closed }`: the thread's full state (transcript, tools, queue, pending interactions). |
| `useWorkspaces()` | Workspaces by id. |
| `useWorkspace(workspaceId)` | The workspace and its repositories' git status. |
| `useViews(workspaceId)` | The workspace's views manifest, widget folders and stored state (the `views/<id>` topic). |
| `useConnection()` | The connection status. |
| `useWindowState(key)` | Window-local state; `SELECTED_THREAD` is the thread this window shows. |
| `useCurrentWorkspaceId()` | The workspace this window works in: the selected thread's, else the draft thread's. |
| `useThreadActions()` | `select`, `rename`, `archive`, `stop`, `restart`, … for threads. |
| `useCommand(command)` | Registers a `:` command or key binding target while mounted. |
| `useAnnounce()` | `announce(message)` for screen readers. |
| `useSettings()` | The daemon's UI settings. |
| `SELECTED_THREAD` | Window key of the selected thread id (`useWindowState(SELECTED_THREAD)`). |

The SDK also exports the view models and strings the built-in widgets use; read a copied built-in to see them.

## omfp/kit

| Export | What it is |
| --- | --- |
| `Card` | A bordered surface: `title`, `actions`, `tone` (`default`, `error`, `warning`, `success`). |
| `Button` | A button: `onPress`, `variant` (`primary`, `secondary`, `quiet`, `danger`), `size` (`sm`, `md`), `isDisabled`. |
| `IconButton` | An icon-only button: `label`, `icon`, `onPress`. |
| `Progress` | A progress bar: `label` (required), `value` (omit for indeterminate), `max` (default 100), `hideLabel`. |
| `Log` | Monospace lines that follow the end: `lines`. |
| `Badge` | A small status label: `tone` (`default`, `primary`, `error`, `warning`, `success`). |
| `Chip` | A compact labelled item: `tone`, `onRemove`. |
| `Markdown` | Renders Markdown text (no raw HTML): `text`. |
| `Code` | Code: `code`, `block`, `language` (a name or file extension; highlights a block). |
| `Diff` | A unified patch (`git diff` output) as numbered lines: `patch`, `language` (highlighting). |
| `languageOfPath` | `languageOfPath("src/lib.rs")` → `"rust"`: the language to pass to `Code` or `Diff`. |
| `Tabs` | Tabs with `TabList`, `Tab` and `TabPanel`. |
| `ListBox` | A selectable list with `ListBoxItem`. |
| `Menu` | A menu with `MenuItem` and `MenuTrigger`. |
| `Select` | A drop-down: `label`, `items`, `selectedKey`, `onSelectionChange`. |
| `TextField` | A text input: `label`, `value`, `onChange`. |
| `Checkbox` | A checkbox: `isSelected`, `onChange`. |
| `Disclosure` | An expandable section. |
| `Toolbar` | A row of controls. |
