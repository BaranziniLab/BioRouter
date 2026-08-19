//! Reading every link grammar a page body can carry (DR-2), through one entry
//! point.
//!
//! There are **four**, not three, and the fourth is the reason this module
//! exists rather than a fourth copy of the `\[\[([^\]]+)\]\]` regex:
//!
//! | Form | Source | Meaning |
//! | --- | --- | --- |
//! | `[label](/path/to/x.md)` | OKF v0.2 §6.1 | untyped directed edge |
//! | `[[wiki-link]]` / `[[target\|alias]]` | BioRouter today | untyped directed edge |
//! | `[[predicate:: Object \| k=v; k=v]]` | BioOKF v0.5 §4.1 | **typed, attributed** edge |
//! | `edges:` frontmatter | BioOKF v0.5 §6 | typed, attributed edge (not a body form) |
//!
//! DR-2's table listed only the first two body forms. Feed BioOKF's inline edge
//! sugar to the legacy reader and it produces a link to a node literally named
//! `treats:: COVID-19 | knowledge_level=knowledge_assertion; …` — a permanently
//! dangling node, *and* a silently lost typed edge carrying a full provenance
//! triplet. The two `[[…]]` forms are told apart by one rule, stated once here:
//! **the segment before the first `|` contains `::`**. Sugar puts the predicate
//! there and an ordinary page title never contains `::`.
//!
//! ## What this module does not do
//!
//! It does not **resolve** anything. `target` comes back exactly as written —
//! no `.md` trimmed, no path prefix stripped, no case folded — because
//! resolution needs the bundle (DR-3's identity ladder) and belongs to the graph
//! deriver in Stage 2. A reader that half-resolves is the harder thing to debug:
//! the caller cannot tell a link that was never written from one that was
//! rewritten on the way through.
//!
//! It also does not skip fenced code blocks. A page that *documents* this syntax
//! yields links from its own examples. That is the behaviour the tree already
//! has, it is visible rather than silent, and changing it is a Stage 2 decision
//! about the deriver, not a format decision.

use once_cell::sync::Lazy;
use regex::Regex;

/// Which grammar a link was written in. Recorded rather than erased because the
/// forms are not interchangeable: only the sugar form carries a predicate, and
/// BioOKF §4 says of the other two that "Only `edges:` entries are part of the
/// graph" — so a consumer in BioOKF mode needs to know which is which before it
/// can decide precedence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkForm {
    /// OKF §6.1 markdown link.
    OkfMarkdown,
    /// `[[target]]` or `[[target|alias]]`.
    LegacyWiki,
    /// BioOKF §4.1 `[[predicate:: Object | attrs]]`.
    BioOkfEdgeSugar,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkRef {
    pub form: LinkForm,
    /// The link destination as written: a path/URI for [`LinkForm::OkfMarkdown`],
    /// a page title or logical path for [`LinkForm::LegacyWiki`], and the target
    /// node's `identifier` for [`LinkForm::BioOkfEdgeSugar`].
    pub target: String,
    /// The markdown link text, or the wiki-link alias after `|`.
    pub label: Option<String>,
    /// The `#fragment` split off a markdown destination, without the `#`.
    pub fragment: Option<String>,
    /// Sugar only: the BioOKF predicate. Always `None` for the other forms —
    /// which is exactly what makes "this edge has no type" answerable.
    pub predicate: Option<String>,
    /// Sugar only: the `k=v; k=v` attribute list, in written order. Keys are not
    /// validated here; the required triplet (`knowledge_level`, `agent_type`,
    /// `primary_source`) is a BioOKF rule and is checked in Stage 1.
    pub attributes: Vec<(String, String)>,
}

impl LinkRef {
    /// True when the target could name something inside the bundle.
    ///
    /// A markdown body legitimately links to dashboards and papers on the open
    /// web (OKF §4.4's own example does). Turning those into graph edges gives
    /// every page an edge to `https://example.com`, which is the fastest way to
    /// make a knowledge graph useless. §6.2 lists the three accepted spellings
    /// of a path-valued field; an absolute URL is the one that is not local.
    pub fn is_bundle_link(&self) -> bool {
        !self.target.is_empty() && !ABSOLUTE_URI.is_match(&self.target)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FootnoteKind {
    /// `[^id]` in prose: the per-claim attribution (§5.1).
    Reference,
    /// `[^id]: …` at the start of a line: the footnote's own text.
    Definition,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FootnoteRef {
    pub id: String,
    pub kind: FootnoteKind,
}

// `[label](dest)`. The label may be empty; the destination stops at the first
// `)`, which rules out nested parentheses in a URL — the same limit every
// lightweight markdown link scanner has, and preferable to a balanced-paren
// parser that could run away on malformed input written by a model.
static MARKDOWN_LINK: Lazy<Regex> = Lazy::new(|| Regex::new(r"\[([^\]]*)\]\(([^)]*)\)").unwrap());

// `[^id]`. Excludes whitespace so a stray `[^ ]` in prose is not a footnote.
static FOOTNOTE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\[\^([^\]\s]+)\]").unwrap());

// `[[payload]]`. `[^\]]` spans newlines on purpose: BioOKF §4.1's own example of
// inline edge sugar is wrapped across two lines.
static WIKI_LINK: Lazy<Regex> = Lazy::new(|| Regex::new(r"\[\[([^\]]*)\]\]").unwrap());

// A scheme per RFC 3986 (`https:`, `mailto:`), used only to tell an external URI
// from a bundle path.
static ABSOLUTE_URI: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[A-Za-z][A-Za-z0-9+.\-]*:").unwrap());

/// Every link in the body, in document order, across all three body grammars.
///
/// One entry point rather than three readers, because the three copies of the
/// wiki regex already in the tree are what let the sugar form go unnoticed: each
/// copy was individually correct for the form its author had in mind.
pub fn extract_links(body: &str) -> Vec<LinkRef> {
    let mut found: Vec<(usize, LinkRef)> = Vec::new();
    for cap in WIKI_LINK.captures_iter(body) {
        let whole = cap.get(0).expect("group 0 always matches");
        let payload = cap.get(1).map_or("", |m| m.as_str());
        found.push((whole.start(), parse_wiki_payload(payload)));
    }
    for cap in MARKDOWN_LINK.captures_iter(body) {
        let whole = cap.get(0).expect("group 0 always matches");
        let label = cap.get(1).map_or("", |m| m.as_str());
        let dest = cap.get(2).map_or("", |m| m.as_str());
        found.push((whole.start(), parse_markdown_link(label, dest)));
    }
    found.sort_by_key(|(start, _)| *start);
    found.into_iter().map(|(_, link)| link).collect()
}

/// Every `[^id]` in the body, marked as a reference or a definition.
///
/// The two are separated because they answer different questions and §5.1 only
/// makes one of them a join key: "The footnote label is the join key into
/// `sources`; consumers resolve attribution through the matching entry, not by
/// parsing the footnote prose." A *reference* with no matching `sources[].id` is
/// an unattributed claim; a *definition* with none is merely a stray line.
pub fn extract_footnote_refs(body: &str) -> Vec<FootnoteRef> {
    let mut out = Vec::new();
    for line in body.lines() {
        let indent = line.len() - line.trim_start().len();
        for m in FOOTNOTE.captures_iter(line) {
            let whole = m.get(0).expect("group 0 always matches");
            let is_definition = whole.start() == indent
                && line
                    .get(whole.end()..)
                    .is_some_and(|rest| rest.starts_with(':'));
            out.push(FootnoteRef {
                id: m.get(1).map_or("", |g| g.as_str()).to_string(),
                kind: if is_definition {
                    FootnoteKind::Definition
                } else {
                    FootnoteKind::Reference
                },
            });
        }
    }
    out
}

fn parse_markdown_link(label: &str, dest: &str) -> LinkRef {
    // `[x](<...>)` angle-bracket destinations, and `[x](/a.md "Title")` titles.
    let dest = dest.trim();
    let dest = dest.trim_start_matches('<').trim_end_matches('>');
    let dest = dest.split_whitespace().next().unwrap_or("");
    let (target, fragment) = match dest.split_once('#') {
        Some((t, f)) => (t, Some(f.to_string())),
        None => (dest, None),
    };
    LinkRef {
        form: LinkForm::OkfMarkdown,
        target: target.to_string(),
        label: non_empty(label),
        fragment,
        predicate: None,
        attributes: Vec::new(),
    }
}

/// The one place the two `[[…]]` grammars are told apart. See the module header
/// for the rule and the bug it prevents.
fn parse_wiki_payload(payload: &str) -> LinkRef {
    let collapsed = collapse_whitespace(payload);
    let (head, tail) = match collapsed.split_once('|') {
        Some((h, t)) => (h.trim().to_string(), Some(t.trim().to_string())),
        None => (collapsed.trim().to_string(), None),
    };
    match head.split_once("::") {
        Some((predicate, object)) => LinkRef {
            form: LinkForm::BioOkfEdgeSugar,
            target: object.trim().to_string(),
            label: None,
            fragment: None,
            predicate: non_empty(predicate.trim()),
            attributes: parse_attributes(tail.as_deref().unwrap_or("")),
        },
        None => LinkRef {
            form: LinkForm::LegacyWiki,
            target: head,
            label: tail.and_then(|t| non_empty(&t)),
            fragment: None,
            predicate: None,
            attributes: Vec::new(),
        },
    }
}

/// `k=v; k=v`. A fragment with no `=` is kept with an empty value rather than
/// dropped: a malformed attribute is something lint should be able to name, and
/// a reader that discards it leaves lint nothing to name it with.
fn parse_attributes(attrs: &str) -> Vec<(String, String)> {
    attrs
        .split(';')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| match part.split_once('=') {
            Some((k, v)) => (k.trim().to_string(), v.trim().to_string()),
            None => (part.to_string(), String::new()),
        })
        .collect()
}

/// Fold the newlines out of a payload wrapped across lines (BioOKF §4.1 wraps
/// its own example), so `agent_type=manual_agent` written on the second line is
/// one attribute and not two.
fn collapse_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn non_empty(s: &str) -> Option<String> {
    (!s.trim().is_empty()).then(|| s.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::okf::fixtures;

    fn forms(body: &str) -> Vec<LinkForm> {
        extract_links(body).into_iter().map(|l| l.form).collect()
    }

    #[test]
    fn reads_an_okf_markdown_link() {
        let links = extract_links("See the [customers table](/tables/customers.md) for the key.");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].form, LinkForm::OkfMarkdown);
        assert_eq!(links[0].target, "/tables/customers.md");
        assert_eq!(links[0].label.as_deref(), Some("customers table"));
        assert!(links[0].is_bundle_link());
    }

    #[test]
    fn splits_a_markdown_fragment_off_the_target() {
        let links = extract_links("[joins](/tables/orders.md#joins)");
        assert_eq!(links[0].target, "/tables/orders.md");
        assert_eq!(links[0].fragment.as_deref(), Some("joins"));
    }

    #[test]
    fn a_markdown_link_title_is_not_part_of_the_target() {
        let links = extract_links("[x](/a/b.md \"Some title\")");
        assert_eq!(links[0].target, "/a/b.md");
    }

    #[test]
    fn relative_markdown_links_are_bundle_links_and_absolute_urls_are_not() {
        // §6.2's three spellings, and the distinction that keeps every page from
        // gaining an edge to example.com.
        let links = extract_links(
            "[a](/tables/x.md) [b](./other.md) [c](../up.md) \
             [d](https://example.com/dash) [e](mailto:x@example.com)",
        );
        let bundle: Vec<_> = links.iter().map(LinkRef::is_bundle_link).collect();
        assert_eq!(bundle, vec![true, true, true, false, false]);
    }

    #[test]
    fn reads_a_plain_legacy_wiki_link() {
        let links = extract_links("Links to [[zone-2 base]].");
        assert_eq!(links[0].form, LinkForm::LegacyWiki);
        assert_eq!(links[0].target, "zone-2 base");
        assert_eq!(links[0].label, None);
        assert_eq!(links[0].predicate, None);
    }

    #[test]
    fn reads_a_piped_legacy_wiki_link_without_resolving_it() {
        // Verbatim: the `.md` and the path prefix are the deriver's business.
        let links = extract_links("[[knowledge/entities/wanjun-gu.md|Wanjun Gu]]");
        assert_eq!(links[0].form, LinkForm::LegacyWiki);
        assert_eq!(links[0].target, "knowledge/entities/wanjun-gu.md");
        assert_eq!(links[0].label.as_deref(), Some("Wanjun Gu"));
    }

    #[test]
    fn reads_biookf_inline_edge_sugar_as_a_typed_edge() {
        // The DR-2 gap. Read by the legacy reader this becomes a dangling node
        // named after the whole payload, and the provenance triplet is lost.
        let links = extract_links(
            "[[treats:: COVID-19 | knowledge_level=knowledge_assertion; \
             agent_type=manual_agent; primary_source=RECOVERY trial]]",
        );
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].form, LinkForm::BioOkfEdgeSugar);
        assert_eq!(links[0].predicate.as_deref(), Some("treats"));
        assert_eq!(links[0].target, "COVID-19");
        assert_eq!(
            links[0].attributes,
            vec![
                ("knowledge_level".to_string(), "knowledge_assertion".into()),
                ("agent_type".to_string(), "manual_agent".into()),
                ("primary_source".to_string(), "RECOVERY trial".into()),
            ]
        );
    }

    #[test]
    fn edge_sugar_wrapped_across_lines_is_still_one_edge() {
        // BioOKF §4.1 prints its own example wrapped; a reader that stops at the
        // newline reads `agent_type=manual_agent` as part of the previous value.
        let links = extract_links(
            "[[treats:: COVID-19 | knowledge_level=knowledge_assertion;\n  agent_type=manual_agent]]",
        );
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "COVID-19");
        assert_eq!(links[0].attributes.len(), 2);
        assert_eq!(links[0].attributes[1].0, "agent_type");
    }

    #[test]
    fn edge_sugar_without_attributes_is_still_sugar() {
        let links = extract_links("[[binds:: IL6 receptor]]");
        assert_eq!(links[0].form, LinkForm::BioOkfEdgeSugar);
        assert_eq!(links[0].predicate.as_deref(), Some("binds"));
        assert_eq!(links[0].target, "IL6 receptor");
        assert!(links[0].attributes.is_empty());
    }

    #[test]
    fn the_discriminator_is_a_double_colon_before_the_first_pipe() {
        // Stated as a rule in the module header; pinned here so a future
        // "simplification" to `payload.contains(\"::\")` fails loudly. A page
        // whose *alias* contains `::` is still a legacy link.
        assert_eq!(
            forms("[[some page|see IL6:: notes]]"),
            vec![LinkForm::LegacyWiki]
        );
        assert_eq!(
            forms("[[binds:: IL6|ignored]]"),
            vec![LinkForm::BioOkfEdgeSugar]
        );
    }

    #[test]
    fn a_malformed_attribute_is_kept_with_an_empty_value_not_dropped() {
        let links = extract_links("[[treats:: X | knowledge_level; agent_type=manual_agent]]");
        assert_eq!(
            links[0].attributes[0],
            ("knowledge_level".into(), String::new())
        );
        assert_eq!(links[0].attributes.len(), 2);
    }

    #[test]
    fn all_three_forms_come_back_in_document_order() {
        let body = "a [[wiki]] b [md](/x.md) c [[p:: O]] d";
        assert_eq!(
            forms(body),
            vec![
                LinkForm::LegacyWiki,
                LinkForm::OkfMarkdown,
                LinkForm::BioOkfEdgeSugar
            ]
        );
    }

    #[test]
    fn a_wiki_link_is_not_also_read_as_a_markdown_link() {
        assert_eq!(extract_links("[[a]]").len(), 1);
    }

    #[test]
    fn footnote_references_and_definitions_are_distinguished() {
        let notes = extract_footnote_refs(
            "The table is sharded daily.[^ga4-schema]\n\n[^ga4-schema]: GA4 Export schema\n",
        );
        assert_eq!(notes.len(), 2);
        assert_eq!(notes[0].id, "ga4-schema");
        assert_eq!(notes[0].kind, FootnoteKind::Reference);
        assert_eq!(notes[1].kind, FootnoteKind::Definition);
    }

    #[test]
    fn a_definition_must_start_its_line_and_be_followed_by_a_colon() {
        // `[^x]` mid-sentence is a claim attribution even when a colon follows
        // later in the line.
        let notes = extract_footnote_refs("see [^x] : not a definition\n");
        assert_eq!(notes[0].kind, FootnoteKind::Reference);
        // Indented definitions are still definitions.
        let notes = extract_footnote_refs("  [^x]: text\n");
        assert_eq!(notes[0].kind, FootnoteKind::Definition);
    }

    #[test]
    fn footnotes_are_not_mistaken_for_links() {
        assert!(extract_links("A claim.[^pmid-32504360]").is_empty());
    }

    #[test]
    fn the_full_fixture_body_yields_both_a_link_and_a_footnote() {
        let page = crate::knowledge::okf::model::Page::parse(fixtures::FULL_V0_2).unwrap();
        let links = extract_links(&page.body);
        assert!(links.iter().any(|l| l.target == "/tables/customers.md"));
        let notes = extract_footnote_refs(&page.body);
        assert!(notes
            .iter()
            .any(|n| n.id == "ga4-schema" && n.kind == FootnoteKind::Reference));
        assert!(notes
            .iter()
            .any(|n| n.id == "ga4-schema" && n.kind == FootnoteKind::Definition));
    }

    #[test]
    fn the_inline_sugar_fixture_carries_a_full_provenance_triplet() {
        let page = crate::knowledge::okf::model::Page::parse(fixtures::INLINE_SUGAR).unwrap();
        let sugar: Vec<_> = extract_links(&page.body)
            .into_iter()
            .filter(|l| l.form == LinkForm::BioOkfEdgeSugar)
            .collect();
        assert_eq!(sugar.len(), 2);
        let keys: Vec<_> = sugar[0]
            .attributes
            .iter()
            .map(|(k, _)| k.as_str())
            .collect();
        assert!(keys.contains(&"knowledge_level"));
        assert!(keys.contains(&"agent_type"));
        assert!(keys.contains(&"primary_source"));
    }

    #[test]
    fn an_empty_body_yields_nothing_from_either_reader() {
        assert!(extract_links("").is_empty());
        assert!(extract_footnote_refs("").is_empty());
    }
}
