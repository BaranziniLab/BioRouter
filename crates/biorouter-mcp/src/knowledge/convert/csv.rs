use anyhow::Result;

pub fn csv_to_markdown(bytes: &[u8]) -> Result<String> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_reader(bytes);
    let headers: Vec<String> = rdr.headers()?.iter().map(|s| s.to_string()).collect();
    if headers.is_empty() {
        return Ok(String::new());
    }
    let mut rows: Vec<Vec<String>> = Vec::new();
    for rec in rdr.records() {
        let rec = rec?;
        rows.push(rec.iter().map(|s| escape_pipe(s)).collect());
    }
    let mut out = String::new();
    out.push('|');
    for h in &headers {
        out.push_str(&format!(" {} |", escape_pipe(h)));
    }
    out.push('\n');
    out.push('|');
    for _ in &headers {
        out.push_str(" --- |");
    }
    out.push('\n');
    for r in rows {
        out.push('|');
        for (i, _) in headers.iter().enumerate() {
            let cell = r.get(i).map(String::as_str).unwrap_or("");
            out.push_str(&format!(" {} |", cell));
        }
        out.push('\n');
    }
    Ok(out)
}

fn escape_pipe(s: &str) -> String {
    s.replace('|', "\\|")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_table() {
        let csv = "name,score\nAlice,9\nBob,7\n";
        let md = csv_to_markdown(csv.as_bytes()).unwrap();
        assert!(md.contains("| name | score |"));
        assert!(md.contains("| --- | --- |"));
        assert!(md.contains("| Alice | 9 |"));
        assert!(md.contains("| Bob | 7 |"));
    }

    #[test]
    fn escapes_pipes_in_cells() {
        let csv = "a,b\nx|y,z\n";
        let md = csv_to_markdown(csv.as_bytes()).unwrap();
        assert!(md.contains("x\\|y"));
    }

    #[test]
    fn empty_input_returns_empty() {
        assert!(csv_to_markdown(b"").unwrap().is_empty());
    }
}
