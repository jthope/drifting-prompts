//! A file as the rendered view shows it: an HTML fragment for the page.
//!
//! comrak reads the Markdown as GitHub does (tables, task lists, footnotes, alerts, autolinks,
//! strikethrough), with front matter and `$…$`, `$$…$$` and ` ```math ` formulas, gives headings
//! GitHub's ids, and puts `data-sourcepos="3:1-5:10"` (lines from 1) on every block. That is how the
//! outline finds a heading on the page: the outline is pulldown-cmark's, so the two meet by line,
//! never by count. Front matter becomes a block of its own, `<pre class="front-matter">`; formulas
//! become MathML ([`math`]), and code blocks carry their tokens' classes ([`code`]).
//!
//! Raw HTML in a file comes through, and then the whole fragment goes through ammonia, which keeps
//! only harmless tags and attributes: no scripts, styles, frames, forms or event handlers. The
//! page's Content-Security-Policy is the second guard.
//!
//! A picture with a relative `src` gets its path in the project in its place (`docs/shots/a.png`),
//! and one that would leave the project loses its `src`. The page asks the app for each picture
//! whose `src` has no scheme ([`crate::files::picture`]).

mod code;
mod math;

use std::{borrow::Cow, fmt};

use comrak::{
    Arena, Options, format_html_with_plugins,
    nodes::{AstNode, NodeHtmlBlock, NodeValue},
    options::Plugins,
    parse_document,
};
use percent_encoding::percent_decode_str;

/// The HTML of Markdown `text`, file `file` of a project (`docs/setup.md`), as the page shows it.
pub fn render(text: &str, file: &str) -> String {
    let options = options();
    let arena = Arena::new();
    let root = parse_document(&arena, text, &options);
    let mut html = String::new();
    if let Some(first) = root.first_child() {
        let front = match &first.data().value {
            NodeValue::FrontMatter(front) => Some(front.clone()),
            _ => None,
        };
        if let Some(front) = front {
            html.push_str(&front_matter(&front));
            first.detach();
        }
    }
    for node in root.descendants() {
        formula(node);
    }
    let mut plugins = Plugins::default();
    plugins.render.codefence_syntax_highlighter = Some(&code::Highlighter);
    // Writing to a `String` can't fail.
    let _ = format_html_with_plugins(root, &options, &mut html, &plugins);
    cleaner(folder_of(file)).clean(&html).to_string()
}

/// A formula, or a ` ```math ` block, as its MathML.
fn formula<'a>(node: &'a AstNode<'a>) {
    let replaced = match &node.data().value {
        NodeValue::Math(math) => Some(NodeValue::HtmlInline(format!(
            "<span class=\"math\">{}</span>",
            math::mathml(&math.literal, math.display_math)
        ))),
        NodeValue::MathBlock(math) => Some(display(&math.literal, node)),
        NodeValue::CodeBlock(block) if block.info.split_whitespace().next() == Some("math") => {
            Some(display(&block.literal, node))
        }
        _ => None,
    };
    if let Some(value) = replaced {
        node.data_mut().value = value;
    }
}

/// A formula on a line of its own, as a block that keeps the lines it came from.
fn display<'a>(tex: &str, node: &'a AstNode<'a>) -> NodeValue {
    let mut block = NodeHtmlBlock::default();
    block.literal = format!(
        "<div class=\"math\" data-sourcepos=\"{}\">{}</div>\n",
        node.data().sourcepos,
        math::mathml(tex, true)
    );
    NodeValue::HtmlBlock(block)
}

fn options() -> Options<'static> {
    let mut options = Options::default();
    let extension = &mut options.extension;
    extension.strikethrough = true;
    extension.table = true;
    extension.autolink = true;
    extension.tasklist = true;
    extension.footnotes = true;
    extension.alerts = true;
    extension.math_dollars = true;
    extension.math_dollars_block = true;
    extension.header_id_prefix = Some(String::new());
    extension.front_matter_delimiter = Some("---".to_owned());
    let render = &mut options.render;
    render.sourcepos = true;
    render.tasklist_classes = true;
    render.github_pre_lang = true;
    // ammonia makes it harmless.
    render.r#unsafe = true;
    options
}

/// The front matter as written, delimiters and all, on the lines it starts on.
fn front_matter(front: &str) -> String {
    let front = front.trim_end_matches(['\r', '\n']);
    let lines = front.lines().count().max(1);
    let mut html = format!("<pre class=\"front-matter\" data-sourcepos=\"1:1-{lines}:1\"><code>");
    let _ = escape(&mut html, front);
    html.push_str("</code></pre>\n");
    html
}

/// `text` as HTML's text, or an attribute's value.
fn escape(output: &mut dyn fmt::Write, text: &str) -> fmt::Result {
    let mut rest = text;
    while let Some(at) = rest.find(['&', '<', '>', '"']) {
        output.write_str(&rest[..at])?;
        output.write_str(match rest.as_bytes()[at] {
            b'&' => "&amp;",
            b'<' => "&lt;",
            b'>' => "&gt;",
            _ => "&quot;",
        })?;
        rest = &rest[at + 1..];
    }
    output.write_str(rest)
}

fn cleaner(folder: &str) -> ammonia::Builder<'static> {
    let folder = folder.to_owned();
    let mut cleaner = ammonia::Builder::default();
    cleaner
        .add_tags(["input", "section"])
        .add_generic_attributes(["id", "class", "align", "dir", "aria-label"])
        .add_generic_attribute_prefixes(["data-"])
        .add_tag_attributes("input", ["type", "checked", "disabled"])
        .add_tag_attributes("details", ["open"])
        .add_tags(math::TAGS.iter().copied());
    for tag in math::TAGS {
        cleaner.add_tag_attributes(tag, math::ATTRIBUTES.iter().copied());
    }
    cleaner.attribute_filter(
        move |element, attribute, value| match (element, attribute) {
            ("input", "type") => (value == "checkbox").then_some(Cow::Borrowed(value)),
            ("img", "src") => picture_source(&folder, value),
            _ => Some(Cow::Borrowed(value)),
        },
    );
    cleaner
}

fn folder_of(file: &str) -> &str {
    file.rsplit_once('/').map_or("", |(folder, _)| folder)
}

fn has_scheme(url: &str) -> bool {
    let Some((scheme, _)) = url.split_once(':') else {
        return false;
    };
    let mut chars = scheme.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && chars.all(|each| each.is_ascii_alphanumeric() || matches!(each, '+' | '.' | '-'))
}

/// A picture's `src` as the page gets it: as written with a scheme, else its path in the project,
/// or `None` when it would leave the project.
fn picture_source<'u>(folder: &str, src: &'u str) -> Option<Cow<'u, str>> {
    if src.is_empty() || has_scheme(src) {
        return Some(Cow::Borrowed(src));
    }
    let path = src.split(['?', '#']).next().unwrap_or_default();
    let path = percent_decode_str(path).decode_utf8_lossy();
    resolve(folder, &path).map(Cow::Owned)
}

/// `relative` from `folder` in the project, with `/` between parts; `None` when it leaves the
/// project.
fn resolve(folder: &str, relative: &str) -> Option<String> {
    let mut parts: Vec<&str> = if relative.starts_with(['/', '\\']) {
        Vec::new()
    } else {
        folder.split('/').filter(|part| !part.is_empty()).collect()
    };
    for part in relative.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(part),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn starts_on(html: &str, tag: &str) -> Vec<usize> {
        html.match_indices(&format!("<{tag} "))
            .filter_map(|(at, _)| {
                let rest = &html[at..];
                let end = rest.find('>')?;
                let attribute = rest[..end].split("data-sourcepos=\"").nth(1)?;
                attribute.split(':').next()?.parse().ok()
            })
            .collect()
    }

    #[test]
    fn every_block_says_which_line_it_starts_on() {
        let text = "# Title\n\nSome words.\n\n- one\n- two\n\n| a | b |\n| - | - |\n| 1 | 2 |\n\n```rust\nfn main() {}\n```\n\n> A quote.\n";
        let html = render(text, "README.md");
        assert_eq!(starts_on(&html, "h1"), [1]);
        assert_eq!(starts_on(&html, "p"), [3, 16]);
        assert_eq!(starts_on(&html, "ul"), [5]);
        assert_eq!(starts_on(&html, "table"), [8]);
        assert_eq!(starts_on(&html, "pre"), [12]);
        assert_eq!(starts_on(&html, "blockquote"), [16]);
    }

    #[test]
    fn headings_get_github_ids() {
        let html = render("# Getting Started\n\n## What's new?\n", "README.md");
        assert!(html.contains("id=\"getting-started\""), "{html}");
        assert!(html.contains("id=\"whats-new\""), "{html}");
    }

    #[test]
    fn a_setext_heading_starts_where_the_outline_says() {
        let text = "Intro\n\nTitle\n=====\n\nWords.\n";
        let outline = crate::summarize(text);
        let html = render(text, "README.md");
        assert_eq!(outline.headings[0].line, 2);
        assert_eq!(starts_on(&html, "h1"), [3]);
    }

    #[test]
    fn a_heading_in_a_quote_is_not_in_the_outline_but_is_on_the_page() {
        let text = "# One\n\n> ## Quoted\n\n# Two\n";
        let outline = crate::summarize(text);
        let lines: Vec<u32> = outline.headings.iter().map(|each| each.line).collect();
        assert_eq!(lines, [0, 4]);
        let html = render(text, "README.md");
        assert_eq!(starts_on(&html, "h1"), [1, 5]);
        assert_eq!(starts_on(&html, "h2"), [3]);
    }

    #[test]
    fn front_matter_is_a_block_of_its_own_and_the_headings_keep_their_lines() {
        let text = "---\ntitle: Notes\n---\n\n# Notes\n";
        let html = render(text, "README.md");
        assert!(
            html.starts_with(
                "<pre class=\"front-matter\" data-sourcepos=\"1:1-3:1\"><code>---\ntitle: Notes\n---</code></pre>"
            ),
            "{html}"
        );
        assert_eq!(crate::summarize(text).headings[0].line, 4);
        assert_eq!(starts_on(&html, "h1"), [5]);
    }

    #[test]
    fn scripts_styles_and_handlers_in_a_file_are_dropped() {
        let text = "<script>alert(1)</script>\n\n<style>p { color: red }</style>\n\n<p onclick=\"alert(2)\" style=\"color: red\">Hi</p>\n\n<iframe src=\"https://example.com\"></iframe>\n\n[link](javascript:alert(3))\n";
        let html = render(text, "README.md");
        for gone in [
            "<script",
            "alert(1)",
            "<style",
            "onclick",
            "style=",
            "<iframe",
            "javascript:",
        ] {
            assert!(!html.contains(gone), "{gone} in {html}");
        }
        assert!(html.contains(">Hi</p>"), "{html}");
    }

    #[test]
    fn raw_html_a_readme_uses_stays() {
        let text = "<details open><summary>More</summary>\n\nHidden words.\n\n</details>\n\n<p align=\"center\"><kbd>Ctrl</kbd></p>\n";
        let html = render(text, "README.md");
        assert!(html.contains("<details open"), "{html}");
        assert!(html.contains("<summary>More</summary>"), "{html}");
        assert!(
            html.contains("<p align=\"center\"><kbd>Ctrl</kbd></p>"),
            "{html}"
        );
    }

    #[test]
    fn task_lists_keep_their_boxes() {
        let html = render("- [x] done\n- [ ] not yet\n", "README.md");
        assert!(html.contains("type=\"checkbox\""), "{html}");
        assert!(html.contains("checked"), "{html}");
        assert!(html.contains("task-list-item"), "{html}");
    }

    #[test]
    fn a_picture_gets_its_path_in_the_project() {
        let html = render(
            "![a](shots/one%20two.png) ![b](../logo.svg?v=2) ![c](https://example.com/c.png)",
            "docs/guide/setup.md",
        );
        assert!(
            html.contains("src=\"docs/guide/shots/one two.png\""),
            "{html}"
        );
        assert!(html.contains("src=\"docs/logo.svg\""), "{html}");
        assert!(html.contains("src=\"https://example.com/c.png\""), "{html}");
    }

    #[test]
    fn a_picture_leaving_the_project_is_refused() {
        let html = render(
            "![a](../../secret.png) <img src=\"../../x.png\">",
            "docs/setup.md",
        );
        assert!(!html.contains("secret"), "{html}");
        assert!(!html.contains("x.png"), "{html}");
        assert!(html.contains("alt=\"a\""), "{html}");
    }

    #[test]
    fn formulas_footnotes_and_alerts_come_through() {
        let text = "Euler: $e^{i\\pi} = -1$.[^1]\n\n> [!NOTE]\n> Mind this.\n\n[^1]: A note.\n";
        let html = render(text, "README.md");
        assert!(html.contains("<span class=\"math\"><math"), "{html}");
        assert!(html.contains("<msup>"), "{html}");
        assert!(html.contains("class=\"footnotes\""), "{html}");
        assert!(html.contains("markdown-alert-note"), "{html}");
    }

    #[test]
    fn a_formula_on_its_own_lines_is_a_block_that_keeps_its_lines() {
        let html = render(
            "Intro.\n\n$$\nx = 1\n$$\n\n```math\ny = 2\n```\n",
            "README.md",
        );
        assert!(
            html.contains("<div class=\"math\" data-sourcepos=\"3:1-5:2\"><math display=\"block\""),
            "{html}"
        );
        assert!(
            html.contains("<div class=\"math\" data-sourcepos=\"7:1-9:3\"><math display=\"block\""),
            "{html}"
        );
        // A formula's markup is the page's only MathML: the rest of the file keeps to ammonia's rules.
        let raw = render("<math><mi onclick=\"x()\">a</mi></math>", "README.md");
        assert!(!raw.contains("onclick"), "{raw}");
    }

    #[test]
    fn code_blocks_carry_their_tokens_and_keep_their_lines() {
        let html = render("```rust\nfn main() {}\n```\n", "README.md");
        assert!(
            html.contains("<pre data-sourcepos=\"1:1-3:3\" lang=\"rust\">"),
            "{html}"
        );
        assert!(
            html.contains("<span class=\"tok-keyword\">fn</span>"),
            "{html}"
        );
    }
}
