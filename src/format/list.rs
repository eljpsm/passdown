use comrak::nodes::{AstNode, ListType, NodeList, NodeValue};

use super::block::serialize_block;
use super::inline::{Chunk, collect_chunks};
use super::state::Serializer;
use super::wrap::wrap;

/// Serialize a list: bullets become `- `, ordered items `N. `, with blank
/// lines between items only when the list is loose.
pub fn serialize_list<'a>(s: &mut Serializer<'_>, node: &'a AstNode<'a>, list: &NodeList) {
    for (i, item) in node.children().enumerate() {
        if i > 0 && !list.tight {
            s.push_blank();
        }
        serialize_item(s, item, list, list.start + i);
    }
}

/// Serialize one list item under its marker prefix. A task-list checkbox is
/// injected as the first chunk of the leading paragraph so wrapping accounts
/// for its width. Nested sibling lists of the same type get the usual
/// `<!-- -->` separator so they don't merge on reparse.
fn serialize_item<'a>(
    s: &mut Serializer<'_>,
    item: &'a AstNode<'a>,
    list: &NodeList,
    number: usize,
) {
    let checkbox = match &item.data.borrow().value {
        NodeValue::TaskItem(task) => Some(if task.symbol.is_some() { "[x]" } else { "[ ]" }),
        _ => None,
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

    let mut emitted_any = false;
    let mut prev: Option<&AstNode<'_>> = None;
    for (i, child) in item.children().enumerate() {
        if i > 0 && !list.tight {
            s.push_blank();
        }
        if let Some(prev) = prev
            && super::block::needs_list_separator(prev, child)
        {
            // In a tight item the separator carries no blank lines. Blanks
            // inside the item would make the list loose on reparse and break
            // idempotency.
            s.push_line("<!-- -->");
            if !list.tight {
                s.push_blank();
            }
        }
        let is_paragraph = matches!(child.data.borrow().value, NodeValue::Paragraph);
        if i == 0
            && let Some(checkbox) = checkbox
            && is_paragraph
        {
            let mut chunks = collect_chunks(child, &mut s.diags);
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
        } else {
            if i == 0
                && let Some(checkbox) = checkbox
            {
                s.push_line(checkbox);
            }
            serialize_block(s, child);
        }
        emitted_any = true;
        prev = Some(child);
    }
    // An empty item still needs a line to carry its marker (a bare `-`).
    if !emitted_any {
        s.push_line(checkbox.unwrap_or(""));
    }

    s.pop_prefix();
}
