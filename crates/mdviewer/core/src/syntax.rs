//! What each part of a Markdown file's text is, for the raw view to color: its headings, its
//! emphasis, its code, its links and where they go, its lists' and quotes' markers, its code
//! blocks and its front matter, and Markdown's own punctuation around them (a heading's `#`s,
//! `**`, backticks, a link's brackets, a fence, a table's pipes), which the raw view quiets so
//! the words stand out.
//!
//! pulldown-cmark reads the file as the outline does ([`crate::summarize`]), and its offsets say
//! where each part is. A part's punctuation is what its range holds outside its content: a
//! strong's `**` lie before its first word and after its last, a link's `](…)` after its text.
//! The marks are in the raw view's terms, a line and a column in UTF-16 code units, as the
//! editor counts them; they may nest (a strong in a heading) and come in the order they start.

use pulldown_cmark::{CodeBlockKind, Event, LinkType, Options, Parser, Tag, TagEnd};
use std::ops::Range;

/// What a part of a Markdown file is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkKind {
    /// Markdown's own punctuation: a heading's `#`s, `**` and `_`, backticks, a link's brackets,
    /// a fence, a table's pipes and its rule under the head.
    Punctuation,
    /// A heading's text.
    Heading,
    /// Strong text, between its `**`.
    Strong,
    /// Emphasized text, between its `*` or `_`.
    Emphasis,
    /// Struck-through text, between its `~~`.
    Strikethrough,
    /// Code in text (or a formula), between its backticks.
    Code,
    /// A link's text, or a picture's.
    Link,
    /// Where a link or a picture goes.
    Url,
    /// A list item's marker, a task's box, a quote's `>`, a thematic break, a footnote's label.
    Marker,
    /// A quote's text, after its `>`.
    Quote,
    /// A code block's lines, whole: a band behind them.
    CodeBlock,
    /// The language a fence names.
    Language,
    /// The front matter, `---` to `---` at the top: the file's data.
    FrontMatter,
}

/// A place in a file's text: a line and the UTF-16 code units before it on its line, both from 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Place {
    /// The line, from 0.
    pub line: u32,
    /// The UTF-16 code units before it on its line.
    pub column: u32,
}

/// A part of a file's text and what it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mark {
    /// What it is.
    pub kind: MarkKind,
    /// Where it starts.
    pub start: Place,
    /// Where it ends, not included.
    pub end: Place,
}

/// A span (strong, emphasis, struck through, a link, a picture) while its content is read: its
/// whole range, and the content's so far.
struct Open {
    tag: Span,
    range: Range<usize>,
    content: Option<Range<usize>>,
}

/// The spans whose punctuation lies around their content.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Span {
    Strong,
    Emphasis,
    Strikethrough,
    Heading,
    Link(bool),
    Image,
}

/// Every part of Markdown `text` that the raw view colors, in the order they start.
pub fn marks(text: &str) -> Vec<Mark> {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | Options::ENABLE_MATH
        | Options::ENABLE_GFM;
    let mut found: Vec<(MarkKind, Range<usize>)> = Vec::new();
    let mut open: Vec<Open> = Vec::new();

    for (event, range) in Parser::new_ext(text, options).into_offset_iter() {
        // Whatever a span holds widens its content, the spans around it too.
        let widen = |open: &mut Vec<Open>, range: &Range<usize>| {
            for span in open.iter_mut() {
                let content = span.content.get_or_insert(range.clone());
                content.start = content.start.min(range.start);
                content.end = content.end.max(range.end);
            }
        };
        match event {
            Event::Start(tag) => {
                let span = match tag {
                    Tag::Strong => Some(Span::Strong),
                    Tag::Emphasis => Some(Span::Emphasis),
                    Tag::Strikethrough => Some(Span::Strikethrough),
                    Tag::Heading { .. } => Some(Span::Heading),
                    Tag::Link { link_type, .. } => Some(Span::Link(matches!(
                        link_type,
                        LinkType::Autolink | LinkType::Email
                    ))),
                    Tag::Image { .. } => Some(Span::Image),
                    Tag::Item => {
                        found.extend(list_marker(text, range.start));
                        None
                    }
                    Tag::BlockQuote(_) => {
                        quote(text, &range, &mut found);
                        None
                    }
                    Tag::CodeBlock(kind) => {
                        code_block(
                            text,
                            &range,
                            matches!(kind, CodeBlockKind::Fenced(_)),
                            &mut found,
                        );
                        None
                    }
                    Tag::Table(_) => {
                        table(text, &range, &mut found);
                        None
                    }
                    Tag::MetadataBlock(_) => {
                        found.push((MarkKind::FrontMatter, range.clone()));
                        None
                    }
                    Tag::FootnoteDefinition(_) => {
                        found.extend(label(text, range.start));
                        None
                    }
                    _ => None,
                };
                if let Some(tag) = span {
                    // A span inside another is part of its content.
                    widen(&mut open, &range);
                    open.push(Open {
                        tag,
                        range,
                        content: None,
                    });
                }
            }
            Event::End(
                TagEnd::Strong
                | TagEnd::Emphasis
                | TagEnd::Strikethrough
                | TagEnd::Heading(_)
                | TagEnd::Link
                | TagEnd::Image,
            ) => {
                if let Some(done) = open.pop() {
                    close(text, done, &mut found);
                }
            }
            Event::Code(_) | Event::InlineMath(_) | Event::DisplayMath(_) => {
                widen(&mut open, &range);
                found.extend(fenced_inline(text, range));
            }
            Event::Text(_) | Event::Html(_) | Event::InlineHtml(_) => widen(&mut open, &range),
            Event::TaskListMarker(_) | Event::Rule | Event::FootnoteReference(_) => {
                widen(&mut open, &range);
                found.push((MarkKind::Marker, trimmed(text, range)));
            }
            _ => {}
        }
    }

    let places = Places::new(text);
    let mut marks: Vec<Mark> = found
        .into_iter()
        .filter(|(_, range)| range.start < range.end)
        .map(|(kind, range)| Mark {
            kind,
            start: places.of(range.start),
            end: places.of(range.end),
        })
        .collect();
    // By where they start, the one around another first.
    marks.sort_by_key(|mark| (mark.start, std::cmp::Reverse(mark.end)));
    marks
}

/// A finished span's marks: its content, and the punctuation around it.
fn close(text: &str, done: Open, found: &mut Vec<(MarkKind, Range<usize>)>) {
    let range = done.range;
    let content = done.content.unwrap_or(range.start..range.start);
    match done.tag {
        Span::Heading => {
            let line = trimmed(text, range.clone());
            if text[line.clone()].starts_with('#') {
                // ATX: the `#`s before the text, and any closing ones after it.
                let hashes =
                    text[line.clone()].len() - text[line.clone()].trim_start_matches('#').len();
                found.push((MarkKind::Punctuation, line.start..line.start + hashes));
                found.push((
                    MarkKind::Punctuation,
                    trimmed(text, content.end.min(line.end)..line.end),
                ));
            } else {
                // Setext: the `===` or `---` line under the text.
                let under = text[content.end..line.end]
                    .find(|c: char| !c.is_whitespace())
                    .map_or(line.end, |at| content.end + at);
                found.push((MarkKind::Punctuation, under..line.end));
            }
            found.push((MarkKind::Heading, content));
        }
        Span::Link(true) => {
            // `<https://…>`: the address is the text.
            found.push((MarkKind::Punctuation, range.start..content.start));
            found.push((MarkKind::Url, content.clone()));
            found.push((MarkKind::Punctuation, content.end..range.end));
        }
        Span::Link(false) | Span::Image => {
            // `[text](url "title")`, `[text][label]`, `[text]`, or a picture's `![…](…)`.
            let opening = if done.tag == Span::Image { 2 } else { 1 };
            let text_start = (range.start + opening).min(range.end);
            let content = if content.start < text_start {
                text_start..text_start
            } else {
                content
            };
            found.push((MarkKind::Punctuation, range.start..text_start));
            found.push((MarkKind::Link, content.clone()));
            let tail = &text[content.end..range.end];
            if let Some(after) = tail.strip_prefix("](").or_else(|| tail.strip_prefix("][")) {
                let url_start = range.end - after.len();
                let url_end = (range.end - 1).max(url_start);
                found.push((MarkKind::Punctuation, content.end..url_start));
                found.push((MarkKind::Url, url_start..url_end));
                found.push((MarkKind::Punctuation, url_end..range.end));
            } else {
                found.push((MarkKind::Punctuation, content.end..range.end));
            }
        }
        Span::Strong | Span::Emphasis | Span::Strikethrough => {
            let kind = match done.tag {
                Span::Strong => MarkKind::Strong,
                Span::Emphasis => MarkKind::Emphasis,
                _ => MarkKind::Strikethrough,
            };
            found.push((
                MarkKind::Punctuation,
                range.start..content.start.max(range.start),
            ));
            found.push((kind, content.clone()));
            found.push((MarkKind::Punctuation, content.end.min(range.end)..range.end));
        }
    }
}

/// Code in text or a formula: its backticks or dollar signs, and what is between them.
fn fenced_inline(text: &str, range: Range<usize>) -> Vec<(MarkKind, Range<usize>)> {
    let source = &text[range.clone()];
    let Some(fence) = source.chars().next().filter(|c| *c == '`' || *c == '$') else {
        return vec![(MarkKind::Code, range)];
    };
    let open = source.len() - source.trim_start_matches(fence).len();
    let close = source.len() - source.trim_end_matches(fence).len();
    if open + close >= source.len() {
        return vec![(MarkKind::Punctuation, range)];
    }
    vec![
        (MarkKind::Punctuation, range.start..range.start + open),
        (MarkKind::Code, range.start + open..range.end - close),
        (MarkKind::Punctuation, range.end - close..range.end),
    ]
}

/// A list item's marker (`-`, `*`, `+`, `1.`, `1)`) where the item starts.
fn list_marker(text: &str, start: usize) -> Option<(MarkKind, Range<usize>)> {
    let rest = &text[start..];
    let indent = rest.len() - rest.trim_start_matches([' ', '\t']).len();
    let at = start + indent;
    let rest = &text[at..];
    let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    let length = match rest[digits..].chars().next() {
        Some('.' | ')') if digits > 0 => digits + 1,
        Some('-' | '*' | '+') if digits == 0 => 1,
        _ => return None,
    };
    Some((MarkKind::Marker, at..at + length))
}

/// A footnote definition's label, `[^1]:`, where it starts.
fn label(text: &str, start: usize) -> Option<(MarkKind, Range<usize>)> {
    let rest = &text[start..];
    let indent = rest.len() - rest.trim_start_matches(' ').len();
    let end = rest.find("]:")? + 2;
    Some((MarkKind::Marker, start + indent..start + end))
}

/// A quote's `>`s at the start of each of its lines, nested ones too, and its text after them.
fn quote(text: &str, range: &Range<usize>, found: &mut Vec<(MarkKind, Range<usize>)>) {
    for line in lines(text, range.clone()) {
        let mut at = line.start;
        loop {
            let rest = &text[at..line.end];
            let spaces = rest.len() - rest.trim_start_matches(' ').len();
            if spaces > 3 || !rest[spaces..].starts_with('>') {
                break;
            }
            found.push((MarkKind::Marker, at + spaces..at + spaces + 1));
            at += spaces + 1;
        }
        if at > line.start {
            found.push((MarkKind::Quote, trimmed(text, at..line.end)));
        }
    }
}

/// A code block: a band behind its lines, and a fenced one's fences and the language it names.
fn code_block(
    text: &str,
    range: &Range<usize>,
    fenced: bool,
    found: &mut Vec<(MarkKind, Range<usize>)>,
) {
    let lines = lines(text, range.clone());
    let (Some(first), Some(last)) = (lines.first().cloned(), lines.last().cloned()) else {
        return;
    };
    found.push((MarkKind::CodeBlock, first.start..last.end));
    if !fenced {
        return;
    }
    let fence_of = |line: &Range<usize>| {
        let source = &text[line.clone()];
        let indent = source.len() - source.trim_start_matches(' ').len();
        let fence = source[indent..]
            .chars()
            .next()
            .filter(|c| *c == '`' || *c == '~')?;
        let length = source[indent..].len() - source[indent..].trim_start_matches(fence).len();
        (length >= 3).then_some(line.start + indent..line.start + indent + length)
    };
    if let Some(fence) = fence_of(&first) {
        found.push((MarkKind::Punctuation, fence.clone()));
        found.push((MarkKind::Language, trimmed(text, fence.end..first.end)));
    }
    if lines.len() > 1
        && let Some(fence) = fence_of(&last)
    {
        found.push((MarkKind::Punctuation, fence));
    }
}

/// A table's pipes, and the rule under its head.
fn table(text: &str, range: &Range<usize>, found: &mut Vec<(MarkKind, Range<usize>)>) {
    for (index, line) in lines(text, range.clone()).into_iter().enumerate() {
        if index == 1 {
            found.push((MarkKind::Punctuation, trimmed(text, line)));
            continue;
        }
        let mut escaped = false;
        for (at, c) in text[line.clone()].char_indices() {
            if c == '|' && !escaped {
                found.push((MarkKind::Punctuation, line.start + at..line.start + at + 1));
            }
            escaped = c == '\\' && !escaped;
        }
    }
}

/// The lines `range` covers, each without its line break.
fn lines(text: &str, range: Range<usize>) -> Vec<Range<usize>> {
    let mut lines = Vec::new();
    let mut start = range.start;
    while start < range.end {
        let end = text[start..range.end]
            .find('\n')
            .map_or(range.end, |at| start + at);
        lines.push(start..end.min(range.end));
        start = end + 1;
    }
    lines
}

/// `range` without the whitespace and line breaks at its ends.
fn trimmed(text: &str, range: Range<usize>) -> Range<usize> {
    let source = &text[range.clone()];
    let start = range.start + (source.len() - source.trim_start().len());
    let end = range.end - (source.len() - source.trim_end().len());
    start..end.max(start)
}

/// Where each byte offset of a text is, as a line and a UTF-16 column.
struct Places<'a> {
    text: &'a str,
    starts: Vec<usize>,
}

impl<'a> Places<'a> {
    fn new(text: &'a str) -> Self {
        let starts = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(at, _)| at + 1))
            .collect();
        Self { text, starts }
    }

    fn of(&self, offset: usize) -> Place {
        let line = self.starts.partition_point(|&start| start <= offset) - 1;
        let column = self.text[self.starts[line]..offset].encode_utf16().count();
        Place {
            line: line as u32,
            column: column as u32,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The marks of `text`, each as its kind and the text it covers.
    fn parts(text: &str) -> Vec<(MarkKind, String)> {
        let lines: Vec<&str> = text.split('\n').collect();
        marks(text)
            .into_iter()
            .map(|mark| {
                let covered = if mark.start.line == mark.end.line {
                    let line: Vec<u16> = lines[mark.start.line as usize].encode_utf16().collect();
                    String::from_utf16_lossy(
                        &line[mark.start.column as usize..mark.end.column as usize],
                    )
                } else {
                    format!("lines {} to {}", mark.start.line, mark.end.line)
                };
                (mark.kind, covered)
            })
            .collect()
    }

    use MarkKind::*;

    #[test]
    fn a_heading_s_hashes_are_punctuation_and_its_text_the_heading() {
        assert_eq!(
            parts("## Install *now*\n"),
            [
                (Punctuation, "##".into()),
                (Heading, "Install *now*".into()),
                (Punctuation, "*".into()),
                (Emphasis, "now".into()),
                (Punctuation, "*".into()),
            ]
        );
    }

    #[test]
    fn a_setext_heading_s_rule_is_punctuation() {
        assert_eq!(
            parts("Title\n=====\n"),
            [(Heading, "Title".into()), (Punctuation, "=====".into())]
        );
    }

    #[test]
    fn strong_and_struck_text_keep_their_markers_apart() {
        assert_eq!(
            parts("a **bold** and ~~gone~~"),
            [
                (Punctuation, "**".into()),
                (Strong, "bold".into()),
                (Punctuation, "**".into()),
                (Punctuation, "~~".into()),
                (Strikethrough, "gone".into()),
                (Punctuation, "~~".into()),
            ]
        );
    }

    #[test]
    fn code_in_text_is_between_its_backticks() {
        assert_eq!(
            parts("run `cargo test` now"),
            [
                (Punctuation, "`".into()),
                (Code, "cargo test".into()),
                (Punctuation, "`".into()),
            ]
        );
    }

    #[test]
    fn a_link_s_text_and_address_are_apart_from_its_brackets() {
        assert_eq!(
            parts("see [the README](docs/README.md) here"),
            [
                (Punctuation, "[".into()),
                (Link, "the README".into()),
                (Punctuation, "](".into()),
                (Url, "docs/README.md".into()),
                (Punctuation, ")".into()),
            ]
        );
        assert_eq!(
            parts("<https://example.com>"),
            [
                (Punctuation, "<".into()),
                (Url, "https://example.com".into()),
                (Punctuation, ">".into()),
            ]
        );
    }

    #[test]
    fn a_picture_is_a_link_with_its_bang() {
        assert_eq!(
            parts("![a cat](cat.png)"),
            [
                (Punctuation, "![".into()),
                (Link, "a cat".into()),
                (Punctuation, "](".into()),
                (Url, "cat.png".into()),
                (Punctuation, ")".into()),
            ]
        );
    }

    #[test]
    fn lists_quotes_and_tasks_have_markers() {
        assert_eq!(
            parts("- one\n2. two\n- [x] done\n"),
            [
                (Marker, "-".into()),
                (Marker, "2.".into()),
                (Marker, "-".into()),
                (Marker, "[x]".into()),
            ]
        );
        assert_eq!(
            parts("> pinned to *0.4.0*\n"),
            [
                (Marker, ">".into()),
                (Quote, "pinned to *0.4.0*".into()),
                (Punctuation, "*".into()),
                (Emphasis, "0.4.0".into()),
                (Punctuation, "*".into()),
            ]
        );
    }

    #[test]
    fn a_fenced_block_is_a_band_with_its_fences_and_language() {
        assert_eq!(
            parts("```rust\nlet a = 1;\n```\n"),
            [
                (CodeBlock, "lines 0 to 2".into()),
                (Punctuation, "```".into()),
                (Language, "rust".into()),
                (Punctuation, "```".into()),
            ]
        );
    }

    #[test]
    fn front_matter_and_tables_are_marked() {
        assert_eq!(
            parts("---\ntitle: Notes\n---\n\n# Notes\n")[0],
            (FrontMatter, "lines 0 to 2".into())
        );
        assert_eq!(
            parts("| a | b |\n|---|---|\n| 1 | 2 |\n")
                .iter()
                .filter(|(kind, _)| *kind == Punctuation)
                .count(),
            7
        );
    }

    #[test]
    fn columns_count_utf16_units() {
        let found = marks("é😀 `x`");
        let code = found
            .iter()
            .find(|mark| mark.kind == Code)
            .expect("the code");
        // `é` is one unit and the emoji two, then a space and the backtick.
        assert_eq!(code.start, Place { line: 0, column: 5 });
    }
}
