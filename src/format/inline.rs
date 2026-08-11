use comrak::nodes::{AstNode, NodeValue};
use unicode_width::UnicodeWidthStr;

use crate::diagnostics::{DiagKind, Diagnostic};
use crate::normalize;

use super::escape::escape_text;

/// An unbreakable unit of wrapped output. Breaks are only allowed between
/// chunks; `break_after` forces one (hard line break).
#[derive(Debug, Clone)]
pub struct Chunk {
    pub text: String,
    pub break_after: bool,
}

impl Chunk {
    pub fn width(&self) -> usize {
        self.text.width()
    }
}

/// Accumulates chunks while walking inlines. `glue == true` means the next
/// piece of text attaches to the last chunk instead of starting a new one;
/// spaces (and soft breaks) reset it.
pub struct ChunkBuilder {
    chunks: Vec<Chunk>,
    glue: bool,
}

impl ChunkBuilder {
    pub fn new() -> Self {
        ChunkBuilder {
            chunks: Vec::new(),
            glue: false,
        }
    }

    fn piece(&mut self, text: &str) {
        if self.glue
            && let Some(last) = self.chunks.last_mut()
            && !last.break_after
        {
            last.text.push_str(text);
        } else {
            self.chunks.push(Chunk {
                text: text.to_string(),
                break_after: false,
            });
        }
        self.glue = true;
    }

    fn space(&mut self) {
        self.glue = false;
    }

    /// Opening delimiter: attaches to the first word that follows.
    fn open(&mut self, delim: &str) {
        self.piece(delim);
    }

    /// Closing delimiter: always attaches to the last chunk.
    fn close(&mut self, delim: &str) {
        self.glue = true;
        self.piece(delim);
    }

    fn hard_break(&mut self) {
        self.glue = true;
        self.piece("\\");
        if let Some(last) = self.chunks.last_mut() {
            last.break_after = true;
        }
        self.glue = false;
    }

    pub fn finish(self) -> Vec<Chunk> {
        self.chunks
    }
}

/// Flatten the inline children of a block node into wrap-ready chunks.
/// Prose text passes through ASCII punctuation normalization, reporting each
/// converted character as a Note diagnostic.
pub fn collect_chunks<'a>(node: &'a AstNode<'a>, diags: &mut Vec<Diagnostic>) -> Vec<Chunk> {
    let mut builder = ChunkBuilder::new();
    for child in node.children() {
        walk(&mut builder, child, diags);
    }
    builder.finish()
}

/// Inline content joined on single spaces, for contexts that never wrap
/// (headings, table cells).
pub fn collect_text<'a>(node: &'a AstNode<'a>, diags: &mut Vec<Diagnostic>) -> String {
    let chunks = collect_chunks(node, diags);
    chunks
        .iter()
        .map(|c| c.text.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

fn walk<'a>(b: &mut ChunkBuilder, node: &'a AstNode<'a>, diags: &mut Vec<Diagnostic>) {
    match &node.data.borrow().value {
        NodeValue::Text(literal) => {
            let sourcepos = node.data.borrow().sourcepos;
            let (normalized, occurrences) = normalize::normalize(literal);
            for occ in occurrences {
                diags.push(Diagnostic {
                    line: sourcepos.start.line,
                    col: sourcepos.start.column + occ.char_offset,
                    kind: DiagKind::NonAsciiPunct {
                        ch: occ.ch,
                        replacement: occ.replacement,
                    },
                });
            }
            text(b, &normalized);
        }
        NodeValue::SoftBreak => b.space(),
        NodeValue::LineBreak => b.hard_break(),
        NodeValue::Code(code) => b.piece(&code_span(&code.literal)),
        NodeValue::Emph => {
            b.open("*");
            for child in node.children() {
                walk(b, child, diags);
            }
            b.close("*");
        }
        NodeValue::Strong => {
            b.open("**");
            for child in node.children() {
                walk(b, child, diags);
            }
            b.close("**");
        }
        NodeValue::Strikethrough => {
            b.open("~~");
            for child in node.children() {
                walk(b, child, diags);
            }
            b.close("~~");
        }
        NodeValue::Link(link) => {
            if let Some(auto) = autolink(node, &link.url) {
                b.piece(&auto);
            } else {
                b.open("[");
                for child in node.children() {
                    walk(b, child, diags);
                }
                b.close(&format!("]({})", destination(&link.url, &link.title)));
            }
        }
        NodeValue::Image(link) => {
            b.open("![");
            for child in node.children() {
                walk(b, child, diags);
            }
            b.close(&format!("]({})", destination(&link.url, &link.title)));
        }
        NodeValue::Math(math) => b.piece(&math_span(&math.literal, math.display_math)),
        NodeValue::FootnoteReference(footnote) => {
            b.piece(&format!("[^{}]", footnote.name));
        }
        NodeValue::HtmlInline(literal) => b.piece(literal),
        // Anything unexpected: recurse so no content is silently dropped.
        _ => {
            for child in node.children() {
                walk(b, child, diags);
            }
        }
    }
}

/// Split a text run into word chunks. Edge whitespace matters: it breaks the
/// glue that would otherwise attach the first/last word to a neighboring
/// inline's delimiter (`*em* word` vs `*em*word`).
fn text(b: &mut ChunkBuilder, literal: &str) {
    if literal.chars().next().is_some_and(char::is_whitespace) {
        b.space();
    }
    let words: Vec<&str> = literal.split_whitespace().collect();
    for (i, word) in words.iter().enumerate() {
        b.piece(&escape_text(word));
        if i + 1 < words.len() {
            b.space();
        }
    }
    if literal.chars().last().is_some_and(char::is_whitespace) {
        b.space();
    }
}

/// A CommonMark autolink round-trips as a Link whose only child is a Text
/// equal to the URL (or the URL minus a `mailto:` prefix). The URL must
/// carry a real URI scheme: `<...>` around a bare relative path is raw HTML
/// on reparse, not a link.
fn autolink<'a>(node: &'a AstNode<'a>, url: &str) -> Option<String> {
    let first = node.first_child()?;
    if first.next_sibling().is_some() {
        return None;
    }
    let data = first.data.borrow();
    let NodeValue::Text(literal) = &data.value else {
        return None;
    };
    if (literal == url && has_uri_scheme(url)) || url.strip_prefix("mailto:") == Some(literal) {
        Some(format!("<{literal}>"))
    } else {
        None
    }
}

/// CommonMark autolink scheme: 2-32 chars, a letter followed by letters,
/// digits, `+`, `-`, or `.`, terminated by `:`.
fn has_uri_scheme(url: &str) -> bool {
    let Some(colon) = url.find(':') else {
        return false;
    };
    let scheme = &url[..colon];
    (2..=32).contains(&scheme.len())
        && scheme
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

/// Minimal-but-sufficient backtick delimiters and padding for a code span.
pub fn code_span(literal: &str) -> String {
    let literal = literal.replace('\n', " ");
    let longest_run = literal.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    let delim = "`".repeat(longest_run + 1);
    let needs_pad = literal.starts_with('`')
        || literal.ends_with('`')
        || ((literal.starts_with(' ') || literal.ends_with(' '))
            && literal.chars().any(|c| c != ' '));
    if needs_pad {
        format!("{delim} {literal} {delim}")
    } else {
        format!("{delim}{literal}{delim}")
    }
}

/// A math span, verbatim inside its dollar delimiters. Math is never
/// escaped or punctuation-normalized; newlines flatten to spaces so the
/// span stays one unbreakable chunk.
pub fn math_span(literal: &str, display: bool) -> String {
    let literal = literal.replace('\n', " ");
    let delim = if display { "$$" } else { "$" };
    format!("{delim}{literal}{delim}")
}

/// Link/image destination plus optional title, ready to sit inside `(...)`.
fn destination(url: &str, title: &str) -> String {
    let dest = if url.is_empty() {
        "<>".to_string()
    } else if url.chars().any(char::is_whitespace) {
        format!("<{}>", url.replace('<', "\\<").replace('>', "\\>"))
    } else {
        url.replace('(', "\\(").replace(')', "\\)")
    };
    if title.is_empty() {
        dest
    } else {
        let escaped = title.replace('\\', "\\\\").replace('"', "\\\"");
        format!("{dest} \"{escaped}\"")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_span_delimiters_grow_past_content() {
        assert_eq!(code_span("plain"), "`plain`");
        assert_eq!(code_span("a`b"), "``a`b``");
        assert_eq!(code_span("a``b"), "```a``b```");
    }

    #[test]
    fn code_span_padding_rules() {
        assert_eq!(code_span("`lead"), "`` `lead ``");
        assert_eq!(code_span("trail`"), "`` trail` ``");
        assert_eq!(code_span(" spaced "), "`  spaced  `");
        assert_eq!(code_span(" "), "` `");
        assert_eq!(code_span("multi\nline"), "`multi line`");
    }

    #[test]
    fn math_span_forms() {
        assert_eq!(math_span("x_i + y", false), "$x_i + y$");
        assert_eq!(math_span("a + b", true), "$$a + b$$");
        assert_eq!(math_span("a\n+ b", false), "$a + b$");
    }

    #[test]
    fn destination_forms() {
        assert_eq!(destination("http://x", ""), "http://x");
        assert_eq!(destination("a b", ""), "<a b>");
        assert_eq!(destination("", ""), "<>");
        assert_eq!(destination("x(1)", ""), "x\\(1\\)");
        assert_eq!(destination("u", "a \"b\""), "u \"a \\\"b\\\"\"");
    }
}
