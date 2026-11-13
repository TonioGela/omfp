/** L0: the RFC 4180 parser and the cell order of `omfp:file`'s table viewer (UI-566, UI-564). */

import { describe, expect, test } from "bun:test";
import { compareCells, parseDelimited, sortedRows } from "../../src/widgets/file/csv.ts";

describe("parseDelimited", () => {
  // spec: UI-566
  test("splits records on CRLF or LF and fields on the delimiter; a final line break ends the last record", () => {
    expect(parseDelimited("a,b,c\r\n1,2,3\n4,5,6\n", ",")).toEqual({
      records: [
        ["a", "b", "c"],
        ["1", "2", "3"],
        ["4", "5", "6"],
      ],
      open: false,
    });
    expect(parseDelimited("a\tb\n1\t2", "\t")).toEqual({
      records: [
        ["a", "b"],
        ["1", "2"],
      ],
      open: true,
    });
  });

  // spec: UI-566
  test("quoted fields hold delimiters, line breaks and doubled quotes; empty fields stay empty", () => {
    const text = 'name,note\r\n"Smith, J","said ""hi""\r\nthen left"\n,""\n';
    expect(parseDelimited(text, ",").records).toEqual([
      ["name", "note"],
      ["Smith, J", 'said "hi"\r\nthen left'],
      ["", ""],
    ]);
  });

  // spec: UI-566
  test("a byte order mark and blank lines are skipped; a quote inside an unquoted field is kept as text", () => {
    expect(parseDelimited('\uFEFFa,b\n\n5" disk,x\n', ",").records).toEqual([
      ["a", "b"],
      ['5" disk', "x"],
    ]);
  });

  // spec: UI-566, UI-565
  test("text cut inside a quoted field or a record is open: its last record may be incomplete", () => {
    expect(parseDelimited('a,b\n1,"two\nlines', ",")).toEqual({
      records: [
        ["a", "b"],
        ["1", "two\nlines"],
      ],
      open: true,
    });
    expect(parseDelimited("a,b\n1,", ",")).toEqual({
      records: [
        ["a", "b"],
        ["1", ""],
      ],
      open: true,
    });
    expect(parseDelimited("", ",")).toEqual({ records: [], open: false });
  });

  // spec: UI-566
  test("text after a closing quote joins the field", () => {
    expect(parseDelimited('"a"b,c\n', ",").records).toEqual([["ab", "c"]]);
  });
});

describe("cell order", () => {
  // spec: UI-564
  test("numbers compare as numbers and before text, text in natural order case aside, empty cells last", () => {
    const cells = ["b", "10", "", "2", "A", "-1.5", "item 10", "item 9", "1e3"];
    expect([...cells].sort(compareCells)).toEqual(["-1.5", "2", "10", "1e3", "A", "b", "item 9", "item 10", ""]);
  });

  // spec: UI-564
  test("rows sort by a column ascending or descending, stable, with empty cells last both ways", () => {
    const rows = [
      ["x", "2"],
      ["y", ""],
      ["z", "10"],
      ["w", "2"],
    ];
    expect(sortedRows(rows, 1, "ascending").map((row) => row[0])).toEqual(["x", "w", "z", "y"]);
    expect(sortedRows(rows, 1, "descending").map((row) => row[0])).toEqual(["z", "x", "w", "y"]);
    expect(sortedRows(rows, 5, "ascending").map((row) => row[0])).toEqual(["x", "y", "z", "w"]);
  });
});
