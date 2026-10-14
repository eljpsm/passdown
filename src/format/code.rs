use comrak::nodes::{AstNode, NodeCodeBlock};

use crate::diagnostics::{DiagKind, Diagnostic};

use super::state::Serializer;

/// Emit a code block as backtick-fenced with its language tag. A block with
/// no language is unfixable: report an Error diagnostic and re-emit the
/// block's content unchanged. Fences and indented blocks get distinct
/// diagnostics, since only a fence can have a language added to it.
pub fn serialize_code_block<'a>(
    s: &mut Serializer<'_>,
    node: &'a AstNode<'a>,
    code: &NodeCodeBlock,
) {
    let info = code.info.trim();
    if info.is_empty() {
        // Unfixable: a language can't be guessed. Report it and re-emit the
        // block's content unchanged.
        let sourcepos = node.data.borrow().sourcepos;
        s.diags.push(Diagnostic {
            line: sourcepos.start.line,
            col: sourcepos.start.column,
            kind: if code.fenced {
                DiagKind::MissingCodeLanguage
            } else {
                DiagKind::IndentedCodeBlock
            },
        });
        emit_untyped(s, code);
        return;
    }

    // A backtick fence cannot hold a backtick in its info string.
    let fence_char = if info.contains('`') { '~' } else { '`' };
    let fence = fence_char
        .to_string()
        .repeat(fence_length(&code.literal, fence_char));
    // Without a space, `~~~` then `~a` would read as a longer fence.
    let gap = if info.starts_with(fence_char) {
        " "
    } else {
        ""
    };
    s.push_line(&format!("{fence}{gap}{info}"));
    for line in code.literal.lines() {
        s.push_literal_line(line);
    }
    s.push_line(&fence);
}

/// Long enough that no literal line can close the fence early: at least
/// three, and one more than the longest leading `fence_char` run in the
/// content.
fn fence_length(literal: &str, fence_char: char) -> usize {
    let longest = literal
        .lines()
        .filter_map(|line| leading_run(line, fence_char))
        .map(|(run, _)| run)
        .max()
        .unwrap_or(0);
    (longest + 1).max(3)
}

/// The `fence_char` run a line starts with, and the rest of the line. A
/// closing fence may be indented up to three columns, so a run after such an
/// indent counts too. A tab can be as narrow as one column, so each counts
/// as one.
fn leading_run(line: &str, fence_char: char) -> Option<(usize, &str)> {
    let content = line.trim_start_matches([' ', '\t']);
    if line.len() - content.len() > 3 {
        return None;
    }
    let rest = content.trim_start_matches(fence_char);
    Some((content.len() - rest.len(), rest))
}

/// Re-emit an unfixable block from its parsed fields, not from source
/// spans. The parser already removed each line's own container prefix and
/// indent from the literal, so writing it back through the prefix stack
/// reparses to the same block. Source spans give one start column for every
/// line and an end that is unreliable for blocks that are not closed fences.
fn emit_untyped(s: &mut Serializer<'_>, code: &NodeCodeBlock) {
    if !code.fenced {
        push_indented(s, "    ", &code.literal);
        return;
    }
    // Content keeps the fence's indent: the parser strips up to that many
    // spaces, and a fence-like line without it would close the block.
    let indent = " ".repeat(code.fence_offset);
    let fence_char = char::from(code.fence_char);
    // A tab's width depends on its column, and the content column can move.
    // A line the source indented past a closing fence may then close it, so
    // grow the fence past every run that could. Only a run with nothing but
    // whitespace after it can close a fence.
    let closing = code
        .literal
        .lines()
        .filter_map(|line| {
            leading_run(&format!("{indent}{line}"), fence_char)
                .filter(|(_, rest)| rest.trim_matches([' ', '\t']).is_empty())
                .map(|(run, _)| run + 1)
        })
        .max()
        .unwrap_or(0);
    let length = code.fence_length.max(closing);
    let marker = fence_char.to_string().repeat(length);
    let fence = format!("{indent}{marker}");
    s.push_line(&fence);
    push_indented(s, &indent, &code.literal);
    // Always closed: an unclosed fence absorbs the blank lines up to the end
    // of its container, and only a closing fence keeps them.
    s.push_line(&fence);
}

/// Emit each literal line behind `indent`. A blank line stays blank: the
/// indent alone would be trailing whitespace.
fn push_indented(s: &mut Serializer<'_>, indent: &str, literal: &str) {
    for line in literal.lines() {
        if line.is_empty() {
            s.push_line("");
        } else {
            s.push_literal_line(&format!("{indent}{line}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn untyped(first: &str, cont: &str, code: NodeCodeBlock) -> String {
        let mut s = Serializer::new(vec![]);
        s.push_prefix(first, cont);
        emit_untyped(&mut s, &code);
        s.pop_prefix();
        s.finish()
    }

    fn indented(literal: &str) -> NodeCodeBlock {
        NodeCodeBlock {
            literal: literal.to_string(),
            ..NodeCodeBlock::default()
        }
    }

    fn fenced(fence: &str, offset: usize, literal: &str, closed: bool) -> NodeCodeBlock {
        NodeCodeBlock {
            fenced: true,
            fence_char: fence.as_bytes()[0],
            fence_length: fence.len(),
            fence_offset: offset,
            literal: literal.to_string(),
            closed,
            ..NodeCodeBlock::default()
        }
    }

    #[test]
    fn untyped_indented_keeps_every_line_under_prefix() {
        let code = indented("a\n\nb\n");
        assert_eq!(untyped("- ", "  ", code.clone()), "-     a\n\n      b\n");
        assert_eq!(untyped("> ", "> ", code), ">     a\n>\n>     b\n");
    }

    #[test]
    fn untyped_fence_keeps_char_length_and_offset() {
        let code = fenced("~~~~", 2, "a\n ```\n", true);
        assert_eq!(
            untyped("> ", "> ", code),
            ">   ~~~~\n>   a\n>    ```\n>   ~~~~\n"
        );
    }

    #[test]
    fn untyped_unclosed_fence_is_closed_and_keeps_blank_lines() {
        let code = fenced("```", 0, "x\n\n", false);
        assert_eq!(untyped("- ", "  ", code), "- ```\n  x\n\n  ```\n");
    }

    #[test]
    fn untyped_fence_grows_past_tab_indented_runs() {
        let code = fenced("```", 0, "a\n\t```\n", true);
        assert_eq!(untyped("- ", "  ", code), "- ````\n  a\n  \t```\n  ````\n");
        let code = fenced("```", 2, "\t```\n", true);
        assert_eq!(untyped("- ", "  ", code), "-   ````\n    \t```\n    ````\n");
    }

    #[test]
    fn untyped_fence_ignores_runs_that_cannot_close_it() {
        let code = fenced("```", 0, "```rust\n", true);
        assert_eq!(untyped("- ", "  ", code), "- ```\n  ```rust\n  ```\n");
    }

    #[test]
    fn fence_grows_past_content_backticks() {
        assert_eq!(fence_length("plain\ncode\n", '`'), 3);
        assert_eq!(fence_length("```\n", '`'), 4);
        assert_eq!(fence_length("a\n``````\nb\n", '`'), 7);
        assert_eq!(fence_length("", '`'), 3);
    }

    #[test]
    fn fence_counts_runs_indented_up_to_three_spaces() {
        assert_eq!(fence_length("   ```\n", '`'), 4);
        assert_eq!(fence_length("    ```\n", '`'), 3);
        assert_eq!(fence_length("  `````\n", '`'), 6);
        assert_eq!(fence_length("\t```\n", '`'), 4);
        assert_eq!(fence_length("   \t```\n", '`'), 3);
    }

    #[test]
    fn fence_counts_only_its_own_char() {
        assert_eq!(fence_length("~~~\n", '~'), 4);
        assert_eq!(fence_length("```\n", '~'), 3);
    }
}
