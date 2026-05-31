use anyhow::{Context, Result};

pub fn docx_to_markdown(bytes: &[u8]) -> Result<String> {
    use docx_rs::*;
    let docx = read_docx(bytes).context("read_docx failed")?;
    let mut out = String::new();
    for child in docx.document.children {
        if let DocumentChild::Paragraph(p) = child {
            let text = paragraph_text(&p);
            if text.trim().is_empty() { continue; }
            // Style heuristic: paragraph style "Heading1" / "Heading2" → markdown headers.
            let style = p.property.style.as_ref().map(|s| s.val.as_str()).unwrap_or("");
            let prefix = match style {
                s if s.eq_ignore_ascii_case("Heading1") => "# ",
                s if s.eq_ignore_ascii_case("Heading2") => "## ",
                s if s.eq_ignore_ascii_case("Heading3") => "### ",
                _ => "",
            };
            out.push_str(prefix);
            out.push_str(&text);
            out.push_str("\n\n");
        }
    }
    Ok(out.trim_end().to_string())
}

fn paragraph_text(p: &docx_rs::Paragraph) -> String {
    let mut s = String::new();
    for c in &p.children {
        if let docx_rs::ParagraphChild::Run(run) = c {
            for rc in &run.children {
                if let docx_rs::RunChild::Text(t) = rc {
                    s.push_str(&t.text);
                }
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use docx_rs::*;

    fn build_docx() -> Vec<u8> {
        let docx = Docx::new()
            .add_paragraph(
                Paragraph::new()
                    .style("Heading1")
                    .add_run(Run::new().add_text("Title")),
            )
            .add_paragraph(
                Paragraph::new().add_run(Run::new().add_text("First body paragraph.")),
            )
            .add_paragraph(
                Paragraph::new()
                    .style("Heading2")
                    .add_run(Run::new().add_text("Subhead")),
            )
            .add_paragraph(
                Paragraph::new().add_run(Run::new().add_text("More body.")),
            );
        let mut buf: Vec<u8> = Vec::new();
        docx.build().pack(&mut std::io::Cursor::new(&mut buf)).unwrap();
        buf
    }

    #[test]
    fn converts_headings_and_paragraphs() {
        let md = docx_to_markdown(&build_docx()).unwrap();
        assert!(md.contains("# Title"));
        assert!(md.contains("First body paragraph."));
        assert!(md.contains("## Subhead"));
        assert!(md.contains("More body."));
    }
}
