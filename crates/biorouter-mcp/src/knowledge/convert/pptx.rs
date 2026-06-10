//! PPTX → markdown conversion.
//!
//! A .pptx file is a zip of OOXML parts. This extractor walks the slide parts
//! in presentation order and emits, per slide: the title, body paragraphs,
//! tables (`<a:tbl>`) as markdown tables, and **speaker notes** from the
//! matching `ppt/notesSlides/` part — notes often carry more knowledge than
//! the slides themselves and no off-the-shelf Rust crate extracts them.
//!
//! Known limits (acceptable for LLM digestion, marked in the output where
//! possible): SmartArt text (`ppt/diagrams/`) and chart data series are not
//! extracted; reading order is XML document order, not visual position.

use anyhow::{Context, Result};
use roxmltree::{Document, Node};
use std::collections::HashMap;
use std::io::{Cursor, Read};
use zip::ZipArchive;

const R_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

pub struct PptxConversion {
    pub markdown: String,
    pub title: Option<String>,
}

pub fn pptx_to_markdown(bytes: &[u8]) -> Result<PptxConversion> {
    let mut zip = ZipArchive::new(Cursor::new(bytes)).context("open pptx as zip")?;

    let slide_paths = ordered_slide_paths(&mut zip)?;
    if slide_paths.is_empty() {
        anyhow::bail!("pptx contains no slides");
    }

    let doc_title = read_zip_string(&mut zip, "docProps/core.xml")
        .and_then(|xml| core_title(&xml))
        .filter(|t| !t.trim().is_empty());

    let mut out = String::new();
    if let Some(title) = &doc_title {
        out.push_str(&format!("# {title}\n\n"));
    }

    let mut extracted_any = false;
    for (index, slide_path) in slide_paths.iter().enumerate() {
        let slide_no = index + 1;
        let Some(xml) = read_zip_string(&mut zip, slide_path) else {
            continue;
        };
        let notes = notes_for_slide(&mut zip, slide_path);
        let (slide_md, has_content) = slide_to_markdown(&xml, slide_no, notes.as_deref())?;
        if has_content {
            extracted_any = true;
        }
        out.push_str(&slide_md);
        out.push('\n');
    }

    if !extracted_any {
        anyhow::bail!("no extractable text found in pptx slides");
    }

    Ok(PptxConversion {
        markdown: out.trim_end().to_string() + "\n",
        title: doc_title,
    })
}

// ---------------------------------------------------------------------------
// Zip / part plumbing
// ---------------------------------------------------------------------------

fn read_zip_string(zip: &mut ZipArchive<Cursor<&[u8]>>, name: &str) -> Option<String> {
    let mut file = zip.by_name(name).ok()?;
    let mut s = String::new();
    file.read_to_string(&mut s).ok()?;
    Some(s)
}

/// Slide part paths in presentation order (via `sldIdLst` + the presentation
/// rels), falling back to numeric sort of `ppt/slides/slideN.xml` entries.
fn ordered_slide_paths(zip: &mut ZipArchive<Cursor<&[u8]>>) -> Result<Vec<String>> {
    let ordered = (|| -> Option<Vec<String>> {
        let rels_xml = read_zip_string(zip, "ppt/_rels/presentation.xml.rels")?;
        let rels = parse_relationships(&rels_xml)?;
        let pres_xml = read_zip_string(zip, "ppt/presentation.xml")?;
        let doc = Document::parse(&pres_xml).ok()?;

        let mut paths = Vec::new();
        for sld_id in doc.descendants().filter(|n| n.tag_name().name() == "sldId") {
            let rid = sld_id
                .attributes()
                .find(|a| a.name() == "id" && a.namespace() == Some(R_NS))?
                .value();
            let target = rels.get(rid)?;
            paths.push(resolve_part_path("ppt", target));
        }
        if paths.is_empty() {
            None
        } else {
            Some(paths)
        }
    })();

    if let Some(paths) = ordered {
        return Ok(paths);
    }

    // Fallback: every ppt/slides/slideN.xml in numeric order.
    let mut numbered: Vec<(u32, String)> = (0..zip.len())
        .filter_map(|i| {
            let name = zip.by_index(i).ok()?.name().to_string();
            let n: u32 = name
                .strip_prefix("ppt/slides/slide")?
                .strip_suffix(".xml")?
                .parse()
                .ok()?;
            Some((n, name))
        })
        .collect();
    numbered.sort();
    Ok(numbered.into_iter().map(|(_, name)| name).collect())
}

/// Parse a `.rels` part into Id → Target. Returns None on malformed XML.
fn parse_relationships(xml: &str) -> Option<HashMap<String, String>> {
    let doc = Document::parse(xml).ok()?;
    Some(
        doc.descendants()
            .filter(|n| n.tag_name().name() == "Relationship")
            .filter_map(|n| {
                Some((
                    n.attribute("Id")?.to_string(),
                    n.attribute("Target")?.to_string(),
                ))
            })
            .collect(),
    )
}

/// Resolve a rels Target (possibly `../notesSlides/x.xml`) against a base dir.
fn resolve_part_path(base_dir: &str, target: &str) -> String {
    let mut parts: Vec<&str> = base_dir.split('/').filter(|p| !p.is_empty()).collect();
    for seg in target.split('/') {
        match seg {
            ".." => {
                parts.pop();
            }
            "." | "" => {}
            other => parts.push(other),
        }
    }
    parts.join("/")
}

/// Speaker-notes text for a slide part, resolved via the slide's rels.
fn notes_for_slide(zip: &mut ZipArchive<Cursor<&[u8]>>, slide_path: &str) -> Option<String> {
    let (dir, file) = slide_path.rsplit_once('/')?;
    let rels_xml = read_zip_string(zip, &format!("{dir}/_rels/{file}.rels"))?;
    let doc = Document::parse(&rels_xml).ok()?;
    let target = doc
        .descendants()
        .filter(|n| n.tag_name().name() == "Relationship")
        .find(|n| {
            n.attribute("Type")
                .is_some_and(|t| t.ends_with("/notesSlide"))
        })?
        .attribute("Target")?
        .to_string();

    let notes_path = resolve_part_path(dir, &target);
    let xml = read_zip_string(zip, &notes_path)?;
    let notes = notes_text(&xml)?;
    if notes.trim().is_empty() {
        None
    } else {
        Some(notes)
    }
}

// ---------------------------------------------------------------------------
// XML extraction
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum Block {
    Title(String),
    Paragraphs(Vec<String>),
    Table(Vec<Vec<String>>),
    OmittedFigure(&'static str),
}

/// Returns the slide's markdown plus whether the slide carried any real
/// content (text, table, or notes) beyond the synthesized heading.
fn slide_to_markdown(xml: &str, slide_no: usize, notes: Option<&str>) -> Result<(String, bool)> {
    let doc = Document::parse(xml).context("parse slide xml")?;
    let mut blocks = Vec::new();
    if let Some(sp_tree) = doc.descendants().find(|n| n.tag_name().name() == "spTree") {
        walk_shape_tree(sp_tree, &mut blocks);
    }

    let title = blocks.iter().find_map(|b| match b {
        Block::Title(t) if !t.trim().is_empty() => Some(t.clone()),
        _ => None,
    });

    let mut out = match &title {
        Some(t) => format!("## Slide {slide_no}: {t}\n\n"),
        None => format!("## Slide {slide_no}\n\n"),
    };

    for block in &blocks {
        match block {
            Block::Title(_) => {}
            Block::Paragraphs(paras) => {
                for para in paras {
                    if !para.trim().is_empty() {
                        out.push_str(para.trim());
                        out.push_str("\n\n");
                    }
                }
            }
            Block::Table(rows) => {
                out.push_str(&table_markdown(rows));
                out.push('\n');
            }
            Block::OmittedFigure(kind) => {
                out.push_str(&format!("*[{kind} omitted]*\n\n"));
            }
        }
    }

    if let Some(notes) = notes {
        out.push_str("> **Speaker notes:**\n");
        for line in notes.lines().filter(|l| !l.trim().is_empty()) {
            out.push_str(&format!("> {}\n", line.trim()));
        }
        out.push('\n');
    }

    let has_content = notes.is_some()
        || blocks
            .iter()
            .any(|b| matches!(b, Block::Title(_) | Block::Paragraphs(_) | Block::Table(_)));
    Ok((out, has_content))
}

fn walk_shape_tree(node: Node, blocks: &mut Vec<Block>) {
    for child in node.children().filter(Node::is_element) {
        match child.tag_name().name() {
            "sp" => {
                let is_title = child
                    .descendants()
                    .filter(|n| n.tag_name().name() == "ph")
                    .any(|ph| matches!(ph.attribute("type"), Some("title") | Some("ctrTitle")));
                let paras = shape_paragraphs(child);
                if paras.is_empty() {
                    continue;
                }
                if is_title {
                    blocks.push(Block::Title(paras.join(" ")));
                } else {
                    blocks.push(Block::Paragraphs(paras));
                }
            }
            "graphicFrame" => {
                if let Some(tbl) = child.descendants().find(|n| n.tag_name().name() == "tbl") {
                    blocks.push(Block::Table(table_rows(tbl)));
                } else if child
                    .descendants()
                    .any(|n| n.tag_name().name() == "graphicData")
                {
                    // Charts, SmartArt, embedded objects: text lives in other
                    // parts we don't parse — say so instead of dropping it.
                    blocks.push(Block::OmittedFigure("chart or diagram"));
                }
            }
            "grpSp" => walk_shape_tree(child, blocks),
            "pic" => blocks.push(Block::OmittedFigure("image")),
            _ => {}
        }
    }
}

/// One string per `<a:p>` paragraph inside the shape's text body.
fn shape_paragraphs(sp: Node) -> Vec<String> {
    let Some(tx_body) = sp.descendants().find(|n| n.tag_name().name() == "txBody") else {
        return Vec::new();
    };
    tx_body
        .descendants()
        .filter(|n| n.tag_name().name() == "p")
        .map(paragraph_text)
        .filter(|p| !p.trim().is_empty())
        .collect()
}

fn paragraph_text(p: Node) -> String {
    let mut text = String::new();
    for node in p.descendants() {
        match node.tag_name().name() {
            "t" => text.push_str(node.text().unwrap_or_default()),
            "br" => text.push(' '),
            _ => {}
        }
    }
    text
}

fn table_rows(tbl: Node) -> Vec<Vec<String>> {
    tbl.descendants()
        .filter(|n| n.tag_name().name() == "tr")
        .map(|tr| {
            tr.children()
                .filter(|n| n.tag_name().name() == "tc")
                .map(|tc| {
                    tc.descendants()
                        .filter(|n| n.tag_name().name() == "p")
                        .map(paragraph_text)
                        .collect::<Vec<_>>()
                        .join(" ")
                        .trim()
                        .replace('|', "\\|")
                })
                .collect()
        })
        .collect()
}

fn table_markdown(rows: &[Vec<String>]) -> String {
    let mut out = String::new();
    let mut iter = rows.iter();
    let Some(header) = iter.next() else {
        return out;
    };
    out.push_str(&format!("| {} |\n", header.join(" | ")));
    out.push_str(&format!("|{}\n", " --- |".repeat(header.len())));
    for row in iter {
        out.push_str(&format!("| {} |\n", row.join(" | ")));
    }
    out
}

/// Notes body text from a notesSlide part, skipping the slide-image and
/// slide-number placeholders.
fn notes_text(xml: &str) -> Option<String> {
    let doc = Document::parse(xml).ok()?;
    let mut lines = Vec::new();
    for sp in doc.descendants().filter(|n| n.tag_name().name() == "sp") {
        let skip = sp
            .descendants()
            .filter(|n| n.tag_name().name() == "ph")
            .any(|ph| matches!(ph.attribute("type"), Some("sldImg") | Some("sldNum")));
        if skip {
            continue;
        }
        lines.extend(shape_paragraphs(sp));
    }
    Some(lines.join("\n"))
}

fn core_title(xml: &str) -> Option<String> {
    let doc = Document::parse(xml).ok()?;
    doc.descendants()
        .find(|n| n.tag_name().name() == "title")
        .and_then(|n| n.text())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::FileOptions;

    const P_NS: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
    const A_NS: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";

    fn make_pptx(parts: &[(&str, String)]) -> Vec<u8> {
        let mut buf = Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut buf);
            for (name, content) in parts {
                zip.start_file(*name, FileOptions::default()).unwrap();
                zip.write_all(content.as_bytes()).unwrap();
            }
            zip.finish().unwrap();
        }
        buf.into_inner()
    }

    fn fixture_pptx() -> Vec<u8> {
        let presentation = format!(
            r#"<p:presentation xmlns:p="{P_NS}" xmlns:r="{R_NS}">
                 <p:sldIdLst>
                   <p:sldId id="256" r:id="rId1"/>
                   <p:sldId id="257" r:id="rId2"/>
                 </p:sldIdLst>
               </p:presentation>"#
        );
        let pres_rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
              <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide1.xml"/>
              <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide2.xml"/>
            </Relationships>"#
            .to_string();
        let slide1 = format!(
            r#"<p:sld xmlns:p="{P_NS}" xmlns:a="{A_NS}">
                 <p:cSld><p:spTree>
                   <p:sp>
                     <p:nvSpPr><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr>
                     <p:txBody><a:p><a:r><a:t>Study Design</a:t></a:r></a:p></p:txBody>
                   </p:sp>
                   <p:sp>
                     <p:txBody>
                       <a:p><a:r><a:t>Cohort of 120 patients</a:t></a:r></a:p>
                       <a:p><a:r><a:t>Randomized double-blind</a:t></a:r></a:p>
                     </p:txBody>
                   </p:sp>
                 </p:spTree></p:cSld>
               </p:sld>"#
        );
        let slide1_rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
              <Relationship Id="rId7" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesSlide" Target="../notesSlides/notesSlide1.xml"/>
            </Relationships>"#
            .to_string();
        let notes1 = format!(
            r#"<p:notes xmlns:p="{P_NS}" xmlns:a="{A_NS}">
                 <p:cSld><p:spTree>
                   <p:sp>
                     <p:nvSpPr><p:nvPr><p:ph type="sldImg"/></p:nvPr></p:nvSpPr>
                   </p:sp>
                   <p:sp>
                     <p:nvSpPr><p:nvPr><p:ph type="body" idx="1"/></p:nvPr></p:nvSpPr>
                     <p:txBody><a:p><a:r><a:t>Mention the dropout rate here.</a:t></a:r></a:p></p:txBody>
                   </p:sp>
                 </p:spTree></p:cSld>
               </p:notes>"#
        );
        let slide2 = format!(
            r#"<p:sld xmlns:p="{P_NS}" xmlns:a="{A_NS}">
                 <p:cSld><p:spTree>
                   <p:graphicFrame><a:graphic><a:graphicData><a:tbl>
                     <a:tr>
                       <a:tc><a:txBody><a:p><a:r><a:t>Gene</a:t></a:r></a:p></a:txBody></a:tc>
                       <a:tc><a:txBody><a:p><a:r><a:t>Expression</a:t></a:r></a:p></a:txBody></a:tc>
                     </a:tr>
                     <a:tr>
                       <a:tc><a:txBody><a:p><a:r><a:t>TP53</a:t></a:r></a:p></a:txBody></a:tc>
                       <a:tc><a:txBody><a:p><a:r><a:t>high</a:t></a:r></a:p></a:txBody></a:tc>
                     </a:tr>
                   </a:tbl></a:graphicData></a:graphic></p:graphicFrame>
                 </p:spTree></p:cSld>
               </p:sld>"#
        );
        let core = r#"<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/">
              <dc:title>Trial Results Deck</dc:title>
            </cp:coreProperties>"#
            .to_string();

        make_pptx(&[
            ("ppt/presentation.xml", presentation),
            ("ppt/_rels/presentation.xml.rels", pres_rels),
            ("ppt/slides/slide1.xml", slide1),
            ("ppt/slides/_rels/slide1.xml.rels", slide1_rels),
            ("ppt/notesSlides/notesSlide1.xml", notes1),
            ("ppt/slides/slide2.xml", slide2),
            ("docProps/core.xml", core),
        ])
    }

    #[test]
    fn extracts_titles_body_tables_and_notes_in_order() {
        let c = pptx_to_markdown(&fixture_pptx()).unwrap();
        let md = &c.markdown;

        assert_eq!(c.title.as_deref(), Some("Trial Results Deck"));
        assert!(md.contains("# Trial Results Deck"), "got: {md}");
        assert!(md.contains("## Slide 1: Study Design"), "got: {md}");
        assert!(md.contains("Cohort of 120 patients"), "got: {md}");
        assert!(md.contains("Randomized double-blind"), "got: {md}");
        assert!(
            md.contains("> **Speaker notes:**\n> Mention the dropout rate here."),
            "got: {md}"
        );
        assert!(md.contains("| Gene | Expression |"), "got: {md}");
        assert!(md.contains("| TP53 | high |"), "got: {md}");

        let slide1_pos = md.find("## Slide 1").unwrap();
        let slide2_pos = md.find("## Slide 2").unwrap();
        assert!(slide1_pos < slide2_pos);
    }

    #[test]
    fn falls_back_to_numeric_slide_order_without_presentation_xml() {
        let slide = |text: &str| {
            format!(
                r#"<p:sld xmlns:p="{P_NS}" xmlns:a="{A_NS}">
                     <p:cSld><p:spTree><p:sp>
                       <p:txBody><a:p><a:r><a:t>{text}</a:t></a:r></a:p></p:txBody>
                     </p:sp></p:spTree></p:cSld>
                   </p:sld>"#
            )
        };
        let bytes = make_pptx(&[
            ("ppt/slides/slide2.xml", slide("second")),
            ("ppt/slides/slide1.xml", slide("first")),
        ]);
        let c = pptx_to_markdown(&bytes).unwrap();
        assert!(c.markdown.find("first").unwrap() < c.markdown.find("second").unwrap());
    }

    #[test]
    fn rejects_pptx_without_text() {
        let bytes = make_pptx(&[(
            "ppt/slides/slide1.xml",
            format!(r#"<p:sld xmlns:p="{P_NS}"><p:cSld><p:spTree/></p:cSld></p:sld>"#),
        )]);
        let err = pptx_to_markdown(&bytes);
        assert!(err.is_err());
    }

    #[test]
    fn rejects_garbage_bytes() {
        assert!(pptx_to_markdown(b"not a pptx").is_err());
    }
}
