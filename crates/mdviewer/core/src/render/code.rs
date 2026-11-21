//! The page's code blocks in color. syntect parses a block with the syntaxes nsh's own editor and `TextView` have
//! (two-face's), and each run of text gets the class of its kind of token (`tok-keyword`, `tok-string`, ...), which
//! the page colors with the theme's `syntax-*` tokens. A scope's kind is read from its name, innermost first, as
//! nsh reads it, so a block looks on the page as it does in Raw. A fence naming no language it knows stays plain.

use std::{borrow::Cow, collections::BTreeMap, collections::HashMap, fmt, sync::LazyLock};

use comrak::adapters::{SyntaxHighlightMeta, SyntaxHighlighterAdapter};
use syntect::{
    easy::ScopeRegionIterator,
    parsing::{ParseState, Scope, ScopeStack, SyntaxReference, SyntaxSet},
    util::LinesWithEndings,
};

use super::escape;

static SYNTAXES: LazyLock<SyntaxSet> = LazyLock::new(two_face::syntax::extra_newlines);

/// Words that declare, which a grammar may scope as a type.
const DECLARING: &[&str] = &[
    "let",
    "const",
    "var",
    "static",
    "mut",
    "fn",
    "func",
    "function",
    "def",
    "class",
    "struct",
    "enum",
    "union",
    "trait",
    "impl",
    "type",
    "interface",
    "module",
    "mod",
    "namespace",
    "typedef",
    "val",
    "extern",
    "async",
];

/// What a run of code is, colored alike.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Token {
    Comment,
    Keyword,
    String,
    Number,
    /// Names of functions and types.
    Title,
    /// Attributes, parameters, properties and keys.
    Attr,
    /// Built-ins: types, functions; tags.
    Built,
    Deletion,
    Bold,
    Italic,
    /// A keyword when it is a word (`as`), else plain.
    Operator,
    /// A declaring word, or a type.
    Storage,
}

impl Token {
    fn class(self) -> &'static str {
        match self {
            Self::Comment => "tok-comment",
            Self::Keyword | Self::Operator | Self::Storage => "tok-keyword",
            Self::String => "tok-string",
            Self::Number => "tok-number",
            Self::Title => "tok-title",
            Self::Attr => "tok-attr",
            Self::Built => "tok-built",
            Self::Deletion => "tok-deletion",
            Self::Bold => "tok-bold",
            Self::Italic => "tok-italic",
        }
    }

    /// The token of `text` for the tokens its scope leaves open; `None` for plain text.
    fn settle(self, text: &str) -> Option<Self> {
        match self {
            Self::Operator => text
                .chars()
                .all(|each| each.is_ascii_alphabetic())
                .then_some(Self::Keyword),
            Self::Storage if DECLARING.contains(&text) => Some(Self::Keyword),
            Self::Storage => Some(Self::Built),
            token => Some(token),
        }
    }
}

/// The token of a TextMate scope, or `None` to let an enclosing scope decide.
fn token(scope: &str) -> Option<Token> {
    let is = |prefix: &str| {
        scope
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('.'))
    };
    let token = if is("meta.object-literal.key") || is("meta.mapping.key") {
        Token::Attr
    } else if is("punctuation") || is("invalid") || is("meta") || is("source") || is("text") {
        // Delimiters belong to what they delimit; the rest are containers with no color of their own.
        return None;
    } else if is("comment") {
        Token::Comment
    } else if is("string") || is("constant.character.escape") {
        Token::String
    } else if is("constant")
        || is("support.constant")
        || is("variable.other.constant")
        || is("entity.name.constant")
    {
        Token::Number
    } else if is("support.type.property-name")
        || is("entity.other.attribute-name")
        || is("variable.parameter")
        || is("variable.language")
        || is("variable.other.property")
        || is("variable.other.object.property")
        || is("variable.other.member")
        || is("entity.name.tag.yaml")
        || is("entity.name.tag.toml")
        || (is("variable.other") && scope.ends_with(".shell"))
    {
        Token::Attr
    } else if is("support.function.builtin")
        || is("support.macro")
        || is("support.type")
        || is("entity.name.tag")
    {
        Token::Built
    } else if is("keyword.operator") {
        Token::Operator
    } else if is("storage.type") {
        // `storage.type.function.rust` declares, `storage.type.numeric.go` is a type, `storage.type.rust` is either.
        const DECLARES: &[&str] = &[
            "class",
            "enum",
            "function",
            "impl",
            "interface",
            "module",
            "namespace",
            "struct",
            "trait",
            "type.type",
            "union",
            "def",
            "fn",
        ];
        match scope.split('.').count() {
            ..=3 => Token::Storage,
            _ if DECLARES
                .iter()
                .any(|kind| scope["storage.type.".len()..].starts_with(kind)) =>
            {
                Token::Keyword
            }
            _ => Token::Built,
        }
    } else if is("keyword") || is("storage") || is("entity.name.section") || is("markup.heading") {
        Token::Keyword
    } else if is("entity.name")
        || is("entity.other.inherited-class")
        || is("support.function")
        || is("support.class")
        || is("variable.function")
    {
        Token::Title
    } else if is("markup.inserted") {
        Token::String
    } else if is("markup.deleted") {
        Token::Deletion
    } else if is("markup.bold") {
        Token::Bold
    } else if is("markup.italic") {
        Token::Italic
    } else {
        return None;
    };
    Some(token)
}

/// The syntax a fence's language names, as nsh's editor finds it.
fn syntax_of(language: &str) -> Option<&'static SyntaxReference> {
    let language = language.trim();
    if language.is_empty() {
        return None;
    }
    let alias = match language.to_ascii_lowercase().as_str() {
        "shell" | "console" | "shellscript" => "sh",
        "golang" => "go",
        "jsx" => "tsx",
        "mjs" | "cjs" => "js",
        "c++" => "cpp",
        "c#" | "csharp" => "cs",
        "f#" | "fsharp" => "fs",
        "jsonc" | "json5" => "json",
        "objective-c" | "objc" => "m",
        "docker" => "Dockerfile",
        "make" => "Makefile",
        "vue" | "svelte" => "html",
        _ => language,
    };
    SYNTAXES
        .find_syntax_by_token(alias)
        .filter(|syntax| syntax.name != "Plain Text")
}

/// The code of a block, its runs in their tokens' classes.
fn highlight(output: &mut dyn fmt::Write, language: &str, code: &str) -> fmt::Result {
    let Some(syntax) = syntax_of(language) else {
        return escape(output, code);
    };
    let mut state = ParseState::new(syntax);
    let mut scopes = ScopeStack::new();
    let mut tokens: HashMap<Scope, Option<Token>> = HashMap::new();
    // The class of the span open now: runs of one kind share a span.
    let mut open: Option<&'static str> = None;
    let mut lines = LinesWithEndings::from(code);
    for line in lines.by_ref() {
        let Ok(ops) = state.parse_line(line, &SYNTAXES) else {
            // What the grammar can't read stays as written, from there on.
            escape(output, line)?;
            break;
        };
        for (text, op) in ScopeRegionIterator::new(&ops, line) {
            let _ = scopes.apply(op);
            if text.is_empty() {
                continue;
            }
            let kind = scopes
                .as_slice()
                .iter()
                .rev()
                .find_map(|scope| {
                    *tokens
                        .entry(*scope)
                        .or_insert_with(|| token(&scope.build_string()))
                })
                .and_then(|kind| kind.settle(text.trim()));
            let class = kind.map(Token::class);
            if class != open {
                if open.is_some() {
                    output.write_str("</span>")?;
                }
                if let Some(class) = class {
                    write!(output, "<span class=\"{class}\">")?;
                }
                open = class;
            }
            escape(output, text)?;
        }
    }
    if open.is_some() {
        output.write_str("</span>")?;
    }
    for line in lines {
        escape(output, line)?;
    }
    Ok(())
}

fn tag(
    output: &mut dyn fmt::Write,
    name: &str,
    attributes: BTreeMap<&str, Cow<'_, str>>,
) -> fmt::Result {
    write!(output, "<{name}")?;
    for (attribute, value) in attributes {
        write!(output, " {attribute}=\"")?;
        escape(output, &value)?;
        output.write_str("\"")?;
    }
    output.write_str(">")
}

/// comrak's hook for code blocks.
pub(super) struct Highlighter;

impl SyntaxHighlighterAdapter for Highlighter {
    fn write_highlighted(
        &self,
        output: &mut dyn fmt::Write,
        highlight: &SyntaxHighlightMeta<'_>,
    ) -> fmt::Result {
        self::highlight(output, highlight.lang, highlight.code)
    }

    fn write_pre_tag(
        &self,
        output: &mut dyn fmt::Write,
        _: &SyntaxHighlightMeta<'_>,
        attributes: BTreeMap<&str, Cow<'_, str>>,
    ) -> fmt::Result {
        tag(output, "pre", attributes)
    }

    fn write_code_tag(
        &self,
        output: &mut dyn fmt::Write,
        _: &SyntaxHighlightMeta<'_>,
        attributes: BTreeMap<&str, Cow<'_, str>>,
    ) -> fmt::Result {
        tag(output, "code", attributes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn colored(language: &str, code: &str) -> String {
        let mut html = String::new();
        highlight(&mut html, language, code).expect("written");
        html
    }

    #[test]
    fn code_gets_its_tokens_classes() {
        let html = colored("rust", "// Hi\nfn main() { let x = \"a\" as u8; }\n");
        assert!(html.contains("<span class=\"tok-comment\">// Hi"), "{html}");
        assert!(
            html.contains("<span class=\"tok-keyword\">fn</span>"),
            "{html}"
        );
        assert!(
            html.contains("<span class=\"tok-title\">main</span>"),
            "{html}"
        );
        assert!(html.contains("<span class=\"tok-string\">"), "{html}");
        assert!(
            html.contains("<span class=\"tok-keyword\">as</span>"),
            "{html}"
        );
    }

    #[test]
    fn a_language_by_another_name_or_none_at_all() {
        assert!(colored("TS", "const a = 1;").contains("tok-keyword"));
        assert!(colored("shell", "echo $HOME").contains("tok-"));
        assert_eq!(colored("", "<b> & c"), "&lt;b&gt; &amp; c");
        assert_eq!(colored("no-such-language", "x"), "x");
    }
}
