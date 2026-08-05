use comrak::nodes::{AstNode, NodeTable, TableAlignment};
use unicode_width::UnicodeWidthStr;

use super::inline::collect_text;
use super::state::Serializer;

/// Emit a table with leading and trailing pipes, one-space padding, and
/// columns padded to the widest cell by display width. Rows are exempt from
/// the 80-column cap.
pub fn serialize_table<'a>(s: &mut Serializer<'_>, node: &'a AstNode<'a>, table: &NodeTable) {
    let mut rows: Vec<Vec<String>> = Vec::new();
    for row in node.children() {
        let mut cells: Vec<String> = row
            .children()
            .map(|cell| collect_text(cell, &mut s.diags).replace('|', "\\|"))
            .collect();
        cells.resize(table.num_columns, String::new());
        rows.push(cells);
    }

    // Minimum column width 3 so every delimiter form fits, including `:-:`.
    let mut widths = vec![3usize; table.num_columns];
    for row in &rows {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(cell.width());
        }
    }

    let render_row = |cells: &[String]| -> String {
        let mut line = String::from("|");
        for (i, cell) in cells.iter().enumerate() {
            let pad = widths[i] - cell.width();
            line.push(' ');
            line.push_str(cell);
            line.push_str(&" ".repeat(pad));
            line.push_str(" |");
        }
        line
    };

    let delimiter = {
        let mut line = String::from("|");
        for (i, &width) in widths.iter().enumerate() {
            let alignment = table
                .alignments
                .get(i)
                .copied()
                .unwrap_or(TableAlignment::None);
            let dashes = match alignment {
                TableAlignment::None => "-".repeat(width),
                TableAlignment::Left => format!(":{}", "-".repeat(width - 1)),
                TableAlignment::Right => format!("{}:", "-".repeat(width - 1)),
                TableAlignment::Center => format!(":{}:", "-".repeat(width - 2)),
            };
            line.push(' ');
            line.push_str(&dashes);
            line.push_str(" |");
        }
        line
    };

    for (i, row) in rows.iter().enumerate() {
        s.push_line(&render_row(row));
        if i == 0 {
            s.push_line(&delimiter);
        }
    }
}
