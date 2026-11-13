/**
 * The viewers `omfp:file` offers besides the source (UI-560): a rendered form of a text file, chosen by the file's
 * extension, shown while the file is read-only, with a toggle back to the source.
 */

import type { FileContent } from "@omfp/protocol";
import { strings } from "omfp/sdk";
import type { ComponentType } from "react";
import { MarkdownViewer } from "./markdown.tsx";

/** What a viewer gets: the file as read and its text. */
export interface ViewerProps {
  readonly file: FileContent;
  readonly content: string;
}

export interface Viewer {
  /** Its name, on the view as `data-viewer`. */
  readonly name: string;
  /** The toggle's label. */
  readonly label: string;
  readonly View: ComponentType<ViewerProps>;
}

const markdown: Viewer = { name: "markdown", label: strings.shell.file.viewers.markdown, View: MarkdownViewer };

/** The viewers by lower-case extension (a map: an extension such as `constructor` must find nothing). */
const VIEWERS: ReadonlyMap<string, Viewer> = new Map([
  ["md", markdown],
  ["markdown", markdown],
  ["mdx", markdown],
]);

/** The viewer of the file at `path`, by its extension (case aside), if it has one. */
export function viewerOf(path: string): Viewer | undefined {
  const name = path.slice(path.lastIndexOf("/") + 1);
  const dot = name.lastIndexOf(".");
  return dot <= 0 ? undefined : VIEWERS.get(name.slice(dot + 1).toLowerCase());
}
