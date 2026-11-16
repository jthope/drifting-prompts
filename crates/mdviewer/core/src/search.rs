//! Finding text across a project's Markdown files, as the raw view finds it in one.
//!
//! A query means here what it means to nsh's editor, which highlights the same matches in the
//! raw view, so the project's count is the editor's: plain text, or a regular expression in the
//! `regex` crate's syntax (the crate the editor finds with); any case unless asked otherwise, by
//! Unicode's case folding (`É` finds `é`); `^` and `$` at every line's start and end. Whole words
//! only, when asked, keeps a match whose ends are a word's edges, a word being letters, digits and
//! `_`: an end in punctuation (`.len`, `foo(`) counts as an edge by itself.
//!
//! The text is searched as the editor holds it, its line breaks `\n` alone (`\r\n` and `\r` become
//! `\n`), so a match is where the raw view has it: a line and a column in UTF-16 code units, as the
//! editor counts them ([`Place`]). A regular expression may match an empty range (`^`), as the
//! editor counts it too, but never twice at one place.
//!
//! [`search`] reads the files of a listing in its order, leaving out one it can't read (gone since,
//! too large), and stops at [`MOST_MATCHES`], the most the editor counts. It can be called off
//! between two files (`stop`), so a newer query never waits for an older one.

use std::{
    borrow::Cow,
    fmt,
    ops::Range,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

use regex::{Regex, RegexBuilder};

use crate::{
    files::{self, MarkdownFile},
    syntax::Place,
};

/// The most matches a search counts, across all the files: the most nsh's editor counts in one.
pub const MOST_MATCHES: usize = 10_000;

/// What to find, and how.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchQuery {
    /// The text to find, or a regular expression with `regex`. Empty finds nothing.
    pub text: String,
    /// Only text of the same case; otherwise any case.
    pub case_sensitive: bool,
    /// Only whole words.
    pub whole_word: bool,
    /// `text` is a regular expression, in the `regex` crate's syntax.
    pub regex: bool,
}

/// Why a query can't be searched for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SearchError {
    /// The query is meant as a regular expression, and isn't one: why, as the `regex` crate says
    /// it.
    NotARegex(String),
}

impl fmt::Display for SearchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotARegex(why) => write!(
                f,
                "This isn't a regular expression: {why}. Turn regular expressions off to find it as text."
            ),
        }
    }
}

impl std::error::Error for SearchError {}

/// A match in a file: where it starts and where it ends, not included, and its line as a list of
/// results shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchMatch {
    /// Where it starts.
    pub start: Place,
    /// Where it ends, not included.
    pub end: Place,
    /// The line it starts on, around it.
    pub excerpt: Excerpt,
}

/// The line a match starts on, as a list of results shows it: at most [`EXCERPT_BEFORE`]
/// characters before the match, `…` where more were left out, the match (as far as its line goes),
/// and at most [`EXCERPT_AFTER`] after it. The line's indent is left out.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Excerpt {
    /// The text.
    pub text: String,
    /// Where the match starts in `text`, in UTF-16 code units.
    pub start: u32,
    /// Where it ends in `text`, not included, in UTF-16 code units.
    pub end: u32,
}

/// The most characters an excerpt keeps before its match.
pub const EXCERPT_BEFORE: usize = 40;

/// The most characters an excerpt keeps of its match.
pub const EXCERPT_MATCH: usize = 120;

/// The most characters an excerpt keeps after its match.
pub const EXCERPT_AFTER: usize = 120;

/// The matches in one file of a project.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileMatches {
    /// The file, by its path in the project (`docs/setup.md`).
    pub path: String,
    /// Its matches, in order.
    pub matches: Vec<SearchMatch>,
}

/// What a search found in a project.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchResults {
    /// The files with matches, in the listing's order; a file with none is left out.
    pub files: Vec<FileMatches>,
    /// How many matches, in all the files.
    pub total: usize,
    /// Whether the search stopped at [`MOST_MATCHES`], leaving later matches out.
    pub truncated: bool,
}

/// A query, made ready to find: made once, used on every file.
#[derive(Clone, Debug)]
pub struct Finder {
    regex: Regex,
    whole_word: bool,
}

impl Finder {
    /// The finder of `query`; `None` for an empty query, which finds nothing.
    pub fn new(query: &SearchQuery) -> Result<Option<Self>, SearchError> {
        if query.text.is_empty() {
            return Ok(None);
        }
        let pattern = if query.regex {
            query.text.clone()
        } else {
            // A query pasted with `\r\n` still finds the line breaks it was copied from.
            regex::escape(&lines_as_the_editor_has_them(&query.text))
        };
        RegexBuilder::new(&pattern)
            .case_insensitive(!query.case_sensitive)
            .multi_line(true)
            .build()
            .map(|regex| {
                Some(Self {
                    regex,
                    whole_word: query.whole_word,
                })
            })
            .map_err(|error| SearchError::NotARegex(reason(&error)))
    }

    /// Every match in `text`, in order, at most `limit`.
    pub fn find(&self, text: &str, limit: usize) -> Vec<SearchMatch> {
        let text = lines_as_the_editor_has_them(text);
        let mut walker = Walker::new(&text);
        self.ranges(&text, limit)
            .into_iter()
            .map(|range| {
                let start = walker.place(range.start);
                let excerpt = excerpt(&text, walker.line_start, &range);
                SearchMatch {
                    start,
                    end: walker.place(range.end),
                    excerpt,
                }
            })
            .collect()
    }

    /// The byte ranges of the matches in `text`, in order, at most `limit`.
    fn ranges(&self, text: &str, limit: usize) -> Vec<Range<usize>> {
        let mut found = Vec::new();
        let mut at = 0;
        // Where the last match ended: an empty match right there is that match's empty end, not
        // one of its own.
        let mut last_end = None;
        while at <= text.len() && found.len() < limit {
            let Some(found_here) = self.regex.find_at(text, at) else {
                break;
            };
            let range = found_here.range();
            let accepted = !(range.is_empty() && last_end == Some(range.start))
                && (!self.whole_word || is_whole_word(text, &range));
            if accepted {
                last_end = Some(range.end);
                at = if range.is_empty() {
                    next_char(text, range.end)
                } else {
                    range.end
                };
                found.push(range);
            } else {
                // A match refused for its edges can hide one starting inside it (`ab` in "xab ab"
                // for the word "ab"): look again a character on.
                at = next_char(text, range.start);
            }
        }
        found
    }
}

/// What `finder` finds in `files` of the project in `folder`, read in the order they come, at most
/// `most` matches (a search of several projects shares [`MOST_MATCHES`] among them); `None` when
/// `stop` was set before it was done.
pub fn search(
    folder: &Path,
    files: &[MarkdownFile],
    finder: &Finder,
    most: usize,
    stop: &AtomicBool,
) -> Option<SearchResults> {
    let mut results = SearchResults::default();
    for file in files {
        if stop.load(Ordering::Relaxed) {
            return None;
        }
        let Ok(text) = files::read(folder, &file.path) else {
            continue;
        };
        let room = most.saturating_sub(results.total);
        // One past the room says whether there was more.
        let mut matches = finder.find(&text, room + 1);
        if matches.len() > room {
            matches.truncate(room);
            results.truncated = true;
        }
        results.total += matches.len();
        if !matches.is_empty() {
            results.files.push(FileMatches {
                path: file.path.clone(),
                matches,
            });
        }
        if results.truncated {
            break;
        }
    }
    Some(results)
}

/// The most characters a line of a [`ContextBlock`] keeps; a longer one ends with `…`.
pub const CONTEXT_LINE_MOST: usize = 400;

/// Lines of a file around some of its lines: what a list of results shows of each match, in its
/// place.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextBlock {
    /// The first line's number, from 0.
    pub first: u32,
    /// The lines, from `first` on, each at most [`CONTEXT_LINE_MOST`] characters.
    pub lines: Vec<String>,
}

/// The lines of `text` around each of `lines` (from 0, in any order), `around` before and after
/// each, as the editor holds them (line breaks `\n` alone), in blocks that neither overlap nor
/// touch: two lines whose surroundings meet share a block. A line past the text's end is left out.
pub fn context(text: &str, lines: &[u32], around: u32) -> Vec<ContextBlock> {
    let text = lines_as_the_editor_has_them(text);
    let all: Vec<&str> = text.split('\n').collect();
    let last = all.len().saturating_sub(1) as u32;
    let mut wanted: Vec<u32> = lines.iter().copied().filter(|line| *line <= last).collect();
    wanted.sort_unstable();
    wanted.dedup();
    let mut spans: Vec<(u32, u32)> = Vec::new();
    for line in wanted {
        let start = line.saturating_sub(around);
        let end = line.saturating_add(around).min(last);
        match spans.last_mut() {
            Some(span) if start <= span.1 + 1 => span.1 = span.1.max(end),
            _ => spans.push((start, end)),
        }
    }
    spans
        .into_iter()
        .map(|(start, end)| ContextBlock {
            first: start,
            lines: all[start as usize..=end as usize]
                .iter()
                .map(|line| shortened(line))
                .collect(),
        })
        .collect()
}

/// `line` with at most [`CONTEXT_LINE_MOST`] characters, `…` after them when it had more.
fn shortened(line: &str) -> String {
    match line.char_indices().nth(CONTEXT_LINE_MOST) {
        Some((at, _)) => format!("{}…", &line[..at]),
        None => line.to_owned(),
    }
}

/// `text` with its line breaks `\n` alone, as the editor holds it.
fn lines_as_the_editor_has_them(text: &str) -> Cow<'_, str> {
    if text.contains('\r') {
        Cow::Owned(text.replace("\r\n", "\n").replace('\r', "\n"))
    } else {
        Cow::Borrowed(text)
    }
}

/// The excerpt of the match at `range` of `text`, whose line starts at `line_start`. It looks only
/// at the characters it keeps, so a match on a very long line costs no more than one on a short one.
fn excerpt(text: &str, line_start: usize, range: &Range<usize>) -> Excerpt {
    // Before the match: back from it to the line's start, at most EXCERPT_BEFORE characters.
    let before_all = &text[line_start..range.start];
    let mut before_chars = before_all.char_indices().rev().take(EXCERPT_BEFORE + 1);
    let cut_at = before_chars
        .nth(EXCERPT_BEFORE)
        .map(|(at, c)| at + c.len_utf8());
    let before = match cut_at {
        Some(at) => &before_all[at..],
        None => before_all.trim_start(),
    };
    // The match, as far as its line goes, and what follows it on the line.
    let rest = &text[range.start..];
    let matched: String = rest[..range.len()]
        .chars()
        .take_while(|c| *c != '\n')
        .take(EXCERPT_MATCH)
        .collect();
    let after_all = if matched.len() == range.len() {
        &text[range.end..]
    } else {
        ""
    };
    let mut after_chars = after_all.chars().take_while(|c| *c != '\n');
    let after: String = after_chars.by_ref().take(EXCERPT_AFTER).collect();
    let more_after = after_chars.next().is_some();

    let mut excerpt = String::new();
    if cut_at.is_some() {
        excerpt.push('…');
    }
    excerpt.push_str(before);
    let start = excerpt.encode_utf16().count() as u32;
    excerpt.push_str(&matched);
    let end = excerpt.encode_utf16().count() as u32;
    excerpt.push_str(&after);
    if more_after {
        excerpt.push('…');
    }
    Excerpt {
        text: excerpt,
        start,
        end,
    }
}

/// The line of a `regex` error that says what is wrong (`unclosed group`), without the pattern and
/// the caret it draws under it.
fn reason(error: &regex::Error) -> String {
    let said = error.to_string();
    said.lines()
        .rev()
        .find_map(|line| line.trim().strip_prefix("error: "))
        .unwrap_or(said.trim())
        .to_owned()
}

/// Whether `range` starts and ends at a word's edge: at each end, a character that isn't a word's
/// (or the text's edge) on at least one side.
fn is_whole_word(text: &str, range: &Range<usize>) -> bool {
    let before = text[..range.start].chars().next_back();
    let first = text[range.clone()].chars().next();
    let last = text[range.clone()].chars().next_back();
    let after = text[range.end..].chars().next();
    let left = !before.is_some_and(is_word) || !first.is_some_and(is_word);
    let right = !after.is_some_and(is_word) || !last.is_some_and(is_word);
    left && right
}

/// A word's character, in any language: a letter, a digit or `_`.
fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The offset after the character at `offset`; past the text's end at its end.
fn next_char(text: &str, offset: usize) -> usize {
    text[offset..]
        .chars()
        .next()
        .map_or(offset + 1, |c| offset + c.len_utf8())
}

/// Where byte offsets of a text are, as lines and UTF-16 columns, for offsets that never go back:
/// each is counted on from the last, so a long line with many matches is walked once.
struct Walker<'a> {
    text: &'a str,
    at: usize,
    place: Place,
    /// Where the line of the last offset placed starts, as a byte offset.
    line_start: usize,
}

impl<'a> Walker<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            text,
            at: 0,
            place: Place { line: 0, column: 0 },
            line_start: 0,
        }
    }

    fn place(&mut self, offset: usize) -> Place {
        for (at, c) in self.text[self.at..offset].char_indices() {
            if c == '\n' {
                self.place = Place {
                    line: self.place.line + 1,
                    column: 0,
                };
                self.line_start = self.at + at + 1;
            } else {
                self.place.column += c.len_utf16() as u32;
            }
        }
        self.at = offset;
        self.place
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    fn query(text: &str) -> SearchQuery {
        SearchQuery {
            text: text.to_owned(),
            ..SearchQuery::default()
        }
    }

    /// The matches of `query` in `text`, each as its start and end: (line, column) pairs.
    fn find(query: &SearchQuery, text: &str) -> Vec<((u32, u32), (u32, u32))> {
        Finder::new(query)
            .expect("a query")
            .expect("not empty")
            .find(text, usize::MAX)
            .into_iter()
            .map(|found| {
                (
                    (found.start.line, found.start.column),
                    (found.end.line, found.end.column),
                )
            })
            .collect()
    }

    #[test]
    fn text_is_found_in_any_case_on_every_line() {
        assert_eq!(
            find(&query("rust"), "Rust and rust\nno\nRUST"),
            [((0, 0), (0, 4)), ((0, 9), (0, 13)), ((2, 0), (2, 4))]
        );
        assert_eq!(
            find(
                &SearchQuery {
                    case_sensitive: true,
                    ..query("rust")
                },
                "Rust and rust"
            ),
            [((0, 9), (0, 13))]
        );
    }

    #[test]
    fn case_folds_beyond_ascii() {
        assert_eq!(
            find(&query("é"), "É é"),
            [((0, 0), (0, 1)), ((0, 2), (0, 3))]
        );
    }

    #[test]
    fn plain_text_is_taken_as_written() {
        assert_eq!(find(&query("a.b"), "a.b axb"), [((0, 0), (0, 3))]);
    }

    #[test]
    fn whole_words_leave_out_parts_of_words() {
        let whole = SearchQuery {
            whole_word: true,
            ..query("ab")
        };
        assert_eq!(find(&whole, "xab ab abc"), [((0, 4), (0, 6))]);
        // An end in punctuation is an edge by itself.
        let call = SearchQuery {
            whole_word: true,
            ..query(".len")
        };
        assert_eq!(find(&call, "s.len()"), [((0, 1), (0, 5))]);
    }

    #[test]
    fn a_regular_expression_finds_by_pattern_and_anchors_at_each_line() {
        let pattern = SearchQuery {
            regex: true,
            ..query("^#+ (\\w+)")
        };
        assert_eq!(
            find(&pattern, "# One\ntext # no\n## Two"),
            [((0, 0), (0, 5)), ((2, 0), (2, 6))]
        );
    }

    #[test]
    fn a_pattern_that_isn_t_one_says_why() {
        let bad = SearchQuery {
            regex: true,
            ..query("(abc")
        };
        let error = Finder::new(&bad).expect_err("not a regex");
        assert_eq!(error, SearchError::NotARegex("unclosed group".to_owned()));
        assert!(
            error
                .to_string()
                .starts_with("This isn't a regular expression: unclosed group.")
        );
    }

    #[test]
    fn an_empty_query_finds_nothing_and_needs_no_finder() {
        assert!(Finder::new(&query("")).expect("fine").is_none());
    }

    #[test]
    fn empty_matches_count_once_at_each_place() {
        let starts = SearchQuery {
            regex: true,
            ..query("^")
        };
        assert_eq!(find(&starts, "a\nb"), [((0, 0), (0, 0)), ((1, 0), (1, 0))]);
    }

    #[test]
    fn places_are_lines_and_utf16_columns_as_the_editor_counts() {
        // `é` is one unit and the emoji two; `\r\n` is one line break.
        assert_eq!(
            find(&query("x"), "é😀x\r\n😀 x\rx"),
            [((0, 3), (0, 4)), ((1, 3), (1, 4)), ((2, 0), (2, 1))]
        );
    }

    #[test]
    fn a_match_can_cross_a_line_break() {
        assert_eq!(find(&query("a\r\nb"), "xa\nb"), [((0, 1), (1, 1))]);
    }

    /// A project folder holding `files`, each with its text.
    fn project(files: &[(&str, &str)]) -> TempDir {
        let dir = TempDir::new().expect("a folder");
        for (name, text) in files {
            let path = dir.path().join(name);
            fs::create_dir_all(path.parent().expect("a folder")).expect("its folder");
            fs::write(&path, text).expect("the file");
        }
        dir
    }

    fn search_all(dir: &TempDir, query: &SearchQuery) -> SearchResults {
        let listing = files::list(dir.path()).expect("a listing");
        let finder = Finder::new(query).expect("a query").expect("not empty");
        search(
            dir.path(),
            &listing.files,
            &finder,
            MOST_MATCHES,
            &AtomicBool::new(false),
        )
        .expect("not stopped")
    }

    #[test]
    fn a_project_is_searched_file_by_file_in_the_listing_s_order() {
        let dir = project(&[
            ("README.md", "# Notes\nthe toolchain\n"),
            (
                "docs/setup.md",
                "Install the toolchain.\nThe toolchain is Rust.\n",
            ),
            ("docs/usage.md", "Nothing here.\n"),
        ]);
        let results = search_all(&dir, &query("toolchain"));
        assert_eq!(results.total, 3);
        assert!(!results.truncated);
        let counts: Vec<(&str, usize)> = results
            .files
            .iter()
            .map(|file| (file.path.as_str(), file.matches.len()))
            .collect();
        assert_eq!(counts, [("README.md", 1), ("docs/setup.md", 2)]);
        assert_eq!(
            results.files[1].matches[1].start,
            Place { line: 1, column: 4 }
        );
    }

    #[test]
    fn a_search_stops_at_the_most_it_counts() {
        let many = "x ".repeat(MOST_MATCHES);
        let dir = project(&[("a.md", &many), ("b.md", "x")]);
        let results = search_all(&dir, &query("x"));
        assert_eq!(results.total, MOST_MATCHES);
        assert!(results.truncated);
        assert_eq!(results.files.len(), 1);
    }

    #[test]
    fn a_search_called_off_says_so() {
        let dir = project(&[("a.md", "x")]);
        let listing = files::list(dir.path()).expect("a listing");
        let finder = Finder::new(&query("x"))
            .expect("a query")
            .expect("not empty");
        assert_eq!(
            search(
                dir.path(),
                &listing.files,
                &finder,
                MOST_MATCHES,
                &AtomicBool::new(true)
            ),
            None
        );
    }

    #[test]
    fn a_search_stops_at_the_most_it_is_given() {
        let dir = project(&[("a.md", "x x x"), ("b.md", "x")]);
        let listing = files::list(dir.path()).expect("a listing");
        let finder = Finder::new(&query("x"))
            .expect("a query")
            .expect("not empty");
        let results = search(
            dir.path(),
            &listing.files,
            &finder,
            2,
            &AtomicBool::new(false),
        )
        .expect("not stopped");
        assert_eq!(results.total, 2);
        assert!(results.truncated);
    }

    /// The excerpt of the first match of `query` in `text`, with the match in brackets.
    fn excerpt_of(query: &SearchQuery, text: &str) -> String {
        let found = Finder::new(query)
            .expect("a query")
            .expect("not empty")
            .find(text, 1);
        let excerpt = &found[0].excerpt;
        let units: Vec<u16> = excerpt.text.encode_utf16().collect();
        let part =
            |from: u32, to: u32| String::from_utf16_lossy(&units[from as usize..to as usize]);
        format!(
            "{}[{}]{}",
            part(0, excerpt.start),
            part(excerpt.start, excerpt.end),
            part(excerpt.end, units.len() as u32)
        )
    }

    #[test]
    fn an_excerpt_is_the_match_s_line_without_its_indent() {
        assert_eq!(
            excerpt_of(&query("tool"), "# Title\n    the tool, 😀 here\nnext"),
            "the [tool], 😀 here"
        );
    }

    #[test]
    fn an_excerpt_shortens_a_long_line_around_the_match() {
        let line = format!("{}needle{}", "a".repeat(100), "b".repeat(200));
        let shown = excerpt_of(&query("needle"), &line);
        assert_eq!(
            shown,
            format!(
                "…{}[needle]{}…",
                "a".repeat(EXCERPT_BEFORE),
                "b".repeat(EXCERPT_AFTER)
            )
        );
    }

    #[test]
    fn context_is_the_lines_around_each_line_in_blocks_that_never_overlap() {
        let text = (0..20)
            .map(|line| format!("line {line}"))
            .collect::<Vec<_>>()
            .join("\r\n");
        let blocks = context(&text, &[9, 2, 4, 30], 1);
        let shown: Vec<(u32, Vec<&str>)> = blocks
            .iter()
            .map(|block| {
                (
                    block.first,
                    block.lines.iter().map(String::as_str).collect(),
                )
            })
            .collect();
        assert_eq!(
            shown,
            [
                (1, vec!["line 1", "line 2", "line 3", "line 4", "line 5"]),
                (8, vec!["line 8", "line 9", "line 10"]),
            ]
        );
        // At the file's ends, the block stops there.
        assert_eq!(context(&text, &[0], 2)[0].first, 0);
        assert_eq!(context(&text, &[19], 2)[0].lines.len(), 3);
    }

    #[test]
    fn a_long_line_of_context_is_shortened() {
        let line = "x".repeat(CONTEXT_LINE_MOST + 10);
        let block = &context(&line, &[0], 0)[0];
        assert_eq!(block.lines[0].chars().count(), CONTEXT_LINE_MOST + 1);
        assert!(block.lines[0].ends_with('…'));
    }

    #[test]
    fn an_excerpt_of_a_match_across_lines_ends_with_its_first_line() {
        let pattern = SearchQuery {
            regex: true,
            ..query("one\\ntwo")
        };
        assert_eq!(excerpt_of(&pattern, "say one\ntwo three"), "say [one]");
    }
}
