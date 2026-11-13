//! The desktop layout at 1100, 1440 and 1920 px (E2E-38), headless Chromium (omp and git for real: the rows need git statuses): the workspace
//! widget's repository rows with a long name, a dirty status, an origin and an "also in" warning never draw one cell
//! over another nor past the row (UI-453); the side tab bar stays one row (UI-452). Screenshots land in `/tmp` as
//! `omfp-desktop-<width>.png` (or `OMFP_SCREENSHOT_DIR`); nothing compares pixels.

use e2e::{Journey, row};
use omfp_protocol::method::{ThreadsCreate, WorkspacesAddRepo};
use omfp_protocol::{
    AddRepoParams, CreateThreadParams, CreateWorkspaceParams, RepoSource, ThreadWorkspace,
};
use omfp_testkit::TempRepo;

/// The repository name of the QA report that broke the rows.
const LONG: &str = "infra-terraform-modules-shared-configuration";
/// The other workspace linking the same checkout: the row's "also in" warning names it.
const OTHER: &str = "web-client-redesign";

/// A thread in a new workspace `name`, linking `link` when given.
fn new_thread(name: &str, link: Option<String>) -> CreateThreadParams {
    CreateThreadParams {
        title: None,
        workspace: ThreadWorkspace::New(CreateWorkspaceParams {
            name: Some(name.to_owned()),
            link,
        }),
        model: None,
        thinking_level: None,
        approval_mode: None,
        prompt: None,
    }
}

/// Two workspaces sharing the dirty checkout `shared`: the selected thread's links it as [`LONG`] next to `other`;
/// returns the selected thread's workspace id.
async fn workspaces(
    journey: &Journey,
    shared: &TempRepo,
    other: &TempRepo,
) -> anyhow::Result<String> {
    let path = |repo: &TempRepo| repo.path().to_string_lossy().into_owned();
    let mut connection = journey.connect().await?;
    connection
        .call::<ThreadsCreate>(new_thread(OTHER, Some(path(shared))))
        .await?;
    let thread = connection
        .call::<ThreadsCreate>(new_thread("desktop-layout", None))
        .await?;
    for (name, repo) in [(LONG, shared), ("api-server", other)] {
        connection
            .call::<WorkspacesAddRepo>(AddRepoParams {
                workspace_id: thread.workspace_id.clone(),
                name: Some(name.to_owned()),
                source: RepoSource::Link { path: path(repo) },
            })
            .await?;
    }
    connection.close().await?;
    journey.click(&row(&thread.id)).await?;
    journey.opened(&thread.id).await?;
    let long = format!("[data-testid=workspace-repo][data-name=\"{LONG}\"]");
    journey
        .until("the long row with its status and warning", async |page| {
            let ready = page
                .count(&format!(
                    "{long} [data-testid=git-status], {long} [data-testid=shared-repo]"
                ))
                .await?
                == 2;
            Ok(ready.then_some(()))
        })
        .await?;
    Ok(thread.workspace_id)
}

/// The offenders of `script`'s returned list, failing with `what` unless it is empty.
async fn none(journey: &Journey, what: &str, script: &str) -> anyhow::Result<()> {
    let value = journey.eval(script, Vec::new()).await?;
    let offenders = value.as_array().cloned().unwrap_or_default();
    anyhow::ensure!(offenders.is_empty(), "{what}: {offenders:?}");
    Ok(())
}

/// Every repository row: no two of its parts overlap, none runs past the row, none is cut (UI-453).
const ROWS: &str = "
const parts = ['.omfp-workspace__repo-name', '[data-testid=repo-kind]', '[data-testid=git-status]',
  '[data-testid=remove-repo]', '[data-testid=repo-branch]', '[data-testid=repo-origin]', '[data-testid=shared-repo]'];
const out = [];
for (const row of document.querySelectorAll('[data-widget=\"omfp:workspace\"] [data-testid=workspace-repo]')) {
  const box = row.getBoundingClientRect();
  const shown = parts.map((css) => [css, row.querySelector(css)]).filter(([, e]) => e !== null)
    .map(([css, e]) => [css, e, e.getBoundingClientRect()]);
  for (const [css, e, r] of shown) {
    if (r.left < box.left - 0.5 || r.right > box.right + 0.5) out.push(`${row.dataset.name} ${css} out of the row`);
    if (e.scrollWidth > e.clientWidth + 1 && getComputedStyle(e).overflow !== 'visible') out.push(`${row.dataset.name} ${css} cut`);
  }
  for (let i = 0; i < shown.length; i++) for (let j = i + 1; j < shown.length; j++) {
    const [a, , p] = shown[i]; const [b, , q] = shown[j];
    const x = Math.min(p.right, q.right) - Math.max(p.left, q.left);
    const y = Math.min(p.bottom, q.bottom) - Math.max(p.top, q.top);
    if (x > 0.5 && y > 0.5) out.push(`${row.dataset.name} ${a} over ${b}`);
  }
}
return out;";

/// The side slot's tab bar: its tabs on one row, the bar one `bar-h` tall (UI-452).
const SIDE_TABS: &str = "
const bar = [...document.querySelectorAll('[data-testid=view] .omfp-view__tabs:not([data-slot]) > .omfp-view__tabbar')][0];
if (!bar) return ['no side tab bar'];
const tops = new Set([...bar.querySelectorAll('[role=tab]')].map((t) => Math.round(t.getBoundingClientRect().top)));
const rem = parseFloat(getComputedStyle(document.documentElement).fontSize);
const height = bar.getBoundingClientRect().height;
return [...(tops.size === 1 ? [] : [`tabs on ${tops.size} rows`]), ...(height <= 2.6 * rem ? [] : [`bar ${height} px`])];";

/// The rows and the side tabs at `width`, then a screenshot.
async fn layout_at(journey: &Journey, width: u32) -> anyhow::Result<()> {
    journey.driver.set_window_rect(0, 0, width, 900).await?;
    journey
        .until(&format!("a {width} px window"), async |page| {
            let value = page.eval("return innerWidth;", Vec::new()).await?;
            Ok((value.as_u64() == Some(u64::from(width))).then_some(()))
        })
        .await?;
    journey
        .until("the wide frame", async |page| {
            let layout = page.attr("[data-testid=app]", "data-layout").await?;
            Ok((layout.as_deref() == Some("wide")).then_some(()))
        })
        .await?;
    none(journey, &format!("repository rows at {width} px"), ROWS).await?;
    journey.fits("[data-widget=\"omfp:workspace\"]").await?;
    none(journey, &format!("side tabs at {width} px"), SIDE_TABS).await?;
    journey.screenshot(&format!("desktop-{width}")).await?;
    Ok(())
}

// spec: E2E-38, UI-452, UI-453
#[tokio::test(flavor = "multi_thread")]
async fn repository_rows_and_side_tabs_at_desktop_widths() -> anyhow::Result<()> {
    let shared = TempRepo::new()?;
    std::fs::write(shared.path().join("README.md"), "changed\n")?;
    let other = TempRepo::new()?;
    let journey = Journey::start(Vec::new()).await?;
    journey.connected().await?;
    journey.scheme("light").await?;
    workspaces(&journey, &shared, &other).await?;
    for width in [1920, 1440, 1100] {
        layout_at(&journey, width).await?;
    }
    journey
        .finish("desktop layout: repository rows, side tabs")
        .await
}
