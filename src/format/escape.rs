/// Append `text` to `out`, escaping characters in prose that could open
/// inline constructs on reparse. Applied to every word from a Text node;
/// never to code spans, HTML, or link destinations. Deliberately
/// conservative: always escaping `` ` `` `*` `_` `[` `]` `\` keeps output
/// stable regardless of context. Every byte checked is ASCII, so scanning
/// bytes matches scanning chars.
pub fn escape_text_into(out: &mut String, text: &str) {
    let bytes = text.as_bytes();
    let mut start = 0;
    for (i, &b) in bytes.iter().enumerate() {
        let next = bytes.get(i + 1).copied();
        let escape = match b {
            b'\\' | b'`' | b'*' | b'_' | b'[' | b']' => true,
            b'<' => {
                next.is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, b'/' | b'!' | b'?'))
            }
            b'!' => next == Some(b'['),
            b'&' => looks_like_entity(&bytes[i + 1..]),
            _ => false,
        };
        if escape {
            out.push_str(&text[start..i]);
            out.push('\\');
            start = i;
        }
    }
    out.push_str(&text[start..]);
}

/// Escape a code fence's info string. The parser decodes entities in it,
/// then backslash escapes, so a `&` that starts an entity is written as
/// `&amp;` (a backslash would itself be decoded first) and a backslash before
/// punctuation is doubled.
pub fn escape_info(info: &str) -> String {
    let mut out = String::with_capacity(info.len());
    let bytes = info.as_bytes();
    let mut start = 0;
    for (i, &b) in bytes.iter().enumerate() {
        let rep = match b {
            b'\\' if bytes.get(i + 1).is_some_and(|c| c.is_ascii_punctuation()) => "\\\\",
            b'&' if looks_like_entity(&bytes[i + 1..]) => "&amp;",
            _ => continue,
        };
        out.push_str(&info[start..i]);
        out.push_str(rep);
        start = i + 1;
    }
    out.push_str(&info[start..]);
    out
}

/// True if `rest` begins like an HTML entity body (`amp;`, `#38;`, `#x26;`).
fn looks_like_entity(rest: &[u8]) -> bool {
    let (digits, body): (fn(&u8) -> bool, &[u8]) = match rest {
        [b'#', b'x' | b'X', body @ ..] => (u8::is_ascii_hexdigit, body),
        [b'#', body @ ..] => (u8::is_ascii_digit, body),
        body => (u8::is_ascii_alphanumeric, body),
    };
    let n = body.iter().take_while(|b| digits(b)).count();
    n > 0 && body.get(n) == Some(&b';')
}

/// Escape a wrapped line whose first characters would start a block construct
/// on reparse (list marker, heading, quote, setext underline, fence).
/// Emphasis delimiters never trigger this: `*` and `_` in prose are already
/// escaped by `escape_text_into`, and delimiter runs glue to their word.
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

    fn escape_text(text: &str) -> String {
        let mut out = String::new();
        escape_text_into(&mut out, text);
        out
    }

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
    fn info_keeps_escapes_and_references() {
        assert_eq!(escape_info("rust"), "rust");
        assert_eq!(escape_info("a\\*"), "a\\\\*");
        assert_eq!(escape_info("a\\b"), "a\\b");
        assert_eq!(escape_info("a&amp;b"), "a&amp;amp;b");
        assert_eq!(escape_info("a&b"), "a&b");
        assert_eq!(escape_info("\\&amp;"), "\\\\&amp;amp;");
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
