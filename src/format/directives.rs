use comrak::nodes::{AstNode, NodeValue};

use super::block::{separate, serialize_block};
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
///
/// comrak moves footnote definitions to the end of the document. They are
/// held back to the end of the formatted part, so a disabled region that
/// runs to the end of the file does not swallow them. One whose source is
/// in a disabled region is already written there.
pub fn serialize_document<'a>(s: &mut Serializer<'_>, root: &'a AstNode<'a>) {
    let (children, definitions) = split_moved_definitions(root);
    let total_lines = s.source_lines.len();
    let mut verbatim: Vec<(usize, usize)> = Vec::new();

    let mut prev: Option<&AstNode<'_>> = None;
    let mut i = 0;
    while i < children.len() {
        let child = children[i];
        match directive_of(child) {
            Some(Directive::DisableFile) => {
                let start = child.data.borrow().sourcepos.start.line;
                verbatim.push((start, total_lines));
                prev = write_definitions(s, prev, &definitions, &verbatim);
                if let Some(prev) = prev {
                    separate(s, prev, child, false);
                }
                s.push_verbatim_span(start, total_lines);
                return;
            }
            Some(Directive::Disable) => {
                let start = child.data.borrow().sourcepos.start.line;
                let matching = children[i + 1..]
                    .iter()
                    .position(|n| directive_of(n) == Some(Directive::Enable))
                    .map(|offset| i + 1 + offset);
                let Some(enable_idx) = matching else {
                    verbatim.push((start, total_lines));
                    prev = write_definitions(s, prev, &definitions, &verbatim);
                    if let Some(prev) = prev {
                        separate(s, prev, child, false);
                    }
                    s.push_verbatim_span(start, total_lines);
                    return;
                };
                if let Some(prev) = prev {
                    separate(s, prev, child, false);
                }
                let enable = children[enable_idx];
                let end = enable.data.borrow().sourcepos.end.line;
                verbatim.push((start, end));
                s.push_verbatim_span(start, end);
                prev = Some(enable);
                i = enable_idx + 1;
                continue;
            }
            // A stray enable (or none pending) is an ordinary HTML comment.
            // Serialize it as a normal block.
            _ => {
                if let Some(prev) = prev {
                    separate(s, prev, child, false);
                }
                serialize_block(s, child);
                prev = Some(child);
                i += 1;
            }
        }
    }
    write_definitions(s, prev, &definitions, &verbatim);
}

/// The top-level blocks, and apart from them the footnote definitions
/// comrak appended after them.
fn split_moved_definitions<'a>(
    root: &'a AstNode<'a>,
) -> (Vec<&'a AstNode<'a>>, Vec<&'a AstNode<'a>>) {
    let mut blocks: Vec<_> = root.children().collect();
    let first = blocks
        .iter()
        .rposition(|n| !matches!(n.data.borrow().value, NodeValue::FootnoteDefinition(_)))
        .map_or(0, |i| i + 1);
    let moved = blocks.split_off(first);
    (blocks, moved)
}

/// Write the moved footnote definitions after `prev`, except those whose
/// source is in a disabled region. Returns the last block written.
fn write_definitions<'a>(
    s: &mut Serializer<'_>,
    mut prev: Option<&'a AstNode<'a>>,
    definitions: &[&'a AstNode<'a>],
    verbatim: &[(usize, usize)],
) -> Option<&'a AstNode<'a>> {
    for &def in definitions {
        let line = def.data.borrow().sourcepos.start.line;
        if verbatim
            .iter()
            .any(|&(start, end)| (start..=end).contains(&line))
        {
            continue;
        }
        if let Some(prev) = prev {
            separate(s, prev, def, false);
        }
        serialize_block(s, def);
        prev = Some(def);
    }
    prev
}
