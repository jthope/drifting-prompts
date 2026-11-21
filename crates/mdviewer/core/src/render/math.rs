//! The page's formulas: TeX as MathML, by pulldown-latex, which the web view draws with the system's math font.
//! What pulldown-latex can't read shows as an error in place, with the rest of the formula.

use pulldown_latex::{Parser, RenderConfig, Storage, config::DisplayMode, push_mathml};

/// The MathML of formula `tex`, on a line of its own when `display`.
pub(super) fn mathml(tex: &str, display: bool) -> String {
    let storage = Storage::new();
    let parser = Parser::new(tex.trim(), &storage);
    let config = RenderConfig {
        display_mode: if display {
            DisplayMode::Block
        } else {
            DisplayMode::Inline
        },
        ..RenderConfig::default()
    };
    let mut mathml = String::new();
    match push_mathml(&mut mathml, parser, config) {
        Ok(()) => mathml,
        Err(_) => {
            let mut source = String::new();
            let _ = super::escape(&mut source, tex);
            format!("<code>{source}</code>")
        }
    }
}

/// The MathML tags the page keeps.
pub(super) const TAGS: &[&str] = &[
    "math",
    "semantics",
    "annotation",
    "mrow",
    "mi",
    "mn",
    "mo",
    "ms",
    "mtext",
    "mspace",
    "merror",
    "mpadded",
    "mphantom",
    "mstyle",
    "mfrac",
    "msqrt",
    "mroot",
    "msub",
    "msup",
    "msubsup",
    "munder",
    "mover",
    "munderover",
    "mmultiscripts",
    "mprescripts",
    "none",
    "mtable",
    "mtr",
    "mtd",
];

/// The MathML attributes the page keeps, on any of its tags.
pub(super) const ATTRIBUTES: &[&str] = &[
    "display",
    "displaystyle",
    "scriptlevel",
    "mathvariant",
    "stretchy",
    "symmetric",
    "largeop",
    "movablelimits",
    "minsize",
    "maxsize",
    "lspace",
    "rspace",
    "form",
    "fence",
    "separator",
    "accent",
    "accentunder",
    "linethickness",
    "width",
    "height",
    "depth",
    "voffset",
    "columnalign",
    "rowalign",
    "columnspan",
    "rowspan",
    "encoding",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_formula_is_mathml_inline_or_on_its_own_line() {
        let inline = mathml("x^2", false);
        assert!(inline.starts_with("<math"), "{inline}");
        assert!(inline.contains("<msup>"), "{inline}");
        assert!(!inline.contains("display=\"block\""), "{inline}");
        assert!(mathml("\\frac{a}{b}", true).contains("display=\"block\""));
    }

    #[test]
    fn a_formula_it_cannot_read_says_so_in_place() {
        let shown = mathml("\\frac{a", false);
        assert!(
            shown.contains("merror") || shown.contains("<code>"),
            "{shown}"
        );
    }
}
