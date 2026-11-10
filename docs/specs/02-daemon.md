# 02 — Daemon (`pmod`)

The daemon owns repositories, workspaces, threads and omp processes. It serves the protocol in [03](03-protocol.md) and, in web
mode, the UI's static files.

## Startup and configuration

```text
pmod [--bind 127.0.0.1:7420] [--data-dir DIR] [--omp-bin PATH] [--static-dir DIR] [--allow-omp-mismatch]
```

| ID | Requirement |
| --- | --- |
| DMN-001 | On startup the daemon MUST run `omp --version` and compare it with the expected version compiled into the binary (the same version as the `omp-rpc` tag, see [01](01-dev-environment.md)). On mismatch it MUST refuse to start unless `--allow-omp-mismatch` is given, in which case a warning is also shown in the UI. |
| DMN-002 | The default `--bind` MUST be `127.0.0.1` (never `0.0.0.0` by default). |
| DMN-003 | On first run the daemon MUST generate a random token (≥ 256 bits) in `<data-dir>/token` with mode `0600`; every WebSocket connection MUST present it (see PRT-002). |
| DMN-004 | The daemon MUST also work as a library (`pmo_daemon::start(config) -> DaemonHandle` exposing the bound address and token) for in-process use by the desktop app (ARC-003). |
| DMN-005 | On SIGTERM/SIGINT or `DaemonHandle::shutdown` the daemon MUST close every omp process's stdin (omp drains accepted work and exits), wait at most 10 s, then close the `omp-rpc` client (SIGTERM, then SIGKILL to the process group). |

## Persistence (SQLite)

Initial schema (`PRAGMA user_version = 1`):

| Table | Main columns |
| --- | --- |
| `repositories` | `id` (ULID), `name`, `path` (absolute, unique), `default_base_ref`, `remote_url` (nullable), `created_at` |
| `workspaces` | `id` (ULID), `path` (absolute, unique), `kind` (`managed_worktree` \| `adopted`), `repository_id` (nullable), `branch` (nullable), `base_ref` (nullable), `pr_url` (nullable), `pr_number` (nullable), `created_by` (`user` \| thread id), `created_at`, `removed_at` (nullable) |
| `threads` | `id` (ULID), `title`, `session_dir`, `omp_session_id` (nullable), `approval_mode` (nullable), `model` (nullable, `provider/id`), `thinking_level` (nullable), `created_by` (`user` \| thread id), `created_at`, `archived_at` (nullable) |
| `thread_workspaces` | `thread_id`, `workspace_id`, `position` (0 = cwd) — primary key (`thread_id`, `workspace_id`) |
| `thread_tags` | `thread_id`, `tag` |
| `settings` | `key`, `value` (JSON) |

| ID | Requirement |
| --- | --- |
| DMN-010 | Migrations MUST be embedded in the binary and applied in order using `PRAGMA user_version`, each in its own transaction. |
| DMN-011 | The database MUST use `journal_mode=WAL` and `foreign_keys=ON`. |
| DMN-012 | Transcripts are NOT stored in the database: the source of truth is omp's `.jsonl` session in the thread's `session_dir`. |

## Repositories and workspaces

A `Git` port (trait) with a real implementation over the `git` CLI (`tokio::process`) and a fake one for tests. A
`Forge` port (trait) resolves PRs; first implementation over the `gh` CLI **[VERIFY in S-08]**.

| ID | Requirement |
| --- | --- |
| DMN-020 | Registering a repository MUST verify the path is the root of a git repository (`git rev-parse --show-toplevel`) and reject duplicates. Registration is optional: no other feature may require it. |
| DMN-021 | Creating a managed workspace MUST run `git worktree add` in the target repository: new branch (`-b <branch> <path> <base_ref>`, default branch name `pmo/<slug>-<suffix>`, unique in the repo) or existing branch. The repository may be registered or given by path. |
| DMN-022 | Creating a workspace **from a PR** (URL or `owner/repo#N`) MUST fetch the PR head into a local branch, create the worktree on it, and store `pr_url`/`pr_number`. |
| DMN-023 | **Adopting** an existing directory MUST record it as `adopted` (git or not); the daemon MUST never delete an adopted directory, only forget it. |
| DMN-024 | Removing a managed workspace MUST refuse when it has uncommitted changes unless forced, and MUST refuse while a thread with a live process uses it. The branch is kept unless explicitly requested. |
| DMN-025 | Git operations on the same repository MUST be serialized (per-repository lock), because `git worktree` mutates the shared `.git`. |
| DMN-026 | For each git workspace the daemon MUST report: branch, commits ahead/behind `base_ref`, changed files, the diff (`git diff <base_ref>...HEAD` plus uncommitted changes) and the linked PR, for the "Changes" panel. |
| DMN-027 | A workspace MAY be attached to several threads; the daemon MUST expose which threads use it, so the UI can warn about concurrent edits. It MUST NOT forbid it. |

### Threads and workspaces

| ID | Requirement |
| --- | --- |
| DMN-028 | A thread MAY have zero workspaces; its cwd is then its own directory `threads/<id>/home` (created on demand). |
| DMN-029 | Attaching a workspace to a thread with a live RPC process MUST send `/add-dir <path>` through `prompt` (omp built-in available over RPC, verified on 18.18.1); detaching MUST send `/remove-dir <path>`. Changing the first workspace (cwd) MUST restart the process. Behavior while the thread is `Busy` **[VERIFY in S-07]**. |

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
| DMN-030 | The process MUST be started as `omp --mode rpc-ui` with cwd = the thread's first workspace (or its home dir, DMN-028), one `--add-dir <path>` per other workspace, plus `--approval-mode` and `--model` when set on the thread. |
| DMN-031 | After the `ready` frame the daemon MUST check that `supportedProtocolVersions` contains `2` (`omp-rpc` performs the negotiation), then send in order: `open_session { sessionDir }`, `set_event_filter { events: null, messageUpdates: "delta" }`, `set_subagent_subscription { level: "progress" }`, `set_ask_dialog { enabled: true }`, `set_host_tools`. A failure in any step moves the thread to `Failed` with the error message. |
| DMN-032 | After `open_session` the daemon MUST store `sessionId` in `threads.omp_session_id` and load history with `get_messages` (paged by `omp-rpc`) as the reducer's initial event. |
| DMN-033 | Before starting omp on a thread that already has `omp_session_id`, the daemon MUST check the session lease (`omp session lease --json -- <id>`): if another omp process owns the session, startup MUST fail with an error saying so (e.g. the session is open in an external terminal). |
| DMN-034 | A process that is `Idle`, settled, and not watched by any client MUST be stopped after `idle_timeout` (default 15 minutes, configurable). A `Busy` process or one with pending interactive requests is never stopped. |
| DMN-035 | If the process exits unexpectedly, the thread MUST move to `Failed` with the exit code and the last stderr lines; pending interactive requests MUST be cancelled towards the UI. Restart happens only on the user's next command. |
| DMN-036 | omp's stderr MUST be written to a per-thread log file (`<data-dir>/threads/<id>/omp.log`, rotated) and never mixed with protocol stdout. |
| DMN-037 | Commands to the same thread MUST be serialized by its actor; different threads proceed in parallel. |
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
| DMN-054 | `ask` answers MUST match omp's required shape (one entry per question, in order, exact labels); the daemon MUST validate them before forwarding and reject malformed ones with an error. |

### Daemon-provided tools (host tools)

| ID | Requirement |
| --- | --- |
| DMN-060 | The daemon MUST register its host tools through `omp-rpc`'s `ClientOptions::tools`. Initial set: `open_in_editor { path, line? }` (opens the file with the configured editor command, or asks the UI to). |
| DMN-061 | Every host tool MUST honour cancellation (`HostToolContext::is_cancelled`) and return an error (`isError: true`) instead of panicking. |

## Commands towards omp

Mapping between protocol commands ([03](03-protocol.md)) and omp RPC commands. All go through the thread actor.

| pmo command | omp RPC |
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
