# 08 — Agent control (direction)

**Status:** direction. The features in this document are the **last milestone** (M8). Only the section
[Constraints that apply from day one](#constraints-that-apply-from-day-one) is binding now; everything else is a sketch
to be turned into requirements when M8 starts. No earlier choice may make this direction harder.

## Vision

The interface is a surface shared by the user and the agents, and agents get **the same power as the user** (D-17):
everything the UI can do, an agent can do — directly through tools, or through UI it builds itself.

1. **Everything in the protocol**: list threads and see what each one is doing (state, transcript, tool runs, pending
   questions), spawn threads, send them messages, wait for them, rename, tag, archive; create, adopt and attach
   workspaces; read git status and diffs. Every protocol method is an agent tool (AGT-001).
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
| `pmo_call` | Calls any protocol method by name with JSON params (the escape hatch: nothing is out of reach) |
| `pmo_thread_wait` | Waits for a thread's `prompt_result` or settled state; returns its last assistant text |
| `pmo_ui_mount` / `_update` / `_remove` | Mounts a UI piece (inline code or a saved component) with parameters |
| `pmo_component_save` / `_list` | Saves a UI piece to the library / lists the library |
| `pmo_notify` | Attention flag + desktop notification |

## Delivery to agents

One tool registry in the daemon, several adapters:

| Adapter | Reaches | Notes |
| --- | --- | --- |
| **RPC host tools** (`set_host_tools`) | every thread run by the daemon over RPC | Already used for `open_in_editor` (DMN-060). omp may mount them as `xd://` devices (`tools.xdev`), invoked through `write`. |
| **MCP server** (Streamable HTTP at `/mcp`, bearer token per thread) | threads in terminal mode, omp sessions outside pmo, other MCP-capable agents | Injected per thread through `<thread home>/.omp/mcp.json` (project config follows the cwd, D-11), with the thread's token in the `Authorization` header — verified in S-07. |
| **Host URI scheme** `pmo://` (`set_host_uri_schemes`) | RPC threads | State for the agent's `read` tool: `pmo://threads`, `pmo://threads/<id>`, `pmo://ui/…`. |
| **JS SDK** | UI pieces | The same API from inside a UI piece (below). |

## Agent-made UI (JavaScript)

**A UI piece is arbitrary web code.** HTML, CSS and JavaScript, rendered inside the client's webview (the browser on
web; the webview on desktop — Dioxus desktop renders through one), isolated in its own frame so a broken piece cannot
break the app or another piece. No fixed vocabulary limits what it can draw.

**A common layer, never a cage.** Every piece gets an `omfp` SDK and a widget kit so most pieces look native and take a
few lines, while anything else stays possible:

- **API**: `omfp.call(method, params)` and `omfp.subscribe(topic, since?)` — the full daemon protocol with the snapshot
  and replay semantics of 03 (so state survives reconnects and is the same in every window), plus helpers such as
  `omfp.threads()`, `omfp.thread(id).prompt(text)`, `omfp.thread(id).rename(title)`.
- **Sources**: `omfp.tool(toolCallId)` streams an omp tool run's output (e.g. the `bash` call the agent just started),
  `omfp.job(command)` starts a daemon-run job (output lines, exit code, cancel), plus thread state, workspace git
  status, timers.
- **Talking to agents**: `omfp.agent.send(text)` / `omfp.agent.event(payload)` to the owning thread (or any thread).
- **Storage**: `omfp.storage` — key-value state per component and per instance, kept by the daemon, shared by every
  window and session.
- **Widgets and theme**: web components (`<omfp-progress>`, `<omfp-log>`, `<omfp-table>`, `<omfp-card>`,
  `<omfp-button>`, `<omfp-diff>`, `<omfp-markdown>`, …) and the app's CSS custom properties, so pieces match light and
  dark themes. Optional to use.

```html
<!-- A piece the agent could mount, and save for later. -->
<omfp-card head="Tests">
  <omfp-progress id="bar"></omfp-progress>
  <omfp-log id="log" lines="20"></omfp-log>
  <omfp-button id="cancel">Cancel</omfp-button>
</omfp-card>
<script type="module">
  const { params, tool } = omfp;
  let done = 0;
  for await (const line of tool(params.toolCallId).lines()) {
    if (line.startsWith("ok ")) bar.value = ++done / params.total;
    log.append(line);
  }
  cancel.onclick = () => omfp.call("thread.abort", { threadId: params.threadId });
</script>
```

**Persistence and sharing.** A component is a folder (`manifest.json` with name, version, parameters; `index.html` and
any assets), so it can be reviewed and versioned like a skill:

| Library | Where | Shared with |
| --- | --- | --- |
| User | data directory | every thread and session of the user |
| Project | `.omfp/ui/` in a repository | everyone working on the repository (git) |
| Thread draft | the thread's home | that thread, until saved |

An agent saves a piece with `pmo_component_save`; the user can do the same with "save as component" on any mounted
piece. Agents discover the library through `pmo_component_list` and `pmo://ui/`. Mounted instances record the
component version (content hash).

**Open points (spike before M8)**: the exact SDK surface and widget kit; how pieces load on desktop (custom protocol vs
`srcdoc`); whether pieces may also run without a window (e.g. QuickJS in the daemon for background jobs) or leave that
to agents; how agents learn the SDK (tool description, instructions injected like skills).

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
| AGT-007 | Protocol methods are self-describing: params and results have JSON Schemas generated from `pmo-protocol` types (`schemars`), checked by golden tests, so agent tool schemas can be derived automatically. |
| AGT-008 | No operation an agent could reasonably perform may require a human in the loop by design (e.g. creating a thread or workspace has no mandatory interactive step); where confirmation is required, it goes through interactions (DMN-050), which agent-originated requests can raise too. |
