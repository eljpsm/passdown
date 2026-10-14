use unicode_width::UnicodeWidthStr;

use crate::diagnostics::Diagnostic;

/// The fixed output width. Deliberately not configurable.
pub const LINE_WIDTH: usize = 80;

/// Minimum usable width so deeply nested content still wraps sanely.
const MIN_BUDGET: usize = 20;

struct PrefixFrame {
    first: String,
    cont: String,
    used_first: bool,
}

/// All output flows through `push_line`, which applies the current prefix
/// stack (list indents, `> `). No serializer code writes indentation by hand.
pub struct Serializer<'a> {
    out: String,
    pub source_lines: Vec<&'a str>,
    prefix: Vec<PrefixFrame>,
    pub diags: Vec<Diagnostic>,
}

impl<'a> Serializer<'a> {
    pub fn new(source_lines: Vec<&'a str>) -> Self {
        Serializer {
            out: String::new(),
            source_lines,
            prefix: Vec::new(),
            diags: Vec::new(),
        }
    }

    /// Enter a container block. `first` prefixes the first line emitted after
    /// this call (e.g. `- `); `cont` prefixes every later line (e.g. two
    /// spaces). The two must have equal display width.
    pub fn push_prefix(&mut self, first: &str, cont: &str) {
        self.prefix.push(PrefixFrame {
            first: first.to_string(),
            cont: cont.to_string(),
            used_first: false,
        });
    }

    pub fn pop_prefix(&mut self) {
        self.prefix.pop();
    }

    /// Join the stack for one line, consuming each frame's `first` on its
    /// first use. Nested frames pushed on the same line all spend their
    /// `first` together (e.g. `- - inner` for a list directly inside a
    /// list item).
    fn compose_prefix(&mut self) -> String {
        self.compose_frames(self.prefix.len())
    }

    /// `compose_prefix` over the bottom `count` frames only.
    fn compose_frames(&mut self, count: usize) -> String {
        let mut composed = String::new();
        for frame in &mut self.prefix[..count] {
            if frame.used_first {
                composed.push_str(&frame.cont);
            } else {
                composed.push_str(&frame.first);
                frame.used_first = true;
            }
        }
        composed
    }

    /// Index of the third frame in a run of unspent `- ` markers, if any.
    /// Three such markers alone on a line reparse as a thematic break.
    fn dash_run_cut(&self) -> Option<usize> {
        let mut run = 0;
        self.prefix.iter().position(|frame| {
            run = if !frame.used_first && frame.first == "- " {
                run + 1
            } else {
                0
            };
            run == 3
        })
    }

    /// The column where the next line's content starts. Counted in bytes, as
    /// the parser counts columns for tab stops.
    pub fn next_column(&self) -> usize {
        self.prefix
            .iter()
            .map(|f| if f.used_first { &f.cont } else { &f.first }.len())
            .sum()
    }

    /// Emit one line of content under the current prefix. Trailing whitespace
    /// is always trimmed (this is what turns a blank quoted line into a bare
    /// `>`).
    pub fn push_line(&mut self, content: &str) {
        // A marker-only line must not hold `- - -`. Spend the markers before
        // the third on a line of their own.
        if content.is_empty() {
            while let Some(cut) = self.dash_run_cut() {
                let markers = self.compose_frames(cut);
                self.out.push_str(markers.trim_end());
                self.out.push('\n');
            }
        }
        let prefix = self.compose_prefix();
        let mut line = prefix;
        line.push_str(content);
        let trimmed = line.trim_end();
        self.out.push_str(trimmed);
        self.out.push('\n');
    }

    /// Emit one line of a literal block (code, HTML, front matter). Trailing
    /// whitespace is content there, so only an empty line is trimmed.
    pub fn push_literal_line(&mut self, content: &str) {
        if content.is_empty() {
            self.push_line("");
            return;
        }
        let prefix = self.compose_prefix();
        self.out.push_str(&prefix);
        self.out.push_str(content);
        self.out.push('\n');
    }

    pub fn push_blank(&mut self) {
        self.push_line("");
    }

    /// Emit source lines `start..=end` (1-indexed) exactly as written. Only
    /// valid at the top level, where no prefix applies.
    pub fn push_verbatim_span(&mut self, start: usize, end: usize) {
        debug_assert!(self.prefix.is_empty());
        for idx in start..=end {
            if let Some(line) = self.source_lines.get(idx - 1) {
                self.out.push_str(line);
                self.out.push('\n');
            }
        }
    }

    /// Columns available for content on continuation lines.
    pub fn width_budget(&self) -> usize {
        let used: usize = self.prefix.iter().map(|f| f.cont.width()).sum();
        LINE_WIDTH.saturating_sub(used).max(MIN_BUDGET)
    }

    /// Final-output policy: no leading blanks (never produced), no trailing
    /// blanks, exactly one final newline; an empty document stays empty.
    pub fn finish(self) -> String {
        let mut out = self.out;
        while out.ends_with('\n') {
            out.pop();
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_first_then_cont() {
        let mut s = Serializer::new(vec![]);
        s.push_prefix("- ", "  ");
        s.push_line("first");
        s.push_line("second");
        s.pop_prefix();
        assert_eq!(s.finish(), "- first\n  second\n");
    }

    #[test]
    fn nested_prefixes_consume_first_together() {
        let mut s = Serializer::new(vec![]);
        s.push_prefix("- ", "  ");
        s.push_prefix("- ", "  ");
        s.push_line("inner");
        s.push_line("more");
        assert_eq!(s.finish(), "- - inner\n    more\n");
    }

    #[test]
    fn marker_only_line_never_forms_thematic_break() {
        let mut s = Serializer::new(vec![]);
        s.push_prefix("- ", "  ");
        s.push_prefix("- ", "  ");
        s.push_prefix("- ", "  ");
        s.push_line("");
        assert_eq!(s.finish(), "- -\n    -\n");
    }

    #[test]
    fn blank_line_in_quote_is_bare_marker() {
        let mut s = Serializer::new(vec![]);
        s.push_prefix("> ", "> ");
        s.push_line("a");
        s.push_blank();
        s.push_line("b");
        assert_eq!(s.finish(), "> a\n>\n> b\n");
    }

    #[test]
    fn literal_line_keeps_trailing_whitespace() {
        let mut s = Serializer::new(vec![]);
        s.push_prefix("> ", "> ");
        s.push_literal_line("a  ");
        s.push_literal_line("");
        s.push_literal_line("b");
        assert_eq!(s.finish(), "> a  \n>\n> b\n");
    }

    #[test]
    fn budget_shrinks_with_nesting() {
        let mut s = Serializer::new(vec![]);
        assert_eq!(s.width_budget(), 80);
        s.push_prefix("> ", "> ");
        assert_eq!(s.width_budget(), 78);
        s.push_prefix("- ", "  ");
        assert_eq!(s.width_budget(), 76);
    }

    #[test]
    fn finish_normalizes_final_newline() {
        let mut s = Serializer::new(vec![]);
        s.push_line("x");
        s.push_blank();
        assert_eq!(s.finish(), "x\n");
        let s = Serializer::new(vec![]);
        assert_eq!(s.finish(), "");
    }
}
