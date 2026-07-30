//! Explicit resource references — the skills, extensions and knowledge bases a
//! user attaches to a message before sending it.
//!
//! # The canonical form is a tag
//!
//! ```text
//! <biorouter-ref type="skill" name="single-cell RNA &quot;QC&quot;">
//! <biorouter-ref type="extension" name="Chat Recall">
//! <biorouter-ref type="knowledge_base" id="soul" label="Soul &amp; Body">
//! ```
//!
//! Everything a composer emits uses this form. It exists because the compact
//! `/skill:name` markers cannot carry a name with a space in it: the extractor
//! for those splits the message on whitespace, so `/skill:my skill` reaches the
//! resolver as `my` and resolves to nothing (issue #65). Normalising the name
//! away — the fix that worked for `/ext:` in #60, where every consumer
//! re-normalises anyway — cannot transfer, because `skills_extension`'s
//! `loadSkill` looks the name up *exactly*: `myskill` is not `my skill`, so
//! that fix would trade a truncation for a quieter failure.
//!
//! The tag delimits the value explicitly instead of by whitespace, so any name
//! survives it.
//!
//! # Escaping — the contract other implementations must match
//!
//! Attribute values are escaped with a **fixed six-entry table** of XML
//! character references, applied to every occurrence regardless of position:
//!
//! | character | escaped as |
//! |-----------|------------|
//! | `&`       | `&amp;`    |
//! | `"`       | `&quot;`   |
//! | `<`       | `&lt;`     |
//! | `>`       | `&gt;`     |
//! | `\n`      | `&#10;`    |
//! | `\r`      | `&#13;`    |
//!
//! XML entities were chosen over percent-encoding or backslash escapes because
//! the syntax is already XML-shaped: `&quot;` is what a reader — human or
//! machine — expects to find inside `name="…"`, and it keeps the tag parseable
//! by a real XML parser should anyone ever want to use one.
//!
//! The table is **closed**. There is no general numeric-character-reference
//! support and no `&apos;`/`&nbsp;`: `&#65;` decodes to the literal text
//! `&#65;`, not to `A`. A closed table is a map an implementation in another
//! language can copy verbatim, which is the point — [`encode_ref_value`] and
//! [`decode_ref_value`] are exact inverses of each other and every emitter must
//! agree with them.
//!
//! Two rules make or break an implementation:
//!
//! * **Encoding must escape `&` first.** Written as a chain of replacements,
//!   `"<"` → `"&lt;"` → (`&` pass) → `"&amp;lt;"` corrupts every other escape.
//!   The implementation here iterates characters once and maps each
//!   independently, which is order-free by construction; a chained
//!   `String.replace` port must do `&` before all others.
//! * **Decoding must resolve `&amp;` last** for the mirror-image reason —
//!   otherwise a name containing the literal text `&quot;`, which encodes to
//!   `&amp;quot;`, decodes back to `"`. [`decode_ref_value`] scans left to
//!   right and consumes a whole entity at a time, which again removes the
//!   ordering question; a chained port must do `&amp;` last.
//!
//! `'` is deliberately *not* escaped, and single-quoted attribute values are
//! *not* accepted, so there is exactly one quoting style to agree on.
//!
//! # What a producer must guarantee
//!
//! A value is scanned to the next raw `"`, and a tag is abandoned at the next
//! raw `<`. Both are unreachable in correctly encoded output, so this costs
//! nothing — but a hand-written tag that skips the encoding will truncate.
//!
//! # Compatibility: the compact markers still parse
//!
//! `/skill:`, `/ext:`, `/kb:`, `/skill(…)`, `kb_id:` and the quoted legacy
//! phrases all still resolve. They appear in persisted sessions, saved
//! workflows, skill documents and the CLI completer, so removing them would
//! break messages already on disk. They keep their whitespace limitation — that
//! is now an acceptable tradeoff rather than a bug, because a producer that
//! needs to carry a name the compact form cannot represent has the tag to reach
//! for, and `reference_marker` picks between the two automatically.

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KnowledgeBaseRef {
    pub id: String,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ResourceRefs {
    pub skills: Vec<String>,
    pub extensions: Vec<String>,
    pub knowledge_bases: Vec<KnowledgeBaseRef>,
}

impl ResourceRefs {
    pub fn is_empty(&self) -> bool {
        self.skills.is_empty() && self.extensions.is_empty() && self.knowledge_bases.is_empty()
    }
}

/// The element name every reference tag uses.
pub const REF_TAG_NAME: &str = "biorouter-ref";

/// The escape table. Closed by design — see the module docs.
///
/// Order is irrelevant to both [`encode_ref_value`] and [`decode_ref_value`]:
/// each maps one character or one whole entity at a time rather than sweeping
/// the string once per rule, so neither can corrupt an escape it already
/// produced. No two entities share a first character after the `&`, so the
/// decoder's match is unambiguous.
const REF_ENTITIES: &[(char, &str)] = &[
    ('&', "&amp;"),
    ('"', "&quot;"),
    ('<', "&lt;"),
    ('>', "&gt;"),
    ('\n', "&#10;"),
    ('\r', "&#13;"),
];

/// Which kind of resource a reference names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefKind {
    Skill,
    Extension,
    KnowledgeBase,
}

impl RefKind {
    /// The `type="…"` keyword. A closed set, so it is never escaped.
    pub fn tag_type(self) -> &'static str {
        match self {
            RefKind::Skill => "skill",
            RefKind::Extension => "extension",
            RefKind::KnowledgeBase => "knowledge_base",
        }
    }

    /// The attribute the resource's identity travels in. Knowledge bases are
    /// named by `id` because that is what `kb_search` takes; skills and
    /// extensions by `name`.
    pub fn value_attr(self) -> &'static str {
        match self {
            RefKind::KnowledgeBase => "id",
            _ => "name",
        }
    }
}

/// Escape `value` for an attribute of a reference tag.
///
/// The inverse of [`decode_ref_value`]; see the module docs for the table and
/// for why the two must not be implemented as chained replacements.
pub fn encode_ref_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match REF_ENTITIES.iter().find(|(escaped, _)| *escaped == ch) {
            Some((_, entity)) => out.push_str(entity),
            None => out.push(ch),
        }
    }
    out
}

/// Unescape an attribute value from a reference tag.
///
/// Anything that is not one of the six entities in the table is passed through
/// untouched, including a lone `&`, an unknown entity (`&nbsp;`) and a numeric
/// reference outside the table (`&#65;`). Dropping them would silently mangle
/// names, and guessing at them would make this disagree with the encoder.
pub fn decode_ref_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;

    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let tail = &rest[amp..];
        match REF_ENTITIES
            .iter()
            .find(|(_, entity)| tail.starts_with(*entity))
        {
            Some((decoded, entity)) => {
                out.push(*decoded);
                rest = &tail[entity.len()..];
            }
            // Not an entity we emit: keep the `&` verbatim and carry on from
            // the next character, so `&amp;` inside `&amp;amp;` still decodes
            // exactly once.
            None => {
                out.push('&');
                rest = &tail['&'.len_utf8()..];
            }
        }
    }

    out.push_str(rest);
    out
}

/// The canonical tag naming `value`.
pub fn ref_tag(kind: RefKind, value: &str) -> String {
    format!(
        "<{REF_TAG_NAME} type=\"{}\" {}=\"{}\">",
        kind.tag_type(),
        kind.value_attr(),
        encode_ref_value(value)
    )
}

/// The canonical tag naming `value`, carrying a display string for the chip a
/// UI renders in its place.
///
/// Only a knowledge base's label is read back out (it has somewhere to go — see
/// `KnowledgeBaseRef::label`); on the other kinds it is presentation-only and
/// the parser ignores it, which is exactly the tolerance a composer needs to
/// add attributes without coordinating with this module.
pub fn labelled_ref_tag(kind: RefKind, value: &str, label: &str) -> String {
    format!(
        "<{REF_TAG_NAME} type=\"{}\" {}=\"{}\" label=\"{}\">",
        kind.tag_type(),
        kind.value_attr(),
        encode_ref_value(value),
        encode_ref_value(label)
    )
}

pub(crate) fn extract_resource_refs(text: &str) -> ResourceRefs {
    let mut refs = ResourceRefs::default();

    extract_tag_refs(text, "skill", "name", &mut refs.skills);
    extract_tag_refs(text, "extension", "name", &mut refs.extensions);
    extract_kb_tag_refs(text, &mut refs.knowledge_bases);

    extract_legacy_resource_phrases(text, "skill", &mut refs.skills);
    extract_legacy_resource_phrases(text, "extension", &mut refs.extensions);
    extract_inline_refs(text, "skill:", &mut refs.skills);
    extract_inline_refs(text, "ext:", &mut refs.extensions);
    extract_inline_kb_refs(text, &mut refs.knowledge_bases);
    extract_function_refs(text, "/skill(", ")", &mut refs.skills);
    extract_function_refs(text, "/ext(", ")", &mut refs.extensions);
    extract_function_kb_refs(text, &mut refs.knowledge_bases);
    extract_legacy_kb_refs(text, &mut refs.knowledge_bases);

    dedup(&mut refs.skills);
    dedup(&mut refs.extensions);
    dedup_kbs(&mut refs.knowledge_bases);

    refs
}

// A `/ext:` reference is resolved to a concrete extension by
// `extension_manager::resolve_bundled_extension`, which keys off the extension
// id *and* the registry that owns it. This module only extracts the raw
// reference text from the message.

fn extract_tag_refs(text: &str, tag_type: &str, attr: &str, out: &mut Vec<String>) {
    let needle = format!("<biorouter-ref type=\"{tag_type}\" {attr}=\"");
    let mut rest = text;
    while let Some(start) = rest.find(&needle) {
        let value_start = start + needle.len();
        let value_rest = slice_from(rest, value_start);
        let Some(end) = value_rest.find('"') else {
            break;
        };
        push_trimmed(out, slice_to(value_rest, end));
        rest = slice_from(value_rest, end);
    }
}

fn extract_kb_tag_refs(text: &str, out: &mut Vec<KnowledgeBaseRef>) {
    let needle = "<biorouter-ref type=\"knowledge_base\" id=\"";
    let mut rest = text;
    while let Some(start) = rest.find(needle) {
        let id_start = start + needle.len();
        let id_rest = slice_from(rest, id_start);
        let Some(id_end) = id_rest.find('"') else {
            break;
        };
        let id = slice_to(id_rest, id_end).trim();
        if !id.is_empty() {
            out.push(KnowledgeBaseRef {
                id: id.to_string(),
                label: None,
            });
        }
        rest = slice_from(id_rest, id_end);
    }
}

fn extract_function_refs(text: &str, prefix: &str, suffix: &str, out: &mut Vec<String>) {
    let mut rest = text;
    while let Some(start) = rest.find(prefix) {
        let value_start = start + prefix.len();
        let value_rest = slice_from(rest, value_start);
        let Some(end) = value_rest.find(suffix) else {
            break;
        };
        push_trimmed(out, slice_to(value_rest, end));
        rest = slice_from(value_rest, end + suffix.len());
    }
}

fn extract_function_kb_refs(text: &str, out: &mut Vec<KnowledgeBaseRef>) {
    let mut refs = Vec::new();
    extract_function_refs(text, "/kb(", ")", &mut refs);
    for id in refs {
        out.push(KnowledgeBaseRef { id, label: None });
    }
}

fn extract_legacy_resource_phrases(text: &str, kind: &str, out: &mut Vec<String>) {
    let marker = format!("\" {kind}");
    let mut rest = text;
    while let Some(marker_start) = rest.find(&marker) {
        let before_marker = slice_to(rest, marker_start);
        let Some(open_quote) = before_marker.rfind('"') else {
            rest = slice_from(rest, marker_start + marker.len());
            continue;
        };
        push_trimmed(out, slice_from(before_marker, open_quote + 1));
        rest = slice_from(rest, marker_start + marker.len());
    }
}

fn extract_inline_refs(text: &str, prefix: &str, out: &mut Vec<String>) {
    for token in text.split_whitespace() {
        let trimmed = token.trim_matches(|c: char| c == ',' || c == '.' || c == ';');
        let value = trimmed.strip_prefix(&format!("/{prefix}"));
        if let Some(value) = value {
            push_trimmed(out, value);
        }
    }
}

fn extract_inline_kb_refs(text: &str, out: &mut Vec<KnowledgeBaseRef>) {
    for token in text.split_whitespace() {
        let trimmed = token.trim_matches(|c: char| c == ',' || c == '.' || c == ';');
        let value = trimmed.strip_prefix("/kb:");
        if let Some(value) = value {
            let id = value.trim();
            if !id.is_empty() {
                out.push(KnowledgeBaseRef {
                    id: id.to_string(),
                    label: None,
                });
            }
        }
    }
}

fn extract_legacy_kb_refs(text: &str, out: &mut Vec<KnowledgeBaseRef>) {
    let mut rest = text;
    while let Some(kb_start) = rest.find("kb_id:") {
        let after = slice_from(rest, kb_start + "kb_id:".len()).trim_start();
        let after = after.strip_prefix('"').unwrap_or(after);
        let after = after.strip_prefix('`').unwrap_or(after);
        let id_len = after
            .find(|c: char| c == ')' || c == '"' || c == '`' || c == ',' || c.is_whitespace())
            .unwrap_or(after.len());
        let id = slice_to(after, id_len).trim();
        if !id.is_empty() {
            out.push(KnowledgeBaseRef {
                id: id.to_string(),
                label: None,
            });
        }
        rest = slice_from(after, id_len);
    }

    let mut phrase_rest = text;
    while let Some(start) = phrase_rest.find("focus the \"") {
        let value_start = start + "focus the \"".len();
        let value_rest = slice_from(phrase_rest, value_start);
        let Some(end) = value_rest.find("\" knowledge base") else {
            break;
        };
        let id = slice_to(value_rest, end).trim();
        if !id.is_empty() {
            out.push(KnowledgeBaseRef {
                id: id.to_string(),
                label: None,
            });
        }
        phrase_rest = slice_from(value_rest, end);
    }
}

fn slice_from(value: &str, start: usize) -> &str {
    value.get(start..).unwrap_or("")
}

fn slice_to(value: &str, end: usize) -> &str {
    value.get(..end).unwrap_or(value)
}

fn push_trimmed(out: &mut Vec<String>, value: &str) {
    let value = value.trim();
    if !value.is_empty() {
        out.push(value.to_string());
    }
}

fn dedup(values: &mut Vec<String>) {
    let mut seen = std::collections::HashSet::new();
    values.retain(|value| seen.insert(value.clone()));
}

fn dedup_kbs(values: &mut Vec<KnowledgeBaseRef>) {
    let mut seen = std::collections::HashSet::new();
    values.retain(|value| seen.insert(value.id.clone()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_machine_readable_refs() {
        let refs = extract_resource_refs(
            r#"<biorouter-ref type="skill" name="rna-qc"> <biorouter-ref type="extension" name="developer"> <biorouter-ref type="knowledge_base" id="soul">"#,
        );

        assert_eq!(refs.skills, vec!["rna-qc"]);
        assert_eq!(refs.extensions, vec!["developer"]);
        assert_eq!(
            refs.knowledge_bases,
            vec![KnowledgeBaseRef {
                id: "soul".to_string(),
                label: None
            }]
        );
    }

    #[test]
    fn extracts_legacy_inserted_phrases() {
        let refs = extract_resource_refs(
            r#"Use the "literature-review" skill for this request, Use the "pubmed" extension for this request, Using the Knowledge extension, focus knowledge base "Soul" (kb_id: soul) for this request"#,
        );

        assert_eq!(refs.skills, vec!["literature-review"]);
        assert_eq!(refs.extensions, vec!["pubmed"]);
        assert_eq!(refs.knowledge_bases[0].id, "soul");
    }

    #[test]
    fn extracts_inline_refs() {
        let refs = extract_resource_refs(
            "/skill:rna-qc /ext:developer /kb:soul /skill(rna-qc-v2) /ext(agentdrafter) /kb(project-notes)",
        );

        assert_eq!(refs.skills, vec!["rna-qc", "rna-qc-v2"]);
        assert_eq!(refs.extensions, vec!["developer", "agentdrafter"]);
        assert_eq!(refs.knowledge_bases[0].id, "soul");
        assert_eq!(refs.knowledge_bases[1].id, "project-notes");
    }

    // Alias coverage (`agentdrafter` -> `agent_drafter`, `autovisualizer` ->
    // `autovisualiser`) moved to
    // `extension_manager::tests::resolves_bundled_extension_spelling_aliases`,
    // which owns reference resolution now.

    /// The corpus every escaping test runs over: one entry per character class
    /// that has ever broken an escaping scheme, plus the two literal entity
    /// spellings that catch a wrong `&` ordering.
    const HOSTILE_NAMES: &[&str] = &[
        "rna-qc",
        "my skill",
        r#"say "hi""#,
        "tom & jerry",
        "a &amp; b",
        r#"a &quot; b"#,
        "&",
        "&&&",
        "<script>alert(1)</script>",
        "a < b > c",
        "commas, everywhere,",
        "line one\nline two",
        "carriage\r\nreturn",
        "naïve café — 生物路由器 🧬",
        r#"<biorouter-ref type="skill" name="nested">"#,
        "",
        "   ",
        "&#10;",
        "&#65;",
    ];

    /// The property the whole scheme rests on.
    #[test]
    fn escaping_round_trips_every_hostile_name() {
        for name in HOSTILE_NAMES {
            assert_eq!(
                decode_ref_value(&encode_ref_value(name)),
                *name,
                "round trip lost `{name}` (encoded as `{}`)",
                encode_ref_value(name)
            );
        }
    }

    /// The ampersand ordering, from both sides. A chained-replacement encoder
    /// that escapes `&` last produces `&amp;lt;` for `<`; a chained decoder
    /// that resolves `&amp;` first turns the encoding of the literal text
    /// `&quot;` into a bare `"`.
    #[test]
    fn the_ampersand_is_escaped_first_and_decoded_last() {
        assert_eq!(encode_ref_value("<"), "&lt;");
        assert_eq!(encode_ref_value("&"), "&amp;");
        assert_eq!(encode_ref_value("&lt;"), "&amp;lt;");
        assert_eq!(encode_ref_value(r#"&quot;"#), "&amp;quot;");

        assert_eq!(decode_ref_value("&amp;lt;"), "&lt;");
        assert_eq!(decode_ref_value("&amp;quot;"), r#"&quot;"#);
        assert_eq!(decode_ref_value("&amp;amp;"), "&amp;");
    }

    /// Position-independent: every occurrence is escaped, not just the first,
    /// and no character outside the table is touched.
    #[test]
    fn escaping_covers_the_whole_table_and_nothing_else() {
        assert_eq!(
            encode_ref_value("a&b\"c<d>e\nf\rg"),
            "a&amp;b&quot;c&lt;d&gt;e&#10;f&#13;g"
        );
        // `'`, tab and non-ASCII are deliberately left alone.
        assert_eq!(encode_ref_value("it's\ta — b"), "it's\ta — b");
    }

    /// An entity we do not emit must survive decoding verbatim rather than
    /// being dropped or guessed at.
    #[test]
    fn an_unknown_or_malformed_entity_is_left_alone() {
        for input in [
            "caf&eacute;",
            "&#65;",
            "&#x41;",
            "&apos;",
            "&nbsp;",
            "AT&T",
            "a & b",
            "&amp",
            "&",
            "&;",
            "&quot",
        ] {
            assert_eq!(decode_ref_value(input), input, "mangled `{input}`");
        }

        // ...and a known entity next to an unknown one still decodes.
        assert_eq!(decode_ref_value("&nbsp;&amp;&nbsp;"), "&nbsp;&&nbsp;");
    }

    #[test]
    fn tag_builders_escape_the_values_they_embed() {
        assert_eq!(
            ref_tag(RefKind::Skill, r#"say "hi" & bye"#),
            r#"<biorouter-ref type="skill" name="say &quot;hi&quot; &amp; bye">"#
        );
        assert_eq!(
            ref_tag(RefKind::Extension, "Chat Recall"),
            r#"<biorouter-ref type="extension" name="Chat Recall">"#
        );
        assert_eq!(
            labelled_ref_tag(RefKind::KnowledgeBase, "soul", "Soul & Body"),
            r#"<biorouter-ref type="knowledge_base" id="soul" label="Soul &amp; Body">"#
        );
    }
}
