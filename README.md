# passdown

A really, really simple Markdown formatter with as few knobs as possible.

## Install

```bash
# Straight from GitHub:
cargo install --git https://github.com/eljpsm/passdown

# With Nix:
nix run github:eljpsm/passdown

# From a clone (installs to ~/.cargo/bin):
make install
```

## Usage

```bash
# Format the directories in place.
passdown fix
passdown fix docs README.md
# Check the directories for issues, exit 1 on issues.
passdown check
```

| Exit code | Description                                            |
| --------- | ------------------------------------------------------ |
| `0`       | Clean, no issues found.                                |
| `1`       | Issues found (unformatted files or unfixable errors).  |
| `2`       | Operational failure (unreadable file, invalid config). |

## Configuration

There are exactly two options, read from a `passdown.toml` found by walking
upward from the current directory:

```toml
# Files or directories to skip, in gitignore glob syntax.
ignore = ["vendor/", "CHANGELOG.md"]

# Whether .gitignore and .git/info/exclude entries are used. Default: true.
use_gitignore = true
```

Global Git ignore files and other VCS ignore formats are not read. Directory
walks always skip `.git`, `.hg`, `.svn`, and `.jj` metadata directories.

## Disabling formatting

You can disable an entire file formatting using `passdown-disable-file`.

```markdown
<!-- passdown-disable-file -->
```

You can disable formatting for a region of a file using `passdown-disable` and
`passdown-enable`.

```markdown
<!-- passdown-disable -->

anything     here keeps
its exact formatting

<!-- passdown-enable -->
```

## The style

The style is fixed. Every choice is made for you:

| Element               | Rule                                                                                                                                                                                                            |
| --------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Prose                 | Re-flowed to fill 80 columns (display width, CJK-aware). Long unbreakable tokens such as URLs get their own line and may exceed it.                                                                             |
| Headings              | ATX everywhere, `#` through `######`. Setext underlines are converted.                                                                                                                                          |
| Unordered lists       | `-` marker at column 0, one space before content. Continuation lines indent two columns.                                                                                                                        |
| Ordered lists         | `1.` markers at column 0, renumbered sequentially from the first number in the source.                                                                                                                          |
| Code fences           | Backticks with the language attached: ```` ```rust ````. Minimum three, growing past any backtick run in the content.                                                                                           |
| Code languages        | Required. A fence without one is an unfixable error: both subcommands report it and exit 1, and the block is left exactly as written.                                                                           |
| Punctuation           | Typographic Unicode becomes ASCII in prose: curly quotes to straight quotes, ellipsis to `...`, em dash to `--`, en dash to `-`, non-breaking spaces to spaces. Code, HTML, and front matter are never touched. |
| Emphasis              | `*em*` and `**strong**`.                                                                                                                                                                                        |
| Thematic breaks       | `---`.                                                                                                                                                                                                          |
| Hard breaks           | A trailing backslash.                                                                                                                                                                                           |
| Links                 | Reference links become inline links. Link definitions are removed.                                                                                                                                              |
| Tables                | Padded pipes aligned by display width. Exempt from the 80-column limit.                                                                                                                                         |
| Front matter and HTML | Pass through unchanged.                                                                                                                                                                                         |
| Blank lines           | Exactly one between blocks, one trailing newline, LF line endings.                                                                                                                                              |

Formatting is idempotent: `passdown fix` twice always equals `passdown fix`
once. This is enforced by every test in the repository.

## Acknowledgements

passdown is heavily inspired by [hongdown by Hong
Minhee](https://github.com/dahlia/hongdown).

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
