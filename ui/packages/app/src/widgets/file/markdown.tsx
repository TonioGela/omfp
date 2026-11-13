/** The Markdown preview of `omfp:file` (UI-560..562): the kit's renderer, links and images relative to the file. */

import { Markdown, type MarkdownImageProps } from "omfp/kit";
import { strings, useOpenFile } from "omfp/sdk";
import { createContext, type ReactNode, useCallback, useContext } from "react";
import { type UseFileUrl, useFileUrl } from "./file-url.ts";
import type { ViewerProps } from "./viewers.tsx";

const text = strings.shell.file;

/** An image's alt text in place of the image. */
function ImageAlt({ alt }: Pick<MarkdownImageProps, "alt">): ReactNode {
  return (
    <span className="omfp-file__image-alt" data-testid="file-image-alt">
      {alt}
    </span>
  );
}

/** No URL: relative images show their alt text. */
const noFileUrl: UseFileUrl = () => undefined;

/** The hook giving a workspace file's URL to relative images (UI-562): the SDK's `useFileUrl` when it has one. */
export const FileUrlHook = createContext<UseFileUrl>(useFileUrl ?? noFileUrl);

/** The image of a relative source: loaded from its file's URL, its alt text while there is none (UI-562). */
export function FileImage({ path, alt, title }: MarkdownImageProps): ReactNode {
  const useUrl = useContext(FileUrlHook);
  const url = useUrl(path);
  return url === undefined ? <ImageAlt alt={alt} /> : <img src={url} alt={alt} title={title} />;
}

/** The folder of a workspace path (`""` at the root). */
function folderOf(path: string): string {
  const slash = path.lastIndexOf("/");
  return slash < 0 ? "" : path.slice(0, slash);
}

export function MarkdownViewer({ file, content }: ViewerProps): ReactNode {
  const openFile = useOpenFile();
  const openPath = useCallback(
    (reference: { path: string; line: number | undefined }) => openFile(reference.path, { line: reference.line }),
    [openFile],
  );
  return (
    <Markdown
      text={content}
      onOpenPath={openPath}
      linkBase={folderOf(file.path)}
      image={FileImage}
      aria-label={text.previewLabel(file.path)}
    />
  );
}
