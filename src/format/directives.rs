use comrak::nodes::{AstNode, NodeValue};

use super::block::{needs_list_separator, serialize_block};
use super::state::Serializer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Directive {
    Disable,
    Enable,
    DisableFile,
}

/// Directives are recognized only as top-level HTML comment blocks whose
/// content, after trimming, is exactly the directive name.
fn directive_of<'a>(node: &'a AstNode<'a>) -> Option<Directive> {
    let data = node.data.borrow();
    let NodeValue::HtmlBlock(html) = &data.value else {
        return None;
    };
    let inner = html
        .literal
        .trim()
        .strip_prefix("<!--")?
        .strip_suffix("-->")?
        .trim();
    match inner {
        "passdown-disable" => Some(Directive::Disable),
        "passdown-enable" => Some(Directive::Enable),
        "passdown-disable-file" => Some(Directive::DisableFile),
        _ => None,
    }
}

/// Top-level document serialization: normal blocks flow through the
/// serializer; disable directives switch to verbatim source passthrough.
/// An unmatched `passdown-disable` runs to end of file.
pub fn serialize_document<'a>(s: &mut Serializer<'_>, root: &'a AstNode<'a>) {
    let children: Vec<&AstNode<'_>> = root.children().collect();
    let total_lines = s.source_lines.len();

    let mut prev: Option<&AstNode<'_>> = None;
    let mut i = 0;
    while i < children.len() {
        let child = children[i];
        match directive_of(child) {
            Some(Directive::DisableFile) => {
                if prev.is_some() {
                    s.push_blank();
                }
                let start = child.data.borrow().sourcepos.start.line;
                s.push_verbatim_span(start, total_lines);
                return;
            }
            Some(Directive::Disable) => {
                if prev.is_some() {
                    s.push_blank();
                }
                let start = child.data.borrow().sourcepos.start.line;
                let matching = children[i + 1..]
                    .iter()
                    .position(|n| directive_of(n) == Some(Directive::Enable))
                    .map(|offset| i + 1 + offset);
                let Some(enable_idx) = matching else {
                    s.push_verbatim_span(start, total_lines);
                    return;
                };
                let enable = children[enable_idx];
                let end = enable.data.borrow().sourcepos.end.line;
                s.push_verbatim_span(start, end);
                prev = Some(enable);
                i = enable_idx + 1;
                continue;
            }
            // A stray enable (or none pending) is an ordinary HTML comment.
            // Serialize it as a normal block.
            _ => {
                if let Some(prev) = prev {
                    s.push_blank();
                    if needs_list_separator(prev, child) {
                        s.push_line("<!-- -->");
                        s.push_blank();
                    }
                }
                serialize_block(s, child);
                prev = Some(child);
                i += 1;
            }
        }
    }
}
