//! Splitting a concept document into `(frontmatter, body)`.
//!
//! OKF §4 defines a concept document as exactly two parts: "a **YAML
//! frontmatter block**, delimited by `---` on its own line at the start of the
//! file and a closing `---` on its own line", then "a **markdown body**".
//!
//! ## Why this is not [`crate::knowledge::store::split_frontmatter`]
//!
//! The tree already has a splitter, and it is deliberately left alone here
//! (Stage 3 owns replacing it). It differs from the spec's reference parser in
//! three ways that matter to a *format* module:
//!
//! 1. It searches for the byte sequence `"\n---\n"`, so a closing delimiter
//!    carrying a trailing space, or a CRLF file, is never found and the whole
//!    file silently becomes body — the frontmatter is not reported missing, it
//!    is reported as *prose*. This module matches on **lines**, per the spec's
//!    wording, and tolerates `\r`.
//! 2. It returns `Value::Null` for both "there is no frontmatter" and "the
//!    frontmatter is unparseable YAML". Those are opposite facts: the first is
//!    legal-but-nonconformant (OKF §11 rule 1), the second is a producer bug,
//!    and a validator that cannot tell them apart can only report neither.
//! 3. It hands back a `Value`, so a caller that re-serialises loses nothing only
//!    by accident. Here the mapping is the unit of round-tripping and unknown
//!    keys ride in it verbatim — OKF §4.1: "Consumers SHOULD preserve unknown
//!    keys when round-tripping and MUST NOT reject documents with unrecognized
//!    fields."
//!
//! ## What is an error here, and what is not
//!
//! An **absent** block is not an error: it yields an empty mapping, and the
//! missing `type` is reported by [`super::conformance`] as a diagnostic rather
//! than by refusing to parse. An **unterminated** block and a **non-mapping**
//! top level are errors, because in both cases there is no honest way to say
//! where the frontmatter stops and the prose starts — guessing would attribute
//! body text to metadata or vice versa. Per DR-7 an error here still never
//! rejects a *page*: `conformance::check_source` turns it into a diagnostic.

use serde_yaml::{Mapping, Value};

/// The delimiter line, as a line and not as a byte pattern (see the module
/// header: the byte-pattern spelling is what makes a CRLF file parse as prose).
pub const DELIMITER: &str = "---";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum FrontmatterError {
    #[error("frontmatter opened with `---` but no closing `---` line was found")]
    Unterminated,
    #[error("frontmatter is a YAML {found}, but OKF §4.1 frontmatter is a key/value mapping")]
    NotAMapping { found: &'static str },
    #[error("frontmatter is not valid YAML: {0}")]
    Yaml(String),
}

/// The result of splitting one document.
///
/// `had_block` distinguishes "the block was there and empty" from "there was no
/// block", which is the distinction the old splitter could not make and which
/// OKF §11 rule 1 is stated in terms of.
#[derive(Debug, Clone, PartialEq)]
pub struct Split {
    pub frontmatter: Mapping,
    pub body: String,
    pub had_block: bool,
}

pub fn split(text: &str) -> Result<Split, FrontmatterError> {
    // A UTF-8 BOM ahead of the opening `---` is invisible in every editor and
    // would otherwise make line 1 not equal `---`, i.e. it would present as a
    // page that mysteriously has no frontmatter.
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let Some(rest) = strip_opening_delimiter(text) else {
        return Ok(Split {
            frontmatter: Mapping::new(),
            body: text.to_string(),
            had_block: false,
        });
    };
    let (yaml, body) = split_at_closing_delimiter(rest).ok_or(FrontmatterError::Unterminated)?;
    Ok(Split {
        frontmatter: parse_mapping(&yaml)?,
        body,
        had_block: true,
    })
}

/// Re-assemble a document. The inverse of [`split`] for content, not for bytes:
/// serde_yaml re-emits the mapping in its own style, so quoting and scalar
/// formatting may change while every key and value survives. The gate test in
/// [`super::model`] therefore compares parsed mappings, never strings — a
/// byte-comparison would fail on `3.0e-6` being re-emitted as `3e-6` and tell
/// us nothing about whether any content was lost.
pub fn join(frontmatter: &Mapping, body: &str) -> String {
    let mut out = String::from(DELIMITER);
    out.push('\n');
    if !frontmatter.is_empty() {
        let yaml = serde_yaml::to_string(&Value::Mapping(frontmatter.clone()))
            .unwrap_or_else(|_| String::new());
        out.push_str(&yaml);
        if !yaml.ends_with('\n') {
            out.push('\n');
        }
    }
    out.push_str(DELIMITER);
    out.push('\n');
    out.push_str(body);
    out
}

/// Split off the first line without slicing (the workspace denies
/// `clippy::string_slice`), returning it with its `\r` trimmed so a CRLF
/// document behaves identically to an LF one.
fn split_first_line(s: &str) -> (&str, &str) {
    match s.split_once('\n') {
        Some((line, rest)) => (line.trim_end_matches('\r'), rest),
        None => (s.trim_end_matches('\r'), ""),
    }
}

fn is_delimiter_line(line: &str) -> bool {
    // `trim_end` and not `trim`: OKF says the delimiter is on its own line, and
    // an indented `---` inside a YAML block scalar must not close the block.
    line.trim_end() == DELIMITER
}

fn strip_opening_delimiter(text: &str) -> Option<&str> {
    let (first, rest) = split_first_line(text);
    is_delimiter_line(first).then_some(rest)
}

/// Walk lines until a closing delimiter. Returns `None` for an unterminated
/// block rather than treating end-of-file as a close, because a file that ends
/// mid-frontmatter is a truncated write, and silently accepting it would commit
/// the truncation.
fn split_at_closing_delimiter(rest: &str) -> Option<(String, String)> {
    let mut yaml = String::new();
    let mut cursor = rest;
    loop {
        if cursor.is_empty() {
            return None;
        }
        let (line, next) = split_first_line(cursor);
        if is_delimiter_line(line) {
            return Some((yaml, next.to_string()));
        }
        yaml.push_str(line);
        yaml.push('\n');
        cursor = next;
    }
}

fn parse_mapping(yaml: &str) -> Result<Mapping, FrontmatterError> {
    let value: Value =
        serde_yaml::from_str(yaml).map_err(|e| FrontmatterError::Yaml(e.to_string()))?;
    match value {
        // `---\n---\n` is an empty block, not a missing one. YAML calls that
        // null; OKF calls it a mapping with no keys, and every consumer here
        // wants the mapping so it can ask for `type` without a null check.
        Value::Null => Ok(Mapping::new()),
        Value::Mapping(m) => Ok(m),
        other => Err(FrontmatterError::NotAMapping {
            found: yaml_type_name(&other),
        }),
    }
}

fn yaml_type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Sequence(_) => "sequence",
        Value::Mapping(_) => "mapping",
        Value::Tagged(_) => "tagged value",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(map: &Mapping, k: &str) -> Option<String> {
        map.get(Value::String(k.to_string()))
            .and_then(|v| v.as_str())
            .map(str::to_string)
    }

    #[test]
    fn splits_a_normal_document() {
        let s = split("---\ntype: Molecule\n---\n# Body\n\ntext\n").unwrap();
        assert!(s.had_block);
        assert_eq!(key(&s.frontmatter, "type").as_deref(), Some("Molecule"));
        assert_eq!(s.body, "# Body\n\ntext\n");
    }

    #[test]
    fn absent_frontmatter_yields_an_empty_mapping_and_the_whole_text_as_body() {
        // OKF §11 rule 1 makes this non-conformant, but a consumer that refused
        // it would refuse every plain README in a bundle. The mapping is empty,
        // `had_block` is false, and `conformance` reports it.
        let s = split("# Just prose\n").unwrap();
        assert!(!s.had_block);
        assert!(s.frontmatter.is_empty());
        assert_eq!(s.body, "# Just prose\n");
    }

    #[test]
    fn an_empty_block_is_a_block_with_no_keys_not_an_absent_block() {
        let s = split("---\n---\nbody\n").unwrap();
        assert!(s.had_block, "the delimiters were present");
        assert!(s.frontmatter.is_empty());
        assert_eq!(s.body, "body\n");
    }

    #[test]
    fn unterminated_block_is_an_error() {
        assert_eq!(
            split("---\ntype: Molecule\nno closing delimiter\n").unwrap_err(),
            FrontmatterError::Unterminated
        );
    }

    #[test]
    fn a_file_that_ends_immediately_after_the_opening_delimiter_is_unterminated() {
        assert_eq!(split("---\n").unwrap_err(), FrontmatterError::Unterminated);
    }

    #[test]
    fn non_mapping_top_level_is_an_error_and_names_what_it_found() {
        let err = split("---\n- one\n- two\n---\nbody\n").unwrap_err();
        assert_eq!(err, FrontmatterError::NotAMapping { found: "sequence" });
        let err = split("---\njust a scalar\n---\nbody\n").unwrap_err();
        assert_eq!(err, FrontmatterError::NotAMapping { found: "string" });
    }

    #[test]
    fn malformed_yaml_is_reported_as_yaml_not_as_missing_frontmatter() {
        // The distinction the old byte-pattern splitter could not draw: this is
        // a producer bug, not a page without metadata.
        let err = split("---\ntype: [unclosed\n---\nbody\n").unwrap_err();
        assert!(matches!(err, FrontmatterError::Yaml(_)), "got {err:?}");
    }

    #[test]
    fn crlf_documents_split_exactly_like_lf_documents() {
        // `store::split_frontmatter`'s `"\n---\n"` search fails here and hands
        // the caller the entire file as body.
        let s = split("---\r\ntype: Molecule\r\n---\r\nbody\r\n").unwrap();
        assert!(s.had_block);
        assert_eq!(key(&s.frontmatter, "type").as_deref(), Some("Molecule"));
        assert_eq!(s.body, "body\r\n");
    }

    #[test]
    fn a_closing_delimiter_with_trailing_whitespace_still_closes() {
        let s = split("---\ntype: Molecule\n---   \nbody\n").unwrap();
        assert!(s.had_block);
        assert_eq!(s.body, "body\n");
    }

    #[test]
    fn four_dashes_is_not_a_delimiter() {
        // A horizontal rule written as `----` inside frontmatter would otherwise
        // truncate the block at an arbitrary point.
        assert_eq!(
            split("---\ntype: X\n----\nstill yaml\n").unwrap_err(),
            FrontmatterError::Unterminated
        );
    }

    #[test]
    fn an_indented_delimiter_does_not_close_the_block() {
        let s = split("---\nnote: |\n  ---\n  still the scalar\n---\nbody\n").unwrap();
        assert_eq!(s.body, "body\n");
        assert!(key(&s.frontmatter, "note").unwrap().contains("---"));
    }

    #[test]
    fn a_leading_bom_does_not_hide_the_frontmatter() {
        let s = split("\u{feff}---\ntype: Molecule\n---\nbody\n").unwrap();
        assert!(s.had_block);
        assert_eq!(key(&s.frontmatter, "type").as_deref(), Some("Molecule"));
    }

    #[test]
    fn a_closing_delimiter_at_eof_without_a_newline_still_closes() {
        let s = split("---\ntype: Molecule\n---").unwrap();
        assert!(s.had_block);
        assert_eq!(s.body, "");
    }

    #[test]
    fn body_bytes_are_preserved_verbatim_including_inner_delimiters() {
        // A `---` used as a horizontal rule in prose must not be re-interpreted.
        let body = "para\n\n---\n\nmore\n";
        let s = split(&format!("---\ntype: X\n---\n{body}")).unwrap();
        assert_eq!(s.body, body);
    }

    #[test]
    fn join_round_trips_content_and_leaves_the_body_untouched() {
        let original = "---\ntype: Molecule\ntags:\n- a\n- b\n---\nbody with [[link]]\n";
        let s = split(original).unwrap();
        let rejoined = join(&s.frontmatter, &s.body);
        let again = split(&rejoined).unwrap();
        assert_eq!(again.frontmatter, s.frontmatter);
        assert_eq!(again.body, s.body);
    }

    #[test]
    fn join_of_an_empty_mapping_emits_a_well_formed_empty_block() {
        let out = join(&Mapping::new(), "body\n");
        assert_eq!(out, "---\n---\nbody\n");
        assert!(split(&out).unwrap().had_block);
    }

    #[test]
    fn serde_yaml_does_not_emit_its_own_document_marker() {
        // `join` prepends `---` itself; if serde_yaml ever started emitting one
        // the output would carry two and every page would fail to parse. Pinned
        // here so a dependency bump fails loudly in this file rather than in
        // every knowledge test at once.
        let mut m = Mapping::new();
        m.insert(Value::String("type".into()), Value::String("X".into()));
        let s = serde_yaml::to_string(&Value::Mapping(m)).unwrap();
        assert!(!s.starts_with("---"), "got {s:?}");
    }
}
