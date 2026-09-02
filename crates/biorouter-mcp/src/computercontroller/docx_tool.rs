use docx_rs::*;
use image::{self, ImageFormat};
use rmcp::model::{Content, ErrorCode, ErrorData};
use std::borrow::Cow;
use std::{fs, io::Cursor};

#[derive(Debug)]
enum UpdateMode {
    Append,
    Replace {
        old_text: String,
    },
    InsertStructured {
        level: Option<String>,
        style: Option<DocxStyle>,
    },
    AddImage {
        image_path: String,
        width: Option<u32>,
        height: Option<u32>,
    },
}

#[derive(Debug, Clone, Default)]
struct DocxStyle {
    bold: bool,
    italic: bool,
    underline: bool,
    size: Option<usize>,
    color: Option<String>,
    alignment: Option<AlignmentType>,
}

impl DocxStyle {
    fn from_json(value: &serde_json::Value) -> Option<Self> {
        let obj = value.as_object()?;
        Some(Self {
            bold: obj.get("bold").and_then(|v| v.as_bool()).unwrap_or(false),
            italic: obj.get("italic").and_then(|v| v.as_bool()).unwrap_or(false),
            underline: obj
                .get("underline")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            size: obj.get("size").and_then(|v| v.as_u64()).map(|s| s as usize),
            color: obj.get("color").and_then(|v| v.as_str()).map(String::from),
            alignment: obj
                .get("alignment")
                .and_then(|v| v.as_str())
                .and_then(parse_alignment),
        })
    }

    fn apply_to_run(&self, mut run: Run) -> Run {
        if self.bold {
            run = run.bold();
        }
        if self.italic {
            run = run.italic();
        }
        if self.underline {
            run = run.underline("single");
        }
        if let Some(size) = self.size {
            run = run.size(size);
        }
        if let Some(color) = &self.color {
            run = run.color(color);
        }
        run
    }

    fn apply_to_paragraph(&self, mut para: Paragraph) -> Paragraph {
        if let Some(alignment) = self.alignment {
            para = para.align(alignment);
        }
        para
    }
}

fn parse_alignment(a: &str) -> Option<AlignmentType> {
    match a {
        "left" => Some(AlignmentType::Left),
        "center" => Some(AlignmentType::Center),
        "right" => Some(AlignmentType::Right),
        "justified" => Some(AlignmentType::Both),
        _ => None,
    }
}

fn docx_error(message: impl Into<String>) -> ErrorData {
    ErrorData {
        code: ErrorCode::INTERNAL_ERROR,
        message: Cow::from(message.into()),
        data: None,
    }
}

fn invalid_params(message: impl Into<String>) -> ErrorData {
    ErrorData {
        code: ErrorCode::INVALID_PARAMS,
        message: Cow::from(message.into()),
        data: None,
    }
}

/// #154. A `word/media/*` entry of ZERO length makes `read_docx` spin forever —
/// no error, no progress. Every operation here (append, replace, insert,
/// extract) goes through this function, so one such file wedges the agent's
/// turn indefinitely, and nothing recovers it because the parse has no timeout.
///
/// Measured: the same document with a real 70-byte 1x1 PNG parses in 6ms; with
/// that one entry emptied it was still running at 60s, three times over. Every
/// other zip entry was byte-identical, and the container is otherwise valid
/// (20 entries, well-formed `word/document.xml`).
///
/// Checking the archive first is a deterministic refusal rather than a timeout
/// guess, and it names the offending part. It does not claim to catch every
/// malformed document — see the test — only to stop the one shape this tool
/// PRODUCES itself, via the `load_image_as_png` hole fixed alongside it.
fn reject_degenerate_media(bytes: &[u8], path: &str) -> Result<(), ErrorData> {
    let Ok(mut archive) = zip::ZipArchive::new(Cursor::new(bytes)) else {
        // Not a readable zip at all: let `read_docx` produce its own message.
        return Ok(());
    };
    for i in 0..archive.len() {
        let Ok(entry) = archive.by_index(i) else {
            continue;
        };
        let name = entry.name().to_string();
        if name.starts_with("word/media/") && !name.ends_with('/') && entry.size() == 0 {
            return Err(docx_error(format!(
                "'{path}' embeds a zero-length image at '{name}', which this document format \
                 cannot represent. Re-create the document, or add the image again from a file \
                 that is not empty."
            )));
        }
    }
    Ok(())
}

fn read_docx_file(path: &str) -> Result<Docx, ErrorData> {
    let file =
        fs::read(path).map_err(|e| docx_error(format!("Failed to read DOCX file: {}", e)))?;
    reject_degenerate_media(&file, path)?;
    read_docx(&file).map_err(|e| docx_error(format!("Failed to parse DOCX file: {}", e)))
}

fn read_or_create_docx(path: &str) -> Result<Docx, ErrorData> {
    if std::path::Path::new(path).exists() {
        read_docx_file(path)
    } else {
        Ok(Docx::new())
    }
}

fn write_docx_file(path: &str, doc: Docx) -> Result<(), ErrorData> {
    let mut buf = Vec::new();
    doc.build()
        .pack(&mut Cursor::new(&mut buf))
        .map_err(|e| docx_error(format!("Failed to build DOCX: {}", e)))?;
    fs::write(path, &buf).map_err(|e| docx_error(format!("Failed to write DOCX file: {}", e)))
}

fn extract_paragraph_text(p: &Paragraph) -> String {
    p.children
        .iter()
        .filter_map(|child| {
            if let ParagraphChild::Run(run) = child {
                Some(
                    run.children
                        .iter()
                        .filter_map(|rc| {
                            if let RunChild::Text(t) = rc {
                                Some(t.text.clone())
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(""),
                )
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("")
}

fn add_styled_paragraphs(mut doc: Docx, content: &str, style: &Option<DocxStyle>) -> Docx {
    for para in content.split('\n').filter(|p| !p.trim().is_empty()) {
        let mut run = Run::new().add_text(para);
        let mut paragraph = Paragraph::new();
        if let Some(s) = style {
            run = s.apply_to_run(run);
            paragraph = s.apply_to_paragraph(paragraph);
        }
        doc = doc.add_paragraph(paragraph.add_run(run));
    }
    doc
}

fn parse_update_mode(
    params: Option<&serde_json::Value>,
) -> Result<(UpdateMode, Option<DocxStyle>), ErrorData> {
    let Some(params) = params else {
        return Ok((UpdateMode::Append, None));
    };

    let mode_str = params
        .get("mode")
        .and_then(|v| v.as_str())
        .unwrap_or("append");
    let style = params.get("style").and_then(DocxStyle::from_json);

    let mode = match mode_str {
        "append" => UpdateMode::Append,
        "replace" => {
            let old_text = params
                .get("old_text")
                .and_then(|v| v.as_str())
                .ok_or_else(|| invalid_params("old_text parameter required for replace mode"))?;
            UpdateMode::Replace {
                old_text: old_text.to_string(),
            }
        }
        "structured" => UpdateMode::InsertStructured {
            level: params
                .get("level")
                .and_then(|v| v.as_str())
                .map(String::from),
            style: style.clone(),
        },
        "add_image" => {
            let image_path = params
                .get("image_path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    invalid_params("image_path parameter required for add_image mode")
                })?;
            UpdateMode::AddImage {
                image_path: image_path.to_string(),
                width: params
                    .get("width")
                    .and_then(|v| v.as_u64())
                    .map(|w| w as u32),
                height: params
                    .get("height")
                    .and_then(|v| v.as_u64())
                    .map(|h| h as u32),
            }
        }
        _ => {
            return Err(invalid_params(
                "Invalid mode. Must be 'append', 'replace', 'structured', or 'add_image'",
            ))
        }
    };
    Ok((mode, style))
}

fn extract_text_from_docx(docx: &Docx) -> String {
    let mut text = String::new();
    for element in docx.document.children.iter() {
        if let DocumentChild::Paragraph(p) = element {
            let para_text = extract_paragraph_text(p);
            if !para_text.trim().is_empty() {
                text.push_str(&para_text);
                text.push('\n');
            }
        }
    }
    text
}

fn extract_structure_from_docx(docx: &Docx) -> Vec<String> {
    let mut structure = Vec::new();
    let mut current_level = None;

    for element in docx.document.children.iter() {
        if let DocumentChild::Paragraph(p) = element {
            if let Some(style) = p.property.style.as_ref() {
                if style.val.starts_with("Heading") {
                    current_level = Some(style.val.clone());
                    structure.push(format!("{}: ", style.val));
                }
            }
            let para_text = extract_paragraph_text(p);
            if !para_text.trim().is_empty() && current_level.is_some() {
                if let Some(s) = structure.last_mut() {
                    s.push_str(&para_text);
                }
                current_level = None;
            }
        }
    }
    structure
}

fn do_extract_text(path: &str) -> Result<Vec<Content>, ErrorData> {
    let docx = read_docx_file(path)?;
    let text = extract_text_from_docx(&docx);
    let structure = extract_structure_from_docx(&docx);

    let result = if !structure.is_empty() {
        format!(
            "Document Structure:\n{}\n\nFull Text:\n{}",
            structure.join("\n"),
            text
        )
    } else {
        format!("Extracted Text:\n{}", text)
    };
    Ok(vec![Content::text(result)])
}

fn do_append(
    path: &str,
    content: &str,
    style: &Option<DocxStyle>,
) -> Result<Vec<Content>, ErrorData> {
    let doc = read_or_create_docx(path)?;
    let doc = add_styled_paragraphs(doc, content, style);
    write_docx_file(path, doc)?;
    Ok(vec![Content::text(format!(
        "Successfully wrote content to {}",
        path
    ))])
}

fn do_replace(
    path: &str,
    content: &str,
    old_text: &str,
    style: &Option<DocxStyle>,
) -> Result<Vec<Content>, ErrorData> {
    let docx = read_docx_file(path)?;
    let mut new_doc = Docx::new();
    let mut found_text = false;

    for element in docx.document.children.iter() {
        if let DocumentChild::Paragraph(p) = element {
            let para_text = extract_paragraph_text(p);
            if para_text.contains(old_text) {
                found_text = true;
                new_doc = add_styled_paragraphs(new_doc, content, style);
            } else {
                let mut para = Paragraph::new();
                if let Some(s) = &p.property.style {
                    para = para.style(&s.val);
                }
                for child in p.children.iter() {
                    if let ParagraphChild::Run(run) = child {
                        for rc in run.children.iter() {
                            if let RunChild::Text(t) = rc {
                                para = para.add_run(Run::new().add_text(&t.text));
                            }
                        }
                    }
                }
                new_doc = new_doc.add_paragraph(para);
            }
        }
    }

    if !found_text {
        return Err(docx_error(format!(
            "Could not find text to replace: {}",
            old_text
        )));
    }
    write_docx_file(path, new_doc)?;
    Ok(vec![Content::text(format!(
        "Successfully replaced content in {}",
        path
    ))])
}

fn do_insert_structured(
    path: &str,
    content: &str,
    level: &Option<String>,
    style: &Option<DocxStyle>,
) -> Result<Vec<Content>, ErrorData> {
    let mut doc = read_or_create_docx(path)?;

    for para in content.split('\n').filter(|p| !p.trim().is_empty()) {
        let mut run = Run::new().add_text(para);
        let mut paragraph = Paragraph::new();
        if let Some(lvl) = level {
            paragraph = paragraph.style(lvl);
        }
        if let Some(s) = style {
            run = s.apply_to_run(run);
            paragraph = s.apply_to_paragraph(paragraph);
        }
        doc = doc.add_paragraph(paragraph.add_run(run));
    }

    write_docx_file(path, doc)?;
    Ok(vec![Content::text(format!(
        "Successfully added structured content to {}",
        path
    ))])
}

fn load_image_as_png(image_path: &str) -> Result<Vec<u8>, ErrorData> {
    let image_data = fs::read(image_path)
        .map_err(|e| docx_error(format!("Failed to read image file: {}", e)))?;

    let extension = std::path::Path::new(image_path)
        .extension()
        .and_then(|e| e.to_str())
        .ok_or_else(|| docx_error("Invalid image file extension"))?
        .to_lowercase();

    if extension == "png" {
        // #154: the `.png` fast path returned these bytes UNVALIDATED, so an
        // empty or non-PNG file was embedded verbatim — producing a document
        // this very tool then hangs on forever. Every other extension goes
        // through `image::load_from_memory` below, which rejects both. This
        // path has to make the same two checks itself.
        const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";
        if image_data.is_empty() {
            return Err(docx_error(format!(
                "Image file '{image_path}' is empty; refusing to embed a zero-length image"
            )));
        }
        if !image_data.starts_with(PNG_MAGIC) {
            return Err(docx_error(format!(
                "Image file '{image_path}' is named .png but is not a PNG"
            )));
        }
        return Ok(image_data);
    }

    let img = image::load_from_memory(&image_data)
        .map_err(|e| docx_error(format!("Failed to load image: {}", e)))?;
    let mut png_data = Vec::new();
    img.write_to(&mut Cursor::new(&mut png_data), ImageFormat::Png)
        .map_err(|e| docx_error(format!("Failed to convert image to PNG: {}", e)))?;
    Ok(png_data)
}

fn do_add_image(
    path: &str,
    content: &str,
    image_path: &str,
    width: Option<u32>,
    height: Option<u32>,
    style: &Option<DocxStyle>,
) -> Result<Vec<Content>, ErrorData> {
    let mut doc = read_or_create_docx(path)?;
    let image_data = load_image_as_png(image_path)?;

    if !content.trim().is_empty() {
        let mut caption = Paragraph::new();
        if let Some(s) = style {
            caption = s.apply_to_paragraph(caption);
            caption = caption.add_run(s.apply_to_run(Run::new().add_text(content)));
        } else {
            caption = caption.add_run(Run::new().add_text(content));
        }
        doc = doc.add_paragraph(caption);
    }

    let mut paragraph = Paragraph::new();
    if let Some(s) = style {
        paragraph = s.apply_to_paragraph(paragraph);
    }

    let mut pic = Pic::new(&image_data);
    if let (Some(w), Some(h)) = (width, height) {
        pic = pic.size(w, h);
    }

    paragraph = paragraph.add_run(Run::new().add_image(pic));
    doc = doc.add_paragraph(paragraph);

    write_docx_file(path, doc)?;
    Ok(vec![Content::text(format!(
        "Successfully added image to {}",
        path
    ))])
}

pub async fn docx_tool(
    path: &str,
    operation: &str,
    content: Option<&str>,
    params: Option<&serde_json::Value>,
) -> Result<Vec<Content>, ErrorData> {
    match operation {
        "extract_text" => do_extract_text(path),
        "update_doc" => {
            let content = content
                .ok_or_else(|| invalid_params("Content parameter required for update_doc"))?;
            let (mode, style) = parse_update_mode(params)?;

            match mode {
                UpdateMode::Append => do_append(path, content, &style),
                UpdateMode::Replace { old_text } => do_replace(path, content, &old_text, &style),
                UpdateMode::InsertStructured {
                    level,
                    style: mode_style,
                } => do_insert_structured(path, content, &level, &mode_style.or(style)),
                UpdateMode::AddImage {
                    image_path,
                    width,
                    height,
                } => do_add_image(path, content, &image_path, width, height, &style),
            }
        }
        _ => Err(invalid_params(format!(
            "Invalid operation: {}. Valid operations are: 'extract_text', 'update_doc'",
            operation
        ))),
    }
}

#[cfg(test)]
mod zero_byte_image_tests {
    //! #154. A `.docx` carrying a zero-length `word/media/*` entry made
    //! `read_docx` spin forever, and this tool's own `add_image` produced
    //! exactly that file: the `.png` fast path in `load_image_as_png` returned
    //! its bytes unvalidated.
    //!
    //! Both halves are pinned here because either alone leaves a hole — fixing
    //! only the writer still hangs on a document some other program wrote, and
    //! fixing only the reader leaves the tool manufacturing broken documents.
    use super::{load_image_as_png, reject_degenerate_media};
    use std::io::{Cursor, Write};

    /// A real 1x1 PNG. The measured control: the SAME document carrying this
    /// instead of an empty entry parses in 6ms.
    const TINY_PNG: &[u8] = &[
        0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, b'I', b'H', b'D',
        b'R', 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f,
        0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0a, b'I', b'D', b'A', b'T', 0x78, 0x9c, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, b'I',
        b'E', b'N', b'D', 0xae, 0x42, 0x60, 0x82,
    ];

    fn docx_with_media(media: &[u8]) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(Cursor::new(&mut buf));
            let opts = zip::write::FileOptions::default();
            zip.start_file("word/document.xml", opts).unwrap();
            zip.write_all(b"<w:document/>").unwrap();
            zip.start_file("word/media/rIdImage1.png", opts).unwrap();
            zip.write_all(media).unwrap();
            zip.finish().unwrap();
        }
        buf
    }

    #[test]
    fn a_zero_length_media_entry_is_refused_by_name() {
        let err = reject_degenerate_media(&docx_with_media(b""), "report.docx")
            .expect_err("a zero-length image must be refused, not handed to a parser that hangs");
        let msg = format!("{err:?}");
        assert!(
            msg.contains("word/media/rIdImage1.png"),
            "must name the part: {msg}"
        );
        assert!(msg.contains("report.docx"), "must name the document: {msg}");
    }

    #[test]
    fn a_real_image_is_left_alone() {
        reject_degenerate_media(&docx_with_media(TINY_PNG), "report.docx")
            .expect("the control: an ordinary document must pass through untouched");
    }

    #[test]
    fn a_non_zip_is_left_to_the_parsers_own_error() {
        reject_degenerate_media(b"not a zip at all", "report.docx")
            .expect("this guard reports one specific shape; it must not invent errors");
    }

    #[test]
    fn an_empty_png_is_never_embedded() {
        let dir = tempfile::tempdir().unwrap();
        let empty = dir.path().join("empty.png");
        std::fs::write(&empty, b"").unwrap();
        let err = load_image_as_png(empty.to_str().unwrap())
            .expect_err("#154: the .png fast path returned empty bytes unvalidated");
        assert!(format!("{err:?}").contains("empty"), "{err:?}");

        let liar = dir.path().join("liar.png");
        std::fs::write(&liar, b"GIF89a not really a png").unwrap();
        load_image_as_png(liar.to_str().unwrap())
            .expect_err("a .png that is not a PNG must be refused too");

        let good = dir.path().join("good.png");
        std::fs::write(&good, TINY_PNG).unwrap();
        assert_eq!(
            load_image_as_png(good.to_str().unwrap()).unwrap(),
            TINY_PNG,
            "a real PNG must still pass through byte-for-byte"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_docx_text_extraction() {
        let test_docx_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src/computercontroller/tests/data/sample.docx");

        println!("Testing text extraction from: {}", test_docx_path.display());

        let result = docx_tool(test_docx_path.to_str().unwrap(), "extract_text", None, None).await;

        assert!(result.is_ok(), "DOCX text extraction should succeed");
        let content = result.unwrap();
        assert!(!content.is_empty(), "Extracted text should not be empty");
        let text = content[0].as_text().unwrap();
        println!("Extracted text:\n{}", text.text);
        assert!(
            !text.text.trim().is_empty(),
            "Extracted text should not be empty"
        );
    }

    #[tokio::test]
    async fn test_docx_update_append() {
        let test_output_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src/computercontroller/tests/data/test_output.docx");

        let test_content =
            "Test Heading\nThis is a test paragraph.\n\nAnother paragraph with some content.";

        let result = docx_tool(
            test_output_path.to_str().unwrap(),
            "update_doc",
            Some(test_content),
            None,
        )
        .await;

        assert!(result.is_ok(), "DOCX update should succeed");
        assert!(test_output_path.exists(), "Output file should exist");

        // Now try to read it back
        let result = docx_tool(
            test_output_path.to_str().unwrap(),
            "extract_text",
            None,
            None,
        )
        .await;
        assert!(
            result.is_ok(),
            "Should be able to read back the written file"
        );
        let content = result.unwrap();
        let text = content[0].as_text().unwrap();
        assert!(
            text.text.contains("Test Heading"),
            "Should contain written content"
        );
        assert!(
            text.text.contains("test paragraph"),
            "Should contain written content"
        );

        // Clean up
        fs::remove_file(test_output_path).unwrap();
    }

    #[tokio::test]
    async fn test_docx_update_styled() {
        let test_output_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src/computercontroller/tests/data/test_styled.docx");

        let test_content = "Styled Heading\nThis is a styled paragraph.";
        let params = json!({
            "mode": "structured",
            "level": "Heading1",
            "style": {
                "bold": true,
                "color": "FF0000",
                "size": 24,
                "alignment": "center"
            }
        });

        let result = docx_tool(
            test_output_path.to_str().unwrap(),
            "update_doc",
            Some(test_content),
            Some(&params),
        )
        .await;

        assert!(result.is_ok(), "DOCX styled update should succeed");
        assert!(test_output_path.exists(), "Output file should exist");

        // Clean up
        fs::remove_file(test_output_path).unwrap();
    }

    #[tokio::test]
    async fn test_docx_update_replace() {
        let test_output_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src/computercontroller/tests/data/test_replace.docx");

        // First create a document
        let initial_content = "Original content\nThis should be replaced.\nKeep this text.";
        let _ = docx_tool(
            test_output_path.to_str().unwrap(),
            "update_doc",
            Some(initial_content),
            None,
        )
        .await;

        // Now replace part of it
        let replacement = "New content here";
        let params = json!({
            "mode": "replace",
            "old_text": "This should be replaced",
            "style": {
                "italic": true
            }
        });

        let result = docx_tool(
            test_output_path.to_str().unwrap(),
            "update_doc",
            Some(replacement),
            Some(&params),
        )
        .await;

        assert!(result.is_ok(), "DOCX replace should succeed");

        // Verify the content
        let result = docx_tool(
            test_output_path.to_str().unwrap(),
            "extract_text",
            None,
            None,
        )
        .await;
        assert!(result.is_ok());
        let content = result.unwrap();
        let text = content[0].as_text().unwrap();
        assert!(
            text.text.contains("New content here"),
            "Should contain new content"
        );
        assert!(
            text.text.contains("Keep this text"),
            "Should keep unmodified content"
        );
        assert!(
            !text.text.contains("This should be replaced"),
            "Should not contain replaced text"
        );

        // Clean up
        fs::remove_file(test_output_path).unwrap();
    }

    #[tokio::test]
    async fn test_docx_add_image() {
        let test_output_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src/computercontroller/tests/data/test_image.docx");

        // Create a test image file
        let test_image_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src/computercontroller/tests/data/test_image.png");

        // Create a simple test PNG image using the image crate
        let imgbuf = image::ImageBuffer::from_fn(32, 32, |x, y| {
            let dx = x as f32 - 16.0;
            let dy = y as f32 - 16.0;
            if dx * dx + dy * dy < 16.0 * 16.0 {
                image::Rgb([0u8, 0u8, 255u8]) // Blue circle
            } else {
                image::Rgb([255u8, 255u8, 255u8]) // White background
            }
        });
        imgbuf
            .save(&test_image_path)
            .expect("Failed to create test image");

        let params = json!({
            "mode": "add_image",
            "image_path": test_image_path.to_str().unwrap(),
            "width": 100,
            "height": 100,
            "style": {
                "alignment": "center"
            }
        });

        let result = docx_tool(
            test_output_path.to_str().unwrap(),
            "update_doc",
            Some("Image Caption"),
            Some(&params),
        )
        .await;

        assert!(result.is_ok(), "DOCX image addition should succeed");
        assert!(test_output_path.exists(), "Output file should exist");

        // Clean up
        fs::remove_file(test_output_path).unwrap();
        fs::remove_file(test_image_path).unwrap();
    }

    #[tokio::test]
    async fn test_docx_invalid_path() {
        let result = docx_tool("nonexistent.docx", "extract_text", None, None).await;
        assert!(result.is_err(), "Should fail with invalid path");
    }

    #[tokio::test]
    async fn test_docx_invalid_operation() {
        let test_docx_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src/computercontroller/tests/data/sample.docx");

        let result = docx_tool(
            test_docx_path.to_str().unwrap(),
            "invalid_operation",
            None,
            None,
        )
        .await;

        assert!(result.is_err(), "Should fail with invalid operation");
    }

    #[tokio::test]
    async fn test_docx_update_without_content() {
        let test_output_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src/computercontroller/tests/data/test_output.docx");

        let result = docx_tool(test_output_path.to_str().unwrap(), "update_doc", None, None).await;

        assert!(result.is_err(), "Should fail without content");
    }

    #[tokio::test]
    async fn test_docx_update_preserve_content() {
        let test_output_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src/computercontroller/tests/data/test_preserve.docx");

        // First create a document with initial content
        let initial_content =
            "Initial content\nThis is the first paragraph.\nThis should stay in the document.";
        let result = docx_tool(
            test_output_path.to_str().unwrap(),
            "update_doc",
            Some(initial_content),
            None,
        )
        .await;
        assert!(result.is_ok(), "Initial document creation should succeed");

        // Now append new content
        let new_content = "New content\nThis is an additional paragraph.";
        let params = json!({
            "mode": "append",
            "style": {
                "bold": true
            }
        });

        let result = docx_tool(
            test_output_path.to_str().unwrap(),
            "update_doc",
            Some(new_content),
            Some(&params),
        )
        .await;
        assert!(result.is_ok(), "Content append should succeed");

        // Verify both old and new content exists
        let result = docx_tool(
            test_output_path.to_str().unwrap(),
            "extract_text",
            None,
            None,
        )
        .await;
        assert!(result.is_ok());
        let content = result.unwrap();
        let text = content[0].as_text().unwrap();

        // Check for initial content
        assert!(
            text.text.contains("Initial content"),
            "Should contain initial content"
        );
        assert!(
            text.text.contains("first paragraph"),
            "Should contain first paragraph"
        );
        assert!(
            text.text.contains("should stay in the document"),
            "Should preserve existing content"
        );

        // Check for new content
        assert!(
            text.text.contains("New content"),
            "Should contain new content"
        );
        assert!(
            text.text.contains("additional paragraph"),
            "Should contain appended paragraph"
        );

        // Clean up
        fs::remove_file(test_output_path).unwrap();
    }
}
