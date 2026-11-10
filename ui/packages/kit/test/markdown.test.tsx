import { afterEach, describe, expect, test } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import { Code, Markdown } from "../src/index.ts";

afterEach(cleanup);

function renderMarkdown(text: string): HTMLElement {
  render(<Markdown text={text} data-testid="md" />);
  return screen.getByTestId("md");
}

describe("Markdown", () => {
  // spec: UI-040
  test.each([
    ["script", "Hello <script>window.pwned = true</script> world", "script"],
    ["img onerror", 'Look <img src="x" onerror="window.pwned = true"> here', "img"],
    ["iframe", '<iframe src="https://example.com/"></iframe>', "iframe"],
    ["block html", '<div onclick="window.pwned = true">\n\n*text*\n\n</div>', "[onclick]"],
    ["inline style", '<span style="color: red">red</span>', "span"],
  ])("raw HTML (%s) never becomes DOM", (_name, text, selector) => {
    const root = renderMarkdown(text);
    expect(root.querySelector(selector)).toBeNull();
    expect(root.querySelector("[onerror], [onclick], script, iframe")).toBeNull();
    expect(Reflect.get(window, "pwned")).toBeUndefined();
  });

  // spec: UI-040
  test("javascript: links lose their URL", () => {
    const root = renderMarkdown("[click](javascript:alert(1))");
    expect(root.querySelector("a")?.getAttribute("href") ?? "").not.toContain("javascript:");
  });

  // spec: UI-040
  test("remote images load and code-language class names are kept", () => {
    const root = renderMarkdown("![chart](https://example.com/chart.png)\n\n```rust\nfn main() {}\n```");
    const image = root.querySelector("img");
    expect(image?.getAttribute("src")).toBe("https://example.com/chart.png");
    expect(image?.getAttribute("alt")).toBe("chart");
    expect(root.querySelector("pre code")?.className).toContain("language-rust");
  });

  // spec: UI-040
  test("links open in a new tab without a referrer", () => {
    const root = renderMarkdown("See [docs](https://example.com/docs).");
    const link = root.querySelector("a");
    expect(link?.getAttribute("href")).toBe("https://example.com/docs");
    expect(link?.getAttribute("target")).toBe("_blank");
    expect(link?.getAttribute("rel")).toBe("noreferrer");
  });

  // spec: UI-040
  test("GitHub-flavoured Markdown renders tables, task lists and strikethrough", () => {
    const root = renderMarkdown("| a | b |\n| - | - |\n| 1 | 2 |\n\n- [x] done\n\n~~old~~ https://example.com");
    expect(root.querySelectorAll("table td")).toHaveLength(2);
    expect(root.querySelector('input[type="checkbox"]')?.hasAttribute("checked")).toBe(true);
    expect(root.querySelector("del")?.textContent).toBe("old");
    expect(root.querySelector('a[href="https://example.com"]')).not.toBeNull();
  });

  // spec: UI-094
  test("prose gets the prose class, code blocks the code class", () => {
    const root = renderMarkdown("Some *prose* and `inline`.\n\n```\nblock\n```");
    expect(root.classList.contains("omfp-prose")).toBe(true);
    expect(root.querySelector("pre")?.classList.contains("omfp-code")).toBe(true);
    expect(root.querySelector("p code")?.textContent).toBe("inline");
  });
});

describe("Code", () => {
  // spec: UI-094
  test("renders inline code or a pre block with the code class", () => {
    render(
      <>
        <Code code="ls -la" data-testid="inline" />
        <Code code={"a\n  b"} block data-testid="block" />
      </>,
    );
    const inline = screen.getByTestId("inline");
    expect(inline.tagName).toBe("CODE");
    expect(inline.classList.contains("omfp-code")).toBe(true);
    const block = screen.getByTestId("block");
    expect(block.tagName).toBe("PRE");
    expect(block.querySelector("code")?.textContent).toBe("a\n  b");
  });
});
