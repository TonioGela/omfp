/**
 * A subagent's detail under its row of `omfp:background` (UI-490, UI-491): its kind and use, its transcript in a box
 * titled on its border (UI-187), read incrementally with `thread.subagent_messages` (`fromByte`) whenever its progress
 * changes or on request, and, while it runs, a steer field (`thread.subagent_steer`). Its row keeps Cancel (UI-272).
 */

import { BorderBox, Button, Icon, Markdown, Message, TextField } from "omfp/kit";
import {
  appendTranscriptPage,
  EMPTY_TRANSCRIPT,
  type SubagentRow,
  type SubagentTranscript,
  strings,
  subagentLines,
  useCall,
  useReportError,
} from "omfp/sdk";
import { type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from "react";

const text = strings.shell.background;

export interface SubagentDetailProps {
  readonly threadId: string;
  readonly row: SubagentRow;
  /** The id its row's `aria-controls` names. */
  readonly id: string;
}

export function SubagentDetail({ threadId, row, id }: SubagentDetailProps): ReactNode {
  const call = useCall();
  const reportError = useReportError();
  const [transcript, setTranscript] = useState<SubagentTranscript | undefined>(undefined);
  const [requested, setRequested] = useState(0);
  const subagentId = row.id;
  // Changes with the subagent's progress: the transcript reads what is new.
  const version = `${row.status}:${row.meta}:${row.activity ?? ""}`;

  // Where the next read starts: the last page's `nextByte`.
  const nextByte = useRef(0);
  useEffect(() => {
    nextByte.current = transcript?.nextByte ?? 0;
  }, [transcript]);

  // biome-ignore lint/correctness/useExhaustiveDependencies: `version` and `requested` trigger a read of what is new.
  useEffect(() => {
    let current = true;
    call("thread.subagent_messages", { threadId, subagentId, fromByte: nextByte.current }).then((page) => {
      if (current) {
        setTranscript((latest) => appendTranscriptPage(latest ?? EMPTY_TRANSCRIPT, page));
      }
    }, reportError);
    return () => {
      current = false;
    };
  }, [call, reportError, threadId, subagentId, version, requested]);

  const lines = useMemo(() => (transcript === undefined ? [] : subagentLines(transcript.messages)), [transcript]);
  return (
    <BorderBox
      id={id}
      title={text.transcript(row.id)}
      level={2}
      className="omfp-background__subagent"
      data-testid="background-subagent"
    >
      <p className="omfp-background__use" data-testid="background-subagent-use">
        {row.meta === "" ? row.agent : `${row.agent} · ${row.meta}`}
      </p>
      {transcript === undefined ? (
        <p className="omfp-subagents__empty">{text.loading}</p>
      ) : lines.length === 0 ? (
        <p className="omfp-subagents__empty">{text.empty}</p>
      ) : (
        <div className="omfp-subagents__lines">
          {lines.map((line, index) =>
            line.role === "tool" ? (
              // biome-ignore lint/suspicious/noArrayIndexKey: transcript lines only ever grow at the end
              <p key={index} className="omfp-subagents__tool" data-testid="subagent-line" data-role="tool">
                {line.text}
              </p>
            ) : (
              <Message
                // biome-ignore lint/suspicious/noArrayIndexKey: transcript lines only ever grow at the end
                key={index}
                author={line.role}
                name={line.role === "user" ? text.roleUser : text.roleAssistant}
                data-testid="subagent-line"
                data-role={line.role}
              >
                <Markdown text={line.text} />
              </Message>
            ),
          )}
        </div>
      )}
      <div className="omfp-subagents__actions">
        <Button
          size="sm"
          variant="quiet"
          icon={<Icon name="refresh-cw" />}
          onPress={() => setRequested((count) => count + 1)}
          data-testid="subagent-more"
        >
          {text.more}
        </Button>
        {row.live && <Steer threadId={threadId} row={row} />}
      </div>
    </BorderBox>
  );
}

function Steer({ threadId, row }: { threadId: string; row: SubagentRow }): ReactNode {
  const call = useCall();
  const reportError = useReportError();
  const [message, setMessage] = useState("");
  const subagentId = row.id;
  const steer = useCallback(() => {
    const trimmed = message.trim();
    if (trimmed === "") {
      return;
    }
    call("thread.subagent_steer", { threadId, subagentId, text: trimmed }).then(() => setMessage(""), reportError);
  }, [call, reportError, threadId, subagentId, message]);
  return (
    <form
      className="omfp-subagents__steer"
      onSubmit={(event) => {
        event.preventDefault();
        steer();
      }}
    >
      <TextField
        label={text.steerLabel(row.id)}
        hideLabel
        placeholder={text.steerPlaceholder}
        value={message}
        onChange={setMessage}
        inputProps={{ "data-testid": "subagent-steer-input" }}
      />
      <Button size="sm" type="submit" icon={<Icon name="arrow-up" />} data-testid="subagent-steer">
        {text.steer}
      </Button>
    </form>
  );
}
