/**
 * `omfp:todos` (UI-082): the selected thread's todo list, phase by phase, from its state (`todo_phases`), under the
 * session's goal when it has one (`goal`). Each phase is a box titled on its border with its progress at the end
 * (UI-187); a task is a `●` like the todo tool card's list (UI-163), blocked tasks say on what.
 */

import { BorderBox, EmptyState } from "omfp/kit";
import { SELECTED_THREAD, strings, todosView, useThread, useWindowState, type WidgetProps } from "omfp/sdk";
import { type ReactNode, useMemo } from "react";

const text = strings.shell.todos;

export function TodosWidget(_props: WidgetProps): ReactNode {
  const [threadId] = useWindowState(SELECTED_THREAD);
  const state = useThread(threadId)?.state;
  const view = useMemo(() => (state === undefined ? undefined : todosView(state)), [state]);
  if (threadId === undefined) {
    return <EmptyState icon="inbox" title={text.noThread} data-testid="todos-no-thread" />;
  }
  return (
    <section className="omfp-todos" data-testid="todos" aria-label={text.label}>
      {view?.goal !== undefined && (
        <BorderBox
          title={text.goal}
          level={3}
          end={view.goal.statusLabel}
          className="omfp-todos__goal"
          data-testid="todos-goal"
          data-status={view.goal.status}
        >
          <p className="omfp-todos__objective">{view.goal.objective}</p>
          <p className="omfp-todos__meta">{view.goal.usage}</p>
        </BorderBox>
      )}
      {view === undefined || view.phases.length === 0 ? (
        <EmptyState icon="check" title={text.none} data-testid="todos-none">
          {text.noneHint}
        </EmptyState>
      ) : (
        view.phases.map((phase) => (
          <BorderBox
            key={phase.name}
            title={phase.name}
            level={3}
            end={phase.progress}
            className="omfp-todos__phase"
            data-testid="todos-phase"
          >
            <ul className="omfp-todo">
              {phase.tasks.map((task) => (
                <li key={task.content} className="omfp-todo__item" data-testid="todos-task" data-status={task.status}>
                  <span className="omfp-todo__mark" aria-hidden="true">
                    ●
                  </span>
                  <span className="omfp-todo__text">
                    {task.content}
                    {task.blocker !== undefined && (
                      <span className="omfp-todos__blocker">{text.blocked(task.blocker)}</span>
                    )}
                  </span>
                  <span className="omfp-visually-hidden">{task.statusLabel}</span>
                </li>
              ))}
            </ul>
          </BorderBox>
        ))
      )}
    </section>
  );
}
