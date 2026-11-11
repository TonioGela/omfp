//! The daemon's tray icon (DMN-128..130, UI-217..219): `omfp daemon --tray`, which `omfp desktop` passes to the
//! daemon it starts, shows a KDE/freedesktop `StatusNotifierItem` with a `com.canonical.dbusmenu` menu on the session
//! bus, over gio's D-Bus API. The icon and menu follow the daemon's own thread index; the menu opens windows, the
//! browser, and stops the daemon. Without a session bus the daemon runs without a tray; without a
//! `StatusNotifierWatcher` it says so once and registers when one appears.

use std::collections::HashMap;
use std::ffi::OsString;
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::thread::JoinHandle;

use gio::glib::{self, ToVariant as _, Variant, VariantTy};
use omfp_core::ProcessState;
use omfp_daemon::{DaemonHandle, StopRequest};
use omfp_protocol::ThreadSummary;

/// What the tray shows of the thread index (UI-217): non-archived threads, those whose agent works, and those waiting
/// for the user (newest first).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TrayStatus {
    pub threads: usize,
    pub running: usize,
    pub need_you: Vec<NeedsYou>,
}

/// A thread waiting for the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NeedsYou {
    pub id: String,
    pub title: String,
}

/// The tray icon (UI-219).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrayIcon {
    /// No agent works and nobody waits.
    Idle,
    /// At least one agent works.
    Running,
    /// At least one thread waits for the user.
    NeedsYou,
}

impl TrayStatus {
    /// The status of `threads` as the left column counts them (UI-020): attention first, then a working process
    /// (starting, or running and not settled).
    #[must_use]
    pub fn of(threads: &[ThreadSummary]) -> Self {
        let live: Vec<&ThreadSummary> = threads
            .iter()
            .filter(|thread| thread.archived_at.is_none())
            .collect();
        let mut waiting: Vec<&ThreadSummary> = live
            .iter()
            .copied()
            .filter(|thread| thread.needs_attention)
            .collect();
        waiting.sort_by_key(|thread| std::cmp::Reverse(thread.updated_at));
        Self {
            threads: live.len(),
            running: live
                .iter()
                .filter(|thread| {
                    !thread.needs_attention
                        && match thread.process {
                            ProcessState::Starting => true,
                            ProcessState::Running => !thread.settled,
                            _ => false,
                        }
                })
                .count(),
            need_you: waiting
                .into_iter()
                .map(|thread| NeedsYou {
                    id: thread.id.clone(),
                    title: thread.title.clone(),
                })
                .collect(),
        }
    }

    /// The icon: a thread waiting for the user wins over a working one.
    #[must_use]
    pub fn icon(&self) -> TrayIcon {
        if !self.need_you.is_empty() {
            TrayIcon::NeedsYou
        } else if self.running > 0 {
            TrayIcon::Running
        } else {
            TrayIcon::Idle
        }
    }

    /// The menu's status line and the tooltip: `N threads · N running · N need you`.
    #[must_use]
    pub fn summary(&self) -> String {
        let threads = if self.threads == 1 {
            "1 thread".to_owned()
        } else {
            format!("{} threads", self.threads)
        };
        let need_you = if self.need_you.len() == 1 {
            "1 needs you".to_owned()
        } else {
            format!("{} need you", self.need_you.len())
        };
        format!("{threads} · {} running · {need_you}", self.running)
    }

    /// The threads whose work the quit interrupts: working or waiting for the user.
    #[must_use]
    pub fn busy(&self) -> usize {
        self.running + self.need_you.len()
    }

    /// `Quit daemon (stops N running threads)`, or `Quit daemon` when nothing is busy (DMN-130).
    #[must_use]
    pub fn quit_label(&self) -> String {
        match self.busy() {
            0 => "Quit daemon".to_owned(),
            1 => "Quit daemon (stops 1 running thread)".to_owned(),
            busy => format!("Quit daemon (stops {busy} running threads)"),
        }
    }
}

/// What a menu entry does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// A new window on the daemon (`omfp desktop`).
    NewWindow,
    /// The daemon's UI in the browser, with its token.
    OpenInBrowser,
    /// A new window on this thread (`omfp desktop --thread <id>`).
    OpenThread(String),
    /// Stops the daemon (DMN-130).
    Quit,
}

/// One entry of the dbusmenu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuItem {
    pub id: i32,
    /// The dbusmenu label (`_` doubled: a single one marks a mnemonic); empty for a separator.
    pub label: String,
    pub enabled: bool,
    pub separator: bool,
    pub action: Option<Action>,
}

const NEW_WINDOW: i32 = 1;
const OPEN_IN_BROWSER: i32 = 2;
const STATUS: i32 = 3;
const QUIT: i32 = 4;
const SEPARATOR_TOP: i32 = 5;
const SEPARATOR_BOTTOM: i32 = 6;
/// The first id of a thread entry.
const FIRST_THREAD_ID: i32 = 100;
/// Longer thread titles are cut with an ellipsis.
const TITLE_CHARS: usize = 60;

/// Stable menu ids of the waiting threads: a thread keeps its id while it stays in the menu, so a click on a menu the
/// host shows from an older layout never opens another thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadIds {
    ids: HashMap<String, i32>,
    next: i32,
}

impl Default for ThreadIds {
    fn default() -> Self {
        Self {
            ids: HashMap::new(),
            next: FIRST_THREAD_ID,
        }
    }
}

impl ThreadIds {
    /// The ids of `threads`, new ones numbered on; threads no longer listed lose theirs.
    fn assign(&mut self, threads: &[NeedsYou]) -> Vec<i32> {
        let mut ids = HashMap::with_capacity(threads.len());
        let assigned = threads
            .iter()
            .map(|thread| {
                let id = self.ids.get(&thread.id).copied().unwrap_or_else(|| {
                    let id = self.next;
                    self.next += 1;
                    id
                });
                ids.insert(thread.id.clone(), id);
                id
            })
            .collect();
        self.ids = ids;
        assigned
    }
}

/// The tray menu (UI-218): `New window`, `Open in browser`, the status line (not clickable), the threads waiting for
/// the user, `Quit daemon`.
#[must_use]
pub fn menu(status: &TrayStatus, ids: &mut ThreadIds) -> Vec<MenuItem> {
    let item = |id, label: String, action: Option<Action>| MenuItem {
        id,
        enabled: action.is_some(),
        label,
        separator: false,
        action,
    };
    let separator = |id| MenuItem {
        id,
        label: String::new(),
        enabled: true,
        separator: true,
        action: None,
    };
    let mut items = vec![
        item(NEW_WINDOW, "New window".into(), Some(Action::NewWindow)),
        item(
            OPEN_IN_BROWSER,
            "Open in browser".into(),
            Some(Action::OpenInBrowser),
        ),
        separator(SEPARATOR_TOP),
        item(STATUS, status.summary(), None),
    ];
    let thread_ids = ids.assign(&status.need_you);
    items.extend(status.need_you.iter().zip(thread_ids).map(|(thread, id)| {
        item(
            id,
            format!("◆ {}", menu_label(&thread.title)),
            Some(Action::OpenThread(thread.id.clone())),
        )
    }));
    items.push(separator(SEPARATOR_BOTTOM));
    items.push(item(QUIT, status.quit_label(), Some(Action::Quit)));
    items
}

/// A thread title as a dbusmenu label: `New thread` before the first message (as the UI shows it), cut to
/// [`TITLE_CHARS`], underscores doubled.
fn menu_label(title: &str) -> String {
    let title = title.trim();
    let title = if title.is_empty() {
        "New thread"
    } else {
        title
    };
    let mut label: String = title.chars().take(TITLE_CHARS).collect();
    if title.chars().nth(TITLE_CHARS).is_some() {
        label.push('…');
    }
    label.replace('_', "__")
}

/// The command line of a new window on the daemon of `data_dir`, on `thread` when given (UI-218).
#[must_use]
pub fn window_command(data_dir: &Path, thread: Option<&str>) -> Vec<OsString> {
    let mut line = vec![
        OsString::from("desktop"),
        "--data-dir".into(),
        data_dir.as_os_str().to_owned(),
    ];
    if let Some(thread) = thread {
        line.extend(["--thread".into(), thread.into()]);
    }
    line
}

/// The `a(iiay)` pixmaps of an icon: every rendered size, ARGB32 in network byte order (built by build.rs).
#[must_use]
pub fn pixmaps(icon: TrayIcon) -> Variant {
    macro_rules! pixmap {
        ($name:literal) => {
            include_bytes!(concat!(env!("OUT_DIR"), "/tray-", $name, ".argb")).as_slice()
        };
    }
    let sizes: [(i32, &[u8]); 2] = match icon {
        TrayIcon::Idle => [(32, pixmap!("idle-32")), (64, pixmap!("idle-64"))],
        TrayIcon::Running => [(32, pixmap!("running-32")), (64, pixmap!("running-64"))],
        TrayIcon::NeedsYou => [(32, pixmap!("attention-32")), (64, pixmap!("attention-64"))],
    };
    Variant::array_from_iter_with_type(
        pixmap_type(),
        sizes.into_iter().map(|(size, argb)| {
            Variant::tuple_from_iter([
                size.to_variant(),
                size.to_variant(),
                Variant::array_from_fixed_array(argb),
            ])
        }),
    )
}

fn pixmap_type() -> &'static VariantTy {
    VariantTy::new("(iiay)").expect("a valid type string")
}

/// The dbusmenu properties of `item` (`label`, `enabled`, `type`).
#[must_use]
pub fn item_properties(item: &MenuItem) -> HashMap<String, Variant> {
    let mut properties = HashMap::new();
    if item.separator {
        properties.insert("type".to_owned(), "separator".to_variant());
    } else {
        properties.insert("label".to_owned(), item.label.to_variant());
        properties.insert("enabled".to_owned(), item.enabled.to_variant());
    }
    properties
}

/// The `(ia{sv}av)` layout of the menu: the root (id 0) and its entries.
#[must_use]
pub fn layout(items: &[MenuItem]) -> Variant {
    let mut root = HashMap::new();
    root.insert("children-display".to_owned(), "submenu".to_variant());
    layout_node(
        0,
        &root,
        items
            .iter()
            .map(|item| layout_node(item.id, &item_properties(item), [])),
    )
}

fn layout_node(
    id: i32,
    properties: &HashMap<String, Variant>,
    children: impl IntoIterator<Item = Variant>,
) -> Variant {
    Variant::tuple_from_iter([
        id.to_variant(),
        properties.to_variant(),
        Variant::array_from_iter_with_type(
            VariantTy::VARIANT,
            children
                .into_iter()
                .map(|child| Variant::from_variant(&child)),
        ),
    ])
}

const ITEM_PATH: &str = "/StatusNotifierItem";
const MENU_PATH: &str = "/MenuBar";
const ITEM_INTERFACE: &str = "org.kde.StatusNotifierItem";
const MENU_INTERFACE: &str = "com.canonical.dbusmenu";
const WATCHER: &str = "org.kde.StatusNotifierWatcher";
const WATCHER_PATH: &str = "/StatusNotifierWatcher";

const ITEM_XML: &str = r#"<node>
  <interface name="org.kde.StatusNotifierItem">
    <property name="Category" type="s" access="read"/>
    <property name="Id" type="s" access="read"/>
    <property name="Title" type="s" access="read"/>
    <property name="Status" type="s" access="read"/>
    <property name="WindowId" type="i" access="read"/>
    <property name="IconName" type="s" access="read"/>
    <property name="IconPixmap" type="a(iiay)" access="read"/>
    <property name="AttentionIconName" type="s" access="read"/>
    <property name="AttentionIconPixmap" type="a(iiay)" access="read"/>
    <property name="ToolTip" type="(sa(iiay)ss)" access="read"/>
    <property name="ItemIsMenu" type="b" access="read"/>
    <property name="Menu" type="o" access="read"/>
    <method name="ContextMenu"><arg name="x" type="i" direction="in"/><arg name="y" type="i" direction="in"/></method>
    <method name="Activate"><arg name="x" type="i" direction="in"/><arg name="y" type="i" direction="in"/></method>
    <method name="SecondaryActivate"><arg name="x" type="i" direction="in"/><arg name="y" type="i" direction="in"/></method>
    <method name="Scroll"><arg name="delta" type="i" direction="in"/><arg name="orientation" type="s" direction="in"/></method>
    <signal name="NewTitle"/>
    <signal name="NewIcon"/>
    <signal name="NewAttentionIcon"/>
    <signal name="NewToolTip"/>
    <signal name="NewStatus"><arg name="status" type="s"/></signal>
  </interface>
</node>"#;

const MENU_XML: &str = r#"<node>
  <interface name="com.canonical.dbusmenu">
    <property name="Version" type="u" access="read"/>
    <property name="TextDirection" type="s" access="read"/>
    <property name="Status" type="s" access="read"/>
    <property name="IconThemePath" type="as" access="read"/>
    <method name="GetLayout">
      <arg type="i" name="parentId" direction="in"/>
      <arg type="i" name="recursionDepth" direction="in"/>
      <arg type="as" name="propertyNames" direction="in"/>
      <arg type="u" name="revision" direction="out"/>
      <arg type="(ia{sv}av)" name="layout" direction="out"/>
    </method>
    <method name="GetGroupProperties">
      <arg type="ai" name="ids" direction="in"/>
      <arg type="as" name="propertyNames" direction="in"/>
      <arg type="a(ia{sv})" name="properties" direction="out"/>
    </method>
    <method name="GetProperty">
      <arg type="i" name="id" direction="in"/>
      <arg type="s" name="name" direction="in"/>
      <arg type="v" name="value" direction="out"/>
    </method>
    <method name="Event">
      <arg type="i" name="id" direction="in"/>
      <arg type="s" name="eventId" direction="in"/>
      <arg type="v" name="data" direction="in"/>
      <arg type="u" name="timestamp" direction="in"/>
    </method>
    <method name="EventGroup">
      <arg type="a(isvu)" name="events" direction="in"/>
      <arg type="ai" name="idErrors" direction="out"/>
    </method>
    <method name="AboutToShow">
      <arg type="i" name="id" direction="in"/>
      <arg type="b" name="needUpdate" direction="out"/>
    </method>
    <method name="AboutToShowGroup">
      <arg type="ai" name="ids" direction="in"/>
      <arg type="ai" name="updatesNeeded" direction="out"/>
      <arg type="ai" name="idErrors" direction="out"/>
    </method>
    <signal name="ItemsPropertiesUpdated">
      <arg type="a(ia{sv})" name="updatedProps"/>
      <arg type="a(ias)" name="removedProps"/>
    </signal>
    <signal name="LayoutUpdated">
      <arg type="u" name="revision"/>
      <arg type="i" name="parent"/>
    </signal>
    <signal name="ItemActivationRequested">
      <arg type="i" name="id"/>
      <arg type="u" name="timestamp"/>
    </signal>
  </interface>
</node>"#;

/// What the tray acts on: the daemon's data directory, URL and stop.
#[derive(Debug, Clone)]
struct Daemon {
    exe: PathBuf,
    data_dir: PathBuf,
    /// The UI's URL with the token in the fragment.
    url: String,
    stop: StopRequest,
}

impl Daemon {
    fn act(&self, connection: &gio::DBusConnection, action: &Action) {
        match action {
            Action::NewWindow => self.open_window(None),
            Action::OpenThread(thread) => self.open_window(Some(thread)),
            Action::OpenInBrowser => self.open_in_browser(connection),
            Action::Quit => {
                eprintln!("omfp daemon: quit from the tray");
                self.stop.request();
            }
        }
    }

    /// `omfp desktop --data-dir <dir> [--thread <id>]`, detached like the daemon itself (ARC-003): its own process
    /// group, stdin `/dev/null`, its output in the daemon's log.
    fn open_window(&self, thread: Option<&str>) {
        spawn_detached(
            Command::new(&self.exe).args(window_command(&self.data_dir, thread)),
            "omfp desktop",
        );
    }

    /// The daemon's URL through the desktop portal's `OpenURI`, else `xdg-open`.
    fn open_in_browser(&self, connection: &gio::DBusConnection) {
        let options: HashMap<String, Variant> = HashMap::new();
        let url = self.url.clone();
        connection.call(
            Some("org.freedesktop.portal.Desktop"),
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.OpenURI",
            "OpenURI",
            Some(&("", url.as_str(), options).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            -1,
            None::<&gio::Cancellable>,
            move |reply| {
                if reply.is_err() {
                    spawn_detached(Command::new("xdg-open").arg(&url), "xdg-open");
                }
            },
        );
    }
}

fn spawn_detached(command: &mut Command, name: &str) {
    match command.stdin(Stdio::null()).process_group(0).spawn() {
        Ok(mut child) => {
            // Reaped while the daemon lives.
            std::thread::spawn(move || child.wait());
        }
        Err(error) => eprintln!("omfp daemon: cannot start {name}: {error}"),
    }
}

/// The tray's state, read by the D-Bus handlers and replaced on every index change.
#[derive(Debug)]
struct State {
    status: TrayStatus,
    items: Vec<MenuItem>,
    ids: ThreadIds,
    revision: u32,
    icons: HashMap<TrayIcon, Variant>,
}

impl State {
    fn new(threads: &[ThreadSummary]) -> Self {
        let status = TrayStatus::of(threads);
        let mut ids = ThreadIds::default();
        let items = menu(&status, &mut ids);
        Self {
            status,
            items,
            ids,
            revision: 1,
            icons: [TrayIcon::Idle, TrayIcon::Running, TrayIcon::NeedsYou]
                .into_iter()
                .map(|icon| (icon, pixmaps(icon)))
                .collect(),
        }
    }

    /// Follows `threads`; returns whether the menu changed and the icon before, when it changed.
    fn update(&mut self, threads: &[ThreadSummary]) -> (bool, Option<TrayIcon>) {
        let status = TrayStatus::of(threads);
        if status == self.status {
            return (false, None);
        }
        let before = self.status.icon();
        self.items = menu(&status, &mut self.ids);
        self.status = status;
        self.revision += 1;
        (true, (before != self.status.icon()).then_some(before))
    }

    fn sni_status(&self) -> &'static str {
        if self.status.need_you.is_empty() {
            "Active"
        } else {
            "NeedsAttention"
        }
    }

    fn icon(&self, icon: TrayIcon) -> Variant {
        self.icons
            .get(&icon)
            .cloned()
            .unwrap_or_else(|| pixmaps(icon))
    }

    fn property(&self, name: &str, menu_path: &Variant) -> Variant {
        match name {
            "Category" => "ApplicationStatus".to_variant(),
            "Id" | "Title" => "omfp".to_variant(),
            "Status" => self.sni_status().to_variant(),
            "WindowId" => 0i32.to_variant(),
            "IconName" | "AttentionIconName" => "".to_variant(),
            "IconPixmap" => self.icon(self.status.icon()),
            "AttentionIconPixmap" => self.icon(TrayIcon::NeedsYou),
            "ToolTip" => Variant::tuple_from_iter([
                "".to_variant(),
                Variant::array_from_iter_with_type(pixmap_type(), std::iter::empty::<Variant>()),
                "omfp".to_variant(),
                self.status.summary().to_variant(),
            ]),
            "ItemIsMenu" => false.to_variant(),
            "Menu" => menu_path.clone(),
            _ => Variant::from_none(VariantTy::STRING),
        }
    }

    fn item(&self, id: i32) -> Option<&MenuItem> {
        self.items.iter().find(|item| item.id == id)
    }
}

fn lock(state: &Mutex<State>) -> std::sync::MutexGuard<'_, State> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The tray of a running daemon; dropping it removes the icon.
#[derive(Debug)]
pub struct Tray {
    context: glib::MainContext,
    main_loop: glib::MainLoop,
    thread: Option<JoinHandle<()>>,
}

impl Tray {
    /// Shows the daemon in the tray (DMN-128) on a thread of its own running a `GLib` main loop; `None` (said once in
    /// the log) when there is no session bus.
    #[must_use]
    pub fn start(daemon: &DaemonHandle) -> Option<Self> {
        let exe = match std::env::current_exe() {
            Ok(exe) => exe,
            Err(error) => {
                eprintln!("omfp daemon: no tray icon, cannot find this executable: {error}");
                return None;
            }
        };
        let target = Daemon {
            exe,
            data_dir: daemon.data_dir().to_path_buf(),
            url: daemon.url(),
            stop: daemon.stop_request(),
        };
        let threads = daemon.threads();
        let (ready, started) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("omfp-tray".into())
            .spawn(move || run(&target, threads, &ready))
            .ok()?;
        if let Ok(Some((context, main_loop))) = started.recv() {
            Some(Self {
                context,
                main_loop,
                thread: Some(thread),
            })
        } else {
            let _ = thread.join();
            None
        }
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        // Quit from inside the loop: a quit before `run` started would be lost.
        let main_loop = self.main_loop.clone();
        self.context.invoke(move || main_loop.quit());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The tray thread: registers the item and the menu, owns `org.kde.StatusNotifierItem-<pid>-1`, registers with every
/// `StatusNotifierWatcher` that appears, and follows `threads` until the main loop quits.
fn run(
    daemon: &Daemon,
    threads: tokio::sync::watch::Receiver<Vec<ThreadSummary>>,
    ready: &mpsc::Sender<Option<(glib::MainContext, glib::MainLoop)>>,
) {
    let context = glib::MainContext::new();
    let inner = context.clone();
    let result = context.with_thread_default(move || match Registration::new(daemon, &threads) {
        Ok(registration) => {
            let context = inner;
            let main_loop = glib::MainLoop::new(Some(&context), false);
            context.spawn_local(follow(
                registration.connection.clone(),
                Arc::clone(&registration.state),
                threads,
            ));
            let _ = ready.send(Some((context.clone(), main_loop.clone())));
            main_loop.run();
            drop(registration);
        }
        Err(error) => {
            eprintln!("omfp daemon: no tray icon: {error:#}");
            let _ = ready.send(None);
        }
    });
    if let Err(error) = result {
        eprintln!("omfp daemon: no tray icon: {error}");
        let _ = ready.send(None);
    }
}

/// Updates the state on every index change and tells the host: `LayoutUpdated` for the menu, `NewIcon`,
/// `NewAttentionIcon` and `NewStatus` when the icon changes, `NewToolTip` for the counts.
async fn follow(
    connection: gio::DBusConnection,
    state: Arc<Mutex<State>>,
    mut threads: tokio::sync::watch::Receiver<Vec<ThreadSummary>>,
) {
    while threads.changed().await.is_ok() {
        let list = threads.borrow_and_update().clone();
        let (changed, icon_before, revision, status) = {
            let mut state = lock(&state);
            let (changed, icon_before) = state.update(&list);
            (changed, icon_before, state.revision, state.sni_status())
        };
        if !changed {
            continue;
        }
        let emit = |path: &str, interface: &str, signal: &str, parameters: Option<Variant>| {
            if let Err(error) =
                connection.emit_signal(None, path, interface, signal, parameters.as_ref())
            {
                eprintln!("omfp daemon: tray signal {signal} failed: {error}");
            }
        };
        emit(
            MENU_PATH,
            MENU_INTERFACE,
            "LayoutUpdated",
            Some((revision, 0i32).to_variant()),
        );
        emit(ITEM_PATH, ITEM_INTERFACE, "NewToolTip", None);
        if icon_before.is_some() {
            emit(ITEM_PATH, ITEM_INTERFACE, "NewIcon", None);
            emit(ITEM_PATH, ITEM_INTERFACE, "NewAttentionIcon", None);
            emit(
                ITEM_PATH,
                ITEM_INTERFACE,
                "NewStatus",
                Some((status,).to_variant()),
            );
        }
    }
}

/// The objects, the bus name and the watcher subscription of the tray; released on drop.
struct Registration {
    connection: gio::DBusConnection,
    state: Arc<Mutex<State>>,
    objects: Vec<gio::RegistrationId>,
    owner: Option<gio::OwnerId>,
    /// Stops watching for a `StatusNotifierWatcher`.
    watcher: Arc<Mutex<Option<Unwatch>>>,
}

impl Registration {
    fn new(
        daemon: &Daemon,
        threads: &tokio::sync::watch::Receiver<Vec<ThreadSummary>>,
    ) -> anyhow::Result<Self> {
        let connection = gio::bus_get_sync(gio::BusType::Session, None::<&gio::Cancellable>)
            .map_err(|error| anyhow::anyhow!("no session bus ({error})"))?;
        let state = Arc::new(Mutex::new(State::new(&threads.borrow())));
        let menu_path = glib::variant::ObjectPath::try_from(MENU_PATH.to_owned())?.to_variant();
        let mut registration = Self {
            connection: connection.clone(),
            state: Arc::clone(&state),
            objects: Vec::new(),
            owner: None,
            watcher: Arc::new(Mutex::new(None)),
        };

        let item = interface(ITEM_XML, ITEM_INTERFACE)?;
        registration.objects.push(connection.register_object(
            ITEM_PATH,
            &item,
            {
                let daemon = daemon.clone();
                move |connection, _, _, _, method, _, invocation| {
                    if method == "Activate" {
                        daemon.act(&connection, &Action::NewWindow);
                    }
                    // `ContextMenu`, `SecondaryActivate` and `Scroll` do nothing: the host shows `Menu`.
                    invocation.return_value(None);
                }
            },
            {
                let state = Arc::clone(&state);
                move |_, _, _, _, property| lock(&state).property(property, &menu_path)
            },
            |_, _, _, _, _, _| false,
        )?);

        let menu = interface(MENU_XML, MENU_INTERFACE)?;
        registration.objects.push(connection.register_object(
            MENU_PATH,
            &menu,
            {
                let state = Arc::clone(&state);
                let daemon = daemon.clone();
                move |connection, _, _, _, method, parameters, invocation| {
                    menu_call(
                        &state,
                        &daemon,
                        &connection,
                        method,
                        &parameters,
                        invocation,
                    );
                }
            },
            |_, _, _, _, property| match property {
                "Version" => 3u32.to_variant(),
                "TextDirection" => "ltr".to_variant(),
                "Status" => "normal".to_variant(),
                _ => Vec::<String>::new().to_variant(),
            },
            |_, _, _, _, _, _| false,
        )?);

        let name = format!("org.kde.StatusNotifierItem-{}-1", std::process::id());
        let watcher = Arc::clone(&registration.watcher);
        registration.owner = Some(gio::bus_own_name_on_connection(
            &connection,
            &name,
            gio::BusNameOwnerFlags::NONE,
            move |connection, name| {
                // Registered with every watcher that appears (a panel restart included).
                let reported = Arc::new(AtomicBool::new(false));
                let service = name.to_owned();
                let id = gio::bus_watch_name_on_connection(
                    &connection,
                    WATCHER,
                    gio::BusNameWatcherFlags::NONE,
                    move |connection, _, _| register_item(&connection, &service),
                    move |_, _| {
                        if !reported.swap(true, Ordering::Relaxed) {
                            eprintln!(
                                "omfp daemon: no StatusNotifierWatcher on the session bus: no tray icon until one appears"
                            );
                        }
                    },
                );
                let unwatch: Unwatch = Box::new(move || gio::bus_unwatch_name(id));
                let previous = watcher
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .replace(unwatch);
                if let Some(previous) = previous {
                    previous();
                }
            },
            |_, name| eprintln!("omfp daemon: no tray icon: cannot own {name} on the session bus"),
        ));
        Ok(registration)
    }
}

/// `gio::bus_unwatch_name` of one watch (gio has two `WatcherId` types, one of them unnameable).
type Unwatch = Box<dyn FnOnce() + Send>;

impl Drop for Registration {
    fn drop(&mut self) {
        let unwatch = self
            .watcher
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(unwatch) = unwatch {
            unwatch();
        }
        if let Some(owner) = self.owner.take() {
            gio::bus_unown_name(owner);
        }
        for object in self.objects.drain(..) {
            let _ = self.connection.unregister_object(object);
        }
        // The name is released before the process goes on shutting down, so the host drops the icon now.
        let _ = self.connection.flush_sync(None::<&gio::Cancellable>);
    }
}

fn interface(xml: &str, name: &str) -> anyhow::Result<gio::DBusInterfaceInfo> {
    gio::DBusNodeInfo::for_xml(xml)?
        .lookup_interface(name)
        .ok_or_else(|| anyhow::anyhow!("no {name} in the introspection data"))
}

fn register_item(connection: &gio::DBusConnection, service: &str) {
    connection.call(
        Some(WATCHER),
        WATCHER_PATH,
        WATCHER,
        "RegisterStatusNotifierItem",
        Some(&(service,).to_variant()),
        None,
        gio::DBusCallFlags::NONE,
        -1,
        None::<&gio::Cancellable>,
        |reply| {
            if let Err(error) = reply {
                eprintln!("omfp daemon: the StatusNotifierWatcher refused the tray icon: {error}");
            }
        },
    );
}

/// A `com.canonical.dbusmenu` method call.
fn menu_call(
    state: &Mutex<State>,
    daemon: &Daemon,
    connection: &gio::DBusConnection,
    method: &str,
    parameters: &Variant,
    invocation: gio::DBusMethodInvocation,
) {
    let unknown = |invocation: gio::DBusMethodInvocation, id: i32| {
        invocation.return_dbus_error(
            "com.canonical.dbusmenu.Error.UnknownId",
            &format!("no menu item {id}"),
        );
    };
    match method {
        "GetLayout" => {
            let Some((parent, _, _)) = parameters.get::<(i32, i32, Vec<String>)>() else {
                return invocation.return_dbus_error(
                    "org.freedesktop.DBus.Error.InvalidArgs",
                    "GetLayout takes (iias)",
                );
            };
            let state = lock(state);
            let node = if parent == 0 {
                layout(&state.items)
            } else if let Some(item) = state.item(parent) {
                layout_node(item.id, &item_properties(item), [])
            } else {
                return unknown(invocation, parent);
            };
            invocation.return_value(Some(&Variant::tuple_from_iter([
                state.revision.to_variant(),
                node,
            ])));
        }
        "GetGroupProperties" => {
            let (ids, _) = parameters
                .get::<(Vec<i32>, Vec<String>)>()
                .unwrap_or_default();
            let state = lock(state);
            let entries = state
                .items
                .iter()
                .filter(|item| ids.is_empty() || ids.contains(&item.id))
                .map(|item| (item.id, item_properties(item)).to_variant());
            invocation.return_value(Some(&Variant::tuple_from_iter([
                Variant::array_from_iter_with_type(
                    VariantTy::new("(ia{sv})").expect("a valid type string"),
                    entries,
                ),
            ])));
        }
        "GetProperty" => {
            let (id, name) = parameters.get::<(i32, String)>().unwrap_or_default();
            let value = lock(state)
                .item(id)
                .and_then(|item| item_properties(item).remove(&name));
            match value {
                Some(value) => {
                    invocation.return_value(Some(&(value,).to_variant()));
                }
                None => unknown(invocation, id),
            }
        }
        "Event" => {
            // gio checked the signature `(isvu)`.
            let event = (
                parameters.child_value(0).get::<i32>().unwrap_or(-1),
                parameters
                    .child_value(1)
                    .get::<String>()
                    .unwrap_or_default(),
            );
            let (actions, errors) = clicks(state, [event]);
            if let Some(&id) = errors.first() {
                return unknown(invocation, id);
            }
            invocation.return_value(None);
            for action in actions {
                daemon.act(connection, &action);
            }
        }
        "EventGroup" => {
            // gio checked the signature `(a(isvu))`.
            let events: Vec<(i32, String)> = parameters
                .child_value(0)
                .iter()
                .map(|event| {
                    (
                        event.child_value(0).get::<i32>().unwrap_or(-1),
                        event.child_value(1).get::<String>().unwrap_or_default(),
                    )
                })
                .collect();
            let (actions, errors) = clicks(state, events);
            invocation.return_value(Some(&(errors,).to_variant()));
            for action in actions {
                daemon.act(connection, &action);
            }
        }
        "AboutToShow" => invocation.return_value(Some(&(false,).to_variant())),
        "AboutToShowGroup" => {
            invocation.return_value(Some(&(Vec::<i32>::new(), Vec::<i32>::new()).to_variant()));
        }
        _ => invocation.return_dbus_error(
            "org.freedesktop.DBus.Error.UnknownMethod",
            &format!("no method {method}"),
        ),
    }
}

/// The actions of the `clicked` events among `events` (`(id, eventId)`), and the ids no item has.
fn clicks(
    state: &Mutex<State>,
    events: impl IntoIterator<Item = (i32, String)>,
) -> (Vec<Action>, Vec<i32>) {
    let state = lock(state);
    let mut actions = Vec::new();
    let mut errors = Vec::new();
    for (id, event) in events {
        match state.item(id) {
            Some(item) => {
                if event == "clicked"
                    && let Some(action) = &item.action
                {
                    actions.push(action.clone());
                }
            }
            None => errors.push(id),
        }
    }
    (actions, errors)
}

#[cfg(test)]
mod tests {
    use omfp_protocol::{CreatedBy, Provenance, TitleSource};

    use super::*;

    fn thread(id: &str, title: &str, process: ProcessState) -> ThreadSummary {
        ThreadSummary {
            id: id.into(),
            title: title.into(),
            title_source: TitleSource::Set,
            workspace_id: "w".into(),
            process,
            settled: true,
            needs_attention: false,
            provenance: Provenance {
                created_by: CreatedBy::User,
                client: None,
            },
            created_at: 1,
            updated_at: 1,
            archived_at: None,
        }
    }

    fn working(id: &str) -> ThreadSummary {
        ThreadSummary {
            settled: false,
            ..thread(id, id, ProcessState::Running)
        }
    }

    fn waiting(id: &str, title: &str, updated_at: i64) -> ThreadSummary {
        ThreadSummary {
            needs_attention: true,
            settled: false,
            updated_at,
            ..thread(id, title, ProcessState::Running)
        }
    }

    // spec: UI-217, UI-219
    #[test]
    fn the_status_counts_threads_like_the_left_column() {
        assert_eq!(
            TrayStatus::of(&[]).summary(),
            "0 threads · 0 running · 0 need you"
        );
        assert_eq!(TrayStatus::of(&[]).icon(), TrayIcon::Idle);
        assert_eq!(TrayStatus::of(&[]).quit_label(), "Quit daemon");

        let idle = thread("idle", "idle", ProcessState::Running);
        let stopped = thread("stopped", "stopped", ProcessState::Stopped);
        let starting = thread("starting", "starting", ProcessState::Starting);
        let archived = ThreadSummary {
            archived_at: Some(5),
            ..working("archived")
        };
        let status = TrayStatus::of(&[idle.clone(), stopped.clone(), archived.clone()]);
        assert_eq!(status.summary(), "2 threads · 0 running · 0 need you");
        assert_eq!(status.icon(), TrayIcon::Idle);

        let status = TrayStatus::of(&[idle.clone(), working("a"), starting, stopped.clone()]);
        assert_eq!(status.summary(), "4 threads · 2 running · 0 need you");
        assert_eq!(status.icon(), TrayIcon::Running);
        assert_eq!(status.quit_label(), "Quit daemon (stops 2 running threads)");

        // Waiting wins over working, the newest first; a waiting thread is not counted as running.
        let status = TrayStatus::of(&[
            idle,
            working("a"),
            waiting("old", "old one", 10),
            waiting("new", "new one", 20),
            ThreadSummary {
                archived_at: Some(1),
                ..waiting("gone", "archived", 30)
            },
        ]);
        assert_eq!(status.summary(), "4 threads · 1 running · 2 need you");
        assert_eq!(status.icon(), TrayIcon::NeedsYou);
        assert_eq!(
            status.need_you,
            [
                NeedsYou {
                    id: "new".into(),
                    title: "new one".into()
                },
                NeedsYou {
                    id: "old".into(),
                    title: "old one".into()
                }
            ]
        );
        assert_eq!(status.quit_label(), "Quit daemon (stops 3 running threads)");

        let one = TrayStatus::of(&[waiting("w", "w", 1)]);
        assert_eq!(one.summary(), "1 thread · 0 running · 1 needs you");
        assert_eq!(one.quit_label(), "Quit daemon (stops 1 running thread)");
    }

    fn labels(items: &[MenuItem]) -> Vec<String> {
        items
            .iter()
            .map(|item| {
                if item.separator {
                    "---".to_owned()
                } else if item.enabled {
                    item.label.clone()
                } else {
                    format!("({})", item.label)
                }
            })
            .collect()
    }

    // spec: UI-218, DMN-130
    #[test]
    fn the_menu_lists_the_actions_the_status_and_the_waiting_threads() {
        let mut ids = ThreadIds::default();
        let status = TrayStatus::of(&[working("a")]);
        let items = menu(&status, &mut ids);
        assert_eq!(
            labels(&items),
            [
                "New window",
                "Open in browser",
                "---",
                "(1 thread · 1 running · 0 need you)",
                "---",
                "Quit daemon (stops 1 running thread)"
            ]
        );
        let actions: Vec<_> = items
            .iter()
            .filter_map(|item| item.action.clone())
            .collect();
        assert_eq!(
            actions,
            [Action::NewWindow, Action::OpenInBrowser, Action::Quit]
        );

        let long = "x".repeat(70);
        let status = TrayStatus::of(&[
            waiting("t1", "fix_the_login", 2),
            waiting("t2", "", 1),
            waiting("t3", &long, 0),
        ]);
        let items = menu(&status, &mut ids);
        assert_eq!(
            labels(&items)[4..7],
            [
                "◆ fix__the__login".to_owned(),
                "◆ New thread".to_owned(),
                format!("◆ {}…", "x".repeat(60)),
            ]
        );
        let t2 = items
            .iter()
            .find(|item| item.action == Some(Action::OpenThread("t2".into())))
            .unwrap()
            .id;

        // A thread keeps its id while it waits; a new one gets a fresh id, never a former one's.
        let items = menu(
            &TrayStatus::of(&[waiting("t2", "", 1), waiting("t4", "four", 0)]),
            &mut ids,
        );
        let id_of = |thread: &str| {
            items
                .iter()
                .find(|item| item.action == Some(Action::OpenThread(thread.into())))
                .map(|item| item.id)
        };
        assert_eq!(id_of("t2"), Some(t2));
        assert!(id_of("t4").unwrap() > t2 + 1);
        let all: Vec<i32> = items.iter().map(|item| item.id).collect();
        let mut unique = all.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), all.len());
    }

    // spec: UI-218
    #[test]
    fn the_layout_is_a_dbusmenu_tree_under_the_root() {
        let items = menu(&TrayStatus::default(), &mut ThreadIds::default());
        let tree = layout(&items);
        assert_eq!(tree.type_().as_str(), "(ia{sv}av)");
        assert_eq!(tree.child_value(0).get::<i32>(), Some(0));
        let children = tree.child_value(2);
        assert_eq!(children.n_children(), items.len());
        let status = children.child_value(3).as_variant().unwrap();
        let properties = status
            .child_value(1)
            .get::<HashMap<String, Variant>>()
            .unwrap();
        assert_eq!(
            properties["label"].get::<String>(),
            Some("0 threads · 0 running · 0 need you".into())
        );
        assert_eq!(properties["enabled"].get::<bool>(), Some(false));
        let separator = children
            .child_value(2)
            .as_variant()
            .unwrap()
            .child_value(1)
            .get::<HashMap<String, Variant>>()
            .unwrap();
        assert_eq!(separator["type"].get::<String>(), Some("separator".into()));
    }

    // spec: DMN-129, UI-219
    #[test]
    fn an_index_change_bumps_the_menu_revision_and_a_new_icon_is_announced() {
        let mut state = State::new(&[]);
        assert_eq!((state.revision, state.sni_status()), (1, "Active"));
        // One more thread: a new layout, the same icon.
        assert_eq!(
            state.update(&[thread("s", "s", ProcessState::Stopped)]),
            (true, None)
        );
        assert_eq!(state.revision, 2);
        // Nothing the tray shows changed (a stopped thread's title): no signal.
        assert_eq!(
            state.update(&[thread("s", "renamed", ProcessState::Stopped)]),
            (false, None)
        );
        assert_eq!(state.revision, 2);
        assert_eq!(state.update(&[working("a")]), (true, Some(TrayIcon::Idle)));
        assert_eq!(state.status.icon(), TrayIcon::Running);
        assert_eq!(
            state.update(&[waiting("a", "a", 1)]),
            (true, Some(TrayIcon::Running))
        );
        assert_eq!((state.revision, state.sni_status()), (4, "NeedsAttention"));
        assert_eq!(
            state.property("IconPixmap", &true.to_variant()),
            pixmaps(TrayIcon::NeedsYou)
        );
        assert_eq!(
            state
                .property("ToolTip", &true.to_variant())
                .child_value(3)
                .get::<String>(),
            Some("1 thread · 0 running · 1 needs you".into())
        );
        assert!(state.items.iter().any(|item| item.label == "◆ a"));
    }

    // spec: UI-219
    #[test]
    fn each_state_has_its_own_argb_pixmaps() {
        // The ARGB of the pixel at (x, y), fractions of the side.
        let pixel = |argb: &[u8], size: i32, x: f64, y: f64| {
            let side = f64::from(size);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let (column, row) = ((x * side) as usize, (y * side) as usize);
            let start = (row * usize::try_from(size).unwrap() + column) * 4;
            argb[start..start + 4].to_vec()
        };
        let mut dots = HashMap::new();
        for icon in [TrayIcon::Idle, TrayIcon::Running, TrayIcon::NeedsYou] {
            let pixmaps = pixmaps(icon);
            assert_eq!(pixmaps.type_().as_str(), "a(iiay)");
            assert_eq!(pixmaps.n_children(), 2);
            for (index, size) in [32, 64].into_iter().enumerate() {
                let pixmap = pixmaps.child_value(index);
                assert_eq!(pixmap.child_value(0).get::<i32>(), Some(size));
                assert_eq!(pixmap.child_value(1).get::<i32>(), Some(size));
                let argb = pixmap.child_value(2).fixed_array::<u8>().unwrap().to_vec();
                assert_eq!(argb.len(), usize::try_from(size * size * 4).unwrap());
                // Transparent around the π and between its legs; its left leg is opaque.
                assert_eq!(argb[0], 0, "{icon:?} {size}");
                assert_eq!(pixel(&argb, size, 0.5, 0.5)[0], 0, "{icon:?} {size}");
                assert_eq!(pixel(&argb, size, 0.275, 0.5)[0], 255, "{icon:?} {size}");
                if size == 64 {
                    dots.insert(
                        icon,
                        (
                            pixel(&argb, size, 0.853, 0.147),
                            pixel(&argb, size, 0.853, 0.84),
                        ),
                    );
                }
            }
        }
        // Running: a cyan dot at the bottom right; needs you: an amber one at the top right (ARGB).
        let (idle_top, idle_bottom) = &dots[&TrayIcon::Idle];
        let (running_top, running_bottom) = &dots[&TrayIcon::Running];
        let (waiting_top, waiting_bottom) = &dots[&TrayIcon::NeedsYou];
        assert_eq!(running_bottom, &[255, 0x16, 0xb3, 0xcc]);
        assert_eq!(waiting_top, &[255, 0xe0, 0x8a, 0x00]);
        assert_eq!(running_top, idle_top);
        assert_eq!(waiting_bottom, idle_bottom);
        assert_ne!(idle_bottom, running_bottom);
        assert_ne!(idle_top, waiting_top);
    }

    // spec: UI-218
    #[test]
    fn a_window_command_names_the_data_directory_and_the_thread() {
        let strings = |line: Vec<OsString>| {
            line.into_iter()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            strings(window_command(Path::new("/d"), None)),
            ["desktop", "--data-dir", "/d"]
        );
        assert_eq!(
            strings(window_command(Path::new("/d"), Some("t1"))),
            ["desktop", "--data-dir", "/d", "--thread", "t1"]
        );
    }
}
