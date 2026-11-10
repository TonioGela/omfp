import type { HTMLAttributes } from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import { cx } from "./class.ts";

const REMARK_PLUGINS = [remarkGfm];

/** Links open in a new tab without leaking the referrer; everything else keeps react-markdown's elements. */
const COMPONENTS: Components = {
  a: ({ node: _node, ...props }) => <a {...props} target="_blank" rel="noreferrer" />,
  pre: ({ node: _node, className, ...props }) => (
    <pre {...props} className={cx("omfp-code omfp-code--block", className)} />
  ),
};

export interface MarkdownProps extends Omit<HTMLAttributes<HTMLDivElement>, "children"> {
  /** Markdown source (CommonMark + GFM). */
  text: string;
}

/**
 * Renders Markdown as prose (serif). Raw HTML in the source never becomes DOM: react-markdown without `rehype-raw`
 * drops it, and its URL filter removes `javascript:` links. Remote images and code-language classes are kept (UI-040).
 */
export function Markdown({ text, className, ...props }: MarkdownProps) {
  return (
    <div {...props} className={cx("omfp-prose omfp-markdown", className)}>
      <ReactMarkdown remarkPlugins={REMARK_PLUGINS} components={COMPONENTS}>
        {text}
      </ReactMarkdown>
    </div>
  );
}

export interface CodeProps extends Omit<HTMLAttributes<HTMLElement>, "children"> {
  code: string;
  /** A `<pre>` block (keeps whitespace, scrolls horizontally) instead of inline code. */
  block?: boolean;
}

/** Monospace code, inline or as a block. */
export function Code({ code, block = false, className, ...props }: CodeProps) {
  if (block) {
    return (
      <pre {...props} className={cx("omfp-code omfp-code--block", className)}>
        <code>{code}</code>
      </pre>
    );
  }
  return (
    <code {...props} className={cx("omfp-code", className)}>
      {code}
    </code>
  );
}
