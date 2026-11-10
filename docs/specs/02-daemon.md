# 02 — Daemon (`omfp daemon`)

The daemon owns repositories, workspaces, threads and omp processes. It serves the protocol in [03](03-protocol.md) and, in web
mode, the UI's static files.

## Startup and configuration

```text
omfp daemon [--bind 127.0.0.1:7420] [--data-dir DIR] [--omp-bin PATH] [--static-dir DIR] [--allow-omp-mismatch]
```

| ID | Requirement |
| --- | --- |
| DMN-001 | On startup the daemon MUST run `omp --version` and compare it with `omfp_core::OMP_VERSION` (the version of the `omp-rpc` tag, see [01](01-dev-environment.md)). On mismatch it MUST refuse to start unless `--allow-omp-mismatch` is given, in which case a warning is also shown in the UI. |
| DMN-002 | The default `--bind` MUST be `127.0.0.1:7420`; any address and port MAY be configured. Exposure beyond the machine (tunnels, reverse proxies, TLS) is the deployer's job (D-13). |
| DMN-003 | On first run the daemon MUST generate a random token (≥ 256 bits) in `<data-dir>/token` with mode `0600`; every WebSocket connection MUST present it (see PRT-002). |
| DMN-004 | The daemon MUST also work as a library (`omfp_daemon::start(config) -> DaemonHandle` exposing the bound address and token) for in-process use by the desktop app (ARC-003). |
| DMN-005 | On SIGTERM/SIGINT or `DaemonHandle::shutdown` the daemon MUST stop every omp process and finish within 10 s. Stopping closes the `omp-rpc` client, which closes omp's stdin and then terminates its process group (SIGTERM, SIGKILL after 1 s); omp-rpc 18.18.6 gives omp no longer drain window. |
| DMN-006 | The omp binary MUST be resolved in this order: `--omp-bin`; the omp bundled next to the daemon executable (packaging, D-14); `omp` on `PATH` (dev shell). The resolved path is logged at startup. |
| DMN-007 | At most one daemon MUST run per data directory (exclusive lock file). A running daemon MUST publish its address in `<data-dir>/daemon.json` (removed on shutdown), so local clients such as `omfp desktop` can find it and authenticate with the `token` file (D-16). |
| DMN-008 | The web UI bundle MUST be resolved in this order: `--static-dir`; the `web/` directory next to the daemon executable when it holds an `index.html` (packaging: the `dx build` output ships there); otherwise only the API is served and a warning is logged. The bundle is served at `/`, unknown paths fall back to `index.html`. |

## Persistence (SQLite)

Schema (`PRAGMA user_version = 3`; migration 2 adds `created_client`; migration 3 moves to workspace folders, D-11):

| Table | Main columns |
| --- | --- |
| `repositories` | `id` (ULID), `name`, `path` (absolute, unique), `default_base_ref`, `remote_url` (nullable), `created_at` — known repositories, a shortcut for adding repos to workspaces |
| `workspaces` | `id` (ULID), `name`, `path` (absolute, unique), `owned` (the daemon created the folder), `created_by` (`user` \| thread id), `created_client` (nullable, PRT-033), `created_at` |
| `threads` | `id` (ULID), `title`, `workspace_id`, `session_dir`, `omp_session_id` (nullable), `approval_mode` (nullable), `model` (nullable, `provider/id`), `thinking_level` (nullable), `created_by` (`user` \| thread id), `created_client` (nullable), `created_at`, `archived_at` (nullable) |
| `thread_roots` | `thread_id`, `path` — the `--add-dir` roots last given to the thread's omp session (DMN-029) |
| `thread_tags` | `thread_id`, `tag` |
| `settings` | `key`, `value` (JSON) |

Repositories inside a workspace are not stored: they are discovered from the folder (DMN-022).

| ID | Requirement |
| --- | --- |
| DMN-010 | Migrations MUST be embedded in the binary and applied in order using `PRAGMA user_version`, each in its own transaction. |
| DMN-011 | The database MUST use `journal_mode=WAL` and `foreign_keys=ON`. |
| DMN-012 | Transcripts are NOT stored in the database: the source of truth is omp's `.jsonl` session in the thread's `session_dir`. |

## Workspaces and their repositories

A workspace is a folder; its repositories are the folder's top-level entries (D-11, verified in S-10). A `Git` port
(trait) with a real implementation over the `git` CLI (`tokio::process`) and a fake one for tests. A `Forge` port
resolves PRs; first implementation over the `gh` CLI **[VERIFY in S-08]**.

| ID | Requirement |
| --- | --- |
| DMN-020 | Creating a workspace MUST either make a new folder `<workspaces-root>/<slug>` (`--workspaces-root`, default `~/omfp/workspaces`; `owned = true`), or record an existing folder given by path (`owned = false`), which may itself be a git repository. Names and paths are unique. |
| DMN-021 | Registering a known repository MUST verify the path is the root of a git repository (`git rev-parse --show-toplevel`) and reject duplicates. Registration is optional: no other feature may require it. |
| DMN-022 | The daemon MUST discover a workspace's repositories by scanning its folder: the folder itself when it is a git root, and every top-level entry that is a git root, following symlinks. Each repo has a `name` (entry name), its `path` inside the workspace (the symlink path, never the target), and a `kind`: `link` (symlink; with its target), `worktree` (linked worktree; with the main repository from `git rev-parse --git-common-dir`), or `clone`; plus branch and HEAD. Non-git entries, hidden entries (names starting with `.`, such as `.omp`) and anything deeper are ignored. |
| DMN-023 | The daemon MUST rescan a workspace on startup, on every workspace method, after every `tool_execution_end` of a thread in it, and on `workspaces.rescan`; a change updates the `index` topic. No filesystem watcher is required. |
| DMN-024 | Adding a repository to a workspace MUST support: `link` (create a symlink `<name> → <path>`), `worktree` (`git worktree add <workspace>/<name>` on a new branch from `baseRef`, default branch `omfp/<slug>-<suffix>` unique in the repo, or on an existing branch; the repository given by id or path), `clone` (`git clone <url> <name>`), and, after S-08, `pr` (fetch the PR head into a branch, then as `worktree`, keeping the PR URL). |
| DMN-025 | Removing a repository from a workspace MUST: for `link`, delete the symlink only (never the target); for `worktree`, run `git worktree remove`, refusing with uncommitted changes unless forced, keeping the branch unless asked; for `clone`, delete the folder only when forced or clean. Git operations on the same repository MUST be serialized (per-repository lock). |
| DMN-026 | For each repository in a workspace the daemon MUST report: branch, commits ahead/behind its upstream or base, changed files, the diff (`git diff <base>...HEAD` plus uncommitted changes) and the linked PR, for the "Changes" panel. |
| DMN-027 | The daemon MUST expose which threads use each workspace and which workspaces contain the same repository (same canonical path), so the UI can warn about concurrent edits. It MUST NOT forbid either. |
| DMN-028 | Removing a workspace MUST refuse while it has threads that are not deleted. With `deleteFolder` it removes an owned folder after removing its repositories as in DMN-025 (links unlinked, worktrees removed); a folder the daemon does not own is only forgotten. |

### Threads in a workspace

| ID | Requirement |
| --- | --- |
| DMN-029 | A thread's omp MUST run with cwd = its workspace folder and one `--add-dir <repo path>` per discovered repository except the folder itself (S-10: without them omp sees no repo context). Because omp merges roots saved in the session with the flags, the daemon MUST keep `thread_roots` in sync: after start and after every rescan of a live thread it sends `/add-dir` for new repos and `/remove-dir` for roots that disappeared, through `prompt` (local commands, no model turn). omp refuses both while the agent streams, so a busy thread's sync runs once its session settles. |

Each thread keeps a private directory `threads/<id>/` (omp sessions, `omp.log`); it is not the cwd and never a
workspace.

## Agent supervision

Each thread has an actor (tokio task) that owns its `omp_rpc::Client`. The client is blocking: calls go through
`spawn_blocking`, and a dedicated thread reads the event receiver and forwards into a tokio channel.

### Process states

```mermaid
stateDiagram-v2
  [*] --> Stopped
  Stopped --> Starting: thread opened / prompt
  Starting --> Idle: ready + open_session + setup ok
  Starting --> Failed: startup error
  Idle --> Busy: prompt admitted
  Busy --> Idle: session_settled
  Busy --> Busy: prompt_result (background work pending)
  Idle --> Stopped: inactivity (reaper) / explicit stop
  Busy --> Failed: process exited
  Idle --> Failed: process exited
  Failed --> Starting: explicit restart or new prompt
  Idle --> Terminal: handoff to terminal
  Stopped --> Terminal: handoff to terminal
  Terminal --> Stopped: terminal closed
```

`AwaitingInput` is not a process state but an attribute: there are pending interactive requests (approval, `ask`,
dialogs).

| ID | Requirement |
| --- | --- |
| DMN-030 | The process MUST be started as `omp --mode rpc-ui --session-dir <thread sessions> --continue` with cwd = the thread's workspace folder, one `--add-dir <repo path>` per discovered repository (DMN-029), `--approval-mode` and `--model` when set on the thread, and `--append-system-prompt` carrying the daemon's workspace conventions (workspace name and path; repositories are its top-level entries; bring a project in with a symlink, isolate a branch with `git worktree add` into the workspace, clone into it; never nest repos deeper) followed by the user's own append prompt, which the flag would otherwise disable: `<workspace>/.omp/APPEND_SYSTEM.md` if present, else `<agent dir>/APPEND_SYSTEM.md` (S-10). The daemon MUST NOT write files into a workspace for this. Binding the session at launch is required: S-07 found that `--add-dir` roots do not survive a later `open_session` to another session. |
| DMN-031 | After the `ready` frame the daemon MUST check that protocol v2 was negotiated (`omp-rpc` does it), register its host tools (at connect, through `omp-rpc`), then send in order: `open_session { sessionDir }` (a no-op on the session bound at launch that reports whether it was resumed), `set_event_filter { events: null, messageUpdates: "delta" }` (sent as the daemon's own frame: the generated command of omp-rpc 18.18.6 lacks `messageUpdates`), `set_subagent_subscription { level: "progress" }`, `set_ask_dialog { enabled: true }`. A failure in any step moves the thread to `Failed` with the error message. |
| DMN-032 | After `open_session` the daemon MUST store `sessionId` in `threads.omp_session_id` and load history with `get_messages` (paged by `omp-rpc`) as the reducer's initial event. |
| DMN-033 | Before starting omp on a thread that already has `omp_session_id`, the daemon MUST check the session lease (`omp session lease --json -- <id>`): if another omp process owns the session, startup MUST fail with an error saying so (e.g. the session is open in an external terminal). |
| DMN-034 | A process that is `Idle`, settled, and not watched by any client MUST be stopped after `idle_timeout` (default 15 minutes, configurable). A `Busy` process or one with pending interactive requests is never stopped. |
| DMN-035 | If the process exits unexpectedly, the thread MUST move to `Failed` with the last stderr lines (and the exit code when available — omp-rpc 18.18.6 does not expose it); pending interactive requests MUST be cancelled towards the UI. Restart happens only on the user's next command. |
| DMN-036 | omp's stderr MUST be written to a per-thread log file (`<data-dir>/threads/<id>/omp.log`, rotated) and never mixed with protocol stdout. |
| DMN-037 | Lifecycle operations on a thread (start, stop, restart, root sync after a rescan (DMN-029), handoff to terminal) MUST be serialized by its actor. omp commands are forwarded without waiting for each other, because omp orders them itself and some must overtake a running one (`abort` during `compact`, `steer` during a turn). Different threads proceed in parallel. |
| DMN-038 | The maximum number of concurrent omp processes MUST be configurable (default 8); beyond the limit, starting a stopped thread MUST fail with an explicit error (no silent queueing). |

### Events

| ID | Requirement |
| --- | --- |
| DMN-040 | Every frame received from omp (session events, `prompt_result`, `session_settled`, subagent frames, `available_commands_update`, `notice`, …) MUST get a per-thread increasing `seq` and an `epoch` (identifies the process start), and MUST be applied to the reducer before being forwarded to clients. |
| DMN-041 | The daemon MUST keep, per thread, the last N events in memory (default 2000) so reconnecting clients can catch up (PRT-020). |
| DMN-042 | Unknown frames (`Unknown(Value)` from `omp-rpc`) MUST be forwarded as `unknown` and logged, without stalling the thread. |
| DMN-043 | `agent_end.messages` MUST NOT be used as the complete transcript (it can be compacted); the transcript is built from `message_end` events and the initial history. |

### Interactive requests

| ID | Requirement |
| --- | --- |
| DMN-050 | `extension_ui_request` frames with method `select`, `confirm`, `input`, `editor` or `ask` MUST become interactive requests on the thread, forwarded to every connected client and kept until answered (even if no client is connected). |
| DMN-051 | The first valid answer from any client MUST be forwarded to omp as `extension_ui_response`; later answers to the same request MUST get an `already_resolved` error. |
| DMN-052 | A `cancel` from omp (with `targetId`) MUST close the matching request in every client. |
| DMN-053 | Presentation-only methods (`notify`, `setStatus`, `setWidget`, `set_editor_text`, `open_url`, `setTitle`) MUST become UI events with no reply to omp. |
| DMN-054 | `ask` answers MUST match omp's required shape (one entry per question, in order, exact labels); the daemon MUST validate them before forwarding and reject malformed ones with an error. Note: omp 18.18.6's `ask` tool takes one HTML form and walks it with `select`/`input`/`editor` dialogs instead of the RPC `ask` dialog (contract scenario `ask_answered`); the `ask` method stays supported for extensions that use it. |

### Daemon-provided tools (host tools)

| ID | Requirement |
| --- | --- |
| DMN-060 | The daemon MUST register its host tools when it launches a thread's omp (they are per thread). Initial set: `open_in_editor { path, line? }`. The daemon runs the user's editor itself — `$VISUAL`, else `$EDITOR`, from the daemon's environment, with the file (and `+<line>` when given) — but only when the window the user last used on that thread is a desktop window on the daemon's machine (M7). Otherwise, and when no editor is set, the tool returns an error the agent can act on (e.g. "open_in_editor is not available from the web client"). A terminal editor (`vim`, `nano`, `hx`, …) has no terminal to run in: the tool reports that too instead of spawning it. |
| DMN-061 | Every host tool MUST honour cancellation (`HostToolContext::is_cancelled`) and return an error (`isError: true`) instead of panicking. |

## Commands towards omp

Mapping between protocol commands ([03](03-protocol.md)) and omp RPC commands. All go through the thread actor.

| omfp command | omp RPC |
| --- | --- |
| `thread.prompt` | `prompt` (with `streamingBehavior` `steer`/`followUp` when the thread is `Busy`) |
| `thread.steer` / `thread.follow_up` | `steer` / `follow_up` |
| `thread.abort` | `abort_and_restore_queue` (returns queued messages to put back in the composer) |
| `thread.queue.remove` / `thread.queue.promote` | `remove_queued_message` / `promote_queued_message` |
| `thread.set_model` / `thread.set_thinking` | `set_model` / `set_thinking_level` (also stored on the thread) |
| `thread.models` / `thread.thinking_levels` | `get_available_models` / `get_available_thinking_levels` |
| `thread.commands` | `get_available_commands` |
| `thread.compact` | `compact` |
| `thread.tree` / `thread.navigate` / `thread.fork` / `thread.label` | `get_tree` / `navigate_tree` / `fork` / `set_label` |
| `thread.subagents` / `.messages` / `.steer` / `.cancel` | `get_subagents` / `get_subagent_messages` / `steer_subagent` / `cancel_subagent` |
| `thread.goal` | `goal` |
| `thread.btw` | `btw` |
| `thread.bash` | `bash` (with `excludeFromContext` when the `ready` frame supports it) |
| `thread.state` | `get_state` |

| ID | Requirement |
| --- | --- |
| DMN-070 | A command on a `Stopped` thread that needs omp (e.g. `thread.prompt`) MUST start the process first. Read-only commands on a stopped thread (e.g. `thread.tree`) MAY start it. |
| DMN-071 | omp errors (`success: false`, with `code` when present: `session_busy`, `stale_cursor`, `entry_not_found`, …) MUST reach the UI with the same `code`. |
| DMN-072 | After a `fork` the daemon MUST re-read `get_state` and update `omp_session_id` (fork moves the process to a new session file). |
| DMN-073 | Thread titles come from the conversation, not from the user at creation: `threads.create` takes no title from the UI (an optional `title` stays for scripts and agents); until a title exists the thread shows a placeholder from the first words of its first message. omp 18.18.6 forces `PI_NO_TITLE` in RPC modes, so after the first message the daemon generates the title itself by running omp once in print mode with the user's configuration and its small model role, no session and no tools **[VERIFY flags and role name]**, then stores it and sends `set_session_name` to the thread's omp. A title set by the user (`threads.update`) or by the agent (host tool `set_thread_title { title }`, DMN-060) is never replaced. |
| DMN-074 | A new thread's approval mode defaults to `yolo` (the thread's `approval_mode`, passed as `--approval-mode`); `threads.create` and `threads.update` may set another mode. |
