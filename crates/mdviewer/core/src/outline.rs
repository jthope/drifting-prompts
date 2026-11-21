//! A Markdown file's outline (its headings and their lines) and word count. Only top-level headings
//! count: the rendered view finds a heading's section by counting the page's own.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

/// A heading of a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Heading {
    /// Its level: 1 for `#`, up to 6.
    pub level: u8,
    /// Its plain text.
    pub title: String,
    /// The line it starts on, from 0.
    pub line: u32,
}

/// A file's outline and length.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    /// Its top-level headings, in order.
    pub headings: Vec<Heading>,
    /// How many words its text has, code and front matter left out.
    pub words: usize,
}

/// The outline and length of Markdown `text`.
pub fn summarize(text: &str) -> Summary {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | Options::ENABLE_MATH
        | Options::ENABLE_GFM;
    let line_starts: Vec<usize> = std::iter::once(0)
        .chain(text.match_indices('\n').map(|(at, _)| at + 1))
        .collect();
    let line_of =
        |offset: usize| (line_starts.partition_point(|&start| start <= offset) - 1) as u32;

    let mut summary = Summary::default();
    // A heading counts only with no block or span open around it.
    let mut open = 0usize;
    let mut heading: Option<Heading> = None;
    let mut aside = false;

    for (event, range) in Parser::new_ext(text, options).into_offset_iter() {
        match event {
            Event::Start(tag) => {
                match tag {
                    Tag::Heading { level, .. } if open == 0 => {
                        heading = Some(Heading {
                            level: level as u8,
                            title: String::new(),
                            line: line_of(range.start),
                        });
                    }
                    Tag::CodeBlock(_) | Tag::MetadataBlock(_) => aside = true,
                    _ => {}
                }
                open += 1;
            }
            Event::End(tag) => {
                open = open.saturating_sub(1);
                match tag {
                    TagEnd::Heading(_) if open == 0 => {
                        if let Some(mut done) = heading.take() {
                            done.title =
                                done.title.split_whitespace().collect::<Vec<_>>().join(" ");
                            summary.headings.push(done);
                        }
                    }
                    TagEnd::CodeBlock | TagEnd::MetadataBlock(_) => aside = false,
                    _ => {}
                }
            }
            Event::Text(words) | Event::Code(words) | Event::InlineMath(words) => {
                if let Some(heading) = heading.as_mut() {
                    heading.title.push_str(&words);
                }
                if !aside {
                    summary.words += words.split_whitespace().count();
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if let Some(heading) = heading.as_mut() {
                    heading.title.push(' ');
                }
            }
            _ => {}
        }
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outline(text: &str) -> Vec<(u8, String, u32)> {
        summarize(text)
            .headings
            .into_iter()
            .map(|heading| (heading.level, heading.title, heading.line))
            .collect()
    }

    fn h(level: u8, title: &str, line: u32) -> (u8, String, u32) {
        (level, title.to_owned(), line)
    }

    #[test]
    fn headings_come_with_their_level_and_line() {
        let text = "# Drifting notes\n\nIntro.\n\n## Install\n\n### Windows\nRun it.\n\n## Usage\n";
        assert_eq!(
            outline(text),
            [
                h(1, "Drifting notes", 0),
                h(2, "Install", 4),
                h(3, "Windows", 6),
                h(2, "Usage", 9),
            ]
        );
    }

    #[test]
    fn a_title_loses_its_markdown() {
        let text = "## Install *now* with `cargo` and [the guide](guide.md)\n";
        assert_eq!(
            outline(text),
            [h(2, "Install now with cargo and the guide", 0)]
        );
    }

    #[test]
    fn underlined_headings_count_from_their_text() {
        let text = "Title\n=====\n\nSubtitle\n--------\n\ntext\n\n---\n";
        assert_eq!(outline(text), [h(1, "Title", 0), h(2, "Subtitle", 3)]);
    }

    #[test]
    fn code_holds_no_headings_or_words() {
        let text = "# Real\n\n```sh\n# not a heading\necho words words words\n```\n\n    # indented code\n";
        let summary = summarize(text);
        assert_eq!(summary.headings.len(), 1);
        assert_eq!(summary.words, 1);
    }

    #[test]
    fn only_top_level_headings_count() {
        let text = "# Top\n\n> # Quoted\n\n- # In a list\n\n## Second\n";
        assert_eq!(outline(text), [h(1, "Top", 0), h(2, "Second", 6)]);
    }

    #[test]
    fn front_matter_is_not_a_heading() {
        let text = "---\ntitle: A post\ndate: 2026-11-14\n---\n\n# A post\n\nHello there.\n";
        let summary = summarize(text);
        assert_eq!(
            summary
                .headings
                .iter()
                .map(|heading| heading.title.as_str())
                .collect::<Vec<_>>(),
            ["A post"]
        );
        assert_eq!(summary.headings[0].line, 5);
        assert_eq!(summary.words, 4);
    }

    #[test]
    fn words_are_counted_across_the_text() {
        let text = "# One two\n\nThree four, five.\n\n- six\n- `seven`\n";
        assert_eq!(summarize(text).words, 7);
    }

    #[test]
    fn windows_line_ends_keep_the_lines() {
        let text = "# One\r\n\r\ntext\r\n\r\n## Two\r\n";
        assert_eq!(outline(text), [h(1, "One", 0), h(2, "Two", 4)]);
    }

    #[test]
    fn an_empty_file_has_no_outline() {
        assert_eq!(summarize(""), Summary::default());
    }
}
