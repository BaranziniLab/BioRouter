//! Spreadsheet → markdown conversion (xlsx / xlsm / xlsb / xls / ods) via
//! calamine. Emits one `## <sheet name>` section with a markdown table per
//! non-empty sheet, using cached computed values (never raw formulas).

use anyhow::{Context, Result};
use calamine::{open_workbook_auto_from_rs, Data, Reader};
use std::io::Cursor;

/// Hard cap on rows emitted per sheet so a million-row export cannot blow up
/// the raw source markdown handed to the digestion sub-agent.
const MAX_ROWS_PER_SHEET: usize = 500;
/// Hard cap on columns emitted per row.
const MAX_COLS_PER_ROW: usize = 64;

pub fn spreadsheet_to_markdown(bytes: &[u8]) -> Result<String> {
    let cursor = Cursor::new(bytes.to_vec());
    let mut workbook =
        open_workbook_auto_from_rs(cursor).context("open spreadsheet with calamine")?;

    let sheet_names = workbook.sheet_names().to_vec();
    let mut out = String::new();

    for name in &sheet_names {
        let range = match workbook.worksheet_range(name) {
            Ok(range) => range,
            Err(_) => continue,
        };
        if range.is_empty() {
            continue;
        }

        let (total_rows, total_cols) = range.get_size();
        let cols = total_cols.min(MAX_COLS_PER_ROW);

        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&format!("## {name}\n\n"));

        let mut rows = range.rows();
        let header: Vec<String> = match rows.next() {
            Some(row) => row.iter().take(cols).map(format_cell).collect(),
            None => continue,
        };
        out.push_str(&format!("| {} |\n", header.join(" | ")));
        out.push_str(&format!("|{}\n", " --- |".repeat(header.len())));

        let mut emitted = 0usize;
        for row in rows {
            if emitted >= MAX_ROWS_PER_SHEET {
                break;
            }
            let cells: Vec<String> = row.iter().take(cols).map(format_cell).collect();
            out.push_str(&format!("| {} |\n", cells.join(" | ")));
            emitted += 1;
        }

        let data_rows = total_rows.saturating_sub(1);
        if data_rows > emitted {
            out.push_str(&format!(
                "\n> … truncated: {} more rows not shown.\n",
                data_rows - emitted
            ));
        }
        if total_cols > cols {
            out.push_str(&format!(
                "\n> … truncated: {} more columns not shown.\n",
                total_cols - cols
            ));
        }
    }

    if out.trim().is_empty() {
        anyhow::bail!("spreadsheet contains no readable cells");
    }
    Ok(out)
}

fn format_cell(cell: &Data) -> String {
    let raw = match cell {
        Data::Empty => String::new(),
        Data::Error(e) => format!("#ERR({e:?})"),
        other => other.to_string(),
    };
    // Keep cells single-line and table-safe.
    raw.replace(['\n', '\r'], " ").replace('|', "\\|")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_xlsx(sheets: &[(&str, Vec<Vec<&str>>)]) -> Vec<u8> {
        let mut book = umya_spreadsheet::new_file();
        // new_file() starts with "Sheet1"; rename it for the first sheet and
        // append the rest.
        for (i, (name, rows)) in sheets.iter().enumerate() {
            if i == 0 {
                book.get_sheet_mut(&0).unwrap().set_name(*name);
            } else {
                book.new_sheet(*name).unwrap();
            }
            let sheet = book.get_sheet_by_name_mut(name).unwrap();
            for (r, row) in rows.iter().enumerate() {
                for (c, value) in row.iter().enumerate() {
                    sheet
                        .get_cell_mut(((c + 1) as u32, (r + 1) as u32))
                        .set_value(*value);
                }
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fixture.xlsx");
        umya_spreadsheet::writer::xlsx::write(&book, &path).unwrap();
        std::fs::read(&path).unwrap()
    }

    #[test]
    fn converts_multi_sheet_xlsx_to_markdown_tables() {
        let bytes = make_xlsx(&[
            (
                "Samples",
                vec![
                    vec!["Sample", "Concentration", "Unit"],
                    vec!["S1", "1.5", "ng/uL"],
                    vec!["S2", "2.25", "ng/uL"],
                ],
            ),
            ("Notes", vec![vec!["Comment"], vec!["pipe | inside"]]),
        ]);

        let md = spreadsheet_to_markdown(&bytes).unwrap();
        assert!(md.contains("## Samples"), "got: {md}");
        assert!(
            md.contains("| Sample | Concentration | Unit |"),
            "got: {md}"
        );
        assert!(md.contains("| S2 | 2.25 | ng/uL |"), "got: {md}");
        assert!(md.contains("## Notes"), "got: {md}");
        assert!(md.contains("pipe \\| inside"), "got: {md}");
    }

    #[test]
    fn truncates_huge_sheets() {
        let mut rows: Vec<Vec<&str>> = vec![vec!["Col"]];
        for _ in 0..(MAX_ROWS_PER_SHEET + 50) {
            rows.push(vec!["x"]);
        }
        let bytes = make_xlsx(&[("Big", rows)]);
        let md = spreadsheet_to_markdown(&bytes).unwrap();
        let tail: String = md.lines().rev().take(5).collect::<Vec<_>>().join("\n");
        assert!(
            md.contains("truncated: 50 more rows"),
            "expected truncation note, got tail: {tail}"
        );
    }

    #[test]
    fn rejects_garbage_bytes() {
        assert!(spreadsheet_to_markdown(b"not a spreadsheet").is_err());
    }
}
