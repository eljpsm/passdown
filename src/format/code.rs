use comrak::nodes::{AstNode, NodeCodeBlock};

use crate::diagnostics::{DiagKind, Diagnostic};

use super::state::Serializer;

/// Emit a code block as backtick-fenced with its language tag. A block with
/// no language is unfixable: report an Error diagnostic and reproduce the
/// block exactly as written. Fences and indented blocks get distinct
/// diagnostics, since only a fence can have a language added to it.
pub fn serialize_code_block<'a>(
    s: &mut Serializer<'_>,
    node: &'a AstNode<'a>,
    code: &NodeCodeBlock,
) {
    let info = code.info.trim();
    if info.is_empty() {
        // Unfixable: a language can't be guessed. Report it and emit the
        // block exactly as written.
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
        // Indented blocks report an end position that can overshoot into
        // trailing blank lines (end.column == 0); walk back to real content.
        let mut end = sourcepos.end.line;
        if sourcepos.end.column == 0 {
            end = end.saturating_sub(1);
        }
        while end > sourcepos.start.line
            && s.source_lines
                .get(end - 1)
                .is_some_and(|l| l.trim().is_empty())
        {
            end -= 1;
        }
        // An indented block's start column sits past its 4-space indent;
        // re-add the indent after stripping the container context so the
        // block stays indented code on reparse.
        let reindent = if code.fenced { "" } else { "    " };
        emit_verbatim(
            s,
            sourcepos.start.line,
            end,
            sourcepos.start.column,
            reindent,
        );
        return;
    }

    let fence = "`".repeat(fence_length(&code.literal));
    s.push_line(&format!("{fence}{info}"));
    for line in code.literal.trim_end_matches('\n').lines() {
        s.push_line(line);
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

/// Emit a block's source lines through the current prefix stack, stripping
/// the columns the block's own container context occupied in the source
/// (list indentation, `> ` markers) so prefixes aren't doubled.
fn emit_verbatim(s: &mut Serializer<'_>, start: usize, end: usize, column: usize, reindent: &str) {
    for idx in start..=end {
        let Some(line) = s.source_lines.get(idx - 1) else {
            continue;
        };
        let stripped: String = line.chars().skip(column - 1).collect();
        if stripped.is_empty() {
            s.push_line("");
        } else {
            s.push_line(&format!("{reindent}{stripped}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
