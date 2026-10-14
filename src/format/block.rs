use comrak::nodes::{AlertType, AstNode, NodeValue};

use super::code::serialize_code_block;
use super::inline::{collect_chunks, collect_text};
use super::list::serialize_list;
use super::state::Serializer;
use super::table::serialize_table;
use super::wrap::wrap;

/// Serialize a run of sibling blocks with exactly one blank line between them,
/// plus an `<!-- -->` separator where `needs_separator` says so.
pub fn serialize_blocks<'a>(
    s: &mut Serializer<'_>,
    children: impl Iterator<Item = &'a AstNode<'a>>,
) {
    let mut prev: Option<&AstNode<'_>> = None;
    for child in children {
        if let Some(prev) = prev {
            s.push_blank();
            if needs_separator(prev, child) {
                s.push_line("<!-- -->");
                s.push_blank();
            }
        }
        serialize_block(s, child);
        prev = Some(child);
    }
}

/// True when `prev` and `next` would merge on reparse without an `<!-- -->`
/// between them:
///
/// - Lists of the same type. The fixed style normalizes every bullet to `-`
///   and every ordered delimiter to `.`, so lists that differed only by
///   marker would become one.
/// - A list, then a block that starts indented. Markers get one space after
///   them, which narrows the content column, so the block would fall into the
///   last item.
/// - Two indented code blocks. A link definition between them is dropped from
///   the output, and the blocks would join into one.
pub fn needs_separator<'a>(prev: &'a AstNode<'a>, next: &'a AstNode<'a>) -> bool {
    match (&prev.data.borrow().value, &next.data.borrow().value) {
        (NodeValue::List(a), NodeValue::List(b)) => a.list_type == b.list_type,
        (NodeValue::List(_), next) => starts_indented(next),
        (NodeValue::CodeBlock(a), NodeValue::CodeBlock(b)) => !a.fenced && !b.fenced,
        _ => false,
    }
}

/// True when the block's first emitted line starts with whitespace: indented
/// code, an untyped fence that keeps its offset, or an indented HTML block.
pub fn starts_indented(value: &NodeValue) -> bool {
    match value {
        NodeValue::CodeBlock(code) => {
            !code.fenced || (code.info.trim().is_empty() && code.fence_offset > 0)
        }
        NodeValue::HtmlBlock(html) => html.literal.starts_with([' ', '\t']),
        _ => false,
    }
}

/// The literal of a paragraph whose entire content is one display-math
/// span with its `$$` fences already on their own lines, or `None`. Only
/// that shape keeps the block form: reflowing content onto its own line
/// could produce a line that interrupts the paragraph on reparse (`- x`,
/// `# h`), which would split the math. Lines that already sat between the
/// fences survived block parsing once, so re-emitting them as-is is safe.
fn sole_display_math<'a>(node: &'a AstNode<'a>) -> Option<String> {
    let first = node.first_child()?;
    if first.next_sibling().is_some() {
        return None;
    }
    match &first.data.borrow().value {
        NodeValue::Math(math)
            if math.display_math
                && math.literal.starts_with('\n')
                && math.literal.ends_with('\n') =>
        {
            Some(math.literal.clone())
        }
        _ => None,
    }
}

/// Emit display math as a fenced block. Content lines are verbatim and,
/// like code blocks and tables, exempt from the 80-column cap.
fn serialize_display_math(s: &mut Serializer<'_>, literal: &str) {
    s.push_line("$$");
    for line in literal.trim_matches('\n').lines() {
        s.push_line(line);
    }
    s.push_line("$$");
}

/// Dispatch one block node to its serializer. Every arm clones or copies
/// what it needs out of `data` and then drops the borrow. Serializing
/// children re-borrows `node.data`, so holding it across the call would
/// panic at runtime.
pub fn serialize_block<'a>(s: &mut Serializer<'_>, node: &'a AstNode<'a>) {
    let data = node.data.borrow();
    match &data.value {
        NodeValue::Paragraph => {
            drop(data);
            if let Some(literal) = sole_display_math(node) {
                serialize_display_math(s, &literal);
                return;
            }
            let chunks = collect_chunks(node, &mut s.diags);
            for line in wrap(&chunks, s.width_budget()) {
                s.push_line(&line);
            }
        }
        NodeValue::Heading(heading) => {
            let level = heading.level as usize;
            drop(data);
            let text = collect_text(node, &mut s.diags);
            let marker = "#".repeat(level);
            if text.is_empty() {
                s.push_line(&marker);
            } else {
                s.push_line(&format!("{marker} {text}"));
            }
        }
        NodeValue::ThematicBreak => {
            drop(data);
            s.push_line("---");
        }
        NodeValue::BlockQuote => {
            drop(data);
            s.push_prefix("> ", "> ");
            serialize_blocks(s, node.children());
            s.pop_prefix();
        }
        NodeValue::List(list) => {
            let list = *list;
            drop(data);
            serialize_list(s, node, &list);
        }
        NodeValue::Alert(alert) => {
            let kind = match alert.alert_type {
                AlertType::Note => "NOTE",
                AlertType::Tip => "TIP",
                AlertType::Important => "IMPORTANT",
                AlertType::Warning => "WARNING",
                AlertType::Caution => "CAUTION",
            };
            let header = match &alert.title {
                Some(title) => format!("[!{kind}] {title}"),
                None => format!("[!{kind}]"),
            };
            drop(data);
            s.push_prefix("> ", "> ");
            s.push_line(&header);
            serialize_blocks(s, node.children());
            s.pop_prefix();
        }
        NodeValue::Table(table) => {
            let table = (**table).clone();
            drop(data);
            serialize_table(s, node, &table);
        }
        NodeValue::CodeBlock(code) => {
            let code = (**code).clone();
            drop(data);
            serialize_code_block(s, node, &code);
        }
        NodeValue::FootnoteDefinition(def) => {
            let first = format!("[^{}]: ", def.name);
            drop(data);
            s.push_prefix(&first, "    ");
            // The parser skips spaces after `[^n]:`, so an indented first
            // block has to start on the next line.
            if node
                .first_child()
                .is_some_and(|child| starts_indented(&child.data.borrow().value))
            {
                s.push_line("");
            }
            serialize_blocks(s, node.children());
            s.pop_prefix();
        }
        NodeValue::FrontMatter(literal) => {
            let literal = literal.clone();
            drop(data);
            for line in literal.trim_end_matches('\n').lines() {
                s.push_line(line);
            }
        }
        NodeValue::HtmlBlock(html) => {
            let literal = html.literal.clone();
            drop(data);
            for line in literal.trim_end_matches('\n').lines() {
                s.push_line(line);
            }
        }
        // Not yet implemented: emit the block verbatim from source. Only
        // reachable at the top level while milestones land.
        _ => {
            let sourcepos = data.sourcepos;
            drop(data);
            s.push_verbatim_span(sourcepos.start.line, sourcepos.end.line);
        }
    }
}
