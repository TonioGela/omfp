import { type HTMLAttributes, useMemo } from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import { cx } from "./class.ts";
import { Highlighted } from "./highlight.tsx";
import { linkReference, type OpenPath, PathLink, pathReference } from "./paths.tsx";

const REMARK_PLUGINS = [remarkGfm];

const FENCE_LANGUAGE = /(?:^|\s)language-(\S+)/;

/** The fence language of a `<pre>`'s `<code>` child (its `language-*` class), if any. */
function fenceLanguage(node: { children?: readonly unknown[] } | undefined): string | undefined {
  const code = node?.children?.[0];
  if (typeof code !== "object" || code === null || !("properties" in code)) {
    return undefined;
  }
  const names = (code.properties as { className?: unknown }).className;
  const name = Array.isArray(names)
    ? names.find((value) => typeof value === "string" && value.startsWith("language-"))
    : undefined;
  return typeof name === "string" ? name.slice("language-".length) : undefined;
}

/** Links open in a new tab without leaking the referrer; fenced code with a language is highlighted (UI-090);
 * everything else keeps react-markdown's elements. */
const COMPONENTS = {
  a: ({ node: _node, ...props }) => <a {...props} target="_blank" rel="noreferrer" />,
  pre: ({ node, className, ...props }) => (
    <pre {...props} className={cx("omfp-code omfp-code--block", className)} data-lang={fenceLanguage(node)} />
  ),
  code: ({ node: _node, className, children, ...props }) => {
    const language = FENCE_LANGUAGE.exec(className ?? "")?.[1];
    return (
      <code {...props} className={className}>
        {language !== undefined && typeof children === "string" ? (
          <Highlighted code={children} language={language} />
        ) : (
          children
        )}
      </code>
    );
  },
} satisfies Components;

/** {@link COMPONENTS} where file paths open with `onOpen` (UI-116): inline code that reads as a path, and links to a
 * relative path. A fenced block's text ends with a line break, so it never counts as inline code. */
function withPaths(onOpen: OpenPath): Components {
  const { a: Link, code: CodeElement } = COMPONENTS;
  return {
    ...COMPONENTS,
    a: (props) => {
      const reference = linkReference(props.href);
      if (reference === undefined) {
        return <Link {...props} />;
      }
      return (
        <PathLink reference={reference} onOpen={onOpen}>
          {props.children}
        </PathLink>
      );
    },
    code: (props) => {
      const { children, className } = props;
      const reference =
        typeof children === "string" && !children.includes("\n") && className === undefined
          ? pathReference(children)
          : undefined;
      if (reference === undefined) {
        return <CodeElement {...props} />;
      }
      return (
        <PathLink reference={reference} onOpen={onOpen}>
          <code>{children}</code>
        </PathLink>
      );
    },
  };
}

export interface MarkdownProps extends Omit<HTMLAttributes<HTMLDivElement>, "children"> {
  /** Markdown source (CommonMark + GFM). */
  text: string;
  /** Opens a file path the text names (inline code or a relative link, UI-116); without it paths stay text. */
  onOpenPath?: OpenPath | undefined;
}

/**
 * Renders Markdown as prose (serif). Raw HTML in the source never becomes DOM: react-markdown without `rehype-raw`
 * drops it, and its URL filter removes `javascript:` links. Remote images and code-language classes are kept (UI-040).
 */
export function Markdown({ text, className, onOpenPath, ...props }: MarkdownProps) {
  const components = useMemo(() => (onOpenPath === undefined ? COMPONENTS : withPaths(onOpenPath)), [onOpenPath]);
  return (
    <div {...props} className={cx("omfp-prose omfp-markdown", className)}>
      <ReactMarkdown remarkPlugins={REMARK_PLUGINS} components={components}>
        {text}
      </ReactMarkdown>
    </div>
  );
}

export interface CodeProps extends Omit<HTMLAttributes<HTMLElement>, "children"> {
  code: string;
  /** A `<pre>` block (keeps whitespace, scrolls horizontally) instead of inline code. */
  block?: boolean;
  /** Highlights a block as this language (a name, alias or file extension, UI-090); plain text when unknown. */
  language?: string | undefined;
}

/** Monospace code (UI-166), inline or as a block; a block names its language in its top-right corner
 * (`data-lang`, drawn by kit.css). */
export function Code({ code, block = false, language, className, ...props }: CodeProps) {
  if (block) {
    return (
      <pre {...props} className={cx("omfp-code omfp-code--block", className)} data-lang={language}>
        <code>
          <Highlighted code={code} language={language} />
        </code>
      </pre>
    );
  }
  return (
    <code {...props} className={cx("omfp-code", className)}>
      {code}
    </code>
  );
}
