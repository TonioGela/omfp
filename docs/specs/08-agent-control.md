# 08 — Agent control (direction)

**Status:** direction. Widgets (agent-made UI in the workspace layout) are milestone **M4**; the rest (agent tools for
every method, MCP, dashboards, the shared component library) is **M10**. Only the section
[Constraints that apply from day one](#constraints-that-apply-from-day-one) is binding now; everything else is a sketch
to be turned into requirements when those milestones start. No earlier choice may make this direction harder.

## Vision

The interface is a surface shared by the user and the agents, and agents get **the same power as the user** (D-17):
everything the UI can do, an agent can do — directly through tools, or through UI it builds itself.

1. **Everything in the protocol**: list threads and see what each one is doing (state, transcript, tool runs, pending
   questions), spawn threads, send them messages, wait for them, rename, tag, archive; create workspaces and
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
| **RPC host tools** (`set_host_tools`) | every thread run by the daemon over RPC | Already used for `open_in_editor` (DMN-060). omp may mount them as `xd://` devices (`tools.xdev`), invoked through `write`. |
| **MCP server** (Streamable HTTP at `/mcp`, bearer token per thread) | threads in terminal mode, omp sessions outside omfp, other MCP-capable agents | Only needed for omp sessions not run by the daemon (RPC threads get host tools); configured in the user's own `mcp.json`, since the daemon writes nothing into workspaces (D-11). S-07 verified that a project `.omp/mcp.json` is honored and its headers are sent. |
| **Host URI scheme** `omfp://` (`set_host_uri_schemes`) | RPC threads | State for the agent's `read` tool: `omfp://threads`, `omfp://threads/<id>`, `omfp://ui/…`. |
| **SDK** | widgets | The same API from inside a widget, as React hooks (below). |

## Agent-made UI: views and widgets (M4)

The window is described by a **views manifest** per workspace; everything in it is a **widget**, a React component.
The shell (D-18) is React too, so built-in widgets, agent widgets and the shell are the same kind of code.

**Views manifest** — `<workspace>/.omfp/views.json` (for a folder the daemon does not own, kept in the daemon's data
dir instead). Declarative, schema-checked, edited by the agent as a file and by the user from the UI (move, resize,
hide, add, switch view), so both always edit the same thing:

```jsonc
{
  "$schema": "omfp://schemas/views.json",
  "default": "work",
  "views": {
    "work": {
      "title": "Work",
      "layout": ["row", { "sizes": [3, 1] },
        ["col", {}, "omfp:transcript", "omfp:composer"],
        ["tabs", {}, "omfp:repos", ["omfp:changes", { "repo": "frontend" }], "./widgets/test-chart"]]
    },
    "review": {
      "title": "Review",
      "layout": ["row", {}, ["omfp:changes", { "groupBy": "repo" }], "omfp:transcript"]
    }
  }
}
```

- Containers: `row`, `col`, `tabs`, `stack` with sizes; leaves are widget references with props. Several views per
  workspace, switched from the shell.
- `omfp:*` are built-in widgets (transcript, composer, thread list, repos, changes, todos, subagents, tree, …);
  `./widgets/<name>` are the workspace's own; `user:<name>` come from the user library in the data dir.
- An invalid manifest never breaks the window: the shell shows the default view and a banner with the validation error
  (which the agent can read through the SDK and fix).
- The conversation with the agent is a widget like any other: `omfp:chat` (transcript, interactions such as approvals
  and `ask`, queue and composer), built from smaller `omfp:transcript`, `omfp:interactions` and `omfp:composer`, all
  copyable and replaceable. Only a thin frame stays outside the manifest: connection banner, view switcher, the
  keymap and command line, and a **safety net** — when the selected thread has a pending interaction that no widget in
  the current view renders, the frame shows it itself, so a manifest can never hide an approval.

**Widgets** — a widget is an ES module whose default export is a React component, in `<workspace>/.omfp/widgets/<name>/`
(`index.tsx` or `index.js`, plus assets). It runs in the window's document, under an error boundary (a crash shows the
error in place of the widget, not a broken window), and gets the SDK as hooks and props:

- **Protocol**: `useCall()`, `useTopic(topic)` — the full daemon protocol with the replay/snapshot semantics of 03, so
  state is the same in every window and survives reconnects; helpers such as `useThreads()`, `useThread(id)`,
  `useWorkspace()`, `useGit(repo)`.
- **Sources**: `useToolRun(toolCallId)` streams an omp tool run's output; `useJob(command)` runs a daemon job (output
  lines, exit code, cancel) — e.g. the test chart runs the test command and parses its output.
- **Agents**: `useAgent(threadId?)` to send a message or an event to the owning thread or any thread.
- **Storage**: `useStored(key, initial)` — state kept by the daemon per widget instance, shared across windows and
  sessions.
- **Kit and theme**: React components of the shell (cards, tables, lists, diff, markdown, progress, log, buttons,
  charts) and the CSS custom properties, so widgets look native in light and dark themes. Optional to use.

```tsx
// .omfp/widgets/test-chart/index.tsx — written by the agent, reloaded live when it changes.
import { Card, Progress, Log, Button } from "omfp/kit";
import { useJob } from "omfp/sdk";

export default function TestChart({ command = "cargo nextest run" }) {
  const run = useJob(command);
  const passed = run.lines.filter((l) => l.includes(" PASS ")).length;
  return (
    <Card title="Tests">
      <Progress value={passed} max={run.lines.filter((l) => /PASS|FAIL/.test(l)).length || undefined} />
      <Log lines={run.lines.slice(-20)} />
      <Button onClick={run.restart}>Run again</Button>
    </Card>
  );
}
```

**System access goes through the daemon.** A widget runs in the browser (or desktop webview), possibly on another
machine; it never touches the system itself. Everything it needs runs in the daemon, on the workspace's machine, as the
user, with no restrictions (full access, D-17):

| Need | SDK | Daemon side |
| --- | --- | --- |
| Follow a command the agent runs with its `bash` tool | `useToolRun(toolCallId)` or `useToolRun({ match: /nextest/ })` (latest matching run in the thread) | omp's `tool_execution_update` frames already stream in the thread topic **[VERIFY: partial output granularity for `bash`]** |
| Run its own command | `useJob(command, { cwd?, env? })` → `{ lines, exitCode, running, kill, restart }` | **jobs**: `jobs.start { workspaceId, command, cwd? }` → `{ jobId }`, topic `job/<id>` (stdout/stderr lines with `seq`, replay and snapshot like threads, last 1 MiB kept), `jobs.kill`, `jobs.list`. A job runs in the workspace folder in its own process group, belongs to the workspace (not to the widget): it survives reloads and closed windows, every window sees it, and it is killed on daemon shutdown. |
| Read or watch a file | `useFile(path)` | `workspaces.read_file` plus change notifications from rescan |
| Share state with the agent | `useStored(key)` | per-instance state the agent can also read and write (`omfp_widget_state` host tool) |

The agent can start jobs too (`omfp_job_start` / `omfp_job_output` host tools): a long build or test run becomes a job,
the agent mounts a widget bound to it and keeps working instead of blocking its turn on `bash`, then reads the output
when it needs it.

**Loading** — the daemon serves workspace widgets over HTTP as ES modules, transpiling TypeScript/JSX on the fly, and an
import map points `react`, `omfp/sdk` and `omfp/kit` at the shell's own copies, so widgets share one React instance.
A rescan (DMN-023) that sees a changed widget or manifest pushes an update; the window reloads that widget only.

```tsx
// A progress bar wired to a command: the agent writes this widget and adds it to views.json.
import { Card, Progress, Log, Button } from "omfp/kit";
import { useJob } from "omfp/sdk";

export default function Tests({ command = "cargo nextest run" }) {
  const job = useJob(command); // runs in the daemon, in the workspace folder
  const total = Number(job.lines.find((l) => /Starting \d+ tests/.test(l))?.match(/\d+/)?.[0]) || undefined;
  const done = job.lines.filter((l) => /^\s*(PASS|FAIL)/.test(l)).length;
  return (
    <Card title={job.running ? `Tests ${done}/${total ?? "?"}` : `Tests finished (exit ${job.exitCode})`}>
      <Progress value={done} max={total} />
      <Log lines={job.lines.slice(-10)} />
      <Button onClick={job.running ? job.kill : job.restart}>{job.running ? "Stop" : "Run again"}</Button>
    </Card>
  );
}
```

**Sharing** — widgets and views are files: commit `.omfp/` with a repository to share them, or save a widget to the user
library ("save as user widget", or the agent copying the folder). Built-in widgets can be copied into `.omfp/widgets/`
and changed; a workspace widget with the same name as a built-in replaces it in that workspace.

**Open points (spike at the start of M4)**: transpiling in the daemon (e.g. the `oxc` Rust crates) versus buildless
modules (`htm` tagged templates, no JSX); hot reload granularity; how much of the shell becomes widgets (transcript and
composer as `omfp:*` widgets is assumed above); how agents learn the SDK (a short note in `--append-system-prompt`, docs
at `omfp://docs/widgets` and the manifest schema at `omfp://schemas/views.json`).

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
