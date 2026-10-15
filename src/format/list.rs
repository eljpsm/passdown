use comrak::nodes::{AstNode, ListType, NodeList, NodeValue};

use super::block::{separate, serialize_block, starts_indented};
use super::inline::{Chunk, collect_chunks};
use super::state::Serializer;
use super::wrap::wrap;

/// Serialize a list: bullets become `- `, ordered items `N. `, with blank
/// lines between items only when the list is loose.
pub fn serialize_list<'a>(s: &mut Serializer<'_>, node: &'a AstNode<'a>, list: &NodeList) {
    let mut prev: Option<&AstNode<'_>> = None;
    for (i, item) in node.children().enumerate() {
        if let Some(prev) = prev {
            separate(s, prev, item, list.tight);
        }
        serialize_item(s, item, list, list.start + i);
        prev = Some(item);
    }
}

/// Serialize one list item under its marker prefix. A task-list checkbox is
/// injected as the first chunk of a leading paragraph on the checkbox line so
/// wrapping accounts for its width. Nested sibling lists of the same type get
/// the usual `<!-- -->` separator so they don't merge on reparse.
fn serialize_item<'a>(
    s: &mut Serializer<'_>,
    item: &'a AstNode<'a>,
    list: &NodeList,
    number: usize,
) {
    let (checkbox, checkbox_line) = match &item.data.borrow().value {
        NodeValue::TaskItem(task) => (
            Some(if task.symbol.is_some() { "[x]" } else { "[ ]" }),
            task.symbol_sourcepos.start.line,
        ),
        _ => (None, 0),
    };

    let (first, cont) = match list.list_type {
        ListType::Bullet => ("- ".to_string(), "  ".to_string()),
        ListType::Ordered => {
            let marker = format!("{number}. ");
            let cont = " ".repeat(marker.len());
            (marker, cont)
        }
    };
    s.push_prefix(&first, &cont);

    // Up to four spaces after a marker join the content column, so an
    // indented first block would shift it. Start the block on the next line.
    // Indented code is exempt: five or more spaces leave the column alone,
    // and an item that starts blank can't interrupt a paragraph.
    if checkbox.is_none()
        && let Some(child) = item.first_child()
        && match &child.data.borrow().value {
            NodeValue::CodeBlock(code) if !code.fenced => false,
            other => starts_indented(other),
        }
    {
        s.push_line("");
    }

    let mut children = item.children();
    match children.next() {
        // An empty item still needs a line to carry its marker (a bare `-`).
        None => s.push_line(checkbox.unwrap_or("")),
        Some(first_child) => {
            // In a loose list, a paragraph that starts below the `[ ]` line
            // had a blank line before it. Joined to the checkbox, it would
            // lose that blank.
            let joins_checkbox = {
                let data = first_child.data.borrow();
                matches!(data.value, NodeValue::Paragraph)
                    && (list.tight || data.sourcepos.start.line == checkbox_line)
            };
            match checkbox {
                Some(checkbox) if joins_checkbox => {
                    let mut chunks = collect_chunks(first_child, &mut s.diags);
                    chunks.insert(
                        0,
                        Chunk {
                            text: checkbox.to_string(),
                            break_after: false,
                        },
                    );
                    for line in wrap(&chunks, s.width_budget()) {
                        s.push_line(&line);
                    }
                }
                Some(checkbox) => {
                    s.push_line(checkbox);
                    // Indented code, a `2.` list or an empty item can't
                    // interrupt the `[ ]` line, and a paragraph would join
                    // it. A loose list allows a blank here. A tight list
                    // never holds such a block, and its paragraphs join the
                    // `[ ]` line.
                    if !list.tight {
                        s.push_blank();
                    }
                    serialize_block(s, first_child);
                }
                None => serialize_block(s, first_child),
            }
            let mut prev = first_child;
            for child in children {
                separate(s, prev, child, list.tight);
                serialize_block(s, child);
                prev = child;
            }
        }
    }

    s.pop_prefix();
}
