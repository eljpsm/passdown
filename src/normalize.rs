//! The fixed Unicode-to-ASCII punctuation table. Applied to prose text nodes
//! only, never to code spans, code blocks, HTML, front matter, or link
//! destinations.

/// The ASCII replacement for a typographic character, or `None` if the
/// character is fine as-is.
pub fn replacement(ch: char) -> Option<&'static str> {
    match ch {
        // double quotes: “ ” „ ‟
        '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{201F}' => Some("\""),
        // single quotes / apostrophes: ‘ ’ ‚ ‛
        '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}' => Some("'"),
        // ellipsis
        '\u{2026}' => Some("..."),
        // em dash, horizontal bar
        '\u{2014}' | '\u{2015}' => Some("--"),
        // en dash, hyphen, non-breaking hyphen, minus sign
        '\u{2013}' | '\u{2010}' | '\u{2011}' | '\u{2212}' => Some("-"),
        // NBSP, en/em/thin space family, narrow NBSP, ideographic space
        '\u{00A0}' | '\u{2000}'..='\u{200A}' | '\u{202F}' | '\u{3000}' => Some(" "),
        // soft hyphen, zero-width space, zero-width no-break space
        '\u{00AD}' | '\u{200B}' | '\u{FEFF}' => Some(""),
        _ => None,
    }
}

/// One occurrence found in a text run: character offset plus what replaced it.
pub struct Occurrence {
    pub char_offset: usize,
    pub ch: char,
    pub replacement: &'static str,
}

/// Rewrite `text` through the table, reporting each occurrence.
pub fn normalize(text: &str) -> (String, Vec<Occurrence>) {
    // The table only maps non-ASCII characters, so ASCII text (nearly all
    // input) can skip the per-char walk.
    if text.is_ascii() {
        return (text.to_string(), Vec::new());
    }
    let mut out = String::with_capacity(text.len());
    let mut occurrences = Vec::new();
    for (char_offset, ch) in text.chars().enumerate() {
        match replacement(ch) {
            Some(replacement) => {
                occurrences.push(Occurrence {
                    char_offset,
                    ch,
                    replacement,
                });
                out.push_str(replacement);
            }
            None => out.push(ch),
        }
    }
    (out, occurrences)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_covers_the_usual_suspects() {
        let (out, occ) = normalize("“curly” and ‘single’, wait… A—B and A–B plus\u{00A0}nbsp");
        assert_eq!(
            out,
            "\"curly\" and 'single', wait... A--B and A-B plus nbsp"
        );
        assert_eq!(occ.len(), 8);
    }

    #[test]
    fn plain_ascii_untouched() {
        let (out, occ) = normalize("nothing special here");
        assert_eq!(out, "nothing special here");
        assert!(occ.is_empty());
    }

    #[test]
    fn removals() {
        let (out, _) = normalize("soft\u{00AD}hyphen zero\u{200B}width");
        assert_eq!(out, "softhyphen zerowidth");
    }
}
