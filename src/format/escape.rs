/// Escape characters in prose text that could open inline constructs on
/// reparse. Applied to every word from a Text node; never to code spans,
/// HTML, or link destinations. Deliberately conservative: always escaping
/// `` ` `` `*` `_` `[` `]` `\` keeps output stable regardless of context.
pub fn escape_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    for (i, &ch) in chars.iter().enumerate() {
        let next = chars.get(i + 1).copied();
        match ch {
            '\\' | '`' | '*' | '_' | '[' | ']' => {
                out.push('\\');
                out.push(ch);
            }
            '<' if next
                .is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '!' | '?')) =>
            {
                out.push('\\');
                out.push(ch);
            }
            '!' if next == Some('[') => {
                out.push('\\');
                out.push(ch);
            }
            '&' if looks_like_entity(&chars[i + 1..]) => {
                out.push('\\');
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out
}

/// True if `rest` begins like an HTML entity body (`amp;`, `#38;`, `#x26;`).
fn looks_like_entity(rest: &[char]) -> bool {
    let mut i = 0;
    if rest.first() == Some(&'#') {
        i += 1;
        if matches!(rest.get(i), Some('x') | Some('X')) {
            i += 1;
            let start = i;
            while rest.get(i).is_some_and(|c| c.is_ascii_hexdigit()) {
                i += 1;
            }
            return i > start && rest.get(i) == Some(&';');
        }
        let start = i;
        while rest.get(i).is_some_and(|c| c.is_ascii_digit()) {
            i += 1;
        }
        return i > start && rest.get(i) == Some(&';');
    }
    let start = i;
    while rest.get(i).is_some_and(|c| c.is_ascii_alphanumeric()) {
        i += 1;
    }
    i > start && rest.get(i) == Some(&';')
}

/// Escape a wrapped line whose first characters would start a block construct
/// on reparse (list marker, heading, quote, setext underline, fence).
/// Emphasis delimiters never trigger this: `*` and `_` in prose are already
/// escaped by `escape_text`, and delimiter runs glue to their word.
pub fn escape_line_start(line: String) -> String {
    let bytes = line.as_bytes();
    if bytes.is_empty() {
        return line;
    }

    let escape_at = |line: &str, idx: usize| -> String {
        let mut out = String::with_capacity(line.len() + 1);
        out.push_str(&line[..idx]);
        out.push('\\');
        out.push_str(&line[idx..]);
        out
    };

    match bytes[0] {
        b'>' => return escape_at(&line, 0),
        b'-' | b'+' => {
            if bytes.get(1).is_none_or(|&b| b == b' ') {
                return escape_at(&line, 0);
            }
            if bytes[0] == b'-' && bytes.iter().all(|&b| b == b'-') {
                return escape_at(&line, 0);
            }
            // A bare `+++` at the top of a document would flip the front
            // matter delimiter to TOML on reparse.
            if line == "+++" {
                return escape_at(&line, 0);
            }
        }
        b'=' if bytes.iter().all(|&b| b == b'=') => {
            return escape_at(&line, 0);
        }
        b'#' => {
            let hashes = bytes.iter().take_while(|&&b| b == b'#').count();
            if hashes <= 6 && bytes.get(hashes).is_none_or(|&b| b == b' ') {
                return escape_at(&line, 0);
            }
        }
        b'~' if bytes.len() >= 3 && bytes[1] == b'~' && bytes[2] == b'~' => {
            return escape_at(&line, 0);
        }
        b'0'..=b'9' => {
            let digits = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
            if digits <= 9
                && matches!(bytes.get(digits), Some(b'.') | Some(b')'))
                && bytes.get(digits + 1).is_none_or(|&b| b == b' ')
            {
                return escape_at(&line, digits);
            }
        }
        _ => {}
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_inline_specials() {
        assert_eq!(escape_text("a*b"), "a\\*b");
        assert_eq!(escape_text("snake_case"), "snake\\_case");
        assert_eq!(escape_text("x`y"), "x\\`y");
        assert_eq!(escape_text("a[b]c"), "a\\[b\\]c");
        assert_eq!(escape_text("back\\slash"), "back\\\\slash");
    }

    #[test]
    fn angle_and_amp_only_when_dangerous() {
        assert_eq!(escape_text("<div>"), "\\<div>");
        assert_eq!(escape_text("a < b"), "a < b");
        assert_eq!(escape_text("&amp;"), "\\&amp;");
        assert_eq!(escape_text("&#38;"), "\\&#38;");
        assert_eq!(escape_text("&#x26;"), "\\&#x26;");
        assert_eq!(escape_text("&#xZZ;"), "&#xZZ;");
        assert_eq!(escape_text("&#x26"), "&#x26");
        assert_eq!(escape_text("this & that"), "this & that");
        assert_eq!(escape_text("!x"), "!x");
        assert_eq!(escape_text("![x"), "\\!\\[x");
    }

    #[test]
    fn line_start_constructs_escaped() {
        assert_eq!(escape_line_start("- item".into()), "\\- item");
        assert_eq!(escape_line_start("-x".into()), "-x");
        assert_eq!(escape_line_start("> quote".into()), "\\> quote");
        assert_eq!(escape_line_start("# head".into()), "\\# head");
        assert_eq!(escape_line_start("#tag".into()), "#tag");
        assert_eq!(escape_line_start("12. x".into()), "12\\. x");
        assert_eq!(escape_line_start("1984)".into()), "1984\\)");
        assert_eq!(escape_line_start("3.14".into()), "3.14");
        assert_eq!(escape_line_start("---".into()), "\\---");
        assert_eq!(escape_line_start("+++".into()), "\\+++");
        assert_eq!(escape_line_start("+++x".into()), "+++x");
        assert_eq!(escape_line_start("===".into()), "\\===");
        assert_eq!(escape_line_start("~~~x".into()), "\\~~~x");
        assert_eq!(escape_line_start("~~x~~".into()), "~~x~~");
    }
}
