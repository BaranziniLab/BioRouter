use anyhow::{Context, Result};

pub struct PdfConversion {
    pub markdown: String,
    pub needs_llm_fallback: bool,
}

pub fn pdf_to_markdown(bytes: &[u8]) -> Result<PdfConversion> {
    let text = pdf_extract::extract_text_from_mem(bytes).context("pdf-extract failed")?;
    let cleaned = normalize_text(&text);
    let needs_llm_fallback = cleaned.trim().len() < 32;
    Ok(PdfConversion {
        markdown: cleaned,
        needs_llm_fallback,
    })
}

fn normalize_text(s: &str) -> String {
    // Collapse runs of whitespace, keep paragraph boundaries (double newlines).
    let mut out = String::new();
    for para in s.split("\n\n") {
        let joined: String = para.split_whitespace().collect::<Vec<_>>().join(" ");
        if !joined.is_empty() {
            out.push_str(&joined);
            out.push_str("\n\n");
        }
    }
    out.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref, Str};

    fn make_pdf(text: &str) -> Vec<u8> {
        let mut pdf = Pdf::new();
        let catalog_id = Ref::new(1);
        let page_tree_id = Ref::new(2);
        let page_id = Ref::new(3);
        let font_id = Ref::new(4);
        let content_id = Ref::new(5);

        pdf.catalog(catalog_id).pages(page_tree_id);
        pdf.pages(page_tree_id).kids([page_id]).count(1);

        let mut page = pdf.page(page_id);
        page.parent(page_tree_id)
            .media_box(Rect::new(0.0, 0.0, 595.0, 842.0))
            .resources()
            .fonts()
            .pair(Name(b"F1"), font_id);
        page.contents(content_id);
        page.finish();

        pdf.type1_font(font_id).base_font(Name(b"Helvetica"));

        let mut content = Content::new();
        content
            .begin_text()
            .set_font(Name(b"F1"), 12.0)
            .next_line(72.0, 770.0)
            .show(Str(text.as_bytes()))
            .end_text();
        pdf.stream(content_id, &content.finish());
        pdf.finish()
    }

    #[test]
    fn extracts_text_from_simple_pdf() {
        // Use a string longer than 32 chars so needs_llm_fallback is false.
        let bytes = make_pdf("Hello, knowledge base! This is a test PDF document.");
        let c = pdf_to_markdown(&bytes).unwrap();
        assert!(c.markdown.contains("Hello"), "got {:?}", c.markdown);
        assert!(!c.needs_llm_fallback);
    }

    #[test]
    fn flags_empty_pdf_for_llm_fallback() {
        let bytes = make_pdf("");
        let c = pdf_to_markdown(&bytes).unwrap();
        assert!(c.needs_llm_fallback);
    }
}
