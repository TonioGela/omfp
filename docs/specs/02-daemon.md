# 02 — Daemon (`omfp daemon`)

The daemon owns repositories, workspaces, threads and omp processes. It serves the protocol in [03](03-protocol.md) and, in web
mode, the UI's static files.

## Startup and configuration

```text
omfp daemon [--bind 127.0.0.1:7420] [--data-dir DIR] [--omp-bin PATH] [--static-dir DIR] [--allow-omp-mismatch]
            [--theme-file PATH] [--no-title-generation] [--config FILE]
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
| DMN-008 | The web UI bundle MUST be resolved in this order: `--static-dir`; the `web/` directory next to the daemon executable when it holds an `index.html` (packaging: the release bundle of `just build-web`, `ui/packages/app/dist`, ships there); otherwise only the API is served and a warning is logged. The bundle is served at `/`, unknown paths fall back to `index.html`. |
| DMN-009 | The daemon MUST read an optional TOML config file, `$XDG_CONFIG_HOME/omfp/config.toml` (else `~/.config/omfp/config.toml`; `--config <file>` picks another): `[daemon]` holds every `omfp daemon` option under the flag's name in snake case (`bind`, `data_dir`, `workspaces_root`, `omp_bin`, `static_dir`, `idle_timeout_minutes`, `max_processes`, `event_buffer`, `allow_origin`, `theme_file`, `title_generation`, …; `~` expanded); precedence is flag > file > default; unknown keys are an error naming the key. `[ui]` (UI-097) and `[keymap]` are UI settings the daemon keeps and serves (`settings.get`, pushed to every window on change); the daemon re-reads the file when it changes (polling) and applies `[ui]`/`[keymap]` live, while `[daemon]` changes need a restart (logged). `settings.set` (e.g. from `:set`) writes the change back into the file, preserving comments and other keys where possible. An invalid file is reported at startup (fatal) or, on reload, logged and shown in the UI while the previous settings stay. |

## Persistence (SQLite)

Schema (`PRAGMA user_version = 4`; migration 2 adds `created_client`; migration 3 moves to workspace folders, D-11;
migration 4 adds `title_source`, DMN-073):

| Table | Main columns |
| --- | --- |
| `repositories` | `id` (ULID), `name`, `path` (absolute, unique), `default_base_ref`, `remote_url` (nullable), `created_at` — known repositories, a shortcut for adding repos to workspaces |
| `workspaces` | `id` (ULID), `name`, `path` (absolute, unique), `owned` (the daemon created the folder), `created_by` (`user` \| thread id), `created_client` (nullable, PRT-033), `created_at` |
| `threads` | `id` (ULID), `title`, `title_source` (`placeholder` \| `generated` \| `set`; `set` for threads older than migration 4), `workspace_id`, `session_dir`, `omp_session_id` (nullable), `approval_mode` (nullable; new threads get `yolo`, DMN-074), `model` (nullable, `provider/id`), `thinking_level` (nullable), `created_by` (`user` \| thread id), `created_client` (nullable), `created_at`, `archived_at` (nullable) |
| `thread_roots` | `thread_id`, `path` — the `--add-dir` roots last given to the thread's omp session (DMN-029) |
| `thread_tags` | `thread_id`, `tag` |
| `settings` | `key`, `value` (JSON); `theme`: the user's theme CSS, a string or `null` (DMN-075) |

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
| DMN-060 | The daemon MUST register its host tools when it launches a thread's omp (they are per thread). Initial set: `open_in_editor { path, line? }` and `set_thread_title { title }` (renames the thread with a `set` title, at most 200 characters, and names the omp session, DMN-073). The daemon runs the user's editor itself — `$VISUAL`, else `$EDITOR`, from the daemon's environment, with the file (and `+<line>` when given) — but only when the window the user last used on that thread is a desktop window on the daemon's machine (M7). Otherwise, and when no editor is set, the tool returns an error the agent can act on (e.g. "open_in_editor is not available from the web client"). A terminal editor (`vim`, `nano`, `hx`, …) has no terminal to run in: the tool reports that too instead of spawning it. |
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
| `threads.update { title }`, host tool `set_thread_title`, a generated title (DMN-073) | `set_session_name` (when the thread's omp runs) |

| ID | Requirement |
| --- | --- |
| DMN-070 | A command on a `Stopped` thread that needs omp (e.g. `thread.prompt`) MUST start the process first. Read-only commands on a stopped thread (e.g. `thread.tree`) MAY start it. |
| DMN-071 | omp errors (`success: false`, with `code` when present: `session_busy`, `stale_cursor`, `entry_not_found`, …) MUST reach the UI with the same `code`. |
| DMN-072 | After a `fork` the daemon MUST re-read `get_state` and update `omp_session_id` (fork moves the process to a new session file). |
| DMN-073 | Thread titles come from the conversation, not from the user at creation: `threads.create` takes no title from the UI (an optional `title` stays for scripts and agents; it is `set`). A thread without a title shows `""` until its first message (`titleSource: placeholder`), then the first words of that message (at most six words and 48 characters, `…` when cut; also the default name of a new workspace). omp 18.18.6 forces `PI_NO_TITLE=1` in RPC modes (`rpc`, `rpc-ui`, `acp`: `packages/coding-agent/src/main.ts` lines 2025–2032, `docs/rpc.md` line 29), so after the first message the daemon generates the title itself with one omp run, in the background, through the agent port (`AgentLauncher::generate_title`; fake in L1/L2, real in L3): the thread's omp binary, environment and workspace folder (the user's configuration), `omp -p --no-session --no-tools --no-title --no-lsp --no-context-files --no-skills --no-rules --model @smol --system-prompt <title instructions> --max-time 120 -- <first message>`. Verified in the omp source at tag `9058ab9`: `-p`/`--print`, `--no-session`, `--no-tools`, `--no-title` (print mode is not an RPC mode, so without it omp would title its throwaway session with an extra model call) in `packages/coding-agent/src/cli/args.ts` lines 363–397 and `docs/cli-reference.md` lines 80, 93, 142, 183; `--model` takes a role as `@<role>` (`docs/cli-reference.md` line 104, `MODEL_ROLE_ALIAS_PREFIX = "@"` in `src/config/model-roles.ts` line 19); `smol` is omp's fast-model role (`src/config/model-roles.ts` line 58, "Fast"); omp's own title generator uses `tiny`, which resolves through `smol` (`src/utils/title-generator.ts` line 160, `src/config/model-resolver.ts` line 1186), and a `smol` role the user did not configure inherits the default model (`src/config/model-resolver.ts` lines 1168–1170). The first line of the output, without quotes or a `Title:` label, becomes the title (`generated`, at most 80 characters); the daemon stores it, sends `set_session_name` to the thread's omp, then publishes it on the `index` topic. A failed run keeps the placeholder. A title set by the user (`threads.update`) or by the agent (host tool `set_thread_title`, DMN-060) is `set` and never replaced, also when it arrives while a title is generated. `--no-title-generation` turns the run off: contract tests and E2E journeys against the fake LLM pass it unless they script the extra model call (contract test `the_first_message_names_the_thread_through_an_omp_print_mode_run` does); the fake launcher answers title runs from its script (`FakeLauncher::push_title`; none scripted: the placeholder stays). |
| DMN-074 | A new thread's approval mode defaults to `yolo` (the thread's `approval_mode`, passed as `--approval-mode`); `threads.create` and `threads.update` may set another mode, which applies from the next start of the thread's omp. |

### User theme

| ID | Requirement |
| --- | --- |
| DMN-075 | The daemon MUST keep one user theme, CSS applied by every window after the built-in tokens: stored in SQLite (`settings.theme`), read with `theme.get` and replaced with `theme.set` (`null` restores the built-in themes; over 1 MiB is `bad_request`), published on the `index` topic (`IndexSnapshot.theme`, event `theme_changed`) so every connected window changes without reload, and served at `GET /theme.css` (`text/css`, empty when unset, `Cache-Control: no-store`, no token, like the bundle). The daemon also follows a theme file on its host, `--theme-file`, by default `$XDG_CONFIG_HOME/omfp/theme.css` (else `~/.config/omfp/theme.css`): read at startup (a missing file keeps the stored theme), then polled every second (no file-watching dependency); new content becomes the theme, removing the file clears it, a file over 1 MiB is ignored with a warning. The last change wins, whether from `theme.set` or from the file. `omfp theme set <file>` sends the file with `theme.set`. |
