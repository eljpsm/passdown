use super::escape::escape_line_start;
use super::inline::Chunk;

/// Greedy-fill chunks into lines of at most `budget` display columns. A chunk
/// that alone exceeds the budget gets its own overlong line, never broken.
/// Line starts that would open a block construct on reparse are escaped
/// afterwards (the escape may push a line one character past the budget;
/// that is accepted).
pub fn wrap(chunks: &[Chunk], budget: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_width = 0usize;

    for chunk in chunks {
        let width = chunk.width();
        if current.is_empty() {
            current = chunk.text.clone();
            current_width = width;
        } else if current_width + 1 + width <= budget {
            current.push(' ');
            current.push_str(&chunk.text);
            current_width += 1 + width;
        } else {
            lines.push(current);
            current = chunk.text.clone();
            current_width = width;
        }
        if chunk.break_after {
            lines.push(std::mem::take(&mut current));
            current_width = 0;
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }

    lines.into_iter().map(escape_line_start).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunks(words: &[&str]) -> Vec<Chunk> {
        words
            .iter()
            .map(|w| Chunk {
                text: w.to_string(),
                break_after: false,
            })
            .collect()
    }

    #[test]
    fn fills_greedily() {
        let out = wrap(&chunks(&["aaa", "bbb", "ccc"]), 7);
        assert_eq!(out, ["aaa bbb", "ccc"]);
    }

    #[test]
    fn overlong_chunk_gets_own_line() {
        let out = wrap(&chunks(&["short", "averyveryverylongtoken", "end"]), 10);
        assert_eq!(out, ["short", "averyveryverylongtoken", "end"]);
    }

    #[test]
    fn hard_break_forces_newline() {
        let mut c = chunks(&["one\\", "two"]);
        c[0].break_after = true;
        let out = wrap(&c, 80);
        assert_eq!(out, ["one\\", "two"]);
    }

    #[test]
    fn wrapped_line_starts_are_escaped() {
        let out = wrap(&chunks(&["xxxxxx", "-", "yyy"]), 6);
        assert_eq!(out, ["xxxxxx", "\\- yyy"]);
    }
}
