# The omfp window

What the user sees around the widgets, so an agent can point at it ("press `g v`", "pick Review in the mode line").
`omfp://docs/widgets` covers views, widgets, init files and TSX views; `omfp://keymap` lists every default key and
built-in command with its title and `:` names.

## Left column

The frame's own column, outside every view: one filter field at the top (`/` focuses it; it matches thread titles,
workspace and repository names and conversation text), "+ workspace" (a name, or a path starting with `/` or `~`, with
completion), then one section per workspace — its name (the current one highlighted, a menu to rename or remove it),
its threads with their state marks (running, awaiting input, failed, idle, unread) and "+ thread", which opens a draft
thread there (the daemon creates the thread when its first message is sent). Archived threads hide behind
"Archived (N)". A section folds with its chevron or `z a`; `d d` deletes the focused row (after a confirmation).
Selecting a thread makes its workspace the **current workspace**: the view, the init file `.omfp/init.ts`, the keymap
`.omfp/keymap.json` and `:widget add` all follow it. A splitter resizes the column (wide windows); under 1100 px it
folds into a strip of state marks (`Space t` opens it), on a phone it is one of the bottom bar's areas.

## Mode line

The bar at the bottom of the view area, like vim's:

- the **mode pill**: `NORMAL` (no text field has the focus), `INSERT` (a text field has it), `COMMAND` (the `:` line is
  open), or vim's own mode while a file editor has the focus;
- the selected thread's title and state;
- the **view tabs**, when the current workspace has several views: `Default · Review · Ops` by title, the current
  one highlighted; a click switches. The choice belongs to the window and survives reloads; a workspace without that
  view shows its default view;
- at the right end the newest **notice** (`◆ title body`, `+n` when more wait): a thread that needs the user, failed
  or finished while the user looked elsewhere, or what a widget or agent notified (`omfp_notify`). `g n` or a click
  opens its thread; it goes after 8 s.

In command mode the `:` line (or the file picker) replaces the title, the view tabs and the notice.

## The `:` line

`:` (or `Space :`) opens it; it doubles as the command palette: typing filters the commands by name, Tab completes,
the arrows move, Enter runs, Escape closes. After a space it completes the command's arguments. Commands run where the
focus was before the line opened. The built-in ones (`omfp://keymap` has them all):

| Command | What it does |
| --- | --- |
| `:new`, `:workspace` | A draft thread in the current workspace; a new workspace. |
| `:archive`, `:unarchive`, `:delete`, `:rename [<title>]` | The selected thread. |
| `:stop`, `:stop!`, `:restart`, `:next` | Stop its process (with every thread it spawned); restart it; the next thread needing the user. |
| `:spawned` | Show only the threads the selected one spawned (again: all). |
| `:open <path>[:<line>]` | Open a file of the workspace in a tab next to the chat. |
| `:rescan` | Rescan the current workspace's repositories. |
| `:view <id>`, `:view reset` | Switch view; write the default `views.json` again (replacing the file). |
| `:widget add <reference>` | Add a widget to the current view (last side tab, else the end) and write `views.json`; refused on views that exist only in an init file or a TSX file. |
| `:widget copy <reference> [<name>]` | Copy a built-in or user widget into `.omfp/widgets/<name>/`. |
| `:widget save[!] <widget> [<name>]` | Save a workspace widget to the user library (`!` replaces). |
| `:theme light\|dark\|system`, `:density compact\|normal`, `:set <key>=<value>` | Appearance and the daemon's settings. |
| `:help` | Every effective key and command, filterable (`?` too). |
| `:describe-key`, `:describe-command <name>` | What a key runs and where it was bound; a command's names, keys and who registered it. |
| `:notice` | Open the newest notice. |

Widgets (`useCommand`) and init files (`omfp.command`) add commands to the same line while they are shown or loaded.

## Default keys

In normal mode (no text field focused):

| Keys | What they do |
| --- | --- |
| `j` / `k`, `g g` / `G`, `Ctrl-d` / `Ctrl-u` | Next / previous, first / last, half a page — threads in the left column, messages in the transcript. |
| `/` | Filter threads. |
| `i` | Focus the composer. |
| `:`, `Space :` | The `:` line. |
| `?` | Help. |
| `g v` | Pick a view: the `:` line opens on `view`, its completions are the views. |
| `] v`, `[ v` | Next / previous view (round the list). |
| `g f` | Open the focused path (a path link, a tool card, a changed file) in a file tab. |
| `g n` | Open the newest notice. |
| `Space a`, `] a`, `Alt-a` | Next thread needing the user. |
| `Space n`, `Alt-n` | New thread; `Space w` new workspace. |
| `Space r`, `Space x`, `Space u`, `Space d` | Rename, archive, unarchive, delete the selected thread. |
| `Space s`, `Space S`, `Space R` | Stop the thread (with the threads it spawned); restart it. |
| `Space t` | Show or hide the threads column (narrow window). |
| `d d`, `z a` | Delete the focused row; fold the focused workspace. |
| `Ctrl-c`, `u` | Clear the draft, then abort the turn; restore the last cleared draft. |

In insert mode `Esc` leaves the text field, `Ctrl-c` clears then aborts, `Alt-a` / `Alt-n` work too; in the composer
Enter sends, Shift-Enter steers the running turn, Alt-Enter adds a line. Keys are rebindable: the daemon's `[keymap]`
config, the workspace's `.omfp/keymap.json`, then the init files' `bind` (each layer wins over the ones before it).
