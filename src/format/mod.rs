//! The formatter: comrak as parser only, plus a hand-written serializer.
//! The serializer keeps the original source lines alongside the AST so
//! disable directives and unfixable blocks can be reproduced verbatim via
//! sourcepos. See `state::Serializer` for the two invariants everything
//! else hangs on (prefix stack, chunk-based wrapping).

mod block;
mod code;
mod directives;
mod escape;
mod inline;
mod list;
mod state;
mod table;
mod wrap;

#[cfg(test)]
mod fixture_tests;

use std::borrow::Cow;

use comrak::{Arena, Options, parse_document};

use crate::diagnostics::Diagnostic;
use state::Serializer;

/// Formatted output plus diagnostics located against the input.
pub(crate) struct FormatResult {
    pub(crate) output: String,
    pub(crate) diagnostics: Vec<Diagnostic>,
}

/// The parser configuration is part of passdown's output contract: only the
/// extensions the fixed style supports, autolink off (bare URLs stay text),
/// smart punctuation off (we enforce the exact reverse). The front matter
/// delimiter comes from the document itself: `+++` (TOML) when the first
/// line is exactly that, `---` (YAML) otherwise. comrak takes only one.
fn comrak_options(input: &str) -> Options<'static> {
    let mut options = Options::default();
    options.extension.table = true;
    options.extension.footnotes = true;
    options.extension.tasklist = true;
    options.extension.strikethrough = true;
    options.extension.alerts = true;
    options.extension.math_dollars = true;
    let delimiter = if input.starts_with("+++\n") || input == "+++" {
        "+++"
    } else {
        "---"
    };
    options.extension.front_matter_delimiter = Some(delimiter.to_string());
    options
}

/// Format one Markdown document. Pure and deterministic: the same input
/// always yields the same output. Formatting its own output is a no-op.
/// The fixture harness asserts both.
pub(crate) fn format_document(input: &str) -> FormatResult {
    let cleaned = clean_input(input);
    let arena = Arena::new();
    let root = parse_document(&arena, &cleaned, &comrak_options(&cleaned));
    let source_lines: Vec<&str> = cleaned.lines().collect();
    let mut serializer = Serializer::new(source_lines);
    directives::serialize_document(&mut serializer, root);
    let diagnostics = std::mem::take(&mut serializer.diags);
    FormatResult {
        output: serializer.finish(),
        diagnostics,
    }
}

/// Strip a leading BOM and normalize line endings to LF before parsing, so
/// sourcepos-based verbatim extraction never sees `\r`.
fn clean_input(input: &str) -> Cow<'_, str> {
    let input = input.strip_prefix('\u{feff}').unwrap_or(input);
    if input.contains('\r') {
        Cow::Owned(input.replace("\r\n", "\n").replace('\r', "\n"))
    } else {
        Cow::Borrowed(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crlf_input_is_normalized_to_lf() {
        let result = format_document("para one\r\nwraps\r\n\r\npara two\r\n");
        assert_eq!(result.output, "para one wraps\n\npara two\n");
    }

    #[test]
    fn lone_carriage_returns_normalize_too() {
        assert_eq!(format_document("a\rb\r").output, "a b\n");
    }

    #[test]
    fn leading_bom_is_stripped() {
        assert_eq!(format_document("\u{feff}# Title\n").output, "# Title\n");
    }

    #[test]
    fn toml_front_matter_passes_through() {
        let input = "+++\ntitle = \"Hugo post\"\n+++\n\nBody   text.\n";
        let expected = "+++\ntitle = \"Hugo post\"\n+++\n\nBody text.\n";
        assert_eq!(format_document(input).output, expected);
    }

    #[test]
    fn unclosed_toml_fence_is_prose() {
        let input = "+++\nnot front matter\n";
        assert_eq!(format_document(input).output, "+++ not front matter\n");
    }

    #[test]
    fn toml_fence_with_trailing_words_is_prose() {
        let input = "+++ words here\nmore\n+++\n";
        assert_eq!(format_document(input).output, "+++ words here more +++\n");
    }

    #[test]
    fn yaml_front_matter_still_passes_through() {
        let input = "---\ntitle: post\n---\n\nBody.\n";
        assert_eq!(format_document(input).output, input);
    }

    #[test]
    fn inline_math_is_not_escaped() {
        let input = "Inline $x_i + y_2$ prose.\n";
        assert_eq!(format_document(input).output, input);
    }

    #[test]
    fn display_math_block_form_is_preserved() {
        let input = "$$\n\\sum_{i=0}^n x_i\n$$\n";
        assert_eq!(format_document(input).output, input);
    }

    #[test]
    fn single_line_display_math_stays_inline() {
        // Not expanded to the block form: content moved onto its own line
        // could interrupt the paragraph on reparse (see sole_display_math).
        assert_eq!(format_document("$$- x$$\n").output, "$$- x$$\n");
        assert_eq!(format_document("$$a + b$$\n").output, "$$a + b$$\n");
    }

    #[test]
    fn dollar_amounts_stay_prose() {
        let input = "costs $5 and $10 today\n";
        assert_eq!(format_document(input).output, input);
    }
}
