# omfp widgets

A widget is a piece of the omfp window written in React. omfp shows the widgets a workspace's **views manifest**
lists, reloads a widget as soon as one of its files changes, and shows any error in the widget's place.

## Files

- `<workspace>/.omfp/views.json` — the views of the window (schema: `omfp://schemas/views.json`). Without it the window
  shows the default view:
  `["row", { "sizes": [3, 1] }, ["col", {}, "omfp:header", ["tabs", { "id": "main" }, "omfp:chat"]], ["tabs", { "id": "side" }, "omfp:workspace", "omfp:changes", "omfp:background"]]`.
  A workspace that has its own `views.json` keeps it as it is (omfp never rewrites it): `:view reset` there, or an
  agent with `omfp_ui_mount`, adds a widget the default view gained later, such as `omfp:background`.
- `<workspace>/.omfp/widgets/<name>/index.tsx` (or `index.ts`, `index.jsx`, `index.js`) — a workspace widget,
  referenced as `./widgets/<name>`. Its default export is a React component; its props are the layout node's props.
  Other files of the folder are imported with relative paths (`./chart.tsx` or `./chart`).
- `<data-dir>/widgets/<name>/` — the user library, referenced as `user:<name>` in every workspace.
- A workspace widget named like a built-in (`.omfp/widgets/chat/` for `omfp:chat`) replaces the built-in in that
  workspace. The built-ins' sources can be copied as a starting point (`:widget copy omfp:workspace` in the window).
- `<workspace>/.omfp/keymap.json` — this workspace's key bindings (schema: `omfp://schemas/keymap.json`, see Keys).
- `<workspace>/.omfp/views/<name>.tsx` — a TSX view (experimental, see TSX views), named in `views.json` as
  `{ "title": "Ops", "view": "./views/ops.tsx" }` instead of a `layout`.
- `<workspace>/.omfp/init.ts` and the user's `init.ts` (`$XDG_CONFIG_HOME/omfp/init.ts`, else `~/.config/omfp/init.ts`,
  next to `theme.css`) — init files run by every window (experimental, see Init files).

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
`omfp:interactions`, `omfp:composer`, `omfp:workspace`, `omfp:changes` (`{ repo? }`: changed files and diffs of the
workspace's repositories, or of the repository named `repo` only), `omfp:background` (the selected thread's background jobs and subagents: state, duration, last output
line; stop a job, cancel a subagent), `omfp:subagents` (the selected thread's subagents, their transcripts, steer and cancel), `omfp:todos`
(its todo list and goal), `omfp:tree` (its session tree: go to an entry, fork from it, label it), `omfp:activity`
(`{ threadId? }`: the activity log of what agents did through omfp, `activity.list`), `omfp:mcp` (the MCP servers of
the selected thread, checked by its omp, or those configured for the workspace: state, tools, error, config file;
reconnect = restart the thread, `thread.mcp_servers`, `thread.mcp_reconnect`), `omfp:file`
(`{ path, line?, diff? }`: a file, viewed, edited with vim bindings, diffed against
`HEAD`). Files opened by the user open as tabs after the children of the `tabs` container with `"id": "main"`, never
written to `views.json`: keep that slot (`["tabs", { "id": "main" }, "omfp:chat"]`) in every view, else files float in
a panel. Props every widget accepts: `instance` and `title` (a tab's label; default: the built-in's name, else the
reference). `instance` names the node's state: `useStored` and `useJob` keep their state per workspace, instance and
key (default instance: the reference), so two nodes of the same widget with the same instance share it — give each
its own `instance` when a widget appears twice (`["./widgets/tests", { "instance": "tests-api" }]`). It also names
the node in `omfp_widget_check`'s report. Built-ins keep no stored state (`omfp:changes` twice, one per `repo`, works
without it), but an `instance` per node costs nothing.

## Views in the window

- The window shows one view at a time. With several views the mode line (the bar at the bottom) shows them as small
  tabs, `Default · Review · Ops`, the current one highlighted: a click switches. `:view <id>` switches too (`:view`
  completes the ids), `g v` opens the `:` line on `view` to pick one, `] v` / `[ v` go to the next / previous view.
  `omfp://docs/shell` describes the rest of the window (left column, mode line, `:` commands, keys).
- The choice belongs to the window and survives reloads (kept in the browser's `localStorage`); while the current
  workspace has no view of that id, the default view shows: `views.json`'s `default` (else its first id). Init files
  keep that default when they use or transform views: a view `views.add` adds never becomes the default by itself,
  only `views.withDefault(id)` changes it (see Views of init files).
- Splitters between the cells of a `row` or `col` resize them for this window only (a double click goes back to the
  layout's `sizes`); nothing the user does by hand rewrites `views.json` except `:widget add <reference>`, which adds
  a widget (built-in, `./widgets/<name>`, `user:<name>`) to the current view as the last tab of its `tabs` with
  `"id": "side"`, else at the end of its root container. It refuses views that exist only in code (an init file's
  `views.add`, a TSX view) and names the file to edit. Agents change views with `omfp_ui_mount` and `views.set`.

## Mounts, panels and the library

An agent shows UI to the user without editing `views.json` by hand:

- `omfp_ui_mount { id, code | widget, props?, view?, where?, panel? }` mounts a widget: `code` (a component's source,
  written to `.omfp/widgets/<id>/index.tsx`) or `widget` (a reference: `user:<name>`, `./widgets/<name>`). It goes
  into the view `view` (default: the default view; a new id creates a view showing only it, a dashboard) at `where`
  (`side-tab`, `main-tab`, `end`), or with `panel: true` floats over every view. The node gets `instance: <id>`, so
  `useStored` state follows the mount; mounting the same id again replaces it. `code` is refused (`conflict`) when
  `.omfp/widgets/<id>/` holds a widget no mount with `code` wrote, or when `<id>` names a built-in (`chat`, `file`…):
  pick another id.
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
bindings. A binding runs a built-in command (`:help` lists the keys and commands in the window, `omfp://keymap` lists
the default keys and the built-in commands with their titles and aliases for agents) or one a widget registers with
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

While a started sequence waits for its next key (`g`, `Space`, `Space h`), the window shows the which-key
panel after 300 ms: every key that completes it with the title of the command it runs now, or the group of the longer
bindings it starts (`h → help…`). It is built from the bindings in force, so the bindings of this file, of init files
and the titles of widget commands show there as soon as they apply; give a command a short `title`. Escape cancels the
sequence. `Space` alone opens it at once: the leader menu. The leader map (experimental, branch idea/init):

| Keys | Command |
| --- | --- |
| `Space Space`, `Space :` | the `:` command line (`:` too) |
| `Space a` / `n` / `r` / `x` / `u` / `d` | next waiting thread / new thread / rename / archive / unarchive / delete |
| `Space s` / `S` / `R` | stop the thread / with the threads it spawned / restart it |
| `Space w`, `Space p n`, `Space p r` | new workspace, again, rescan the workspace |
| `Space t` | show or hide the threads column |
| `Space v v`, `Space v n` / `p` | pick a view (`g v`), next / previous view (`] v` / `[ v`) |
| `Space f o` | open the focused path (`g f`) |
| `Space h h`, `Space h k` | the help panel (`?`, `:help`), `:describe-key` |

Bind your own under a free letter (`Space T` above, or groups like `Space o a`, `Space o b`); a binding of a group's own
keys (`Space h`) replaces the group.

## Init files (experimental)

An init file is the window's `init.el`: an ES module every window imports at startup and runs again whenever it changes.
The user's `init.ts` runs first, then the current workspace's `.omfp/init.ts` (switching to a thread of another
workspace swaps the latter). Both compile like widget files (TypeScript, JSX, the same imports; a single file: no
relative imports). Do the work in the default export, `export default function init(omfp: InitApi)` (the types
`InitApi`, `ViewsApi` and `Layout` come from `omfp/sdk`: `import type { InitApi } from "omfp/sdk"`; types are
stripped, so a plain `.js`-style `init(omfp)` works as well); return a cleanup function if something must be undone. Before the file runs again, or when it goes away, the window calls that cleanup and removes
everything the run registered through the API, so a run never needs to undo its own `command`, `bind`, `views.use`,
`views.transform`, `panel` or `on`. A file that read `views.json` (`views.load`) or the repositories
(`workspace.repos`) also runs again when they change.

```ts
import type { InitApi } from "omfp/sdk";

export default function init(omfp: InitApi) {
  omfp.command({
    id: "tests.run",
    title: "Run the tests",
    description: "Asks the selected thread's agent to run the test suite.",
    aliases: ["tests"],
    run: () => omfp.notify({ title: "tests", body: "started" }),
  });
  omfp.bind([
    { keys: "Space T", command: "tests.run" },
    { mode: "insert", keys: "Ctrl-t", command: "tests.run" },
    { keys: "Space x", command: "" },
  ]);
  omfp.panel("clock", ["user:clock", { title: "Clock" }]);
  omfp.on("attention", (thread) => omfp.notify({ title: thread.title ?? thread.id, body: "needs you", threadId: thread.id }));
  const timer = setInterval(() => {}, 60_000);
  return () => clearInterval(timer);
}
```

| API | What it does |
| --- | --- |
| `file`, `workspaceId` | `"user"` or `"workspace"`; the workspace whose file this is (`undefined` for the user's). |
| `command(command)` | Registers a `:` command / key binding target: `{ id, title, description?, aliases?, scope?, args?, enabled?, run(args) }` (`useCommand`'s shape; `registerCommand(host, command)` from `omfp/sdk` is the same outside an init file). |
| `bind(binding \| bindings)` | `{ keys, command, args?, mode? }` (`mode` default `normal`, `command: ""` removes a binding), applied after the defaults, the user's `[keymap]` and `.omfp/keymap.json`: the user's init file, then the workspace's, win last. |
| `views.load("views.json")` | The workspace's `views.json` as views (the default view while it is missing or invalid): see Views of init files. `"views.json"` is the only file it loads; any other argument throws. |
| `views.use(views)` | Shows `views`, as they are at the call, in this window instead of `views.json`'s (the workspace's file wins over the user's). |
| `views.transform(fn)` | `fn(views)` changes the views it gets (or returns other views): applied after `use`, the user's file's first, each in registration order; one that throws changes nothing. |
| `workspace` | `{ id, repos }`: the current workspace and its repositories (`{ name, path, kind }`, `kind` as `omfp:workspace` shows it: `link`, `worktree`, `clone`, …). |
| `panel(id, node)` | Floats a layout node over every view (its `title` prop labels it); closing it hides it until the file runs again. |
| `on(event, listener)` | `"attention"` (a thread starts needing the user), `"failed"`, `"finished"` (a turn ended): `listener(thread)` gets the thread summary. Use it for auto-actions, e.g. select or notify. |
| `notify({ title, body, threadId? })` | Like `useNotify()`. |
| `run(name, args?)` | Runs any command by id or alias, e.g. `run("view.switch", ["logs"])`. |
| `call(method, params)` | Any protocol method, like `useCall()`. |
| `onCleanup(fn)` | Called before the file runs again or goes away. |
| `host` | The window's SDK host (`commands`, `window` state, `notices`, `client`), for anything else. |

Command ids are dotted, `<area>.<action>` like the built-ins (`thread.archive`, `view.switch`; `omfp://keymap` lists
them): name a workspace's commands `ws.<action>` (`ws.ops`, `ws.deploy`) and the user's own `my.<action>` so they
never clash. A command whose id equals a built-in's (or another widget's) replaces it while the file is loaded — keys
bound to that id run yours — and the built-in comes back when the file goes away. `aliases` are the names typed after
`:` (`:tests`); an alias another command already answers to keeps answering to that older command, so pick unique
ones. `:describe-command <name>` shows which command a name reaches.

A layout node is checked like `views.json`'s: an invalid one throws. Any failure — a compile error, a throw or rejected
promise of the run, of a cleanup, of a transform, of a command or listener it registered — shows in a banner naming the
file; the window keeps working without it, and the next save that runs cleanly removes the banner. Windows report these
errors, and those of TSX views, to the daemon: `omfp_views_errors` (or `views.errors { workspaceId }`) lists what each
window showing the workspace sees now. Before that, `omfp_widget_check { name: "init" }` (`user:init` for the user's
file, `views/ops.tsx` for a TSX view; `omfp://widgets/check/init` reads the same) compiles the file as the windows
import it and adds the windows' errors for it.

### Views of init files

Without an init file, or one that never calls `views.use`, the window shows `views.json`'s views (that is the default
init). Everything is window-local: only `:widget add` and agents (`omfp_ui_mount`, `views.set`) write `views.json`, and
an init file that read it runs again with the new file.

```ts
// ~/.config/omfp/init.ts: every workspace gets the MCP tab.
import type { InitApi } from "omfp/sdk";

export default function init(omfp: InitApi) {
  omfp.views.transform((views) => {
    views.update("default", (layout) => layout.addTab("side", "omfp:mcp"));
  });
}
```

```ts
// <workspace>/.omfp/init.ts
import type { InitApi } from "omfp/sdk";

export default function init(omfp: InitApi) {
  const views = omfp.views.load("views.json"); // views.json stays writable by `:widget add` and agents
  const repos = omfp.workspace.repos;
  views.add("review", {
    title: "Review",
    // The `main` slot keeps the chat and the files the user opens; one `omfp:changes` per repository, each its own
    // `instance` and tab title.
    layout: ["row", {},
      ["tabs", { id: "main" }, "omfp:chat"],
      ["tabs", {}, ...repos.map((r) => ["omfp:changes", { repo: r.name, instance: `changes-${r.name}`, title: r.name }])]],
  });
  views.add("ops", { title: "Ops", view: "./views/ops.tsx" });
  omfp.views.use(views); // the user switches with the mode line's tabs, `:view review`, `g v`, `] v`
}
```

The default view stays `views.json`'s (`default`, else its first id): views `add` adds appear after `views.json`'s in
the mode line's tabs and never become the default by themselves. `views.withDefault("review")` makes one the default:
a window opens it unless the user chose another view (that choice is kept across reloads).

Views and layouts change in place, and every change returns them, so calls follow one another or chain
(`views.add(…).withDefault(…)`).

| Views | What it does |
| --- | --- |
| `ids`, `default` | The view ids in the order of the mode line's tabs; the default view's id. |
| `get(id)` | `{ id, title, layout, view, origin }`: `layout` the view's `Layout`, whose edits change the view (`undefined` for a TSX view); `origin` `"views.json"` (also once init files changed its layout) or the init file that added the view. |
| `add(id, { title?, layout \| view })` | Adds the view, or replaces the one with that id in place (it is then the file's); `layout` is a node or a `Layout`, `view` a TSX file `./views/<name>.tsx`. |
| `update(id, fn)` | `fn(layout)` edits the view's `Layout`, or returns its new layout (a `Layout` or a node). |
| `remove(id)`, `withDefault(id)` | Removes the view (the default goes back to the first view when it was the default); makes another view the default. |

| Layout | What it does |
| --- | --- |
| `node` | The layout node, as in `views.json`. |
| `addTab(tabsId, widget)` | Adds `widget` as the last tab of the `tabs` with that `id` (throws when there is none). |
| `removeWidget(reference)` | Removes every widget `reference` (containers left empty go too). |

`:widget add` writes `views.json`. On a view of `views.json` that init files only changed (the user's `addTab` above),
it adds the widget to the view as `views.json` holds it, then the init files change the new file again. On a view an
init file added (`views.add`) it writes nothing and names the file to edit.

## TSX views (experimental)

A view can be a TSX file of `<workspace>/.omfp/views/`, compiled like widget files (the same imports; relative imports
of other files of `.omfp/views/`) and reloaded on every save: `{ "title": "Ops", "view": "./views/ops.tsx" }` in
`views.json`, or `add("ops", { view: "./views/ops.tsx" })` in an init file. Its default export is a React component
(hooks work: re-rendering it changes the layout) returning `omfp/sdk`'s layout components:

```tsx
// <workspace>/.omfp/views/ops.tsx
import { Row, Col, Tabs, Widget } from "omfp/sdk";

export default function Ops() {
  return (
    <Row sizes={[3, 2]}>
      <Tabs id="main"><Widget use="omfp:chat" /></Tabs>
      <Col sizes={[1, 1]}>
        <Widget use="./widgets/deploy" props={{ env: "staging" }} />
        <Tabs><Widget use="omfp:background" /><Widget use="omfp:mcp" /></Tabs>
      </Col>
    </Row>
  );
}
```

`Row`, `Col`, `Stack` (`sizes`: default flex weights) and `Tabs` (`id`; `id="main"` is where files open) nest the
others; `Widget` is a widget (`use`: its reference, `props`: its props). They are the layout nodes of `views.json`
(`<Row sizes={[3, 2]}>` is `["row", { "sizes": [3, 2] }, …]`), rendered the same way: the splitters resize the cells
for this window (kept by workspace, view and path; a double click goes back to `sizes`), files open as tabs of
`<Tabs id="main">` (without one, in a floating panel). The file holds one layout and only these components, nothing
else. A compile error, a throw while rendering or an invalid layout shows a banner naming the file and the default
view in its place until the next save works (windows report it: `omfp_views_errors`). `:widget add` refuses a TSX
view: edit the file.

`:describe-key` then a key sequence shows what it runs in every mode, the command's title and description, and where
it was bound (default keymap, `[keymap]`, `.omfp/keymap.json`, the user's or the workspace's init file);
`:describe-command <name>` shows a command's names, scope, who registered it and every key bound to it. Point the user
at them to check what an init file did.

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
| `useCommand(command)` | Registers a `:` command or key binding target while mounted (`description` shows in `:describe-command`). |
| `useAnnounce()` | `announce(message)` for screen readers. |
| `useNotify()` | `notify({ title, body, threadId? })`: tells the user something without covering anything. While the window has focus it is a notice at the right end of the mode line (`◆ title body`, `g n` or a click opens it, gone after 8 s); while it has not, a system notification (when the user allowed them; `[ui] notifications = "never"` shows neither). With `threadId`, opening it selects that thread, visiting the thread clears it, and nothing is shown about the thread the user is on. Use it for results worth a look (a build turned red, a long job finished), not for every update. |
| `useSettings()` | The daemon's UI settings. |
| `useFile(path, { follow? })` | `{ status, file, error, reload, save }`: a file of the current workspace (`workspaces.read_file`: `file.content`, `file.hash`, `file.head`, …). `status` is `"loading"`, `"ready"`, `"missing"` or `"error"`. With `follow` (default `true`) it is checked every 2 s and on every git status change. `save(content, expectedHash?)` writes it only if the file still has that hash (default: the one read) and resolves to `{ ok: true, file }` or `{ ok: false, conflict, error }`. |
| `useLive()` | `[live, snapshot]`: this window's live voice session (M6). `live.start(threadId)` opens the microphone and a WebRTC call relayed to omp's realtime voice (`thread.live_connect`), or, without WebRTC on the daemon's machine (`#local=1`, the desktop app), omp's own session (`thread.live_start`, the spectrum then draws omp's levels), `live.end()`, `live.endThread(threadId)` (any window's session), `live.toggleMute()`, `live.source()` (the audio for `Spectrum`), `live.support()` (`"webrtc"`, `"host"`, `"insecure"`, `"unsupported"`); `snapshot.local` is the session (`idle`, `starting`, `running`, `failed`). The thread's `state.live` has omp's phase and transcript. |
| `useOpenFile()` | `openFile(path, { line?, diff? })`: opens a file of the current workspace in a tab next to the chat (the view's `tabs` with `"id": "main"`; keep that slot in your views), else as a tab of a floating panel; `diff: true` shows it against `HEAD`. A bare or partial name (`theme.ts`, `src/theme.ts`) that does not exist as given opens the file it names, or lets the user pick when several match. |
| `useMissingFile(path, missing, { line?, diff? })` | For a widget showing a file: while `missing`, looks for the file `path` names (`workspaces.find_file`); one match opens in its place, several open the window's picker. Gives `"finding"`, `"opened"`, `"choosing"` or `"none"`. |
| `Row`, `Col`, `Tabs`, `Stack`, `Widget` | The layout components of TSX views (see TSX views); they render only in a view file of `.omfp/views/`. |
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
