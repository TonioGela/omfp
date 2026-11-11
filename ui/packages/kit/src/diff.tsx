import { type HTMLAttributes, useMemo } from "react";
import { cx } from "./class.ts";
import { type Token, TokenLine, useHighlight } from "./highlight.tsx";

/** What a line of a unified patch is. */
export type DiffLineKind = "meta" | "hunk" | "context" | "add" | "del" | "note";

/** One line of a unified patch; `text` has no `+`/`-`/space prefix. */
export interface DiffLine {
  readonly kind: DiffLineKind;
  readonly text: string;
  /** Line number in the old file (context and deleted lines). */
  readonly oldLine?: number;
  /** Line number in the new file (context and added lines). */
  readonly newLine?: number;
}

const HUNK = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/;

/** The lines of a unified patch (`git diff` output): file headers, hunk headers, and numbered lines. */
export function parsePatch(patch: string): DiffLine[] {
  const lines = patch.split("\n");
  if (lines.at(-1) === "") {
    lines.pop();
  }
  const out: DiffLine[] = [];
  let oldLine = 0;
  let newLine = 0;
  let inHunk = false;
  for (const line of lines) {
    const hunk = HUNK.exec(line);
    if (hunk !== null) {
      oldLine = Number(hunk[1]);
      newLine = Number(hunk[2]);
      inHunk = true;
      out.push({ kind: "hunk", text: line });
    } else if (!inHunk || line.startsWith("diff --git ")) {
      inHunk = false;
      out.push({ kind: "meta", text: line });
    } else if (line.startsWith("+")) {
      out.push({ kind: "add", text: line.slice(1), newLine: newLine++ });
    } else if (line.startsWith("-")) {
      out.push({ kind: "del", text: line.slice(1), oldLine: oldLine++ });
    } else if (line.startsWith("\\")) {
      out.push({ kind: "note", text: line });
    } else {
      // A context line; some tools strip the space of an empty one.
      out.push({ kind: "context", text: line.slice(1), oldLine: oldLine++, newLine: newLine++ });
    }
  }
  return out;
}

/** File headers that repeat what the widget around the diff already shows. */
const PLAIN_HEADERS = ["diff --git ", "index ", "--- ", "+++ "];

const SIGN: Record<DiffLineKind, string> = { meta: "", hunk: "", context: " ", add: "+", del: "−", note: "" };

export interface DiffProps extends Omit<HTMLAttributes<HTMLTableElement>, "children"> {
  /** A unified patch (`git diff` output), or the diff as lines already (`lines`). */
  patch?: string;
  lines?: readonly DiffLine[];
  /** Highlights the lines as this language (a name, alias or file extension, UI-090); plain text when unknown. */
  language?: string | undefined;
  /** The file is binary: shows `binaryText` instead of lines. */
  binary?: boolean;
  /** Shown for a binary file (`binary file` by default). */
  binaryText?: string;
  /** Shown when there is no line (`no changes` by default). */
  emptyText?: string;
}

/** The old (`del`) or new (`add`) side of a patch, its context lines included: what is highlighted together. */
function side(lines: readonly DiffLine[], kind: "add" | "del"): string {
  return lines
    .filter((line) => line.kind === "context" || line.kind === kind)
    .map((line) => line.text)
    .join("\n");
}

/**
 * A unified diff as a table: old and new line numbers, the sign, the line. Added and deleted lines are marked
 * (`data-kind`) and colored by the theme's success and error tokens; file headers other than mode, rename and binary
 * notes are left out. With a `language`, each side is highlighted as a whole, so tokens spanning lines stay right.
 */
export function Diff({
  patch = "",
  lines,
  language,
  binary = false,
  binaryText = "binary file",
  emptyText = "no changes",
  className,
  ...props
}: DiffProps) {
  const parsed = useMemo(() => lines ?? parsePatch(patch), [lines, patch]);
  const oldTokens = useHighlight(
    useMemo(() => side(parsed, "del"), [parsed]),
    language,
  );
  const newTokens = useHighlight(
    useMemo(() => side(parsed, "add"), [parsed]),
    language,
  );
  let oldIndex = 0;
  let newIndex = 0;
  const rows = parsed.map((line) => {
    let tokens: readonly Token[] | undefined;
    if (line.kind === "del") {
      tokens = oldTokens?.[oldIndex++];
    } else if (line.kind === "add") {
      tokens = newTokens?.[newIndex++];
    } else if (line.kind === "context") {
      oldIndex++;
      tokens = newTokens?.[newIndex++];
    }
    return { line, tokens };
  });
  const shown = rows.filter(
    ({ line }) => line.kind !== "meta" || !PLAIN_HEADERS.some((prefix) => line.text.startsWith(prefix)),
  );
  const notice = binary ? binaryText : shown.length === 0 ? emptyText : undefined;
  return (
    <table {...props} className={cx("omfp-code omfp-diff", className)}>
      <tbody>
        {notice !== undefined && (
          <tr className="omfp-diff__line" data-kind={binary ? "binary" : "empty"}>
            <td className="omfp-diff__header omfp-diff__notice" colSpan={4}>
              {notice}
            </td>
          </tr>
        )}
        {(binary ? [] : shown).map(({ line, tokens }, index) => (
          // The patch is static for one render: positions are stable keys.
          // biome-ignore lint/suspicious/noArrayIndexKey: see above
          <tr key={index} className="omfp-diff__line" data-kind={line.kind}>
            {line.kind === "hunk" || line.kind === "meta" || line.kind === "note" ? (
              <td className="omfp-diff__header" colSpan={4}>
                {line.text}
              </td>
            ) : (
              <>
                <td className="omfp-diff__number">{line.oldLine}</td>
                <td className="omfp-diff__number">{line.newLine}</td>
                <td className="omfp-diff__sign">{SIGN[line.kind]}</td>
                <td className="omfp-diff__code">{tokens === undefined ? line.text : <TokenLine tokens={tokens} />}</td>
              </>
            )}
          </tr>
        ))}
      </tbody>
    </table>
  );
}
