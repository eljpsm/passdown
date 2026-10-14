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

    let fence = "`".repeat(fence_length(&code.literal));
    s.push_line(&format!("{fence}{info}"));
    for line in code.literal.lines() {
        s.push_literal_line(line);
    }
    s.push_line(&fence);
}

/// Long enough that no literal line can close the fence early: at least
/// three, and one more than the longest leading backtick run in the content.
/// A closing fence may be indented up to three spaces, so runs after such an
/// indent count too.
fn fence_length(literal: &str) -> usize {
    let longest = literal
        .lines()
        .map(|line| {
            let content = line.trim_start_matches(' ');
            if line.len() - content.len() > 3 {
                return 0;
            }
            content.chars().take_while(|&c| c == '`').count()
        })
        .max()
        .unwrap_or(0);
    (longest + 1).max(3)
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
    let marker = char::from(code.fence_char)
        .to_string()
        .repeat(code.fence_length);
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
    fn fence_grows_past_content_backticks() {
        assert_eq!(fence_length("plain\ncode\n"), 3);
        assert_eq!(fence_length("```\n"), 4);
        assert_eq!(fence_length("a\n``````\nb\n"), 7);
        assert_eq!(fence_length(""), 3);
    }

    #[test]
    fn fence_counts_runs_indented_up_to_three_spaces() {
        assert_eq!(fence_length("   ```\n"), 4);
        assert_eq!(fence_length("    ```\n"), 3);
        assert_eq!(fence_length("  `````\n"), 6);
    }
}
