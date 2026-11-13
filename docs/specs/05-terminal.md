# 05 — Terminal (removed)

**Status:** removed (D-06, PRT-135). A terminal panel running interactive omp on a thread's session (the escape hatch
to TUI-only features) or a plain shell was designed here but never built; the `terminal.*` methods the daemon refused,
the `terminal/<id>` topic, the terminal permission and the `terminal` process state were removed from the protocol, the
daemon and the UI. The rows below keep their IDs so they are not reused.

| ID | Requirement |
| --- | --- |
| TRM-001 | (Removed.) Opening an `omp` terminal required the thread to be idle or stopped with no pending interactions. |
| TRM-002 | (Removed.) The RPC process was stopped and the session lease checked before spawning the terminal process. |
| TRM-003 | (Removed.) A thread with an open `omp` terminal was in the `Terminal` process state. |
| TRM-004 | (Removed.) The thread became `Stopped` when its `omp` terminal exited. |
| TRM-005 | (Removed.) `shell` terminals did not change the thread's process state. |
| TRM-006 | (Removed.) The terminal environment (`TERM`, `COLORTERM`, `PI_TUI_NATIVE`). |
| TRM-007 | (Removed.) Deleting or archiving a thread closed its terminals. |
| TRM-010 | (Removed.) The scrollback replayed on subscribe. |
| TRM-011 | (Removed.) Several clients attached to one terminal. |
| TRM-012 | (Removed.) Input frames for an unknown channel were dropped. |
| TRM-020 | (Removed.) xterm.js as the terminal panel's dependency. |
| TRM-021 | (Removed.) The terminal widget over binary WebSocket frames. |
| TRM-022 | (Removed.) The thread view's terminal mode with a "return to app" action. |
