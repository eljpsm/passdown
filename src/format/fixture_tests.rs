//! Fixture harness. Each `tests/fixtures/<case>/` holds `input.md` and
//! `expected.md`. Every case asserts three properties:
//!
//! 1. `format(input) == expected`
//! 2. idempotency: `format(expected) == expected`
//! 3. HTML equivalence: input and expected render to the same HTML
//!    (whitespace-normalized outside `<pre>`), unless the case opts out with
//!    a `no-html-check` marker file (punctuation normalization changes text
//!    content on purpose).
//!
//! Bless expected files with: `PASSDOWN_BLESS=1 cargo test`

use std::path::{Path, PathBuf};

use super::{comrak_options, format_document};

#[test]
fn fixtures() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let bless = std::env::var_os("PASSDOWN_BLESS").is_some();

    let mut cases: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("tests/fixtures must exist")
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_dir())
        .collect();
    cases.sort();
    assert!(!cases.is_empty(), "no fixture cases found");

    let mut failures: Vec<String> = Vec::new();
    for case in &cases {
        let name = case.file_name().unwrap().to_string_lossy().to_string();
        let input = read(&case.join("input.md"));
        let result = format_document(&input);

        let expected_path = case.join("expected.md");
        if bless {
            std::fs::write(&expected_path, &result.output).unwrap();
        }
        let expected = read(&expected_path);

        if result.output != expected {
            failures.push(format!(
                "{name}: format(input) != expected\n--- got ---\n{}\n--- expected ---\n{}",
                result.output, expected
            ));
            continue;
        }

        let rendered_diags = result
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.render(Path::new("input.md")) + "\n")
            .collect::<String>();
        let diags_path = case.join("expected.diags");
        if bless {
            if rendered_diags.is_empty() {
                let _ = std::fs::remove_file(&diags_path);
            } else {
                std::fs::write(&diags_path, &rendered_diags).unwrap();
            }
        }
        let expected_diags = if diags_path.exists() {
            read(&diags_path)
        } else {
            String::new()
        };
        if rendered_diags != expected_diags {
            failures.push(format!(
                "{name}: diagnostics mismatch\n--- got ---\n{rendered_diags}\n--- expected ---\n{expected_diags}"
            ));
        }

        let again = format_document(&expected);
        if again.output != expected {
            failures.push(format!(
                "{name}: NOT IDEMPOTENT\n--- second pass ---\n{}\n--- first pass ---\n{}",
                again.output, expected
            ));
        }

        if !case.join("no-html-check").exists() {
            let html_in = normalize_html(&render_html(&input));
            let html_out = normalize_html(&render_html(&expected));
            if html_in != html_out {
                failures.push(format!(
                    "{name}: HTML MEANING CHANGED\n--- input html ---\n{html_in}\n--- output html ---\n{html_out}"
                ));
            }
        }
    }

    if !failures.is_empty() {
        panic!(
            "{} fixture failure(s):\n\n{}",
            failures.len(),
            failures.join("\n\n========\n\n")
        );
    }
}

fn render_html(input: &str) -> String {
    let mut options = comrak_options(input);
    options.render.r#unsafe = true;
    comrak::markdown_to_html(input, &options)
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

/// Collapse whitespace runs to single spaces, but leave `<pre>...</pre>`
/// content untouched (whitespace there is significant). The formatter's own
/// `<!-- -->` list separator is semantically inert; strip it before comparing.
fn normalize_html(html: &str) -> String {
    let html = html.replace("<!-- -->", "");
    let html = html.as_str();
    let mut out = String::new();
    let mut rest = html;
    while let Some(start) = rest.find("<pre") {
        collapse_into(&mut out, &rest[..start]);
        let end = rest[start..]
            .find("</pre>")
            .map(|index| start + index + "</pre>".len())
            .unwrap_or(rest.len());
        out.push_str(&rest[start..end]);
        rest = &rest[end..];
    }
    collapse_into(&mut out, rest);
    out.trim().to_string()
}

fn collapse_into(out: &mut String, segment: &str) {
    let mut first = true;
    for word in segment.split_whitespace() {
        if !first {
            out.push(' ');
        }
        out.push_str(word);
        first = false;
    }
}
