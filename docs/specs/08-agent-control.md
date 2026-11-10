# 08 — Agent control (direction)

**Status:** direction. The features in this document are the **last milestone** (M8). Only the section
[Constraints that apply from day one](#constraints-that-apply-from-day-one) is binding now; everything else is a sketch
to be turned into requirements when M8 starts. No earlier choice may make this direction harder.

## Vision

The interface is a surface shared by the user and the agents. Besides chatting, an agent can:

1. **Manage threads**: spawn new threads (workspaces, model, tags, first prompt), list them, read their state and last
   answer, send them messages (prompt, steer, follow-up), wait for them to finish, archive them.
2. **Manage workspaces**: create a worktree for a branch or PR in any repository, adopt a directory, attach or detach
   workspaces to itself or to other threads.
3. **Publish views**: render content in the UI (a panel in its thread, a global dashboard, a floating window) —
   Markdown, tables, lists, diffs, code, images, simple charts, progress — including controls (buttons, forms, selects)
   whose events go back to the publishing agent.
4. **Ask for attention**: mark a thread as needing the user, send a desktop notification. (Questions to the user already
   exist: omp's `ask` tool, DMN-050.)

Example, the PR workflow: an orchestrating thread receives "implement checkout v2", creates `fe#412` and `be#88`
worktrees in the frontend and backend repositories, spawns one thread per PR (or one thread on both), and publishes a
dashboard listing each PR with CI status and buttons "rerun tests" and "open PR". The user can step into any thread at
any time.

## Tool surface (draft)

Generated from the protocol method table (AGT-001, AGT-007); names are provisional.

| Tool | Protocol method(s) |
| --- | --- |
| `pmo_thread_create` | `threads.create` (with `prompt`) |
| `pmo_thread_list` / `pmo_thread_get` | `index` snapshot / thread summary |
| `pmo_thread_send` | `thread.prompt` / `thread.steer` / `thread.follow_up` |
| `pmo_thread_wait` | waits for `prompt_result` or settled; returns the last assistant text |
| `pmo_workspace_create` / `_from_pr` / `_adopt` | `workspaces.create` / `.from_pr` / `.adopt` |
| `pmo_thread_attach` / `_detach` | `threads.attach` / `threads.detach` |
| `pmo_view_publish` / `_update` / `_remove` | `views.*` (new, M8) |
| `pmo_notify` | attention flag + notification (new, M8) |

## Delivery to agents

One tool registry in the daemon, several adapters:

| Adapter | Reaches | Notes |
| --- | --- | --- |
| **RPC host tools** (`set_host_tools`) | every thread run by the daemon over RPC | Already used for `open_in_editor` (DMN-060). omp may mount them as `xd://` devices (`tools.xdev`), invoked through `write`. |
| **MCP server** (Streamable HTTP at `/mcp`, bearer token per thread) | threads in terminal mode, omp sessions outside pmo, other MCP-capable agents | Injected per thread through `<thread home>/.omp/mcp.json` (project-scoped config follows the cwd, D-11), with the thread's token in the `Authorization` header; to be confirmed in S-07. |
| **Host URI scheme** `pmo://` (`set_host_uri_schemes`) | RPC threads | Read-only state for the agent's `read` tool: `pmo://threads`, `pmo://threads/<id>`, `pmo://views/<id>`. |

## Agent-drawn UI

- A **view** is a declarative document: a tree of nodes from a fixed, versioned vocabulary defined in `pmo-protocol`:
  layout (`row`, `col`, `card`, `tabs`), content (`md`, `table`, `kv`, `list`, `code`, `diff`, `image`, `chart`,
  `progress`, `badge`, `link`, `thread_ref`, `workspace_ref`), controls (`button`, `form` with text/select/checkbox).
  Same idea as Tern's Surface Protocol or omp's `ask`, but ours and transport-independent.
- Views have an id, an owner thread, a placement (thread panel, dashboard, floating), and a lifetime (until removed or
  until the owner is archived). They are stored by the daemon and survive restarts.
- Updates replace the document or apply node operations (add / set / remove / append text), so streaming content does
  not resend everything.
- Control events (`view_event { viewId, nodeId, action, values }`) go to the owner agent. Delivery is open: as a
  follow-up message, or kept for `pmo_thread_wait` / a `pmo_view_events` tool **[decide in M8]**.
- Rendering uses the panel registry (UI-024); each node kind is a component, unknown kinds fall back to formatted JSON.
- Optional, off by default: a **sandboxed HTML** node (`iframe` with `sandbox="allow-scripts"`, no same-origin, CSP
  without network) for free-form visualizations, talking only through a `postMessage` bridge to the same event channel.

## Safety

- Each thread process gets its own scoped principal (PRT-034). Default permissions: thread, workspace and view
  operations; destructive operations (forced workspace removal, thread deletion) raise an interaction for the user.
- Limits: spawned threads per thread, depth of agent-spawned chains, the global process limit (DMN-038), optional cost
  caps for spawned threads (omp `session.maxCost`).
- Provenance (ARC-009) is visible in the UI; filters such as "spawned by X"; a cascade stop for everything a thread
  spawned.
- An audit log records every action taken by an agent principal.

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
