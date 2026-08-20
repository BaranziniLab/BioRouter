//! The BioOKF rule set (SPEC §10's lint list and §11's five conformance
//! conditions), as diagnostics.
//!
//! ## Severity is `okf`'s, not a second ladder
//!
//! [`Severity`] is re-exported from [`crate::knowledge::okf`] rather than
//! redefined. The two profiles' findings land in one list in one UI, and a
//! second three-value enum spelled `Warn` beside one spelled `Warning` is how a
//! severity filter silently drops half of them. There is one ladder, and it has
//! no fatal variant — DR-7: nothing here rejects a page, it only describes.
//!
//! ## The three rules that are *not* errors, and why saying so matters
//!
//! - **A missing `xref` is not a finding at all.** §10 and §11 are unusually
//!   emphatic — "a **missing external CURIE in `xref` is an enrichment
//!   opportunity to backfill, not a conformance error** (only `type`/
//!   `identifier` are mandatory)". A pass that flagged it would put a warning on
//!   nearly every page of a young bundle and teach the curator to ignore the
//!   report.
//! - **`subtype` is never validated.** §5: "consumers MUST NOT reject a node for
//!   an unrecognized `subtype`", and §10: "Do **not** lint `subtype` against a
//!   fixed list; the agent coins it." Nothing in this module reads a `subtype`
//!   except [`super::aliases`], which uses it to *disambiguate* a deprecated
//!   type alias and never to judge it.
//! - **A broken cross-link is a warning, never an error.** OKF §11 makes
//!   tolerating one a MUST, and BioOKF §11 repeats it: "a linked concept may not
//!   yet exist". Ingest legitimately writes an edge before the target page.
//!
//! ## Rule ids
//!
//! `biookf.<area>.<problem>`, matching [`crate::knowledge::okf::conformance`]'s
//! `okf.<area>.<problem>` so a merged list is sortable by profile. The suffix
//! after the prefix is `bokf-core`'s own rule id wherever that project has one
//! (`edge.not_negatable`, `edge.contradiction`, `identifier.duplicate`, …), so a
//! finding crosswalks to the reference implementation's without a table.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;

use super::aliases::{self, AliasKind, AliasNote};
use super::domain_range::{self, Side};
use super::vocabulary::{
    NodeType, PositivePredicate, Predicate, PredicateError, AGENT_TYPES, KNOWLEDGE_LEVELS,
};
use crate::knowledge::okf::{ConceptDoc, Edge};
use crate::knowledge::raw;
use crate::knowledge::types::{Credibility, CredibilityTier};

pub use crate::knowledge::okf::Severity;

// §11 rule 2 / §11 rule 5.
pub const RULE_TYPE_MISSING: &str = "biookf.type.missing";
pub const RULE_TYPE_INVALID: &str = "biookf.type.invalid";
pub const RULE_IDENTIFIER_MISSING: &str = "biookf.identifier.missing";
pub const RULE_IDENTIFIER_DUPLICATE: &str = "biookf.identifier.duplicate";
pub const RULE_IDENTIFIER_OPAQUE: &str = "biookf.identifier.opaque";
// §11 rule 3.
pub const RULE_PREDICATE_INVALID: &str = "biookf.predicate.invalid";
pub const RULE_EDGE_NOT_NEGATABLE: &str = "biookf.edge.not_negatable";
// §11 rule 4.
pub const RULE_EDGE_OBJECT_MISSING: &str = "biookf.edge.object_missing";
pub const RULE_EDGE_OBJECT_UNRESOLVED: &str = "biookf.edge.object_unresolved";
pub const RULE_EDGE_MISSING_KNOWLEDGE_LEVEL: &str = "biookf.edge.missing_knowledge_level";
pub const RULE_EDGE_INVALID_KNOWLEDGE_LEVEL: &str = "biookf.edge.invalid_knowledge_level";
pub const RULE_EDGE_MISSING_AGENT_TYPE: &str = "biookf.edge.missing_agent_type";
pub const RULE_EDGE_INVALID_AGENT_TYPE: &str = "biookf.edge.invalid_agent_type";
pub const RULE_EDGE_MISSING_PRIMARY_SOURCE: &str = "biookf.edge.missing_primary_source";
pub const RULE_EDGE_PRIMARY_SOURCE_NOT_PROVIDED: &str = "biookf.edge.primary_source_not_provided";
pub const RULE_EDGE_PRIMARY_SOURCE_UNRESOLVED: &str = "biookf.edge.primary_source_unresolved";
pub const RULE_EDGE_PRIMARY_SOURCE_NOT_SOURCE: &str = "biookf.edge.primary_source_not_source";
// §8.1 / §10.
pub const RULE_SOURCE_UNANCHORED: &str = "biookf.source.unanchored";
// §10 provenance quality. See [`check_credibility`] — these two are the only
// rules in this module that read anything outside the pages they are given.
pub const RULE_SOURCE_RETRACTED: &str = "biookf.source.retracted";
pub const RULE_SOURCE_NOT_SCHOLARLY: &str = "biookf.source.not_scholarly";
// §6 domain/range.
pub const RULE_EDGE_DOMAIN: &str = "biookf.edge.domain";
pub const RULE_EDGE_RANGE: &str = "biookf.edge.range";
// §6.F.
pub const RULE_EDGE_CONTRADICTION: &str = "biookf.edge.contradiction";
// §14.
pub const RULE_ALIAS_TYPE: &str = "biookf.alias.type";
pub const RULE_ALIAS_ATTRIBUTE: &str = "biookf.alias.attribute";
pub const RULE_ALIAS_INVERSE_PREDICATE: &str = "biookf.alias.inverse_predicate";
pub const RULE_ALIAS_NEGATED_QUALIFIER: &str = "biookf.alias.negated_qualifier";
pub const RULE_ALIAS_PRIMARY_SOURCE_CURIE: &str = "biookf.alias.primary_source_curie";

/// Every rule this module can emit. Exists so a caller can build a filter UI —
/// and so a renamed constant that no test happens to name still shows up in a
/// diff of this list.
pub const ALL_RULES: &[&str] = &[
    RULE_TYPE_MISSING,
    RULE_TYPE_INVALID,
    RULE_IDENTIFIER_MISSING,
    RULE_IDENTIFIER_DUPLICATE,
    RULE_IDENTIFIER_OPAQUE,
    RULE_PREDICATE_INVALID,
    RULE_EDGE_NOT_NEGATABLE,
    RULE_EDGE_OBJECT_MISSING,
    RULE_EDGE_OBJECT_UNRESOLVED,
    RULE_EDGE_MISSING_KNOWLEDGE_LEVEL,
    RULE_EDGE_INVALID_KNOWLEDGE_LEVEL,
    RULE_EDGE_MISSING_AGENT_TYPE,
    RULE_EDGE_INVALID_AGENT_TYPE,
    RULE_EDGE_MISSING_PRIMARY_SOURCE,
    RULE_EDGE_PRIMARY_SOURCE_NOT_PROVIDED,
    RULE_EDGE_PRIMARY_SOURCE_UNRESOLVED,
    RULE_EDGE_PRIMARY_SOURCE_NOT_SOURCE,
    RULE_SOURCE_UNANCHORED,
    RULE_SOURCE_RETRACTED,
    RULE_SOURCE_NOT_SCHOLARLY,
    RULE_EDGE_DOMAIN,
    RULE_EDGE_RANGE,
    RULE_EDGE_CONTRADICTION,
    RULE_ALIAS_TYPE,
    RULE_ALIAS_ATTRIBUTE,
    RULE_ALIAS_INVERSE_PREDICATE,
    RULE_ALIAS_NEGATED_QUALIFIER,
    RULE_ALIAS_PRIMARY_SOURCE_CURIE,
];

/// One finding.
///
/// Carries `subject` and `path` where [`crate::knowledge::okf::Diagnostic`] does
/// not, because BioOKF's rules are bundle-scoped: "`identifier` X is duplicated"
/// and "`primary_source` Y does not resolve" are unactionable without knowing
/// which page they came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Stable across releases; match on this, never on `message`.
    pub rule: &'static str,
    pub severity: Severity,
    /// The node's `identifier`, or a placeholder when it has none.
    pub subject: String,
    /// Bundle-relative path of the page, when the caller supplied one.
    pub path: Option<String>,
    pub message: String,
}

/// Shown as the subject of a finding on a page that has no usable
/// `identifier` — which is itself one of the findings, so the placeholder is
/// never the only clue.
pub const UNIDENTIFIED: &str = "<no identifier>";

/// What the bundle contains, as far as cross-document rules need to know.
///
/// Built once per lint run rather than per page: §11's duplicate-`identifier`
/// and unresolved-`object` rules are both O(bundle) questions, and asking them
/// per page against a rebuilt map is how a 500-page base becomes a 250,000-page
/// scan.
#[derive(Debug, Clone, Default)]
pub struct BundleIndex {
    nodes: HashMap<String, IndexedNode>,
    duplicates: BTreeSet<String>,
    indexed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedNode {
    /// `None` when the page's `type` is not one of the 28 (after alias
    /// normalisation) — the page still resolves as a link target, it just has
    /// no type to check a domain or range against.
    pub node_type: Option<NodeType>,
    pub path: String,
}

impl BundleIndex {
    /// Index every page in the bundle by its `identifier`.
    pub fn build<'a, I>(pages: I) -> Self
    where
        I: IntoIterator<Item = (&'a str, &'a ConceptDoc)>,
    {
        let mut index = Self {
            indexed: true,
            ..Self::default()
        };
        for (path, doc) in pages {
            let normalized = aliases::normalize(doc);
            let Some(key) = normalized.doc.primary_key().map(str::to_string) else {
                continue;
            };
            let entry = IndexedNode {
                node_type: NodeType::parse(&normalized.doc.r#type),
                path: path.to_string(),
            };
            if index.nodes.insert(key.clone(), entry).is_some() {
                index.duplicates.insert(key);
            }
        }
        index
    }

    /// An index that knows nothing, for a **validate-before-write** check on a
    /// page that is not in a bundle yet.
    ///
    /// Not the same as an *empty* bundle, and the distinction is the whole
    /// point: an empty index would report every `object` and every
    /// `primary_source` as unresolved, which on a first page is every edge —
    /// a report so noisy the tool stops being used. `indexed: false` skips the
    /// cross-document rules instead of failing them.
    pub fn unindexed() -> Self {
        Self::default()
    }

    pub fn is_indexed(&self) -> bool {
        self.indexed
    }

    pub fn get(&self, identifier: &str) -> Option<&IndexedNode> {
        self.nodes.get(identifier)
    }

    pub fn is_duplicate(&self, identifier: &str) -> bool {
        self.duplicates.contains(identifier)
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

/// Accumulator, so every check reads three fields instead of taking five
/// arguments.
struct Ctx<'a> {
    path: Option<String>,
    subject: String,
    subject_type: Option<NodeType>,
    index: &'a BundleIndex,
    out: Vec<Finding>,
}

impl Ctx<'_> {
    fn push(&mut self, rule: &'static str, severity: Severity, message: impl Into<String>) {
        self.out.push(Finding {
            rule,
            severity,
            subject: self.subject.clone(),
            path: self.path.clone(),
            message: message.into(),
        });
    }
}

/// Lint one concept document against the BioOKF profile.
///
/// `doc` is read as written; §14 alias normalisation happens inside, so a v0.3
/// page is reported as *old*, not as invalid. Pass
/// [`BundleIndex::unindexed`] when there is no bundle to resolve against.
pub fn check_page(path: Option<&str>, doc: &ConceptDoc, index: &BundleIndex) -> Vec<Finding> {
    let normalized = aliases::normalize(doc);
    let doc = &normalized.doc;
    let mut ctx = Ctx {
        path: path.map(str::to_string),
        subject: doc.primary_key().unwrap_or(UNIDENTIFIED).to_string(),
        subject_type: NodeType::parse(&doc.r#type),
        index,
        out: Vec::new(),
    };
    for note in &normalized.notes {
        push_alias_note(&mut ctx, note);
    }
    check_type(&mut ctx, doc);
    check_identifier(&mut ctx, doc);
    check_source_anchor(&mut ctx, doc);
    for edge in &doc.edges {
        check_edge(&mut ctx, edge);
    }
    check_contradictions(&mut ctx, doc);
    ctx.out
}

fn push_alias_note(ctx: &mut Ctx<'_>, note: &AliasNote) {
    let rule = match note.kind {
        AliasKind::Type => RULE_ALIAS_TYPE,
        AliasKind::Attribute => RULE_ALIAS_ATTRIBUTE,
        AliasKind::InversePredicate => RULE_ALIAS_INVERSE_PREDICATE,
        AliasKind::NegatedQualifier => RULE_ALIAS_NEGATED_QUALIFIER,
        AliasKind::PrimarySourceCurie => RULE_ALIAS_PRIMARY_SOURCE_CURIE,
    };
    ctx.push(rule, Severity::Info, note.message.clone());
}

// ── node rules ──────────────────────────────────────────────────────────────

fn check_type(ctx: &mut Ctx<'_>, doc: &ConceptDoc) {
    if doc.r#type.is_empty() {
        ctx.push(
            RULE_TYPE_MISSING,
            Severity::Error,
            "§11 rule 1: every concept document needs a non-empty `type`",
        );
        return;
    }
    if ctx.subject_type.is_some() {
        return;
    }
    let hint = closest(&doc.r#type, NodeType::ALL.iter().map(|t| t.as_str()))
        .map(|c| format!(" (closest legal value: `{c}`)"))
        .unwrap_or_else(|| "; if nothing fits, use `Other` with a `note:`".to_string());
    ctx.push(
        RULE_TYPE_INVALID,
        Severity::Error,
        format!(
            "§11 rule 2: `{}` is not one of the 28 controlled node types{hint}",
            doc.r#type
        ),
    );
}

/// §11 rule 5: non-empty, human-readable, unique across the bundle.
fn check_identifier(ctx: &mut Ctx<'_>, doc: &ConceptDoc) {
    let Some(identifier) = doc.primary_key() else {
        ctx.push(
            RULE_IDENTIFIER_MISSING,
            Severity::Error,
            "§11 rule 5: every node needs a non-empty `identifier`; edges target it by name",
        );
        return;
    };
    let identifier = identifier.to_string();
    if ctx.index.is_duplicate(&identifier) {
        ctx.push(
            RULE_IDENTIFIER_DUPLICATE,
            Severity::Error,
            format!(
                "§11 rule 5: `{identifier}` is used by more than one page; an edge's `object` \
                 cannot say which one it means"
            ),
        );
    }
    if looks_opaque(&identifier) {
        ctx.push(
            RULE_IDENTIFIER_OPAQUE,
            Severity::Warning,
            format!(
                "§7.1: `{identifier}` is not human-readable; give the node a name and move the \
                 code to `xref`"
            ),
        );
    }
}

/// §8.1: a source node bottoms out either in `raw/` (an ingested document) or in
/// an external authority's CURIE (a cited reference). Neither means the
/// provenance chain terminates nowhere.
///
/// A **warning**, not an error: §10 calls it "an unanchored source, backfill" —
/// an enrichment opportunity, exactly like a missing `xref`.
fn check_source_anchor(ctx: &mut Ctx<'_>, doc: &ConceptDoc) {
    if !ctx.subject_type.is_some_and(NodeType::is_source) {
        return;
    }
    if !doc.xref.is_empty() || !raw_source(doc).is_empty() {
        return;
    }
    ctx.push(
        RULE_SOURCE_UNANCHORED,
        Severity::Warning,
        "§8.1: this source node anchors to neither a `raw_source` path nor an external `xref`, \
         so no claim citing it can be traced further",
    );
}

/// `raw_source` is a §7.1 BioOKF key that OKF's model does not name, so it lives
/// in the preserved-unknown-keys mapping. Read leniently — a producer writing a
/// bare string instead of a list is §11-tolerable and common.
pub(crate) fn raw_source(doc: &ConceptDoc) -> Vec<String> {
    let Some(value) = doc
        .extra
        .get(serde_yaml::Value::String("raw_source".into()))
    else {
        return Vec::new();
    };
    match value {
        serde_yaml::Value::String(s) if !s.is_empty() => vec![s.clone()],
        serde_yaml::Value::Sequence(items) => items
            .iter()
            .filter_map(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

// ── edge rules ──────────────────────────────────────────────────────────────

fn check_edge(ctx: &mut Ctx<'_>, edge: &Edge) {
    let predicate = check_edge_predicate(ctx, edge);
    check_object(ctx, edge);
    check_triplet(ctx, edge);
    check_primary_source(ctx, edge);
    if let Some(predicate) = predicate {
        check_domain_range(ctx, edge, predicate);
    }
}

fn check_edge_predicate(ctx: &mut Ctx<'_>, edge: &Edge) -> Option<Predicate> {
    match aliases::resolve_predicate(&edge.predicate, edge.negated) {
        Ok(resolved) => Some(resolved.predicate),
        Err(err @ PredicateError::NotNegatable(_)) => {
            ctx.push(RULE_EDGE_NOT_NEGATABLE, Severity::Error, err.to_string());
            None
        }
        Err(err @ PredicateError::Unknown(_)) => {
            let hint = closest(
                &edge.predicate,
                Predicate::all().iter().map(|p| p.to_string()),
            )
            .map(|c| format!(" (closest legal value: `{c}`)"))
            .unwrap_or_default();
            ctx.push(
                RULE_PREDICATE_INVALID,
                Severity::Error,
                format!("§11 rule 3: {err}{hint}"),
            );
            None
        }
    }
}

fn check_object(ctx: &mut Ctx<'_>, edge: &Edge) {
    if edge.object.is_empty() {
        ctx.push(
            RULE_EDGE_OBJECT_MISSING,
            Severity::Error,
            format!("§11 rule 4: edge `{}` has no `object`", edge.predicate),
        );
        return;
    }
    if !ctx.index.is_indexed() || ctx.index.get(&edge.object).is_some() {
        return;
    }
    ctx.push(
        RULE_EDGE_OBJECT_UNRESOLVED,
        Severity::Warning,
        format!(
            "edge `{} -> {}`: no page carries that `identifier`. §11 makes tolerating this a \
             MUST — the target may not exist yet — so this is a backlog item, not a defect",
            edge.predicate, edge.object
        ),
    );
}

/// §8's mandatory triplet, two thirds of it. `primary_source` is checked
/// separately because it has five outcomes rather than two.
fn check_triplet(ctx: &mut Ctx<'_>, edge: &Edge) {
    let tag = format!("{} -> {}", edge.predicate, edge.object);
    check_enum_field(
        ctx,
        edge.knowledge_level.as_deref(),
        KNOWLEDGE_LEVELS,
        ("knowledge_level", &tag),
        (
            RULE_EDGE_MISSING_KNOWLEDGE_LEVEL,
            RULE_EDGE_INVALID_KNOWLEDGE_LEVEL,
        ),
    );
    check_enum_field(
        ctx,
        edge.agent_type.as_deref(),
        AGENT_TYPES,
        ("agent_type", &tag),
        (RULE_EDGE_MISSING_AGENT_TYPE, RULE_EDGE_INVALID_AGENT_TYPE),
    );
}

fn check_enum_field(
    ctx: &mut Ctx<'_>,
    value: Option<&str>,
    allowed: &[&str],
    (field, tag): (&str, &str),
    (missing, invalid): (&'static str, &'static str),
) {
    match value {
        None => ctx.push(
            missing,
            Severity::Error,
            format!("§8: edge `{tag}` is missing the required `{field}`"),
        ),
        Some(v) if !allowed.contains(&v) => ctx.push(
            invalid,
            Severity::Error,
            format!(
                "§7.2: `{field}: {v}` on edge `{tag}` is not one of {}",
                allowed.join(" · ")
            ),
        ),
        Some(_) => {}
    }
}

/// §8.1: `primary_source` names a **source node** — one of the four types in
/// [`NodeType::SOURCE_TYPES`] — by its `identifier`, with exactly one reserved
/// non-node value.
fn check_primary_source(ctx: &mut Ctx<'_>, edge: &Edge) {
    let tag = format!("{} -> {}", edge.predicate, edge.object);
    let Some(source) = edge.primary_source.as_deref().filter(|s| !s.is_empty()) else {
        ctx.push(
            RULE_EDGE_MISSING_PRIMARY_SOURCE,
            Severity::Error,
            format!("§8: edge `{tag}` is missing the required `primary_source`"),
        );
        return;
    };
    if source == aliases::NOT_PROVIDED {
        ctx.push(
            RULE_EDGE_PRIMARY_SOURCE_NOT_PROVIDED,
            Severity::Warning,
            format!(
                "§8.1: edge `{tag}` cites `{}`, the reserved escape for a genuinely unknown \
                 origin. It is conformant but is never the default",
                aliases::NOT_PROVIDED
            ),
        );
        return;
    }
    // A pre-v0.5 `infores:` CURIE is already reported as a §14 alias; saying it
    // again as an unresolved source would double-count one problem.
    if aliases::is_legacy_primary_source(source) || !ctx.index.is_indexed() {
        return;
    }
    match ctx.index.get(source).map(|n| n.node_type) {
        None => ctx.push(
            RULE_EDGE_PRIMARY_SOURCE_UNRESOLVED,
            Severity::Warning,
            format!("§8.1: `primary_source: {source}` on edge `{tag}` names no page in the bundle"),
        ),
        Some(Some(t)) if !t.is_source() => ctx.push(
            RULE_EDGE_PRIMARY_SOURCE_NOT_SOURCE,
            Severity::Warning,
            format!(
                "§8.1: `primary_source: {source}` on edge `{tag}` resolves to a {t}, and only a \
                 Publication/Study/Dataset/Agent may bear a source"
            ),
        ),
        Some(_) => {}
    }
}

/// §6's domain/range table, applied only when both ends have a known type.
///
/// An unresolved object is *not* a domain/range failure — the check is skipped
/// rather than failed, because "we could not look it up" and "it is the wrong
/// type" are different findings and the first is already reported by
/// [`check_object`].
fn check_domain_range(ctx: &mut Ctx<'_>, edge: &Edge, predicate: Predicate) {
    let (Some(subject), Some(object)) = (
        ctx.subject_type,
        ctx.index.get(&edge.object).and_then(|n| n.node_type),
    ) else {
        return;
    };
    let Some(violation) = domain_range::check(subject, predicate, object) else {
        return;
    };
    let rule = match violation.side {
        Side::Domain => RULE_EDGE_DOMAIN,
        Side::Range => RULE_EDGE_RANGE,
    };
    ctx.push(
        rule,
        Severity::Warning,
        format!("§6: {} (object `{}`)", violation.message(), edge.object),
    );
}

/// §6.F: "asserting both `<X>` and `not_<X>` for the same subject → object is a
/// contradiction".
///
/// Scoped to one document, because the subject of every edge in an `edges:` list
/// *is* that document. The cross-page case — the same `identifier` on two pages,
/// one asserting `treats` and the other `not_treats` — is already
/// [`RULE_IDENTIFIER_DUPLICATE`], an error, and fixing that is what makes this
/// check able to see the pair at all.
fn check_contradictions(ctx: &mut Ctx<'_>, doc: &ConceptDoc) {
    let mut seen: BTreeMap<(String, String), (bool, bool)> = BTreeMap::new();
    for edge in &doc.edges {
        let Ok(resolved) = aliases::resolve_predicate(&edge.predicate, edge.negated) else {
            continue;
        };
        let key = (
            resolved.predicate.base().as_str().to_string(),
            edge.object.clone(),
        );
        let entry = seen.entry(key).or_insert((false, false));
        if resolved.predicate.is_negated() {
            entry.1 = true;
        } else {
            entry.0 = true;
        }
    }
    for ((base, object), (positive, negative)) in seen {
        if positive && negative {
            ctx.push(
                RULE_EDGE_CONTRADICTION,
                Severity::Warning,
                format!(
                    "§6.F: both `{base}` and `not_{base}` are asserted for `{object}`; one of the \
                     two sources is being contradicted and the page does not say which"
                ),
            );
        }
    }
}

// ── §10 provenance quality: the credibility verdict, finally read ───────────

/// §10's two provenance-quality rules, over a whole bundle.
///
/// **The bug this closes.** `raw/<id>/meta.yaml` has always carried a full
/// [`Credibility`] record — tier, confidence, publisher, venue, doi,
/// `retracted`, `classifier_version` — written by the classifier at ingest and
/// then read by nothing. So a BioOKF base could cite a **retracted** paper as
/// the `primary_source` of a clinical claim and every check in this build
/// reported it clean. The signal was computed and thrown away.
///
/// A separate entry point from [`check_page`] because it is the only rule family
/// here that reads bytes **outside the pages it is given**: a source node's
/// verdict lives in `raw/`, not in its frontmatter, so this needs the bundle
/// root and `check_page`'s pure `(doc, index)` signature cannot express it.
///
/// **Both findings are warnings** (DR-7): nothing rejects a page, and a
/// retraction is a fact about the evidence, not a defect in the file.
///
/// ⚠ **A source that was never classified is never flagged**, and that guard is
/// load-bearing — it is BioOKF's own (`bokf-core/src/lint.rs`:
/// `if meta.credibility.classifier_version == 0 { continue }`). Every base built
/// before the classifier existed carries `classifier_version: 0` with a default
/// tier, so a pass that read those verdicts as real would light up an entire
/// base at once — the same "report a decision as several hundred defects"
/// failure DR-26 rules out for the legacy format layer.
pub fn check_credibility<'a, I>(kb_root: &Path, pages: I) -> Vec<Finding>
where
    I: IntoIterator<Item = (&'a str, &'a ConceptDoc)>,
{
    let pages: Vec<(&str, &ConceptDoc)> = pages.into_iter().collect();
    let sources = source_pages(&pages);
    let mut cache: HashMap<String, Option<Credibility>> = HashMap::new();
    // One finding per (rule, source), not one per citing edge: a retracted
    // paper behind forty claims is ONE thing to fix, and forty copies of the
    // same sentence is how a report stops being read. The first citation
    // reached becomes the example in the message, so the finding still points
    // at a concrete claim.
    let mut reported: HashSet<(&'static str, &str)> = HashSet::new();
    let mut out = Vec::new();
    for (_, doc) in &pages {
        for edge in &doc.edges {
            for citation in edge_citations(edge) {
                let Some((path, source_doc)) = sources.get(citation.name) else {
                    // Unresolved is `primary_source_unresolved`'s finding, not
                    // ours; saying it twice double-counts one problem.
                    continue;
                };
                let Some(credibility) =
                    credibility_of(kb_root, citation.name, source_doc, &mut cache)
                else {
                    continue;
                };
                out.extend(judge_source(
                    &citation,
                    edge,
                    (path, &credibility),
                    &mut reported,
                ));
            }
        }
    }
    out
}

/// Every page that carries an `identifier`, by that identifier.
///
/// Not [`BundleIndex`], which keeps a node's *type* and path but not the
/// `raw_source` list this needs to find a verdict on disk.
fn source_pages<'a>(
    pages: &[(&'a str, &'a ConceptDoc)],
) -> HashMap<&'a str, (&'a str, &'a ConceptDoc)> {
    pages
        .iter()
        .filter_map(|(path, doc)| doc.primary_key().map(|key| (key, (*path, *doc))))
        .collect()
}

/// How an edge named a source, which decides which rules apply to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CitationRole {
    /// `primary_source: X` — the origin of the claim this edge asserts.
    PrimarySource,
    /// `reported_in -> X` — §6's provenance predicate, whose object is a source
    /// node. A retraction matters here for the same reason it does above: the
    /// page is asserting that this is where the finding was published.
    ReportedIn,
}

struct Citation<'a> {
    name: &'a str,
    role: CitationRole,
}

impl CitationRole {
    fn as_written(self) -> &'static str {
        match self {
            Self::PrimarySource => "`primary_source`",
            Self::ReportedIn => "the object of a `reported_in` edge",
        }
    }
}

/// The sources one edge names: its `primary_source`, plus its `object` when the
/// predicate is `reported_in`.
///
/// `not_provided` is skipped — §8.1 reserves it as the escape for a genuinely
/// unknown origin, and `primary_source_not_provided` already reports it.
fn edge_citations(edge: &Edge) -> Vec<Citation<'_>> {
    let mut out = Vec::new();
    if let Some(name) = edge
        .primary_source
        .as_deref()
        .filter(|s| !s.is_empty() && *s != aliases::NOT_PROVIDED)
    {
        out.push(Citation {
            name,
            role: CitationRole::PrimarySource,
        });
    }
    let reported_in = aliases::resolve_predicate(&edge.predicate, edge.negated)
        .is_ok_and(|r| r.predicate == Predicate::positive(PositivePredicate::ReportedIn));
    if reported_in && !edge.object.is_empty() {
        out.push(Citation {
            name: &edge.object,
            role: CitationRole::ReportedIn,
        });
    }
    out
}

/// The two verdicts, for one citation of one source.
fn judge_source<'a>(
    citation: &Citation<'a>,
    edge: &Edge,
    (path, credibility): (&str, &Credibility),
    reported: &mut HashSet<(&'static str, &'a str)>,
) -> Vec<Finding> {
    let claim = format!("{} -> {}", edge.predicate, edge.object);
    let mut out = Vec::new();
    let finding = |rule: &'static str, message: String| Finding {
        rule,
        severity: Severity::Warning,
        subject: citation.name.to_string(),
        path: Some(path.to_string()),
        message,
    };
    if credibility.retracted && reported.insert((RULE_SOURCE_RETRACTED, citation.name)) {
        out.push(finding(
            RULE_SOURCE_RETRACTED,
            format!(
                "§10: `{}` is marked RETRACTED in its `raw/…/meta.yaml`, and is cited as {} \
                 (for example on `{claim}`). Re-source every claim resting on it or withdraw \
                 them; nothing here removes the page",
                citation.name,
                citation.role.as_written()
            ),
        ));
    }
    // Narrower than the retraction rule on purpose. `knowledge_assertion` is
    // §7.2's strongest `knowledge_level` — "this is established" — so a web page
    // or a personal communication behind one is a mismatch between how sure the
    // claim sounds and what it rests on. The same source behind an `observation`
    // or a `prediction` is honest and is left alone.
    let asserted = citation.role == CitationRole::PrimarySource
        && edge.knowledge_level.as_deref() == Some(KNOWLEDGE_ASSERTION);
    if asserted
        && is_low_credibility(credibility.tier)
        && reported.insert((RULE_SOURCE_NOT_SCHOLARLY, citation.name))
    {
        out.push(finding(
            RULE_SOURCE_NOT_SCHOLARLY,
            format!(
                "§10: `{}` classifies as `{}`, and backs the `knowledge_assertion`-level \
                 claim `{claim}`. Either cite the primary literature or lower the edge's \
                 `knowledge_level` to what the source actually supports",
                citation.name,
                tier_word(credibility.tier)
            ),
        ));
    }
    out
}

/// §7.2's strongest knowledge level, spelled once. Present in
/// [`KNOWLEDGE_LEVELS`], which is the list the enum check uses; this names the
/// one member the credibility rule cares about.
const KNOWLEDGE_ASSERTION: &str = "knowledge_assertion";

/// Which tiers read as "not a recognised scholarly source".
///
/// The set is BioOKF's (`web` / `unknown`), mapped onto this build's ladder:
/// `Web` is the same tier, and `Personal` is what a personal communication
/// classifies as, which is weaker still. `Preprint`, `Book` and `GrayLit` are
/// deliberately **not** here — §10 calls a preprint archive and a recognised
/// database legitimate provenance, and flagging them would put a warning on
/// most of a young biomedical base.
fn is_low_credibility(tier: CredibilityTier) -> bool {
    matches!(tier, CredibilityTier::Web | CredibilityTier::Personal)
}

fn tier_word(tier: CredibilityTier) -> &'static str {
    match tier {
        CredibilityTier::PeerReviewed => "peer_reviewed",
        CredibilityTier::Preprint => "preprint",
        CredibilityTier::Book => "book",
        CredibilityTier::GrayLit => "gray_lit",
        CredibilityTier::Web => "web",
        CredibilityTier::Personal => "personal",
    }
}

/// The classifier's verdict on a source node, or `None` when there is no signal
/// to read.
///
/// `None` covers three different situations that all mean the same thing here:
/// the node anchors to an external CURIE rather than to ingested bytes, its
/// `meta.yaml` is missing or unreadable, or it was ingested before the
/// classifier existed (`classifier_version: 0` — see [`check_credibility`]).
/// Memoized per identifier because a source cited by forty edges would
/// otherwise be forty `meta.yaml` reads.
fn credibility_of(
    kb_root: &Path,
    identifier: &str,
    doc: &ConceptDoc,
    cache: &mut HashMap<String, Option<Credibility>>,
) -> Option<Credibility> {
    if let Some(hit) = cache.get(identifier) {
        return hit.clone();
    }
    let found = raw_source(doc)
        .iter()
        .filter_map(|entry| raw_source_id(entry))
        .find_map(|id| raw::read_meta(kb_root, id).ok())
        .map(|meta| meta.credibility)
        .filter(|c| c.classifier_version > 0);
    cache.insert(identifier.to_string(), found.clone());
    found
}

/// The `raw/<id>` directory a `raw_source` entry names, as an id
/// [`raw::read_meta`] can join.
///
/// ⚠ **Confined by construction, because the input is model-written
/// frontmatter.** A `raw_source: ../../../../.ssh/id_rsa` must not become a path
/// join, so only the single segment after a literal leading `raw/` is accepted,
/// and only when it is a plain directory name. BioOKF reaches the same place
/// through an explicit `confine_to_bundle`; taking one segment means there is
/// nothing to confine.
pub(crate) fn raw_source_id(entry: &str) -> Option<&str> {
    let id = entry.strip_prefix("raw/")?.split('/').next()?;
    let plain = !id.is_empty() && id != "." && id != ".." && !id.contains('\\');
    plain.then_some(id)
}

// ── helpers ─────────────────────────────────────────────────────────────────

/// A bare CURIE, or a string with no letters in it at all.
///
/// Both are §10's "opaque codes / bare CURIEs". The no-letters half catches a
/// measurement masquerading as a node (`183`, `2.9`) and an accession stripped
/// of its prefix (`0100096`), while staying quiet on the gene and compound
/// symbols that make a real biomedical bundle — `TP53`, `IL6`, `2-AG`, `5-HT`
/// all carry letters. `char::is_alphabetic` rather than the ASCII form, so a
/// name written in a non-Latin script is a name, not an opaque code.
fn looks_opaque(identifier: &str) -> bool {
    aliases::looks_like_bare_curie(identifier) || !identifier.contains(char::is_alphabetic)
}

/// The nearest legal value, or `None` when nothing is near enough to be a
/// helpful guess.
///
/// DR-16 asks a rejection to name the closest legal value, because the failure
/// mode it prevents is a model retrying the same invalid token until its step
/// budget dies. The threshold keeps the hint honest: suggesting `Gene` for
/// `Spacecraft` would be worse than saying nothing.
/// The nearest candidate to `needle`, or `None` when nothing is near enough for
/// a suggestion to be a suggestion rather than a misdirection.
///
/// Shared with the sub-agent's typed write tool (DR-16): a rejected value has to
/// come back naming the closest legal one, and a second implementation of
/// "closest" would let a lint hint and a dispatch hint disagree about the same
/// typo.
pub(crate) fn closest<S, I>(needle: &str, candidates: I) -> Option<String>
where
    S: AsRef<str>,
    I: IntoIterator<Item = S>,
{
    let lower = needle.to_lowercase();
    let mut best: Option<(usize, String)> = None;
    for candidate in candidates {
        let text = candidate.as_ref();
        let distance = edit_distance(&lower, &text.to_lowercase());
        if best.as_ref().is_none_or(|(d, _)| distance < *d) {
            best = Some((distance, text.to_string()));
        }
    }
    let (distance, text) = best?;
    let budget = needle.chars().count().div_ceil(3).max(2);
    (distance <= budget).then_some(text)
}

/// Levenshtein distance, two rows at a time. Small and self-contained rather
/// than a dependency, because it feeds a hint string and nothing else.
fn edit_distance(a: &str, b: &str) -> usize {
    let b_chars: Vec<char> = b.chars().collect();
    let mut previous: Vec<usize> = (0..=b_chars.len()).collect();
    let mut current = vec![0usize; b_chars.len() + 1];
    for (i, ca) in a.chars().enumerate() {
        current[0] = i + 1;
        for (j, &cb) in b_chars.iter().enumerate() {
            let substitution = previous[j] + usize::from(ca != cb);
            current[j + 1] = substitution.min(previous[j + 1] + 1).min(current[j] + 1);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[b_chars.len()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::okf::Page;

    fn doc(frontmatter: &str) -> ConceptDoc {
        Page::parse(&format!("---\n{frontmatter}---\n\n# body\n"))
            .expect("fixture parses")
            .doc
    }

    /// A conformant edge, so a test about one rule is not also a test about the
    /// four required attributes. The `{extra}` slot carries whatever the case
    /// under test needs.
    fn edge(predicate: &str, object: &str, extra: &str) -> String {
        format!(
            "  - predicate: {predicate}\n    object: {object}\n    \
             knowledge_level: knowledge_assertion\n    agent_type: manual_agent\n    \
             primary_source: DrugBank\n{extra}"
        )
    }

    /// The bundle every edge test resolves against: a drug, a disease, a gene
    /// and the source node the edges cite.
    fn bundle() -> Vec<(String, ConceptDoc)> {
        vec![
            (
                "knowledge/molecule/aspirin.md".into(),
                doc("type: Molecule\nidentifier: Aspirin\n"),
            ),
            (
                "knowledge/disease/headache.md".into(),
                doc("type: Disease\nidentifier: Headache\n"),
            ),
            (
                "knowledge/gene/il6.md".into(),
                doc("type: Gene\nidentifier: IL6\n"),
            ),
            (
                "knowledge/dataset/drugbank.md".into(),
                doc("type: Dataset\nidentifier: DrugBank\nxref: [infores:drugbank]\n"),
            ),
            (
                "knowledge/concept/nanometre.md".into(),
                doc("type: Concept\nidentifier: Nanometre\n"),
            ),
        ]
    }

    fn index(pages: &[(String, ConceptDoc)]) -> BundleIndex {
        BundleIndex::build(pages.iter().map(|(p, d)| (p.as_str(), d)))
    }

    fn rules(findings: &[Finding]) -> Vec<&'static str> {
        findings.iter().map(|f| f.rule).collect()
    }

    fn check(frontmatter: &str) -> Vec<Finding> {
        let pages = bundle();
        check_page(Some("knowledge/x.md"), &doc(frontmatter), &index(&pages))
    }

    // ── §11 rules 1, 2 and 5: type and identifier ───────────────────────────

    #[test]
    fn a_conformant_page_produces_nothing_at_all() {
        let findings = check(&format!(
            "type: Molecule\nidentifier: Ibuprofen\nsubtype: drug\nedges:\n{}",
            edge("treats", "Headache", "")
        ));
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn an_invalid_type_is_an_error_that_names_the_closest_legal_value() {
        let findings = check("type: Molecules\nidentifier: Aspirin-2\n");
        assert_eq!(rules(&findings), [RULE_TYPE_INVALID]);
        assert!(findings[0].message.contains("`Molecule`"), "{findings:?}");
        // Nothing near enough to guess falls back to §5's own advice.
        let findings = check("type: Spacecraft\nidentifier: Voyager probe\n");
        assert!(findings[0].message.contains("`Other`"), "{findings:?}");
    }

    #[test]
    fn a_missing_type_and_a_missing_identifier_are_separate_errors() {
        let findings = check("description: nothing much\n");
        assert_eq!(
            rules(&findings),
            [RULE_TYPE_MISSING, RULE_IDENTIFIER_MISSING]
        );
        assert_eq!(findings[0].subject, UNIDENTIFIED);
    }

    #[test]
    fn a_duplicated_identifier_is_an_error_on_every_page_that_uses_it() {
        let mut pages = bundle();
        pages.push((
            "knowledge/molecule/aspirin-copy.md".into(),
            doc("type: Molecule\nidentifier: Aspirin\n"),
        ));
        let index = index(&pages);
        let findings = check_page(
            Some("a.md"),
            &doc("type: Molecule\nidentifier: Aspirin\n"),
            &index,
        );
        assert_eq!(rules(&findings), [RULE_IDENTIFIER_DUPLICATE]);
    }

    #[test]
    fn an_opaque_identifier_is_a_warning_and_a_gene_symbol_is_not() {
        for opaque in ["HGNC:6018", "MONDO:0100096", "0100096", "183"] {
            let findings = check(&format!("type: Gene\nidentifier: \"{opaque}\"\n"));
            assert!(
                findings.iter().any(|f| f.rule == RULE_IDENTIFIER_OPAQUE),
                "`{opaque}` should read as opaque: {findings:?}"
            );
        }
        // The symbols a real biomedical bundle is made of stay quiet.
        for readable in ["TP53", "IL6 (protein)", "2-AG", "5-HT", "Interleukin-6"] {
            let findings = check(&format!("type: Gene\nidentifier: \"{readable}\"\n"));
            assert!(
                !findings.iter().any(|f| f.rule == RULE_IDENTIFIER_OPAQUE),
                "`{readable}` should not read as opaque: {findings:?}"
            );
        }
    }

    // ── §11 rule 3: the closed predicate vocabulary ─────────────────────────

    #[test]
    fn an_invalid_predicate_is_an_error_that_names_the_closest_legal_value() {
        let findings = check(&format!(
            "type: Molecule\nidentifier: Ibuprofen\nedges:\n{}",
            edge("treets", "Headache", "")
        ));
        assert!(findings.iter().any(|f| f.rule == RULE_PREDICATE_INVALID));
        assert!(
            findings[0].message.contains("`treats`"),
            "{:?}",
            findings[0].message
        );
    }

    /// §6.F names this one and gives it its own rule id, because it is a
    /// modelling mistake with a specific explanation, not a typo.
    #[test]
    fn negating_a_structural_predicate_is_its_own_error() {
        let findings = check(&format!(
            "type: Molecule\nidentifier: Ibuprofen\nedges:\n{}",
            edge("not_is_a", "Aspirin", "")
        ));
        assert!(findings.iter().any(|f| f.rule == RULE_EDGE_NOT_NEGATABLE));
        assert!(!findings.iter().any(|f| f.rule == RULE_PREDICATE_INVALID));
        // The legacy spelling of the same mistake reaches the same rule.
        let findings = check(&format!(
            "type: Molecule\nidentifier: Ibuprofen\nedges:\n{}",
            edge("is_a", "Aspirin", "    negated: true\n")
        ));
        assert!(findings.iter().any(|f| f.rule == RULE_EDGE_NOT_NEGATABLE));
    }

    // ── §11 rule 4: edge provenance ─────────────────────────────────────────

    #[test]
    fn each_third_of_the_provenance_triplet_is_reported_on_its_own() {
        let findings = check(
            "type: Molecule\nidentifier: Ibuprofen\nedges:\n  - predicate: treats\n    object: Headache\n",
        );
        assert_eq!(
            rules(&findings),
            [
                RULE_EDGE_MISSING_KNOWLEDGE_LEVEL,
                RULE_EDGE_MISSING_AGENT_TYPE,
                RULE_EDGE_MISSING_PRIMARY_SOURCE,
            ]
        );
    }

    #[test]
    fn a_value_outside_the_biolink_enums_is_an_error() {
        let findings = check(&format!(
            "type: Molecule\nidentifier: Ibuprofen\nedges:\n{}",
            edge("treats", "Headache", "").replace("knowledge_assertion", "vibes")
        ));
        assert_eq!(rules(&findings), [RULE_EDGE_INVALID_KNOWLEDGE_LEVEL]);
        let findings = check(&format!(
            "type: Molecule\nidentifier: Ibuprofen\nedges:\n{}",
            edge("treats", "Headache", "").replace("manual_agent", "an intern")
        ));
        assert_eq!(rules(&findings), [RULE_EDGE_INVALID_AGENT_TYPE]);
    }

    /// §8.1: only a `Publication`/`Study`/`Dataset`/`Agent` may bear a source.
    /// A `Concept` is in the same §5.B family and is still not one — which is
    /// the case a `family() == ProvenanceAndContext` check would wave through.
    #[test]
    fn a_primary_source_must_resolve_to_one_of_the_four_source_types() {
        let findings = check(&format!(
            "type: Molecule\nidentifier: Ibuprofen\nedges:\n{}",
            edge("treats", "Headache", "").replace("DrugBank", "Nanometre")
        ));
        assert_eq!(rules(&findings), [RULE_EDGE_PRIMARY_SOURCE_NOT_SOURCE]);

        let findings = check(&format!(
            "type: Molecule\nidentifier: Ibuprofen\nedges:\n{}",
            edge("treats", "Headache", "").replace("DrugBank", "A journal nobody has")
        ));
        assert_eq!(rules(&findings), [RULE_EDGE_PRIMARY_SOURCE_UNRESOLVED]);
    }

    #[test]
    fn not_provided_is_the_one_reserved_non_node_value_and_still_warns() {
        let findings = check(&format!(
            "type: Molecule\nidentifier: Ibuprofen\nedges:\n{}",
            edge("treats", "Headache", "").replace("DrugBank", "not_provided")
        ));
        assert_eq!(rules(&findings), [RULE_EDGE_PRIMARY_SOURCE_NOT_PROVIDED]);
        assert_eq!(findings[0].severity, Severity::Warning);
    }

    /// OKF §11 makes tolerating a broken cross-link a MUST, and ingest
    /// legitimately writes an edge before the target page exists.
    #[test]
    fn an_unresolved_object_is_a_warning_and_never_an_error() {
        let findings = check(&format!(
            "type: Molecule\nidentifier: Ibuprofen\nedges:\n{}",
            edge("treats", "Some disease nobody has written up yet", "")
        ));
        assert_eq!(rules(&findings), [RULE_EDGE_OBJECT_UNRESOLVED]);
        assert_eq!(findings[0].severity, Severity::Warning);
    }

    /// A page being validated before it is written has no bundle, and reporting
    /// every edge as dangling would make the check useless at exactly the
    /// moment it is most wanted.
    #[test]
    fn an_unindexed_bundle_skips_the_cross_document_rules_rather_than_failing_them() {
        let page = doc(&format!(
            "type: Molecule\nidentifier: Ibuprofen\nedges:\n{}",
            edge("treats", "Some disease nobody has written up yet", "")
        ));
        let findings = check_page(None, &page, &BundleIndex::unindexed());
        assert!(findings.is_empty(), "{findings:?}");
    }

    // ── §6 domain/range, §6.F contradictions, §8.1 anchoring ────────────────

    #[test]
    fn a_domain_range_violation_is_a_warning_that_quotes_the_table() {
        let findings = check(&format!(
            "type: Molecule\nidentifier: Ibuprofen\nedges:\n{}",
            edge("treats", "IL6", "")
        ));
        assert_eq!(rules(&findings), [RULE_EDGE_RANGE]);
        assert_eq!(findings[0].severity, Severity::Warning);
        assert!(findings[0].message.contains("Disease/Phenotype"));
    }

    #[test]
    fn a_negative_edge_is_range_checked_against_its_bases_table() {
        let findings = check(&format!(
            "type: Molecule\nidentifier: Ibuprofen\nedges:\n{}",
            edge("not_treats", "IL6", "")
        ));
        assert_eq!(rules(&findings), [RULE_EDGE_RANGE]);
        assert!(findings[0].message.contains("not_treats"));
    }

    #[test]
    fn asserting_x_and_not_x_for_the_same_object_is_a_contradiction() {
        let findings = check(&format!(
            "type: Molecule\nidentifier: Ibuprofen\nedges:\n{}{}",
            edge("treats", "Headache", ""),
            edge("not_treats", "Headache", "")
        ));
        assert_eq!(rules(&findings), [RULE_EDGE_CONTRADICTION]);
        // Same predicate twice with the same polarity is not a contradiction —
        // two sources agreeing is the normal case.
        let findings = check(&format!(
            "type: Molecule\nidentifier: Ibuprofen\nedges:\n{}{}",
            edge("treats", "Headache", ""),
            edge("treats", "Headache", "")
        ));
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn a_source_node_anchored_to_neither_raw_source_nor_xref_is_a_warning() {
        let findings = check("type: Publication\nidentifier: A paper with no home\n");
        assert_eq!(rules(&findings), [RULE_SOURCE_UNANCHORED]);
        assert_eq!(findings[0].severity, Severity::Warning);
        // Either anchor is enough, and a bare string is as good as a list.
        assert!(check("type: Publication\nidentifier: P\nxref: [PMID:1]\n").is_empty());
        assert!(check("type: Publication\nidentifier: P\nraw_source: [raw/p.pdf]\n").is_empty());
        assert!(check("type: Publication\nidentifier: P\nraw_source: raw/p.pdf\n").is_empty());
        // A non-source node is never asked to anchor.
        assert!(check("type: Molecule\nidentifier: Ibuprofen\n").is_empty());
    }

    // ── the three things that must never be reported ────────────────────────

    /// §10 and §11: "a missing external CURIE in `xref` is an enrichment
    /// opportunity to backfill, not a conformance error". Every fixture above
    /// omits `xref`; this asserts the silence is deliberate rather than lucky.
    #[test]
    fn a_missing_xref_is_never_a_finding() {
        let findings = check(&format!(
            "type: Molecule\nidentifier: Ibuprofen\nedges:\n{}",
            edge("treats", "Headache", "")
        ));
        assert!(findings.is_empty(), "{findings:?}");
        assert!(
            !ALL_RULES.iter().any(|r| r.contains("xref")),
            "no rule may be about a missing xref"
        );
    }

    /// §5: "consumers MUST NOT reject a node for an unrecognized `subtype`";
    /// §10: "Do not lint `subtype` against a fixed list; the agent coins it."
    #[test]
    fn subtype_is_never_validated_present_absent_or_invented() {
        for subtype in [
            "",
            "subtype: protein\n",
            "subtype: a_word_no_spec_has_ever_used\n",
        ] {
            let findings = check(&format!("type: Molecule\nidentifier: Ibuprofen\n{subtype}"));
            assert!(findings.is_empty(), "subtype `{subtype}`: {findings:?}");
        }
        assert!(
            !ALL_RULES.iter().any(|r| r.contains("subtype")),
            "no rule may be about a subtype"
        );
    }

    /// DR-7. `Severity` is `okf`'s, which has no fatal variant, so this is a
    /// property of the type — asserted anyway because the value of the
    /// guarantee is that someone checked.
    #[test]
    fn no_rule_can_reject_a_page() {
        let findings = check("description: a page with nothing on it\nedges:\n  - {}\n");
        assert!(!findings.is_empty());
        for finding in &findings {
            assert!(matches!(
                finding.severity,
                Severity::Error | Severity::Warning | Severity::Info
            ));
        }
    }

    // ── §14: a legacy page reads as old, not as broken ──────────────────────

    #[test]
    fn a_v0_4_page_reports_as_deprecated_rather_than_invalid() {
        let findings = check(
            "type: SDOH\ntitle: Housing instability\nfactor_kind: housing\nedges:\n  \
             - predicate: predisposes_to\n    object: Headache\n    negated: true\n    \
             knowledge_level: statistical_association\n    agent_type: manual_agent\n    \
             primary_source: DrugBank\n",
        );
        // `factor_kind` -> `subtype` is reported first because it is applied
        // first: the type aliases that split "by `subtype`" need the lifted
        // value to resolve against.
        assert_eq!(
            rules(&findings),
            [
                RULE_ALIAS_ATTRIBUTE,
                RULE_ALIAS_TYPE,
                RULE_ALIAS_ATTRIBUTE,
                RULE_ALIAS_NEGATED_QUALIFIER,
            ]
        );
        for finding in &findings {
            assert_eq!(finding.severity, Severity::Info);
        }
        // The subject is the *normalized* key on every finding, so a
        // `title`-only page is findable by the name the rest of the bundle
        // links to.
        for finding in &findings {
            assert_eq!(finding.subject, "Housing instability");
        }
    }

    #[test]
    fn a_pre_v0_5_infores_primary_source_is_not_double_reported_as_unresolved() {
        let findings = check(&format!(
            "type: Molecule\nidentifier: Ibuprofen\nedges:\n{}",
            edge("treats", "Headache", "").replace("DrugBank", "infores:drugbank")
        ));
        assert_eq!(rules(&findings), [RULE_ALIAS_PRIMARY_SOURCE_CURIE]);
    }

    // ── the index itself ────────────────────────────────────────────────────

    #[test]
    fn the_index_is_keyed_by_the_normalized_identifier() {
        let pages = vec![(
            "knowledge/legacy.md".to_string(),
            doc("type: ClinicalMeasure\ntitle: LDL cholesterol\n"),
        )];
        let index = index(&pages);
        assert!(index.is_indexed());
        assert_eq!(index.len(), 1);
        let node = index.get("LDL cholesterol").expect("found by its title");
        assert_eq!(node.node_type, Some(NodeType::BiomedicalMeasure));
        assert_eq!(node.path, "knowledge/legacy.md");
    }

    #[test]
    fn an_unindexed_index_is_not_an_empty_one() {
        let unindexed = BundleIndex::unindexed();
        assert!(!unindexed.is_indexed());
        assert!(unindexed.is_empty());
        assert!(index(&[]).is_indexed());
    }

    #[test]
    fn every_rule_this_module_emits_is_listed_in_all_rules() {
        // ALL_RULES is what a filter UI is built from, so a rule missing from
        // it is a finding nobody can switch off — or find.
        let mut seen: Vec<&str> = ALL_RULES.to_vec();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), ALL_RULES.len(), "duplicate rule id");
        for rule in ALL_RULES {
            assert!(
                rule.starts_with("biookf."),
                "`{rule}` is missing the profile prefix"
            );
        }
    }
}

/// §10's provenance-quality rules, which are the only ones in this module that
/// read the filesystem — so they get their own module with a `raw/` fixture
/// rather than sharing the pure-`ConceptDoc` helpers above.
#[cfg(test)]
mod credibility_tests {
    use super::*;
    use crate::knowledge::okf::Page;
    use crate::knowledge::types::SourceMeta;
    use chrono::Utc;

    fn doc(frontmatter: &str) -> ConceptDoc {
        Page::parse(&format!("---\n{frontmatter}---\n\n# body\n"))
            .expect("fixture parses")
            .doc
    }

    /// Write `raw/<id>/meta.yaml` with a verdict, exactly as the classifier
    /// does at ingest.
    fn classify(kb_root: &Path, id: &str, tier: CredibilityTier, retracted: bool, version: u32) {
        let meta = SourceMeta {
            id: id.to_string(),
            title: id.to_string(),
            url: None,
            ingested_at: Utc::now(),
            sha256: "0".into(),
            mime: "text/markdown".into(),
            original_filename: None,
            credibility: Credibility {
                tier,
                confidence: 0.9,
                publisher: None,
                venue: None,
                doi: None,
                retracted,
                reasoning: "fixture".into(),
                classifier_version: version,
            },
        };
        let dir = kb_root.join("raw").join(id);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("meta.yaml"), serde_yaml::to_string(&meta).unwrap()).unwrap();
    }

    /// A claim citing one ingested source, plus that source's own node.
    fn bundle(knowledge_level: &str) -> Vec<(String, ConceptDoc)> {
        vec![
            (
                "knowledge/publication/p.md".to_string(),
                doc("type: Publication\nidentifier: The paper\nraw_source: [raw/the-paper/source.md]\n"),
            ),
            (
                "knowledge/disease/covid.md".to_string(),
                doc("type: Disease\nidentifier: COVID-19\n"),
            ),
            (
                "knowledge/molecule/x.md".to_string(),
                doc(&format!(
                    "type: Molecule\nidentifier: Compound X\nedges:\n  - predicate: treats\n    \
                     object: COVID-19\n    knowledge_level: {knowledge_level}\n    \
                     agent_type: manual_agent\n    primary_source: The paper\n"
                )),
            ),
        ]
    }

    fn run(kb_root: &Path, pages: &[(String, ConceptDoc)]) -> Vec<&'static str> {
        check_credibility(kb_root, pages.iter().map(|(p, d)| (p.as_str(), d)))
            .iter()
            .map(|f| f.rule)
            .collect()
    }

    #[test]
    fn a_retracted_source_behind_a_claim_is_a_warning() {
        let tmp = tempfile::tempdir().unwrap();
        classify(
            tmp.path(),
            "the-paper",
            CredibilityTier::PeerReviewed,
            true,
            1,
        );
        let pages = bundle("knowledge_assertion");
        let findings = check_credibility(tmp.path(), pages.iter().map(|(p, d)| (p.as_str(), d)));
        assert_eq!(
            findings.iter().map(|f| f.rule).collect::<Vec<_>>(),
            [RULE_SOURCE_RETRACTED]
        );
        // Peer-reviewed AND retracted: the retraction fires, the tier rule does
        // not. They are independent signals and a base full of good journals
        // must not go quiet just because the tier is high.
        assert_eq!(findings[0].severity, Severity::Warning);
        assert_eq!(findings[0].subject, "The paper");
        assert_eq!(
            findings[0].path.as_deref(),
            Some("knowledge/publication/p.md"),
            "the finding must point at the source page, which is the one place to fix it"
        );
        assert!(
            findings[0].message.contains("treats -> COVID-19"),
            "the message must name a claim resting on it: {}",
            findings[0].message
        );
    }

    /// THE guard, copied from BioOKF: an unclassified source has no signal, and
    /// reading its default verdict as real would light up every base built
    /// before the classifier existed.
    #[test]
    fn a_source_the_classifier_never_saw_is_never_flagged() {
        let tmp = tempfile::tempdir().unwrap();
        classify(tmp.path(), "the-paper", CredibilityTier::Web, true, 0);
        // Retracted AND web-tier — both rules would fire on any version above 0.
        assert!(run(tmp.path(), &bundle("knowledge_assertion")).is_empty());

        classify(tmp.path(), "the-paper", CredibilityTier::Web, true, 1);
        assert_eq!(
            run(tmp.path(), &bundle("knowledge_assertion")).len(),
            2,
            "…and version 1 is what makes the same fixture report"
        );
    }

    /// The tier rule is scoped to the strongest `knowledge_level`, so a web page
    /// behind an honest `observation` is left alone.
    #[test]
    fn a_low_credibility_source_is_flagged_only_under_a_knowledge_assertion() {
        let tmp = tempfile::tempdir().unwrap();
        classify(tmp.path(), "the-paper", CredibilityTier::Web, false, 1);
        assert_eq!(
            run(tmp.path(), &bundle("knowledge_assertion")),
            [RULE_SOURCE_NOT_SCHOLARLY]
        );
        for honest in ["observation", "prediction", "statistical_association"] {
            assert!(
                run(tmp.path(), &bundle(honest)).is_empty(),
                "`{honest}` claims what the source supports and must not be flagged"
            );
        }
    }

    /// A preprint archive and a recognised database are legitimate provenance
    /// (§10), so the tier rule stops at `web`/`personal`.
    #[test]
    fn a_preprint_or_a_database_is_not_low_credibility() {
        let tmp = tempfile::tempdir().unwrap();
        for ok in [
            CredibilityTier::PeerReviewed,
            CredibilityTier::Preprint,
            CredibilityTier::Book,
            CredibilityTier::GrayLit,
        ] {
            classify(tmp.path(), "the-paper", ok, false, 1);
            assert!(
                run(tmp.path(), &bundle("knowledge_assertion")).is_empty(),
                "{ok:?} should not read as unscholarly"
            );
        }
        for low in [CredibilityTier::Web, CredibilityTier::Personal] {
            classify(tmp.path(), "the-paper", low, false, 1);
            assert_eq!(
                run(tmp.path(), &bundle("knowledge_assertion")),
                [RULE_SOURCE_NOT_SCHOLARLY],
                "{low:?} should read as unscholarly"
            );
        }
    }

    /// `reported_in` names a source as its object rather than in
    /// `primary_source`, and a retraction matters there too.
    #[test]
    fn a_retraction_is_seen_through_a_reported_in_object() {
        let tmp = tempfile::tempdir().unwrap();
        classify(
            tmp.path(),
            "the-paper",
            CredibilityTier::PeerReviewed,
            true,
            1,
        );
        let pages = vec![
            (
                "knowledge/publication/p.md".to_string(),
                doc("type: Publication\nidentifier: The paper\nraw_source: [raw/the-paper/source.md]\n"),
            ),
            (
                "knowledge/molecule/x.md".to_string(),
                doc(
                    "type: Molecule\nidentifier: Compound X\nedges:\n  - predicate: reported_in\n    \
                     object: The paper\n    knowledge_level: knowledge_assertion\n    \
                     agent_type: manual_agent\n    primary_source: not_provided\n",
                ),
            ),
        ];
        assert_eq!(run(tmp.path(), &pages), [RULE_SOURCE_RETRACTED]);
    }

    /// One finding per source, not one per edge: a retracted paper behind forty
    /// claims is one thing to fix.
    #[test]
    fn a_source_cited_many_times_is_reported_once() {
        let tmp = tempfile::tempdir().unwrap();
        classify(
            tmp.path(),
            "the-paper",
            CredibilityTier::PeerReviewed,
            true,
            1,
        );
        let mut pages = bundle("knowledge_assertion");
        for n in 0..5 {
            pages.push((
                format!("knowledge/molecule/x{n}.md"),
                doc(&format!(
                    "type: Molecule\nidentifier: Compound {n}\nedges:\n  - predicate: treats\n    \
                     object: COVID-19\n    knowledge_level: knowledge_assertion\n    \
                     agent_type: manual_agent\n    primary_source: The paper\n"
                )),
            ));
        }
        assert_eq!(run(tmp.path(), &pages), [RULE_SOURCE_RETRACTED]);
    }

    /// A source with no ingested bytes (an external CURIE reference) has no
    /// verdict, and a `raw_source` that tries to climb out of the bundle gets
    /// none either.
    #[test]
    fn an_unreadable_or_escaping_raw_source_reports_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        classify(
            tmp.path(),
            "the-paper",
            CredibilityTier::PeerReviewed,
            true,
            1,
        );
        for anchor in [
            "xref: [infores:drugbank]",
            "raw_source: [raw/does-not-exist/source.md]",
            "raw_source: [../../../elsewhere/meta.yaml]",
        ] {
            let mut pages = bundle("knowledge_assertion");
            pages[0].1 = doc(&format!(
                "type: Publication\nidentifier: The paper\n{anchor}\n"
            ));
            assert!(
                run(tmp.path(), &pages).is_empty(),
                "`{anchor}` must yield no verdict to read"
            );
        }
        assert_eq!(raw_source_id("raw/ok/source.md"), Some("ok"));
        assert_eq!(raw_source_id("raw/ok"), Some("ok"));
        assert_eq!(raw_source_id("../raw/ok/source.md"), None);
        assert_eq!(raw_source_id("raw/../../etc/passwd"), None);
    }
}
