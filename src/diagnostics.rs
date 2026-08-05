use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Fixable by `fix`; reported by `check`.
    Note,
    /// Unfixable; both subcommands report it and exit nonzero.
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagKind {
    /// A code block with no language tag. A language can't be guessed, so
    /// this is the one unfixable diagnostic.
    MissingCodeLanguage,
    /// Non-ASCII punctuation rewritten by the fixed table in `normalize`.
    NonAsciiPunct { ch: char, replacement: &'static str },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// 1-indexed source line.
    pub line: usize,
    /// 1-indexed source column.
    pub col: usize,
    pub kind: DiagKind,
}

impl Diagnostic {
    pub fn severity(&self) -> Severity {
        match self.kind {
            DiagKind::MissingCodeLanguage => Severity::Error,
            DiagKind::NonAsciiPunct { .. } => Severity::Note,
        }
    }

    pub fn render(&self, path: &Path) -> String {
        let loc = format!("{}:{}:{}", path.display(), self.line, self.col);
        match &self.kind {
            DiagKind::MissingCodeLanguage => {
                format!("{loc}: error: code block has no language")
            }
            DiagKind::NonAsciiPunct { ch, replacement } => {
                format!("{loc}: note: non-ASCII punctuation {ch:?} (replaced with {replacement:?})")
            }
        }
    }
}
