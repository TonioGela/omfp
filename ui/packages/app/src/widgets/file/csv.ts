/**
 * Delimited text for `omfp:file`'s table viewer (UI-563..566): an RFC 4180 parser (quoted fields with delimiters, line
 * breaks and doubled quotes; CRLF or LF records) and the order the table sorts cells in.
 */

/** The records of a text, and whether it ended inside one (no final line break, or inside quotes): text the daemon
 * cut then ends with a record that may be incomplete. */
export interface Parsed {
  readonly records: string[][];
  readonly open: boolean;
}

const QUOTE = 34;
const LF = 10;
const CR = 13;

/**
 * The records of `text` split on `delimiter`: a field starting with `"` is quoted up to the next lone `"` (a doubled
 * `""` is one quote; delimiters and line breaks inside are text), and text after its closing quote joins it; a quote
 * elsewhere is text. A byte order mark and empty lines are skipped.
 */
export function parseDelimited(text: string, delimiter: string): Parsed {
  const records: string[][] = [];
  const separator = delimiter.charCodeAt(0);
  let record: string[] = [];
  let field = "";
  // Whether the current record has any character (an empty line has none and is skipped).
  let started = false;
  let quoted = false;
  let index = text.charCodeAt(0) === 0xfeff ? 1 : 0;
  while (index < text.length) {
    const code = text.charCodeAt(index);
    if (quoted) {
      const end = text.indexOf('"', index);
      if (end < 0) {
        field += text.slice(index);
        index = text.length;
      } else if (text.charCodeAt(end + 1) === QUOTE) {
        field += `${text.slice(index, end)}"`;
        index = end + 2;
      } else {
        field += text.slice(index, end);
        quoted = false;
        index = end + 1;
      }
      continue;
    }
    if (code === QUOTE && field === "") {
      quoted = true;
      started = true;
    } else if (code === separator) {
      record.push(field);
      field = "";
      started = true;
    } else if (code === LF || code === CR) {
      if (started) {
        record.push(field);
        records.push(record);
      }
      record = [];
      field = "";
      started = false;
      index += code === CR && text.charCodeAt(index + 1) === LF ? 1 : 0;
    } else {
      // The run of text up to the next delimiter or line break, at once.
      let end = index + 1;
      while (end < text.length) {
        const next = text.charCodeAt(end);
        if (next === separator || next === LF || next === CR) {
          break;
        }
        end += 1;
      }
      field += text.slice(index, end);
      started = true;
      index = end;
      continue;
    }
    index += 1;
  }
  const open = started || quoted;
  if (open) {
    record.push(field);
    records.push(record);
  }
  return { records, open };
}

const NUMBER = /^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:e[+-]?\d+)?$/i;
const COLLATOR = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });

/** A cell as a number when it reads as one. */
function numberOf(cell: string): number | undefined {
  const trimmed = cell.trim();
  return NUMBER.test(trimmed) ? Number(trimmed) : undefined;
}

/** The ascending order of cells: numbers by value before text, text in natural order (`item 9` before `item 10`)
 * case aside, empty cells last. */
export function compareCells(a: string, b: string): number {
  if (a.trim() === "" || b.trim() === "") {
    return Number(a.trim() === "") - Number(b.trim() === "");
  }
  const x = numberOf(a);
  const y = numberOf(b);
  if (x !== undefined && y !== undefined) {
    return x - y;
  }
  if (x !== undefined || y !== undefined) {
    return x === undefined ? 1 : -1;
  }
  return COLLATOR.compare(a, b);
}

export type SortDirection = "ascending" | "descending";

/** `rows` sorted by the cells of `column` (stable; empty cells last in both directions). */
export function sortedRows<Row extends readonly string[]>(
  rows: readonly Row[],
  column: number,
  direction: SortDirection,
): Row[] {
  const sign = direction === "ascending" ? 1 : -1;
  return rows
    .map((row, index) => ({ row, index, cell: row[column] ?? "" }))
    .sort((a, b) => {
      const empty = Number(a.cell.trim() === "") - Number(b.cell.trim() === "");
      return empty !== 0 ? empty : sign * compareCells(a.cell, b.cell) || a.index - b.index;
    })
    .map(({ row }) => row);
}
