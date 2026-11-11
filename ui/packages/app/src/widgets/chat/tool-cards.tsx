/**
 * The bodies of the specialized tool cards (UI-050): what `toolView` read from a call, rendered per tool. Failed calls
 * show omp's error first (UI-052); code is highlighted when its language is known (UI-090).
 */

import { Badge, Button, Chip, Code, Diff } from "omfp/kit";
import {
  ansiRuns,
  repoPath,
  type SubagentRun,
  strings,
  type TodoPhase,
  type ToolCard,
  type ToolView,
  useCurrentWorkspaceId,
  useWorkspaces,
} from "omfp/sdk";
import { type ReactNode, useMemo, useState } from "react";

const text = strings.chat.tool;

/** A file path: relative to the workspace repository holding it, with that repository's chip. */
export function FilePath({ path }: { path: string }): ReactNode {
  const workspaceId = useCurrentWorkspaceId();
  const workspace = useWorkspaces().get(workspaceId ?? "");
  const shown = repoPath(path, workspace);
  return (
    <span className="omfp-tool-card__path" data-testid="tool-path" data-repo={shown.repo}>
      {shown.repo !== undefined && <Chip data-testid="tool-repo">{shown.repo}</Chip>}
      <span className="omfp-tool-card__subject">{shown.path}</span>
    </span>
  );
}

/** What the call is about, shown in the card's header: its command, file, query or URL. */
export function ToolSubject({ view }: { view: ToolView }): ReactNode {
  switch (view.kind) {
    case "bash":
      return <Subject>{view.command}</Subject>;
    case "read":
    case "write":
      return <FilePath path={view.path} />;
    case "edit":
      return view.path === undefined ? null : <FilePath path={view.path} />;
    case "fetch":
      return <Subject>{view.url}</Subject>;
    case "search":
    case "web_search":
      return <Subject>{view.query}</Subject>;
    case "task":
      return <Subject>{view.agents.map((agent) => agent.name).join(", ")}</Subject>;
    default:
      return null;
  }
}

function Subject({ children }: { children: ReactNode }): ReactNode {
  return (
    <span className="omfp-tool-card__subject" data-testid="tool-subject">
      {children}
    </span>
  );
}

/** Command output with its ANSI colors and attributes as theme classes. */
export function AnsiOutput({ output }: { output: string }): ReactNode {
  const runs = useMemo(() => ansiRuns(output), [output]);
  return (
    <pre className="omfp-code omfp-code--block omfp-tool-card__output" data-testid="tool-output">
      {runs.map((run, index) =>
        run.style.length === 0 ? (
          run.text
        ) : (
          // biome-ignore lint/suspicious/noArrayIndexKey: runs of one output never move
          <span key={index} className={run.style.map((name) => `omfp-ansi-${name}`).join(" ")}>
            {run.text}
          </span>
        ),
      )}
    </pre>
  );
}

/** The error of a failed call (omp's message). */
function ToolError({ card }: { card: ToolCard }): ReactNode {
  if (card.status !== "failed" || card.result === undefined) {
    return null;
  }
  return (
    <div className="omfp-tool-card__error" data-testid="tool-error">
      {card.result}
    </div>
  );
}

/** Lines of a `write`'s content shown while it is collapsed (UI-053). */
const WRITE_PREVIEW_LINES = 6;

/** A `write`'s content: its first lines, expandable to the whole content (UI-053). */
function WriteContent({ content, language }: { content: string; language: string | undefined }): ReactNode {
  const [expanded, setExpanded] = useState(false);
  const lines = content.split("\n");
  if (lines.at(-1) === "") {
    lines.pop();
  }
  const hidden = lines.length - WRITE_PREVIEW_LINES;
  const shown = expanded || hidden <= 0 ? content : lines.slice(0, WRITE_PREVIEW_LINES).join("\n");
  return (
    <>
      <Code code={shown} block language={language} data-testid="tool-content" />
      {hidden > 0 && (
        <Button
          size="sm"
          variant="quiet"
          aria-expanded={expanded}
          data-testid="tool-more"
          onPress={() => setExpanded((open) => !open)}
        >
          {expanded ? text.fewerLines : text.moreLines(hidden)}
        </Button>
      )}
    </>
  );
}

/** The body of a specialized card. */
export function ToolBody({ card }: { card: ToolCard }): ReactNode {
  const { view } = card;
  switch (view.kind) {
    case "bash":
      return (
        <div className="omfp-tool-card__section">
          {view.cwd !== undefined && <p className="omfp-tool-card__meta">{text.cwd(view.cwd)}</p>}
          {view.output === undefined || view.output === "" ? (
            <p className="omfp-tool-card__meta">{text.noOutput}</p>
          ) : (
            <AnsiOutput output={view.output} />
          )}
          {view.exitCode !== undefined && (
            <Badge tone={view.exitCode === 0 ? "success" : "error"} data-testid="tool-exit">
              {text.exitCode(view.exitCode)}
            </Badge>
          )}
        </div>
      );
    case "read":
      return (
        <div className="omfp-tool-card__section">
          <ToolError card={card} />
          {view.range !== undefined && (
            <p className="omfp-tool-card__meta" data-testid="tool-range">
              {view.range}
            </p>
          )}
          {card.status !== "failed" && view.content !== undefined && (
            <Code
              code={view.content}
              block
              language={view.directory ? undefined : view.language}
              data-testid="tool-content"
            />
          )}
        </div>
      );
    case "fetch":
      return (
        <div className="omfp-tool-card__section">
          <ToolError card={card} />
          <a className="omfp-tool-card__link" href={view.url} target="_blank" rel="noreferrer" data-testid="tool-link">
            {view.url}
          </a>
          {card.status !== "failed" && view.content !== undefined && (
            <Code code={view.content} block language="markdown" data-testid="tool-content" />
          )}
        </div>
      );
    case "edit":
      return (
        <div className="omfp-tool-card__section">
          <ToolError card={card} />
          {view.diff !== undefined ? (
            <Diff lines={view.diff} language={view.language} data-testid="tool-diff" />
          ) : (
            view.input !== undefined && (
              <>
                <h4>{text.edit}</h4>
                <Code code={view.input} block data-testid="tool-input" />
              </>
            )
          )}
        </div>
      );
    case "write":
      return (
        <div className="omfp-tool-card__section">
          <ToolError card={card} />
          {view.content !== undefined && <WriteContent content={view.content} language={view.language} />}
        </div>
      );
    case "search":
      return (
        <div className="omfp-tool-card__section">
          <ToolError card={card} />
          <p className="omfp-tool-card__meta">
            {view.scope !== undefined && <span>{text.in(view.scope)}</span>}
            {view.total !== undefined && <span data-testid="tool-total">{view.total}</span>}
          </p>
          {view.files !== undefined &&
            (view.files.length === 0 ? (
              <p className="omfp-tool-card__meta">{text.noMatch}</p>
            ) : (
              <ul className="omfp-tool-card__files">
                {view.files.map((file) => (
                  <li key={file.path} data-testid="tool-file">
                    <FilePath path={file.path} />
                    {file.count !== undefined && <Badge>{file.count}</Badge>}
                  </li>
                ))}
              </ul>
            ))}
          {view.truncated && <p className="omfp-tool-card__meta">{text.truncated}</p>}
        </div>
      );
    case "todo":
      return (
        <div className="omfp-tool-card__section">
          <ToolError card={card} />
          <TodoList phases={view.phases} />
        </div>
      );
    case "web_search":
      return (
        <div className="omfp-tool-card__section">
          <ToolError card={card} />
          {card.status === "succeeded" && view.links.length === 0 && (
            <p className="omfp-tool-card__meta">{text.noLinks}</p>
          )}
          <ul className="omfp-tool-card__links">
            {view.links.map((link) => (
              <li key={link.url} data-testid="tool-link">
                <a href={link.url} target="_blank" rel="noreferrer">
                  {link.title}
                </a>
                <span className="omfp-tool-card__meta">{link.url}</span>
                {link.snippet !== "" && <span>{link.snippet}</span>}
              </li>
            ))}
          </ul>
        </div>
      );
    case "ask":
      return (
        <div className="omfp-tool-card__section">
          <ToolError card={card} />
          <dl className="omfp-tool-card__answers">
            {view.questions.map((question) => (
              <div key={question.question} data-testid="ask-question">
                <dt>{question.question}</dt>
                {question.answer !== undefined && (
                  <dd data-testid="ask-answer">{question.answer === "" ? text.noAnswer : question.answer}</dd>
                )}
              </div>
            ))}
          </dl>
        </div>
      );
    case "task":
      return (
        <div className="omfp-tool-card__section">
          <ToolError card={card} />
          <ul className="omfp-tool-card__agents">
            {view.agents.map((agent) => (
              <Subagent key={agent.name} agent={agent} />
            ))}
          </ul>
        </div>
      );
    case "generic":
      return (
        <>
          <div className="omfp-tool-card__section">
            <h4>{text.arguments}</h4>
            <Code code={card.args} block data-testid="tool-args" />
          </div>
          {card.result !== undefined && (
            <div className="omfp-tool-card__section">
              <h4>{text.result}</h4>
              <Code code={card.result} block data-testid="tool-result" />
            </div>
          )}
        </>
      );
  }
}

const TODO_MARK: Readonly<Record<string, string>> = { completed: "☑", in_progress: "◐" };

function TodoList({ phases }: { phases: readonly TodoPhase[] }): ReactNode {
  return phases.map((phase) => (
    <section key={phase.name} className="omfp-tool-card__phase" aria-label={phase.name}>
      {phases.length > 1 && <h4>{phase.name}</h4>}
      <ul className="omfp-tool-card__todo">
        {phase.tasks.map((task) => (
          <li key={task.content} data-testid="todo-task" data-status={task.status}>
            <span aria-hidden="true">{TODO_MARK[task.status] ?? "☐"}</span>
            <span>{task.content}</span>
            <span className="omfp-visually-hidden">{text.todoStatus[task.status] ?? task.status}</span>
          </li>
        ))}
      </ul>
    </section>
  ));
}

function Subagent({ agent }: { agent: SubagentRun }): ReactNode {
  return (
    <li className="omfp-tool-card__agent" data-testid="subagent" data-status={agent.status}>
      <div className="omfp-tool-card__agent-head">
        <strong>{agent.name}</strong>
        {agent.agent !== "" && <span className="omfp-tool-card__meta">{agent.agent}</span>}
        <Badge
          tone={agent.status === "completed" ? "success" : agent.status === "running" ? "primary" : "default"}
          data-testid="subagent-status"
        >
          {agent.status}
        </Badge>
        {agent.tools > 0 && <span className="omfp-tool-card__meta">{text.tools(agent.tools)}</span>}
      </div>
      <p className="omfp-tool-card__task">{agent.task}</p>
      {agent.activity !== undefined && (
        <p className="omfp-tool-card__meta" data-testid="subagent-activity">
          {agent.activity}
        </p>
      )}
      {agent.output !== undefined && (
        <p className="omfp-tool-card__agent-output" data-testid="subagent-output">
          {agent.output}
        </p>
      )}
    </li>
  );
}
