use comrak::nodes::{AlertType, AstNode, NodeValue};

use super::code::serialize_code_block;
use super::inline::{collect_chunks, collect_text};
use super::list::serialize_list;
use super::state::Serializer;
use super::table::serialize_table;
use super::wrap::wrap;

/// Serialize a run of sibling blocks, `separate`d from each other.
pub fn serialize_blocks<'a>(
    s: &mut Serializer<'_>,
    children: impl Iterator<Item = &'a AstNode<'a>>,
) {
    let mut prev: Option<&AstNode<'_>> = None;
    for child in children {
        if let Some(prev) = prev {
            separate(s, prev, child, false);
        }
        serialize_block(s, child);
        prev = Some(child);
    }
}

/// Separate `next` from its sibling `prev`: one blank line, plus an
/// `<!-- -->` separator where `needs_separator` says so. Inside a tight list
/// item there are no blank lines, which would make the list loose on reparse.
pub fn separate<'a>(
    s: &mut Serializer<'_>,
    prev: &'a AstNode<'a>,
    next: &'a AstNode<'a>,
    tight: bool,
) {
    if !tight && !ends_with_blank_line(prev) {
        s.push_blank();
    }
    if needs_separator(prev, next) {
        s.push_line("<!-- -->");
        if !tight {
            s.push_blank();
        }
    }
}

/// True when the block's last emitted line is already blank. An unclosed
/// HTML block (`<pre>` with no `</pre>`) runs to the end of its container
/// and keeps the blank lines it took in. Another blank after it would be
/// taken in too. A quote's blank lines are `>` lines, so it never counts.
fn ends_with_blank_line<'a>(node: &'a AstNode<'a>) -> bool {
    match &node.data.borrow().value {
        NodeValue::HtmlBlock(html) => html
            .literal
            .lines()
            .last()
            .is_some_and(|line| line.trim_matches([' ', '\t']).is_empty()),
        NodeValue::List(_)
        | NodeValue::Item(_)
        | NodeValue::TaskItem(_)
        | NodeValue::FootnoteDefinition(_) => node.last_child().is_some_and(ends_with_blank_line),
        _ => false,
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

/// `---`, except where it misparses. In a list item it can join a `- `
/// marker into a bare break or underline the paragraph above it as a
/// heading. As the first block it opens front matter. `***` has neither
/// problem.
fn thematic_break<'a>(node: &'a AstNode<'a>) -> &'static str {
    let in_list = node
        .ancestors()
        .any(|n| matches!(n.data.borrow().value, NodeValue::List(_)));
    let first = node.previous_sibling().is_none()
        && node
            .parent()
            .is_some_and(|p| matches!(p.data.borrow().value, NodeValue::Document));
    if in_list || first { "***" } else { "---" }
}

/// The line as emitted at `column`. A tab's width depends on its column, so
/// at a new column the indent can reach four columns and turn an HTML block
/// into indented code. Only then is each tab replaced with one space. The
/// source indent was at most three columns and each tab took at least one,
/// so that keeps it an HTML block. Otherwise the tabs stay, since they can
/// be content (inside a `<pre>` an earlier block opened).
fn untab_indent(line: &str, column: usize) -> String {
    let rest = line.trim_start_matches([' ', '\t']);
    let indent = &line[..line.len() - rest.len()];
    let end = indent.chars().fold(column, |col, c| {
        if c == '\t' {
            col + 4 - col % 4
        } else {
            col + 1
        }
    });
    if end - column < 4 {
        return line.to_string();
    }
    format!("{}{rest}", indent.replace('\t', " "))
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
            s.push_line(thematic_break(node));
        }
        NodeValue::BlockQuote => {
            drop(data);
            s.push_prefix("> ", "> ");
            // An empty quote still needs its `>`, or its neighbours would
            // merge.
            if node.first_child().is_none() {
                s.push_line("");
            }
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
            // block has to start on the next line. An empty definition still
            // needs its label.
            if node
                .first_child()
                .is_none_or(|child| starts_indented(&child.data.borrow().value))
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
                s.push_literal_line(line);
            }
        }
        NodeValue::HtmlBlock(html) => {
            let literal = html.literal.clone();
            drop(data);
            for (i, line) in literal.lines().enumerate() {
                if i == 0 {
                    let column = s.next_column();
                    s.push_literal_line(&untab_indent(line, column));
                } else {
                    s.push_literal_line(line);
                }
            }
        }
        // Unreachable with the extensions comrak_options enables. Keep the
        // source of any block a comrak upgrade adds rather than drop it.
        _ => {
            let sourcepos = data.sourcepos;
            drop(data);
            s.push_verbatim_span(sourcepos.start.line, sourcepos.end.line);
        }
    }
}
