//! Markdown → HTML with pulldown-cmark, sanitized with ammonia before it reaches the DOM (UI-040): raw HTML from the
//! model never executes (no scripts, event handlers, `javascript:` URLs, styles or forms survive).

use pulldown_cmark::{Options, Parser, html};

/// Sanitized HTML for `markdown`.
#[must_use]
pub fn to_safe_html(markdown: &str) -> String {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES;
    let mut unsafe_html = String::with_capacity(markdown.len() * 3 / 2);
    html::push_html(&mut unsafe_html, Parser::new_ext(markdown, options));
    ammonia::Builder::default()
        // The fence language (`language-rust`), for highlighting later (UI-090).
        .add_tag_attributes("code", &["class"])
        .link_rel(Some("noopener noreferrer nofollow"))
        .set_tag_attribute_value("a", "target", "_blank")
        .clean(&unsafe_html)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    // spec: UI-040
    #[test]
    fn markdown_renders_to_html() {
        assert_eq!(
            to_safe_html("# Title\n\nSome *em* and `code`.\n\n- a\n- b"),
            "<h1>Title</h1>\n<p>Some <em>em</em> and <code>code</code>.</p>\n<ul>\n<li>a</li>\n<li>b</li>\n</ul>\n"
        );
        assert_eq!(
            to_safe_html("```rust\nfn main() {}\n```"),
            "<pre><code class=\"language-rust\">fn main() {}\n</code></pre>\n"
        );
    }

    // spec: UI-040
    #[test]
    fn raw_html_from_the_model_cannot_execute() {
        let html = to_safe_html(
            "<script>alert(1)</script>\n\n<img src=x onerror=\"alert(2)\">\n\n[x](javascript:alert(3))\n\n\
             <a href=\"https://example.test\" onclick=\"alert(4)\">ok</a>\n\n<iframe src=\"https://evil.test\"></iframe>",
        );
        for forbidden in [
            "<script",
            "alert(1)",
            "onerror",
            "javascript:",
            "onclick",
            "<iframe",
        ] {
            assert!(!html.contains(forbidden), "{forbidden} in {html}");
        }
        assert!(
            html.contains("rel=\"noopener noreferrer nofollow\""),
            "{html}"
        );
    }
}
