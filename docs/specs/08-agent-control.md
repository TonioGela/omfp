# 08 — Agent control (direction)

**Status:** direction, except two binding parts: [Views and widgets (M4)](#views-and-widgets-m4) (milestone **M4**,
delivered) and [Constraints that apply from day one](#constraints-that-apply-from-day-one). The rest (agent tools for
every method, MCP, dashboards, the shared component library) is **M10** and stays a sketch to be turned into
requirements when that milestone starts. No earlier choice may make this direction harder.

## Vision

The interface is a surface shared by the user and the agents, and agents get **the same power as the user** (D-17):
everything the UI can do, an agent can do — directly through tools, or through UI it builds itself.

1. **Everything in the protocol**: list threads and see what each one is doing (state, transcript, tool runs, pending
   questions), spawn threads, send them messages, wait for them, rename, archive; create workspaces and
   add or remove their repositories; read git status and diffs. Every protocol method is an agent tool (AGT-001).
2. **Arbitrary UI**: an agent writes its own pieces of interface in JavaScript (HTML, CSS, JS — anything a web page can
   do), mounted in a thread panel, a dashboard or a window, live and interactive, wired to what it is doing (e.g. a real
   progress bar driven by a long command).
3. **Reusable UI**: those pieces can be saved and shared, so later threads and sessions reuse them.
4. **Attention**: mark a thread as needing the user, send a desktop notification (questions already exist: omp's
   `ask` tool, DMN-050).

Example, the PR workflow: an orchestrating thread receives "implement checkout v2", creates `fe#412` and `be#88`
worktrees in the frontend and backend repositories, spawns one thread per PR, and builds a dashboard that lists every
thread with its live status, CI results, and buttons "rerun tests" and "open PR". The dashboard is saved, and the next
feature reuses it.

## Tool surface (draft)

Generated from the protocol method table (AGT-001, AGT-007), so every method is available; names are provisional.
Convenience tools on top:

| Tool | What it does |
| --- | --- |
| `omfp_call` | Calls any protocol method by name with JSON params (the escape hatch: nothing is out of reach) |
| `omfp_thread_wait` | Waits for a thread's `prompt_result` or settled state; returns its last assistant text |
| `omfp_ui_mount` / `_update` / `_remove` | Mounts a UI piece (inline code or a saved component) with parameters |
| `omfp_component_save` / `_list` | Saves a UI piece to the library / lists the library |
| `omfp_notify` | Attention flag + desktop notification |

## Delivery to agents

One tool registry in the daemon, several adapters:

| Adapter | Reaches | Notes |
| --- | --- | --- |
| **RPC host tools** (`set_host_tools`) | every thread run by the daemon over RPC | Already used for `omfp_open_file`, `set_thread_title` and the widget tools (DMN-060). omp may mount them as `xd://` devices (`tools.xdev`), invoked through `write`. |
| **MCP server** (Streamable HTTP at `/mcp`, bearer token per thread) | threads in terminal mode, omp sessions outside omfp, other MCP-capable agents | Only needed for omp sessions not run by the daemon (RPC threads get host tools); configured in the user's own `mcp.json`, since the daemon writes nothing into workspaces (D-11). S-07 verified that a project `.omp/mcp.json` is honored and its headers are sent. |
| **Host URI scheme** `omfp://` (`set_host_uri_schemes`) | RPC threads | State for the agent's `read` tool: `omfp://threads`, `omfp://threads/<id>`, `omfp://ui/…`. |
| **SDK** | widgets | The same API from inside a widget, as React hooks (below). |

## Agent-made UI: views and widgets (M4)

The window is described by a **views manifest** per workspace; everything in it is a **widget**, a React component.
The shell (D-18) is React too, so built-in widgets, agent widgets and the shell are the same kind of code. The
binding rules are the table [Views and widgets (M4)](#views-and-widgets-m4) below; this section explains them.

**Views manifest** — `<workspace>/.omfp/views.json` (every workspace is a folder the daemon made, D-11, DMN-020).
Declarative, checked by the daemon, edited by the agent as a file (or with the `omfp_view_add` host tool) and by the
user from the UI (move, resize, hide, add, switch view), so both always edit the same thing. Every workspace has the
file: the daemon writes the default view into it when it creates the workspace, and at startup for older ones
(DMN-088), and never rewrites it — it is the user's and the agent's; `:view reset` restores the current default. The
default view is the header over the `tabs` slot `main` holding the conversation (opened files show as closable tabs
next to it, UI-115), with the `tabs` slot `side` (workspace and changes widgets) on its right:

```jsonc
{
  "$schema": "omfp://schemas/views.json",
  "default": "work",
  "views": {
    "review": {
      "title": "Review",
      "layout": ["row", {}, "omfp:transcript", "./widgets/tests"]
    },
    "work": {
      "title": "Work",
      "layout": ["row", { "sizes": [3, 1] },
        ["col", {}, "omfp:header", ["tabs", { "id": "main" }, "omfp:chat"]],
        ["tabs", { "id": "side" }, "omfp:workspace", ["./widgets/tests", { "command": "cargo nextest run" }]]]
    }
  }
}
```

- Containers: `row`, `col`, `tabs`, `stack` with `sizes` (flex weights); leaves are widget references with props.
  Several views per workspace, switched from the frame and listed in id order.
- `omfp:*` are built-in widgets (`omfp:header`, `omfp:chat` and its parts `omfp:transcript`, `omfp:interactions`,
  `omfp:composer`, `omfp:workspace`, `omfp:changes` from M5, `omfp:file` from M5c); `./widgets/<name>` are the workspace's own;
  `user:<name>` come from the user library in the data dir.
- An invalid manifest never breaks the window: the frame shows the default view and a banner with the validation error.
- The conversation with the agent is a widget like any other: `omfp:chat` (transcript, interactions such as approvals
  and `ask`, queue and composer), built from smaller `omfp:transcript`, `omfp:interactions` and `omfp:composer`, all
  copyable and replaceable. Only a thin frame stays outside the manifest: the left column (workspaces and threads),
  connection banner, view switcher, the keymap and command line, notifications, the live region, and a **safety net**
  — when the selected thread has a pending interaction that no built-in interactions widget in the current view
  renders, the frame shows it itself, so a manifest can never hide an approval.

**Widgets** — a widget is an ES module whose default export is a React component, in `<workspace>/.omfp/widgets/<name>/`
(`index.tsx`, `index.ts`, `index.jsx` or `index.js`, plus other files it imports relatively). It runs in the window's
document, under an error boundary (a crash shows the error in place of the widget, not a broken window), and gets the
SDK as hooks and its layout props as props:

- **Protocol**: `useCall()`, `useTopic(topic)` — the full daemon protocol with the replay/snapshot semantics of 03, so
  state is the same in every window and survives reconnects; helpers such as `useThreads()`, `useThread(id)`,
  `useWorkspace(id)`, `useIndex()`.
- **Sources**: `useToolRun(toolCallId)` follows an omp tool run of the thread; `useJob(command)` runs a daemon job
  (status, stdout and stderr lines, runs, exit code, kill, restart).
- **Files**: `useFile(path)` reads a workspace file and follows its changes, saves with a conflict check;
  `useOpenFile()` gives `openFile(path, { line, diff })`, which opens a file tab (AGT-027).
- **Storage**: `useStored(key, initial)` — state kept by the daemon per widget instance, shared across windows and
  sessions.
- **Kit and theme**: the React components of the shell (`omfp/kit`: cards, buttons, lists, tabs, progress, log,
  markdown, …) and the CSS custom properties, so widgets look native in light and dark themes. Optional to use.

```tsx
// .omfp/widgets/tests/index.tsx — written by the agent, reloaded live when it changes.
import { Button, Card, Log, Progress } from "omfp/kit";
import { useJob } from "omfp/sdk";

export default function Tests({ command = "cargo nextest run" }: { command?: string }) {
  const job = useJob(command); // runs in the daemon, in the workspace folder
  const total = Number(job.lines.find((l) => /Starting \d+ tests/.test(l))?.match(/\d+/)?.[0]) || 1;
  const done = job.lines.filter((l) => /^\s*(PASS|FAIL)/.test(l)).length;
  return (
    <Card title={job.status === "running" ? `Tests ${done}/${total}` : `Tests ${job.status} (exit ${job.exitCode})`}>
      <Progress value={done} max={total} label="Tests" />
      <Log lines={job.lines.slice(-10)} />
      <Button onPress={job.status === "running" ? job.kill : job.restart}>
        {job.status === "running" ? "Stop" : "Run again"}
      </Button>
    </Card>
  );
}
```

**System access goes through the daemon.** A widget runs in the browser (or desktop webview), possibly on another
machine; it never touches the system itself. Everything it needs runs in the daemon, on the workspace's machine, as the
user, with no restrictions (full access, D-17):

| Need | SDK | Daemon side |
| --- | --- | --- |
| Follow a command the agent runs with its `bash` tool | `useToolRun(toolCallId)` or `useToolRun({ match: /nextest/ })` (latest matching run in the thread) | omp's `tool_execution_update` frames in the thread topic; omp 18.18.6 streams the bash output's latest tail while the command runs (contract test, AGT-020) |
| Run its own command | `useJob(command)` → `{ status, lines, stdout, stderr, exitCode, signal, runs, startedAt, kill, restart }` | **jobs** (DMN-080, DMN-081): `jobs.start { workspaceId, command }`, topic `job/<id>`, `jobs.kill` |
| Share state with the agent | `useStored(key, initial)` | per-instance state in the daemon (DMN-086) |

**Loading** — the daemon serves workspace and user widgets over HTTP as ES modules, transpiling TypeScript/JSX with
`oxc` (S-12), and an import map points `react`, `react/jsx-runtime`, `react-dom`, `omfp/sdk` and `omfp/kit` at the
app bundle's own modules, so widgets share one React instance with the shell. A rescan (DMN-023) or a poll that sees
a changed widget folder or manifest pushes an update; the window reloads that widget only.

**Sharing** — widgets and views are files: commit `.omfp/` with a repository to share them, or put a widget folder
into the user library (`<data-dir>/widgets/<name>/`, by hand or by the agent). Built-in widgets can be copied into
`.omfp/widgets/` (`:widget copy omfp:workspace`) and changed; a workspace widget with the same name as a built-in
replaces it in that workspace.

**How agents learn it** — a short widgets note in the daemon's `--append-system-prompt` text (AGT-024) and read-only
host URIs served to the thread's omp with `set_host_uri_schemes` (DMN-062): `omfp://docs/widgets` (the SDK and kit
reference), `omfp://schemas/views.json` (the manifest schema) and `omfp://widgets/builtin/<name>` (the built-ins'
sources), so the agent never searches the filesystem. Two host tools close the loop without a browser:
`omfp_widget_check { name }` (DMN-063) answers with the daemon's compile result and what the open windows report about
the widget (AGT-026: errors, console errors, the rendered text, its jobs), and `omfp_view_add` (DMN-064) places a
widget in a view without rewriting `views.json` by hand.

**Later (not in M4)**: `useAgent(threadId?)` (messages or events to a thread), the host tools `omfp_widget_state`, `omfp_job_start` and `omfp_job_output` (the agent reads
and writes widget state and starts jobs), `jobs.list`, job `cwd`/`env` options, `:widget save` and saving a widget to
the user library from the UI, widgets in dashboards or windows outside the workspace view, and dragging widgets with
the pointer.

### Views and widgets (M4)

| ID | Requirement |
| --- | --- |
| AGT-010 | **Views manifest**: `<workspace>/.omfp/views.json` is `{ "$schema"?: string, "default"?: <view id>, "views": { <id>: { "title"?: string, "layout": <node> } } }` with at least one view; unknown keys are ignored. A node is a widget reference (`"omfp:chat"`), `[reference, props]`, or `[container, props, ...children]` with `container` one of `row`, `col`, `tabs`, `stack`, at least one child, and `sizes` (when present) one positive number per child; props are objects. References are `omfp:<name>` (built-in), `./widgets/<name>` (the workspace's) and `user:<name>` (the user library); an unknown reference is not a manifest error, it shows an error in place of that widget. A widget instance is identified by its `instance` prop when it is a string, else by its reference. Views are listed in id order; `default` (else the first id) is the view a window opens. The daemon and the app apply the same rules (one shared case list, `fixtures/views/cases.json`). The **default manifest** (written into every workspace, DMN-088, and the window's fallback while the file is missing or invalid) is the view `default`, titled "Default": `["row", { "sizes": [3, 1] }, ["col", {}, "omfp:header", ["tabs", { "id": "main" }, "omfp:chat"]], ["tabs", { "id": "side" }, "omfp:workspace", "omfp:changes"]]`, every built-in an explicit `omfp:*` reference; the daemon's copy (`crates/omfp-daemon/src/widgets/default-views.json`) and the app's are checked equal by a test. The `tabs` container with `"id": "main"` is the slot reserved for opened files (they will open as window-local tabs after the conversation); the one with `"id": "side"` is where `omfp_view_add` puts side tabs (DMN-064). A `tabs` container with one child shows no tab bar. A tab's label is the widget's `title` prop, else a built-in's own name ("Workspace", "Changes", …), else its reference. |
| AGT-011 | **Invalid manifest**: a `views.json` that cannot be read, is not JSON or breaks AGT-010 never breaks the window: the frame shows the default view and a banner with the error (its JSON path and reason); a fixed file recovers on the next push, without reload. |
| AGT-012 | **Widget modules**: a widget is the folder `<workspace>/.omfp/widgets/<name>/` (`<name>` of ASCII letters, digits, `_`, `-`, `.` not first), entry `index.tsx`, `index.ts`, `index.jsx` or `index.js` (the first present); its default export is a React component receiving the node's props. It may import files of its own folder (relative paths, with or without extension, never leaving the folder) and only the specifiers `react`, `react/jsx-runtime`, `react-dom`, `omfp/sdk` and `omfp/kit`, which an import map in the page points at the app bundle's own modules (one React, one SDK). |
| AGT-013 | **Live reload**: a widget is imported from a URL carrying its folder's version (DMN-084); when the views topic pushes a new version, the window imports that widget again and re-renders only it. State in `useStored` survives a reload, component-local state does not. |
| AGT-014 | **Errors in place**: a compile error (syntax, a forbidden import, a name that `omfp/sdk`, `omfp/kit` or React do not export, in any file of the folder, DMN-085), a failed import, a missing entry or default export, and a render error show in place of the widget (file, line and column when known, and the message); the rest of the window keeps working, and the next save recovers without reload. |
| AGT-015 | **Safety net**: when the selected thread has a pending interaction and the current view contains neither built-in `omfp:chat` nor `omfp:interactions` (a workspace override of them does not count), the frame renders the interactions itself. |
| AGT-016 | **SDK** (`omfp/sdk`): the hooks of `ui/packages/app/src/sdk` (connection, index, threads, workspaces, any topic, every protocol method, window state, commands, announcements, thread actions, settings), plus `useWidget()` (instance, reference, workspace id), `useStored`, `useJob`, `useToolRun`, and the view models and strings the built-in widgets use. `omfp://docs/widgets` documents them; a test checks every name it lists is exported. |
| AGT-017 | **Kit** (`omfp/kit`): the components of `@omfp/kit` (UI-093), including `Card`, `Button`, `Progress`, `Log`, `Markdown`, `Tabs`, `ListBox`, `Menu`, `Badge`, and from M5c the CodeMirror `CodeEditor` and `CodeDiff` (loaded on first use, S-13), styled by the theme tokens (UI-091). |
| AGT-018 | **`useStored(key, initial)`** → `[value, set, loaded]`: JSON state kept by the daemon per workspace, widget instance and key (DMN-086), the same in every window and pushed live, surviving reloads and daemon restarts; `set` takes a value or an updater, `null` deletes the key; `loaded` tells whether the workspace's views state has arrived (before, `value` is `initial`). |
| AGT-019 | **`useJob(command)`** → `{ status, lines, stdout, stderr, exitCode, signal, runs, startedAt, endedAt, state, restart, kill }`: `status` is `pending` (no job known yet, or a start or restart on its way), `running`, `succeeded` (exit code 0) or `failed` (another exit code, or a signal); `lines` holds both outputs interleaved as they came, `stdout` and `stderr` each one's lines; `runs` counts the jobs this instance started for the command (a restart adds one) and `startedAt`/`endedAt` (ms) date the current job. The job's id and the runs are stored with `useStored` under `job:<command>` as `{ id, runs }`; while the instance has no job for the command (nothing stored, or the daemon no longer knows the stored job), the hook starts one (`jobs.start`); otherwise it follows the stored job, running or finished, so reloads, other windows and moving the widget never start a second one. `restart` kills a running job and starts a new one; `kill` stops it. |
| AGT-020 | **`useToolRun(toolCallId \| { match, threadId? })`**: the tool run of a thread (the selected one by default) with that id, or the latest whose command (`arguments.command` when it is a string, else its JSON arguments) matches `match`: `{ toolCallId, toolName, status, output, lines, isError }`, where `output` is the latest partial output while it runs (omp 18.18.6 sends the bash output's latest tail in `tool_execution_update`, checked by a contract test) and the result's text once it ended. |
| AGT-021 | **Built-ins are copyable**: every built-in widget (`omfp:header`, `omfp:chat`, `omfp:transcript`, `omfp:interactions`, `omfp:composer`, `omfp:workspace`, `omfp:changes`, `omfp:file`) is written like an agent widget — its folder's value imports are only `react`, `omfp/sdk`, `omfp/kit` and files of its own folder (checked by a test) — and the release bundle ships each one's source as a widget folder (`builtin-widgets/<name>/`). `:widget copy omfp:<name> [<new name>]` (`widgets.copy`, DMN-087) copies one into `.omfp/widgets/`; a `user:<name>` widget is copied the same way. |
| AGT-022 | **Override**: in a workspace, `omfp:<name>` renders `./widgets/<name>` when that folder exists, so a copied built-in (or any widget of that name) replaces the original there. |
| AGT-023 | **User library**: `user:<name>` is the folder `<data-dir>/widgets/<name>/`, filled by hand or by the agent; it is served, versioned, reloaded and listed like workspace widgets (in every workspace). |
| AGT-024 | **Agent note**: the daemon's `--append-system-prompt` text (DMN-030) carries a short widgets note after the workspace conventions: where `views.json` (always present, DMN-088), workspace widgets and the user library live, the allowed imports, a ten-line example widget, the host URIs to read — `omfp://docs/widgets`, `omfp://schemas/views.json` and the built-in sources `omfp://widgets/builtin/<name>` (DMN-062; no filesystem path, so the agent never searches for them) — and the host tools that check a widget (`omfp_widget_check`, DMN-063) and place it in a view (`omfp_view_add`, DMN-064). It also asks the agent to **propose** a widget, in one sentence in the chat, when something would read better as a visualization or controls, and to build it only after the user agrees (no UI approval gate), delegating the building to the `widget-builder` agent (AGT-029). |
| AGT-025 | **Agent docs**: `omfp://docs/widgets` is the SDK and kit reference (hand-written Markdown in the repository, `crates/omfp-daemon/src/widgets/docs.md`) and `omfp://schemas/views.json` the manifest's JSON Schema; both are read-only host URIs of every thread (DMN-062), verified against omp 18.18.6 by a contract test. |
| AGT-026 | **Window reports**: every window reports to the daemon over its connection (`widgets.report`, PRT-043), for each workspace or user widget instance of the view it shows: the import's state, the compile stand-in's, import's and error boundary's errors, console errors and uncaught errors whose stack names the widget's module (`<widgetBase>/<source>/<name>/`), a text snapshot of the widget's DOM, and its `useJob` jobs, with the `{ epoch, seq }` of the views topic it rendered. It reports again when any of that changes (at most every 250 ms) and reports `workspaceId: null` when it shows no workspace. `omfp_widget_check` reads these reports (DMN-063). |
| AGT-027 | **Files** (M5c): `useFile(path, { follow? })` → `{ status: "loading" \| "ready" \| "missing" \| "error", file, error, reload, save }` reads a file of the current workspace (`workspaces.read_file`, DMN-089) and, while `follow` (default `true`), checks it again every 2 s with `knownHash` and whenever `workspace/<id>` publishes a new status, replacing `file` when it changed; `save(content)` writes it with the `expectedHash` of the shown version (`workspaces.write_file`, DMN-090) and resolves to `{ ok: true, file }` or `{ ok: false, conflict: true }` (the file changed on disk) or `{ ok: false, error }`. `useOpenFile()` returns `openFile(path, { line?, diff? })`, which opens the file in a tab of the view's `main` slot or of the floating panel (UI-115). `useMissingFile(path, missing, { line?, diff? })` looks for the file a missing path names (`workspaces.find_file`, DMN-092) and opens it or the window's picker (UI-117), giving `"finding"`, `"opened"`, `"choosing"` or `"none"`. All are exported by `omfp/sdk` and documented in `omfp://docs/widgets`. |
| AGT-028 | **Agents keep `main`**: the files the user opens become tabs of the view's `["tabs", {"id": "main"}, …"omfp:chat"]` slot (UI-115), so the widgets note (AGT-024) and the `omfp_view_add` description (DMN-064) tell the agent to keep that slot in every view it changes, and `omfp_widget_check` (DMN-063) adds a warning when the workspace's valid `views.json` has no view with a `main` slot (the default views without a file have it). |
| AGT-029 | **Widget builder** (M6b, S-14): the daemon ships an omp agent `widget-builder` (DMN-107) whose frontmatter is `tools: read, write, edit, glob, grep` and `model: "@smol"` (the user's smol role; omp falls back to the thread's model), without `blocking`, so the task runs in the background. Its body carries the widgets knowledge a subagent cannot inherit from the appended prompt, generated from the same text as the widgets note (AGT-024): where widgets live, with paths relative to its cwd (the workspace folder), the user library, the allowed imports, the example, `omfp://docs/widgets`, `omfp://widgets/builtin/<name>` and `omfp://schemas/views.json`. It must read `omfp://widgets/check/<name>` (DMN-108) after every write until the report is clean, never edit `views.json`, and finish with a short summary naming the widget and its props. The widgets note tells the main agent: once the user agrees to a widget, delegate the building to a `task` with `agent: "widget-builder"` and a complete brief (what to show, its data sources, where it goes), keep talking, and place the widget with `omfp_view_add` when the result arrives as a follow-up turn. |

## Permissions and limits

Agents run in **full-access mode by default** (D-17): an agent principal may call every method, and so may the UI pieces
it mounts; pieces run with the same access as the window they are in. Limits are opt-in settings, not defaults:
scoped principals (PRT-034), caps on spawned threads and chain depth, the global process limit (DMN-038), cost caps
(omp `session.maxCost`). What stays on regardless: provenance (ARC-009) shown in the UI with filters such as "spawned
by X", a cascade stop for everything a thread spawned, and an activity log of agent-originated actions.

## Constraints that apply from day one

| ID | Requirement |
| --- | --- |
| AGT-001 | API-first (ARC-007, D-12): every UI action is a protocol method; agent tools will be adapters over the method table. |
| AGT-002 | Provenance is recorded on every created entity (ARC-009). |
| AGT-003 | Authorization goes through a per-connection `Principal` checked in one place (PRT-034). |
| AGT-004 | The UI uses a panel registry, not a fixed layout (UI-024). |
| AGT-005 | Threads are not bound to repositories, and workspaces can be attached and detached live (ARC-008, DMN-029). |
| AGT-006 | Host tools are registered from a single registry in the daemon (DMN-060), so adding tools needs no change in the supervisor. |
| AGT-007 | Protocol methods are self-describing: params and results have JSON Schemas generated from `omfp-protocol` types (`schemars`), checked by golden tests, so agent tool schemas can be derived automatically. |
| AGT-008 | No operation an agent could reasonably perform may require a human in the loop by design (e.g. creating a thread or workspace has no mandatory interactive step); where confirmation is required, it goes through interactions (DMN-050), which agent-originated requests can raise too. |
