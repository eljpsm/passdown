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
    /// A code fence with no language tag. A language can't be guessed, so
    /// this is unfixable.
    MissingCodeLanguage,
    /// An indented (4-space) code block. It can never carry a language, so
    /// it is unfixable until converted to a fence by hand.
    IndentedCodeBlock,
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
            DiagKind::MissingCodeLanguage | DiagKind::IndentedCodeBlock => Severity::Error,
            DiagKind::NonAsciiPunct { .. } => Severity::Note,
        }
    }

    pub fn render(&self, path: &Path) -> String {
        let loc = format!("{}:{}:{}", path.display(), self.line, self.col);
        match &self.kind {
            DiagKind::MissingCodeLanguage => {
                format!("{loc}: error: code fence has no language")
            }
            DiagKind::IndentedCodeBlock => {
                format!(
                    "{loc}: error: indented code block (convert to a fenced block with a language)"
                )
            }
            DiagKind::NonAsciiPunct { ch, replacement } => {
                format!("{loc}: note: non-ASCII punctuation {ch:?} (replaced with {replacement:?})")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(kind: DiagKind) -> String {
        Diagnostic {
            line: 3,
            col: 1,
            kind,
        }
        .render(Path::new("doc.md"))
    }

    #[test]
    fn messages() {
        assert_eq!(
            render(DiagKind::MissingCodeLanguage),
            "doc.md:3:1: error: code fence has no language"
        );
        assert_eq!(
            render(DiagKind::IndentedCodeBlock),
            "doc.md:3:1: error: indented code block (convert to a fenced block with a language)"
        );
    }

    #[test]
    fn severities() {
        let diag = |kind| Diagnostic {
            line: 1,
            col: 1,
            kind,
        };
        assert_eq!(
            diag(DiagKind::MissingCodeLanguage).severity(),
            Severity::Error
        );
        assert_eq!(
            diag(DiagKind::IndentedCodeBlock).severity(),
            Severity::Error
        );
        assert_eq!(
            diag(DiagKind::NonAsciiPunct {
                ch: '\u{2014}',
                replacement: "--"
            })
            .severity(),
            Severity::Note
        );
    }
}
