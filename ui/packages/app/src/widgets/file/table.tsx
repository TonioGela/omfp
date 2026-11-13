/**
 * The table viewer of `omfp:file` (UI-563..565): a CSV or TSV file as a table under its first record, sorted by a
 * click on a header, with only the rows in view rendered.
 */

import { strings, useVirtualizer } from "omfp/sdk";
import { type ReactNode, useMemo, useRef, useState } from "react";
import { parseDelimited, type SortDirection, sortedRows } from "./csv.ts";
import type { ViewerProps } from "./viewers.tsx";

const text = strings.shell.file;

/** A body row's height: `.omfp-file__table td` sets the same (one line, no wrapping). */
const ROW_PX = 28;
/** Rows rendered beyond the view on each side. */
const OVERSCAN = 12;

interface Sort {
  readonly column: number;
  readonly direction: SortDirection;
}

/** The records of `content` and its header; a file the daemon cut drops its last record when it may be incomplete. */
function tableOf(content: string, delimiter: string, cut: boolean) {
  const { records, open } = parseDelimited(content, delimiter);
  const body = records.slice(1, cut && open ? -1 : undefined);
  const columns = records.reduce((most, record) => Math.max(most, record.length), 0);
  return { header: records[0] ?? [], body, columns };
}

function TableViewer({ file, content, delimiter }: ViewerProps & { readonly delimiter: string }): ReactNode {
  const cut = file.truncated === true;
  const { header, body, columns } = useMemo(() => tableOf(content, delimiter, cut), [content, delimiter, cut]);
  const [sort, setSort] = useState<Sort | undefined>(undefined);
  const rows = useMemo(() => (sort === undefined ? body : sortedRows(body, sort.column, sort.direction)), [body, sort]);
  const scroller = useRef<HTMLDivElement>(null);
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scroller.current,
    estimateSize: () => ROW_PX,
    overscan: OVERSCAN,
  });
  const items = virtualizer.getVirtualItems();
  const before = items[0]?.start ?? 0;
  const after = virtualizer.getTotalSize() - (items.at(-1)?.end ?? 0);
  const indexes = Array.from({ length: columns }, (_, column) => column);
  const onSort = (column: number) =>
    setSort((current) => ({
      column,
      direction: current?.column === column && current.direction === "ascending" ? "descending" : "ascending",
    }));
  return (
    <>
      {/* biome-ignore lint/a11y/noNoninteractiveTabindex: a scrollable region must be focusable to scroll by keyboard. */}
      <div ref={scroller} className="omfp-file__table" data-testid="file-table" tabIndex={0}>
        <table aria-rowcount={rows.length + 1} aria-label={text.tableLabel(file.path)}>
          <thead>
            <tr>
              {indexes.map((column) => (
                <th key={column} aria-sort={sort?.column === column ? sort.direction : "none"}>
                  <button
                    type="button"
                    className="omfp-file__sort"
                    onClick={() => onSort(column)}
                    data-testid="file-table-sort"
                  >
                    {header[column] ?? ""}
                  </button>
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {/* Spacers standing for the rows out of view, above and below. */}
            {before > 0 && <tr style={{ blockSize: before }} />}
            {items.map((item) => {
              const row = rows[item.index] ?? [];
              return (
                <tr key={item.index} data-index={item.index} aria-rowindex={item.index + 2}>
                  {indexes.map((column) => (
                    <td key={column} title={row[column]}>
                      {row[column] ?? ""}
                    </td>
                  ))}
                </tr>
              );
            })}
            {after > 0 && <tr style={{ blockSize: after }} />}
          </tbody>
        </table>
      </div>
      <p className="omfp-file__notice" data-testid="file-table-summary">
        {text.tableSummary(body.length, columns, cut ? text.size(file.size) : undefined)}
      </p>
    </>
  );
}

export function CsvViewer(props: ViewerProps): ReactNode {
  return <TableViewer {...props} delimiter="," />;
}

export function TsvViewer(props: ViewerProps): ReactNode {
  return <TableViewer {...props} delimiter={"\t"} />;
}
