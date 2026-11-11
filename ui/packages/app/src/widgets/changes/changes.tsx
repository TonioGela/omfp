/**
 * `omfp:changes` (UI-080): for each repository of the current workspace, its branch, ahead/behind and changed files
 * with +/− counts, from the `workspace/<id>` topic (DMN-026); a file expands to its diff (`workspaces.diff`). The
 * status refreshes after each `edit`, `write` or `bash` run of the selected thread ends, and from the refresh button:
 * `workspaces.status` rescans the workspace and publishes the fresh status of every repository on the topic.
 */

import type { FileDiff } from "@omfp/protocol";
import { Diff, Disclosure, IconButton, languageOfPath } from "omfp/kit";
import {
  type ChangedFile,
  type ChangesRepo,
  changeRunsKey,
  changesView,
  SELECTED_THREAD,
  strings,
  useCall,
  useCurrentWorkspaceId,
  useReportError,
  useThread,
  useWindowState,
  useWorkspace,
  type WidgetProps,
} from "omfp/sdk";
import { type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from "react";

const text = strings.shell.changes;

export function ChangesWidget(_props: WidgetProps): ReactNode {
  const workspaceId = useCurrentWorkspaceId();
  if (workspaceId === undefined) {
    return (
      <p className="omfp-changes__empty" data-testid="changes-none">
        {text.none}
      </p>
    );
  }
  return <Changes workspaceId={workspaceId} />;
}

function Changes({ workspaceId }: { workspaceId: string }): ReactNode {
  const { workspace, status } = useWorkspace(workspaceId);
  const [selected] = useWindowState(SELECTED_THREAD);
  const runs = changeRunsKey(useThread(selected)?.state.tools);
  const call = useCall();
  const reportError = useReportError();
  const firstRepo = workspace?.repos[0]?.name;
  const refresh = useCallback(() => {
    const request =
      firstRepo === undefined
        ? call("workspaces.rescan", { workspaceId })
        : call("workspaces.status", { workspaceId, repo: firstRepo });
    request.catch(reportError);
  }, [call, reportError, workspaceId, firstRepo]);

  // Subscribing already computes a fresh status: refresh only when a run ends after that.
  const seenRuns = useRef(runs);
  useEffect(() => {
    if (runs !== seenRuns.current) {
      seenRuns.current = runs;
      refresh();
    }
  }, [runs, refresh]);

  const repos = useMemo(() => (workspace === undefined ? [] : changesView(workspace, status)), [workspace, status]);
  return (
    <section className="omfp-changes" data-testid="changes" aria-label={text.label}>
      <div className="omfp-changes__bar">
        <IconButton label={text.refresh} icon="↻" onPress={refresh} data-testid="changes-refresh" />
      </div>
      {repos.length === 0 ? (
        <p className="omfp-changes__empty" data-testid="changes-no-repo">
          {text.noRepos}
        </p>
      ) : (
        repos.map((repo) => <RepoChanges key={repo.name} workspaceId={workspaceId} repo={repo} runs={runs} />)
      )}
    </section>
  );
}

function RepoChanges({ workspaceId, repo, runs }: { workspaceId: string; repo: ChangesRepo; runs: string }): ReactNode {
  return (
    <section className="omfp-changes__repo" data-testid="changes-repo" data-repo={repo.name} aria-label={repo.name}>
      <h3 className="omfp-changes__repo-head">
        <span className="omfp-changes__repo-name">{repo.name}</span>
        <span className="omfp-changes__meta">{text.branch(repo.branch)}</span>
        {repo.aheadBehind !== undefined && <span className="omfp-changes__meta">{repo.aheadBehind}</span>}
        {repo.summary !== undefined && (
          <span className="omfp-changes__meta" data-testid="changes-summary">
            {repo.summary}
          </span>
        )}
      </h3>
      {repo.files === undefined ? (
        <p className="omfp-changes__empty">{text.loading}</p>
      ) : repo.files.length === 0 ? (
        <p className="omfp-changes__empty">{text.clean}</p>
      ) : (
        <ul className="omfp-changes__files">
          {repo.files.map((file) => (
            <li key={file.path}>
              <FileChanges workspaceId={workspaceId} repo={repo.name} file={file} runs={runs} />
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

function FileChanges({
  workspaceId,
  repo,
  file,
  runs,
}: {
  workspaceId: string;
  repo: string;
  file: ChangedFile;
  runs: string;
}): ReactNode {
  const [expanded, setExpanded] = useState(false);
  return (
    <Disclosure
      className="omfp-changes__file"
      data-testid="changes-file"
      data-path={file.path}
      data-status={file.status}
      level={4}
      toggleTestId="changes-file-toggle"
      expanded={expanded}
      onExpandedChange={setExpanded}
      title={
        <>
          <span className="omfp-changes__status" data-status={file.status} title={text.statusLabel[file.status]}>
            {text.status[file.status]}
          </span>
          <span className="omfp-changes__path">{file.path}</span>
          {file.from !== undefined && <span className="omfp-changes__meta">{text.renamedFrom(file.from)}</span>}
          <span className="omfp-changes__counts" data-testid="changes-counts">
            {text.counts(file.additions, file.deletions)}
          </span>
        </>
      }
    >
      {expanded && <FileDiffView workspaceId={workspaceId} repo={repo} file={file} runs={runs} />}
    </Disclosure>
  );
}

/** The diff of one file, read again whenever its status or the selected thread's ended runs change. */
function FileDiffView({
  workspaceId,
  repo,
  file,
  runs,
}: {
  workspaceId: string;
  repo: string;
  file: ChangedFile;
  runs: string;
}): ReactNode {
  const call = useCall();
  const reportError = useReportError();
  const [diff, setDiff] = useState<FileDiff | undefined>(undefined);
  const { path, status, additions, deletions } = file;
  // biome-ignore lint/correctness/useExhaustiveDependencies: the status, counts and runs say when to read it again
  useEffect(() => {
    let live = true;
    call("workspaces.diff", { workspaceId, repo, path })
      .then((page) => {
        if (live) {
          setDiff(page.files.find((entry) => entry.path === path) ?? { path, patch: "", truncated: false });
        }
      })
      .catch(reportError);
    return () => {
      live = false;
    };
  }, [call, reportError, workspaceId, repo, path, status, additions, deletions, runs]);
  if (diff === undefined) {
    return <p className="omfp-changes__empty">{text.loadingDiff}</p>;
  }
  return (
    <>
      <Diff
        patch={diff.patch}
        language={languageOfPath(path)}
        aria-label={text.diffLabel(path)}
        data-testid="changes-diff"
      />
      {diff.truncated && <p className="omfp-changes__empty">{text.truncated}</p>}
    </>
  );
}
