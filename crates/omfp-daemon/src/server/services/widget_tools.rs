//! The windows' widget reports (`widgets.report`, PRT-043, AGT-026) and what the widget host tools do with them and
//! with the views manifest: `omfp_widget_check` (DMN-063) and `omfp_view_add` (DMN-064).

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use omfp_protocol::{Position, ReportWidgetsParams, WidgetReport, WidgetReportState, WidgetSource};
use tokio::sync::watch;

use super::{Services, blocking, write_atomically};
use crate::host_tools::WidgetHost;
use crate::server::convert::internal;
use crate::widgets::compile::{CompileError, IMPORT_MAP};
use crate::widgets::layout::{self, Addition};
use crate::widgets::{Widgets, folders, manifest, manifest_path};

/// How long `omfp_widget_check` waits for the windows to report the current version (DMN-063).
pub(crate) const CHECK_WAIT: Duration = Duration::from_secs(5);
/// How often a waiting check looks at the cancellation flag.
const CANCEL_POLL: Duration = Duration::from_millis(100);

/// What the daemon keeps of a report (PRT-043).
const MAX_TEXT: usize = 4000;
const MAX_ENTRIES: usize = 20;
const MAX_TAIL: usize = 10;
const MAX_ENTRY: usize = 2000;

/// `text` cut to `max` characters, with `…` when cut.
fn clip(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_owned(),
    }
}

/// The last `max` entries of `entries`, each clipped.
fn last(entries: &[String], max: usize) -> Vec<String> {
    entries[entries.len().saturating_sub(max)..]
        .iter()
        .map(|entry| clip(entry, MAX_ENTRY))
        .collect()
}

/// A report as the daemon keeps it (PRT-043).
fn trimmed(mut report: ReportWidgetsParams) -> ReportWidgetsParams {
    for widget in &mut report.widgets {
        widget.text = clip(&widget.text, MAX_TEXT);
        widget.errors = last(&widget.errors, MAX_ENTRIES);
        widget.console = last(&widget.console, MAX_ENTRIES);
        for job in &mut widget.jobs {
            job.tail = last(&job.tail, MAX_TAIL);
        }
    }
    report
}

/// The latest report of one window (connection).
#[derive(Debug, Clone)]
struct Window {
    /// The `client` of its `system.hello`.
    client: String,
    report: ReportWidgetsParams,
}

/// Every connected window's latest widget report (AGT-026), by connection.
#[derive(Debug)]
pub(crate) struct WidgetReports {
    windows: Mutex<BTreeMap<u64, Window>>,
    next: AtomicU64,
    /// Bumped on every change, for checks waiting for reports.
    changed: watch::Sender<u64>,
}

impl WidgetReports {
    pub(crate) fn new() -> Self {
        Self {
            windows: Mutex::new(BTreeMap::new()),
            next: AtomicU64::new(1),
            changed: watch::channel(0).0,
        }
    }

    /// A new connection's number.
    pub(crate) fn connection(&self) -> u64 {
        self.next.fetch_add(1, Ordering::Relaxed)
    }

    fn windows(&self) -> std::sync::MutexGuard<'_, BTreeMap<u64, Window>> {
        self.windows.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Replaces the report of `connection`.
    pub(crate) fn set(&self, connection: u64, client: &str, report: ReportWidgetsParams) {
        self.windows().insert(
            connection,
            Window {
                client: client.to_owned(),
                report: trimmed(report),
            },
        );
        self.changed.send_modify(|count| *count += 1);
    }

    /// Forgets the report of a closed connection.
    pub(crate) fn forget(&self, connection: u64) {
        if self.windows().remove(&connection).is_some() {
            self.changed.send_modify(|count| *count += 1);
        }
    }

    /// The windows whose latest report is about `workspace`, by connection.
    fn showing(&self, workspace: &str) -> Vec<Window> {
        self.windows()
            .values()
            .filter(|window| window.report.workspace_id.as_deref() == Some(workspace))
            .cloned()
            .collect()
    }
}

/// The widget being checked.
#[derive(Debug)]
struct Target {
    source: WidgetSource,
    name: String,
    folder: PathBuf,
    version: String,
    entry: Option<&'static str>,
    compile: Vec<CompileError>,
    manifest_error: Option<String>,
    /// Why `.omfp/keymap.json` is refused (DMN-122); `None` when valid or absent.
    keymap_error: Option<String>,
    /// A valid `views.json` exists and none of its views has the `main` slot (AGT-028).
    main_missing: bool,
}

impl Target {
    fn reference(&self) -> String {
        match self.source {
            WidgetSource::Workspace => format!("./widgets/{}", self.name),
            WidgetSource::User => format!("user:{}", self.name),
        }
    }

    fn instances<'a>(&self, window: &'a Window) -> Vec<&'a WidgetReport> {
        window
            .report
            .widgets
            .iter()
            .filter(|widget| widget.source == self.source && widget.name == self.name)
            .collect()
    }

    /// Why `window` has not settled on the current version yet; `None` once it has (DMN-063).
    fn unsettled(&self, window: &Window, views: Option<&Position>) -> Option<String> {
        let caught_up = match (views, &window.report.views) {
            (None, _) => true,
            (Some(current), Some(seen)) => seen.epoch == current.epoch && seen.seq >= current.seq,
            (Some(_), None) => false,
        };
        if !caught_up {
            return Some(
                "has not caught up with the latest change of the workspace's views".to_owned(),
            );
        }
        for instance in self.instances(window) {
            if instance.state == WidgetReportState::Loading {
                return Some(format!("is still loading {}", instance.instance));
            }
            if let Some(job) = instance.jobs.iter().find(|job| job.status == "pending") {
                return Some(format!(
                    "is still starting the job `{}` of {}",
                    job.command, instance.instance
                ));
            }
            if instance.version.as_deref() != Some(self.version.as_str()) {
                return Some(format!(
                    "shows version {} of {}, not the current one",
                    instance.version.as_deref().unwrap_or("none"),
                    instance.instance
                ));
            }
        }
        None
    }
}

fn state_name(state: WidgetReportState) -> &'static str {
    match state {
        WidgetReportState::Loading => "loading",
        WidgetReportState::Rendered => "rendered",
        WidgetReportState::CompileError => "compile error",
        WidgetReportState::LoadError => "load error",
        WidgetReportState::RenderError => "render error",
    }
}

fn list(text: &mut String, title: &str, entries: &[String]) {
    if entries.is_empty() {
        let _ = writeln!(text, "  {title}: none");
        return;
    }
    let _ = writeln!(text, "  {title}:");
    for entry in entries {
        let mut lines = entry.lines();
        let _ = writeln!(text, "    - {}", lines.next().unwrap_or_default());
        for line in lines {
            let _ = writeln!(text, "      {line}");
        }
    }
}

/// The answer of `omfp_widget_check` (DMN-063); `windows` are those showing the workspace, with why each one has not
/// settled (`None` when it has).
fn describe(target: &Target, windows: &[(Window, Option<String>)], waited: Duration) -> String {
    let reference = target.reference();
    let mut text = format!(
        "Widget {reference} (folder {}, version {}, entry {})\n\n",
        target.folder.display(),
        target.version,
        target.entry.unwrap_or("missing")
    );
    if target.entry.is_none() {
        text.push_str(
            "No entry file (index.tsx, index.ts, index.jsx or index.js): windows cannot load the widget.\n",
        );
    }
    if target.compile.is_empty() {
        let _ = writeln!(
            text,
            "Daemon compile: every file compiles and imports only its own files and {}.",
            IMPORT_MAP.join(", ")
        );
    } else {
        let _ = writeln!(
            text,
            "Daemon compile: {} file(s) fail; windows show the first error in place of the widget:",
            target.compile.len()
        );
        for error in &target.compile {
            let at = match (error.line, error.column) {
                (Some(line), Some(column)) => format!("{}:{line}:{column}", error.file),
                (Some(line), None) => format!("{}:{line}", error.file),
                _ => error.file.clone(),
            };
            let _ = writeln!(text, "- {at}: {}", error.message);
        }
    }
    if let Some(error) = &target.manifest_error {
        let _ = writeln!(
            text,
            "views.json is invalid ({error}): windows show the default view until it is fixed."
        );
    }
    if let Some(error) = &target.keymap_error {
        let _ = writeln!(
            text,
            "keymap.json is invalid ({error}): windows keep its last valid bindings until it is fixed."
        );
    }
    if target.main_missing {
        text.push_str(
            "Warning: no view of views.json has the main slot [\"tabs\", {\"id\": \"main\"}, …\"omfp:chat\"], so files \
             the user opens float over the view instead of opening as tabs next to the conversation. Keep that slot: \
             put it back around omfp:chat, and place widgets with omfp_view_add.\n",
        );
    }
    text.push('\n');
    if windows.is_empty() {
        text.push_str(
            "No omfp window shows this workspace now, so nothing rendered the widget: only the daemon's compile check \
             above applies.\n",
        );
        return text;
    }
    let showing = windows
        .iter()
        .filter(|(window, _)| !target.instances(window).is_empty())
        .count();
    if showing == 0 {
        let _ = writeln!(
            text,
            "{} window(s) show this workspace, but none shows {reference} in its current view. Add it with \
             omfp_view_add {{ \"widget\": \"{reference}\" }}.",
            windows.len()
        );
        return text;
    }
    for (number, (window, unsettled)) in windows.iter().enumerate() {
        let _ = write!(text, "Window {} ({})", number + 1, window.client);
        if let Some(why) = unsettled {
            let _ = write!(text, ", after waiting {} s, {why}", waited.as_secs());
        }
        text.push_str(":\n");
        let instances = target.instances(window);
        if instances.is_empty() {
            let _ = writeln!(text, "- does not show {reference} in its current view");
        }
        for instance in instances {
            describe_instance(&mut text, instance);
        }
    }
    text
}

/// One widget instance of a window's report, for `omfp_widget_check` (DMN-063).
fn describe_instance(text: &mut String, instance: &WidgetReport) {
    let _ = writeln!(
        text,
        "- {}: {} (version {})",
        instance.instance,
        state_name(instance.state),
        instance.version.as_deref().unwrap_or("none")
    );
    list(text, "errors", &instance.errors);
    list(text, "console errors", &instance.console);
    let jobs: Vec<String> = instance
        .jobs
        .iter()
        .map(|job| {
            let mut line = format!("`{}`: {}, run {}", job.command, job.status, job.runs);
            if let Some(code) = job.exit_code {
                let _ = write!(line, ", exit code {code}");
            }
            if let Some(signal) = job.signal {
                let _ = write!(line, ", signal {signal}");
            }
            if !job.tail.is_empty() {
                let _ = write!(line, "\nlast lines:\n{}", job.tail.join("\n"));
            }
            line
        })
        .collect();
    list(text, "jobs", &jobs);
    if instance.text.trim().is_empty() {
        text.push_str("  text: (empty)\n");
    } else {
        text.push_str("  text:\n");
        for line in instance.text.lines().filter(|line| !line.trim().is_empty()) {
            let _ = writeln!(text, "    {}", line.trim_end());
        }
    }
}

impl Services {
    /// A new connection's number, naming its widget report (PRT-043).
    pub fn reports_connection(&self) -> u64 {
        self.reports.connection()
    }

    /// `widgets.report` from `connection` (PRT-043): replaces its report.
    pub fn report_widgets(&self, connection: u64, client: &str, report: ReportWidgetsParams) {
        self.reports.set(connection, client, report);
    }

    /// Forgets the report of a closed connection (PRT-043).
    pub fn forget_widget_reports(&self, connection: u64) {
        self.reports.forget(connection);
    }

    /// `omfp_widget_check` (DMN-063): the daemon's compile of the widget `name` of workspace `workspace_id`, then
    /// what the windows showing the workspace report, waiting at most `wait` for them to settle on the current version.
    pub async fn check_widget(
        &self,
        workspace_id: &str,
        name: &str,
        is_cancelled: &dyn Fn() -> bool,
        wait: Duration,
    ) -> Result<String, String> {
        let (store, widgets) = (Arc::clone(&self.store), Arc::clone(&self.widgets));
        let (id, name) = (workspace_id.to_owned(), name.to_owned());
        let (source, name, folder) = blocking(move || {
            let path = store.workspace(&id)?.path;
            widgets.locate(&path, &name).map_err(internal)
        })
        .await
        .map_err(|error| error.message)?;
        // Windows get the folder's current version (and the manifest) before they are asked about it.
        self.refresh_views(workspace_id).await;
        let (store, widgets) = (Arc::clone(&self.store), Arc::clone(&self.widgets));
        let id = workspace_id.to_owned();
        let target = blocking(move || {
            let workspace = store.workspace(&id)?.path;
            let (manifest, manifest_error) = Widgets::manifest(&workspace);
            Ok(Target {
                source,
                name,
                version: folders::version(&folder),
                entry: folders::entry(&folder),
                compile: widgets.check(&folder),
                manifest_error,
                keymap_error: crate::widgets::keymap::read(&crate::widgets::keymap::path(
                    &workspace,
                ))
                .err(),
                main_missing: manifest
                    .as_ref()
                    .is_some_and(|manifest| !layout::has_main_slot(manifest)),
                folder,
            })
        })
        .await
        .map_err(|error| error.message)?;

        let views = self
            .views_log_position(workspace_id)
            .map(|position| Position {
                epoch: position.epoch,
                seq: position.seq,
            });
        let mut changes = self.reports.changed.subscribe();
        let deadline = tokio::time::Instant::now() + wait;
        loop {
            changes.borrow_and_update();
            let windows: Vec<(Window, Option<String>)> = self
                .reports
                .showing(workspace_id)
                .into_iter()
                .map(|window| {
                    let unsettled = target.unsettled(&window, views.as_ref());
                    (window, unsettled)
                })
                .collect();
            let settled = windows.iter().all(|(_, unsettled)| unsettled.is_none());
            if settled || tokio::time::Instant::now() >= deadline {
                return Ok(describe(&target, &windows, wait));
            }
            if is_cancelled() {
                return Err("cancelled".to_owned());
            }
            let tick = deadline.min(tokio::time::Instant::now() + CANCEL_POLL);
            let _ = tokio::time::timeout_at(tick, changes.changed()).await;
        }
    }

    /// `omfp_view_add` (DMN-064): adds the widget to a view of the workspace's manifest (the default manifest without
    /// a file), writes `views.json` as `views.set` does and publishes it.
    pub async fn add_to_view(
        &self,
        workspace_id: &str,
        addition: &Addition<'_>,
    ) -> Result<String, String> {
        // A read, a change and a write, like the mounts (PRT-080..082).
        let _edit = self.manifest_edits.lock().await;
        let store = Arc::clone(&self.store);
        let id = workspace_id.to_owned();
        let path = blocking(move || Ok(store.workspace(&id)?.path))
            .await
            .map_err(|error| error.message)?;
        let (current, error) = Widgets::manifest(&path);
        if let Some(error) = error {
            return Err(format!(
                ".omfp/views.json is invalid ({error}); fix it first (or ask the user to run `:view reset`), then \
                 add the widget"
            ));
        }
        let (manifest, said) = layout::add(current.as_ref(), addition)?;
        let file = manifest_path(&path);
        write_atomically(&file, &manifest::to_text(&manifest))
            .map_err(|error| format!("cannot write {}: {error}", file.display()))?;
        let missing = addition
            .reference
            .strip_prefix("./widgets/")
            .filter(|name| !crate::widgets::workspace_widgets(&path).join(name).is_dir());
        self.refresh_views(workspace_id).await;
        let mut text = format!(
            "Added {} {said} in .omfp/views.json; open windows show it now.",
            addition.reference
        );
        if let Some(name) = missing {
            let _ = write!(
                text,
                " There is no folder .omfp/widgets/{name}/ yet: windows show an error in its place until there is."
            );
        }
        Ok(text)
    }
}

/// The server's [`WidgetHost`]: runs the services' futures on the daemon's runtime from the `omp-rpc` handler
/// thread a host tool runs on.
pub(crate) struct ServicesWidgetHost {
    services: Services,
    runtime: tokio::runtime::Handle,
}

impl ServicesWidgetHost {
    pub(crate) fn new(services: Services) -> Self {
        Self {
            services,
            runtime: tokio::runtime::Handle::current(),
        }
    }
}

impl WidgetHost for ServicesWidgetHost {
    fn check_widget(
        &self,
        workspace_id: &str,
        name: &str,
        is_cancelled: &dyn Fn() -> bool,
    ) -> Result<String, String> {
        self.runtime.block_on(self.services.check_widget(
            workspace_id,
            name,
            is_cancelled,
            CHECK_WAIT,
        ))
    }

    fn add_to_view(&self, workspace_id: &str, addition: &Addition<'_>) -> Result<String, String> {
        self.runtime
            .block_on(self.services.add_to_view(workspace_id, addition))
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use omfp_protocol::WidgetJobReport;
    use serde_json::{Map, json};
    use tempfile::TempDir;

    use super::*;
    use crate::agent::FakeLauncher;
    use crate::git::{FakeGit, Git};
    use crate::paths::DataDir;
    use crate::store::{Store, Workspace};
    use crate::supervisor::{Supervisor, SupervisorConfig};
    use crate::threads::Threads;
    use crate::widgets::layout::Place;
    use crate::workspaces::{NewWorkspace, Workspaces};

    struct Harness {
        _dir: TempDir,
        services: Services,
        workspace: String,
        folder: PathBuf,
    }

    async fn services(dir: &TempDir, git: Arc<FakeGit>) -> Services {
        let data = DataDir::new(dir.path().join("data"));
        std::fs::create_dir_all(data.root()).unwrap();
        let root = dir.path().join("workspaces");
        let store = Arc::new(Store::open(&data.db(), &root).unwrap());
        let threads = Arc::new(Threads::new(Arc::clone(&store), data.clone()));
        let workspaces = Arc::new(Workspaces::new(
            Arc::clone(&store),
            git as Arc<dyn Git>,
            root,
        ));
        let widgets = Arc::new(Widgets::new(data.widgets(), None));
        let supervisor = Supervisor::new(
            Arc::new(FakeLauncher::new()),
            Arc::clone(&threads),
            Arc::clone(&workspaces),
            data,
            SupervisorConfig::new("omp"),
        );
        Services::new(store, threads, workspaces, supervisor, widgets)
            .await
            .unwrap()
    }

    /// Services with one workspace holding the widget `tests`.
    async fn harness() -> Harness {
        let dir = tempfile::tempdir().unwrap();
        let services = services(&dir, Arc::new(FakeGit::new())).await;
        let view = services.workspaces.create(NewWorkspace::new()).unwrap();
        let folder = view.workspace.path.clone();
        write(
            &folder,
            ".omfp/widgets/tests/index.tsx",
            "export default function T() { return <p>ok</p>; }\n",
        );
        Harness {
            _dir: dir,
            services,
            workspace: view.workspace.id,
            folder,
        }
    }

    fn write(folder: &Path, relative: &str, text: &str) {
        let path = folder.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    impl Harness {
        fn version(&self) -> String {
            folders::version(&self.folder.join(".omfp/widgets/tests"))
        }

        fn report(&self, connection: u64, views: Option<Position>, widgets: Vec<WidgetReport>) {
            self.services.report_widgets(
                connection,
                "web",
                ReportWidgetsParams {
                    workspace_id: Some(self.workspace.clone()),
                    views,
                    widgets,
                },
            );
        }

        async fn check(&self, wait: Duration) -> String {
            self.services
                .check_widget(&self.workspace, "tests", &|| false, wait)
                .await
                .unwrap()
        }
    }

    fn instance(version: Option<String>, state: WidgetReportState) -> WidgetReport {
        WidgetReport {
            reference: "./widgets/tests".to_owned(),
            instance: "./widgets/tests".to_owned(),
            source: WidgetSource::Workspace,
            name: "tests".to_owned(),
            version,
            state,
            errors: vec!["index.tsx:3:7: boom".to_owned()],
            console: vec!["TypeError: x is undefined\n    at Tests (index.tsx:4:2)".to_owned()],
            text: "Tests 1/3\n\nPASS 1".to_owned(),
            jobs: vec![WidgetJobReport {
                command: "make test".to_owned(),
                status: "running".to_owned(),
                exit_code: None,
                signal: None,
                runs: 2,
                started_at: Some(1),
                tail: vec!["PASS 1".to_owned()],
            }],
        }
    }

    // spec: DMN-063, DMN-122
    #[tokio::test]
    async fn without_windows_the_check_gives_the_compile_result_and_says_nothing_rendered() {
        let h = harness().await;
        let text = h.check(Duration::ZERO).await;
        assert!(
            text.starts_with("Widget ./widgets/tests (folder "),
            "{text}"
        );
        assert!(
            text.contains(&format!("version {}, entry index.tsx", h.version())),
            "{text}"
        );
        assert!(
            text.contains("Daemon compile: every file compiles"),
            "{text}"
        );
        assert!(
            text.contains("No omfp window shows this workspace now"),
            "{text}"
        );

        write(
            &h.folder,
            ".omfp/widgets/tests/lib.ts",
            "export const = 1;\n",
        );
        write(&h.folder, ".omfp/views.json", "{ \"views\": {} }");
        let text = h.check(Duration::ZERO).await;
        assert!(text.contains("Daemon compile: 1 file(s) fail"), "{text}");
        assert!(text.contains("- lib.ts:1:"), "{text}");
        assert!(text.contains("views.json is invalid ("), "{text}");
        assert!(!text.contains("keymap.json"), "no keymap: {text}");
        write(&h.folder, ".omfp/keymap.json", "{ \"bindings\": [1] }");
        let text = h.check(Duration::ZERO).await;
        assert!(
            text.contains(
                "keymap.json is invalid (bindings[0]: expected an object with keys and command): windows keep \
                 its last valid bindings until it is fixed."
            ),
            "{text}"
        );

        let error = h
            .services
            .check_widget(&h.workspace, "nothing", &|| false, Duration::ZERO)
            .await
            .unwrap_err();
        assert!(error.contains("no widget nothing"), "{error}");
    }

    // spec: AGT-028
    #[tokio::test]
    async fn the_check_warns_when_views_json_lost_the_main_slot() {
        let h = harness().await;
        let warning = "Warning: no view of views.json has the main slot";
        let text = h.check(Duration::ZERO).await;
        assert!(!text.contains(warning), "the default views keep it: {text}");

        write(
            &h.folder,
            ".omfp/views.json",
            r#"{ "views": { "default": { "layout": ["row", {}, "omfp:chat", "./widgets/tests"] } } }"#,
        );
        let text = h.check(Duration::ZERO).await;
        assert!(text.contains(warning), "{text}");
        assert!(text.contains("Keep that slot"), "{text}");
    }

    // spec: DMN-063, PRT-043, AGT-026
    #[tokio::test]
    async fn the_check_reports_what_each_window_shows_and_names_omfp_view_add_when_none_shows_it() {
        let h = harness().await;
        h.report(7, None, Vec::new());
        let text = h.check(Duration::ZERO).await;
        assert!(
            text.contains("1 window(s) show this workspace, but none shows ./widgets/tests"),
            "{text}"
        );
        assert!(
            text.contains("omfp_view_add { \"widget\": \"./widgets/tests\" }"),
            "{text}"
        );

        h.report(
            7,
            None,
            vec![instance(Some(h.version()), WidgetReportState::RenderError)],
        );
        let text = h.check(Duration::ZERO).await;
        for needle in [
            "Window 1 (web):\n- ./widgets/tests: render error (version ",
            "  errors:\n    - index.tsx:3:7: boom\n",
            "  console errors:\n    - TypeError: x is undefined\n          at Tests (index.tsx:4:2)\n",
            "  jobs:\n    - `make test`: running, run 2\n      last lines:\n      PASS 1\n",
            "  text:\n    Tests 1/3\n    PASS 1\n",
        ] {
            assert!(text.contains(needle), "{needle:?} missing: {text}");
        }

        // A closed window's report goes with it.
        h.services.forget_widget_reports(7);
        assert!(
            h.check(Duration::ZERO)
                .await
                .contains("No omfp window shows this workspace now")
        );
    }

    // spec: DMN-063, AGT-026
    #[tokio::test]
    async fn the_check_waits_for_windows_to_render_the_current_version_and_names_a_late_one() {
        let h = harness().await;
        // A window follows the views topic: the check waits until it reported that topic's latest position.
        h.services.views_log(&h.workspace).await.unwrap();
        let old = Position {
            epoch: "old".to_owned(),
            seq: 1,
        };
        h.report(
            1,
            Some(old),
            vec![instance(Some("0".to_owned()), WidgetReportState::Loading)],
        );
        let text = h.check(Duration::from_millis(50)).await;
        assert!(
            text.contains(
                "Window 1 (web), after waiting 0 s, has not caught up with the latest change"
            ),
            "{text}"
        );

        let current = h.services.views_log_position(&h.workspace).unwrap();
        let current = Position {
            epoch: current.epoch,
            seq: current.seq,
        };
        let (services, workspace, version) = (h.services.clone(), h.workspace.clone(), h.version());
        let seen = current.clone();
        let late = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            services.report_widgets(
                1,
                "web",
                ReportWidgetsParams {
                    workspace_id: Some(workspace),
                    views: Some(seen),
                    widgets: vec![instance(Some(version), WidgetReportState::Rendered)],
                },
            );
        });
        let text = h.check(Duration::from_secs(30)).await;
        late.await.unwrap();
        assert!(
            text.contains("Window 1 (web):\n- ./widgets/tests: rendered"),
            "{text}"
        );

        h.report(
            1,
            Some(current.clone()),
            vec![instance(Some(h.version()), WidgetReportState::Loading)],
        );
        let text = h.check(Duration::from_millis(50)).await;
        assert!(text.contains("is still loading ./widgets/tests"), "{text}");

        // A job the widget is still starting is waited for too.
        let mut starting = instance(Some(h.version()), WidgetReportState::Rendered);
        starting.jobs[0].status = "pending".to_owned();
        h.report(1, Some(current), vec![starting]);
        let text = h.check(Duration::from_millis(50)).await;
        assert!(
            text.contains("is still starting the job `make test` of ./widgets/tests"),
            "{text}"
        );

        let cancelled = h
            .services
            .check_widget(&h.workspace, "tests", &|| true, Duration::from_secs(30))
            .await;
        assert_eq!(cancelled, Err("cancelled".to_owned()));
    }

    // spec: PRT-043
    #[tokio::test]
    async fn reports_are_trimmed_to_what_the_daemon_keeps() {
        let mut widget = instance(None, WidgetReportState::Rendered);
        widget.text = "x".repeat(MAX_TEXT + 10);
        widget.console = (0..30).map(|n| format!("error {n}")).collect();
        widget.errors = vec!["e".repeat(MAX_ENTRY + 5)];
        widget.jobs[0].tail = (0..15).map(|n| n.to_string()).collect();
        let kept = trimmed(ReportWidgetsParams {
            workspace_id: None,
            views: None,
            widgets: vec![widget],
        });
        let widget = &kept.widgets[0];
        assert_eq!(widget.text.chars().count(), MAX_TEXT + 1, "cut, with …");
        assert_eq!(widget.console.len(), MAX_ENTRIES);
        assert_eq!(widget.console[0], "error 10", "the newest are kept");
        assert_eq!(widget.errors[0].chars().count(), MAX_ENTRY + 1);
        assert_eq!(
            widget.jobs[0].tail,
            (5..15).map(|n| n.to_string()).collect::<Vec<_>>()
        );
    }

    fn addition(reference: &str) -> Addition<'_> {
        Addition {
            reference,
            props: Map::new(),
            view: None,
            place: Place::SideTab,
        }
    }

    // spec: DMN-120, DMN-121, PRT-060
    #[tokio::test]
    async fn the_views_topic_carries_the_workspace_keymap_and_an_invalid_file_keeps_the_last_valid_one()
     {
        use omfp_protocol::{KeymapMode, ViewsEvent, WorkspaceBinding};

        let h = harness().await;
        let file = h.folder.join(".omfp/keymap.json");
        let log = h.services.views_log(&h.workspace).await.unwrap();
        let keymap = || log.with_state(|state| (state.keymap.clone(), state.keymap_error.clone()));
        assert_eq!(keymap(), (Vec::new(), None), "no file: no bindings");
        let mut live = log.subscribe(Some(&log.position())).live;
        let mut next = move || match live.try_recv().ok().map(|event| event.event.clone()) {
            Some(ViewsEvent::ViewsKeymap { keymap, error }) => Some((keymap, error)),
            _ => None,
        };

        let valid =
            r#"{ "bindings": [{ "keys": "g t", "command": "tests.run", "args": ["unit"] }] }"#;
        write(&h.folder, ".omfp/keymap.json", valid);
        h.services.refresh_views(&h.workspace).await;
        let bound = vec![WorkspaceBinding {
            mode: KeymapMode::Normal,
            keys: "g t".to_owned(),
            command: "tests.run".to_owned(),
            args: vec!["unit".to_owned()],
        }];
        assert_eq!(keymap(), (bound.clone(), None));
        assert_eq!(next(), Some((bound.clone(), None)), "pushed on change");
        h.services.refresh_views(&h.workspace).await;
        assert_eq!(next(), None, "nothing changed, nothing pushed");

        let invalid = r#"{ "bindings": [{ "keys": "", "command": "tests.run" }] }"#;
        write(&h.folder, ".omfp/keymap.json", invalid);
        h.services.refresh_views(&h.workspace).await;
        let error = "bindings[0].keys: expected a key sequence such as \"Space t\"".to_owned();
        assert_eq!(
            next(),
            Some((bound.clone(), Some(error.clone()))),
            "the last valid bindings stay, with the error"
        );
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            invalid,
            "never rewritten"
        );

        std::fs::remove_file(&file).unwrap();
        h.services.refresh_views(&h.workspace).await;
        assert_eq!(
            next(),
            Some((Vec::new(), None)),
            "a removed file drops them"
        );
    }

    // spec: DMN-064, DMN-088
    #[tokio::test]
    async fn view_add_writes_views_json_and_refuses_an_invalid_one() {
        let h = harness().await;
        let file = h.folder.join(".omfp/views.json");
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            crate::widgets::DEFAULT_MANIFEST,
            "a new workspace has the default views.json"
        );
        let log = h.services.views_log(&h.workspace).await.unwrap();

        let said = h
            .services
            .add_to_view(&h.workspace, &addition("./widgets/tests"))
            .await
            .unwrap();
        assert_eq!(
            said,
            "Added ./widgets/tests as a tab of the side tabs of view \"default\" in .omfp/views.json; open windows \
             show it now."
        );
        let written: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(
            written["views"]["default"]["layout"][3],
            json!(["tabs", { "id": "side" }, "omfp:workspace", "omfp:changes", "./widgets/tests"])
        );
        assert_eq!(
            log.with_state(|state| state.manifest.clone()),
            Some(written),
            "published before answering"
        );

        let said = h
            .services
            .add_to_view(&h.workspace, &addition("./widgets/later"))
            .await
            .unwrap();
        assert!(
            said.contains("There is no folder .omfp/widgets/later/ yet"),
            "{said}"
        );

        std::fs::write(&file, "not json").unwrap();
        let error = h
            .services
            .add_to_view(&h.workspace, &addition("user:clock"))
            .await
            .unwrap_err();
        assert!(error.contains(".omfp/views.json is invalid ("), "{error}");
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "not json",
            "never overwritten"
        );
    }

    // spec: DMN-088, DMN-030
    #[tokio::test]
    async fn startup_writes_the_default_views_json_where_it_is_missing_but_never_into_a_repository()
    {
        let dir = tempfile::tempdir().unwrap();
        let git = Arc::new(FakeGit::new());
        let root = dir.path().join("workspaces");
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        let store = Store::open(&dir.path().join("data/omfp.db"), &root).unwrap();
        let mut folders = Vec::new();
        for name in ["old", "kept", "repo"] {
            let path = root.join(name);
            std::fs::create_dir_all(&path).unwrap();
            store
                .insert_workspace(&Workspace {
                    id: format!("01{name}"),
                    name: name.to_owned(),
                    path: path.clone(),
                    created_by: crate::store::Provenance::User,
                    created_at: 0,
                })
                .unwrap();
            folders.push(path);
        }
        write(&folders[1], ".omfp/views.json", "{ \"mine\": true }");
        git.add_repo(&folders[2], "main");
        drop(store);

        let _services = services(&dir, git).await;

        let views = |folder: &Path| std::fs::read_to_string(folder.join(".omfp/views.json")).ok();
        assert_eq!(
            views(&folders[0]).as_deref(),
            Some(crate::widgets::DEFAULT_MANIFEST)
        );
        assert_eq!(views(&folders[1]).as_deref(), Some("{ \"mine\": true }"));
        assert_eq!(views(&folders[2]), None, "the folder is a repository");
    }
}
