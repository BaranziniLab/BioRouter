//! The merge's semantics, on real bases on disk.
//!
//! Every test here builds two knowledge bases through the same service the app
//! uses, merges one into the other, and reads the result off the filesystem.
//! Nothing is asserted about the planner's internal bookkeeping — a plan that
//! agrees with itself and disagrees with the disk is the failure these are for.

use super::*;
use crate::knowledge::{
    caller::KbCaller,
    okf::model::Page,
    raw as raw_store,
    service::KnowledgeService,
    types::{Credibility, CredibilityTier, SourceMeta},
};
use std::collections::BTreeMap;

// ── fixtures ────────────────────────────────────────────────────────────────

fn service() -> (tempfile::TempDir, KnowledgeService) {
    let dir = tempfile::tempdir().unwrap();
    let svc = KnowledgeService::new(dir.path().to_path_buf());
    (dir, svc)
}

fn base(svc: &KnowledgeService, id: &str) -> PathBuf {
    svc.create_base(id, id, None).unwrap();
    svc.root().join(id)
}

/// A page written through the store, so it is **committed**. That matters:
/// `abort_txn` restores the destination by checking out `main`, so a fixture
/// page left uncommitted would be deleted by the very rollback the atomicity
/// test is measuring, and the test would pass for the wrong reason.
fn put_page(kb: &Path, rel: &str, content: &str) {
    store::write_page(kb, rel, content, "fixture", None).unwrap();
}

/// An OKF/BioOKF-shaped page: a typed identifier, optional `raw_source`,
/// optional typed edges, and a body.
struct PageSpec<'a> {
    r#type: &'a str,
    identifier: &'a str,
    raw_source: Vec<&'a str>,
    /// `(predicate, object, primary_source)`
    edges: Vec<(&'a str, &'a str, &'a str)>,
    body: &'a str,
}

impl PageSpec<'_> {
    fn render(&self) -> String {
        let mut fm = String::from("---\n");
        fm.push_str(&format!("type: {}\n", self.r#type));
        fm.push_str(&format!("identifier: {}\n", self.identifier));
        if !self.raw_source.is_empty() {
            fm.push_str("raw_source:\n");
            for r in &self.raw_source {
                fm.push_str(&format!("- {r}\n"));
            }
        }
        if !self.edges.is_empty() {
            fm.push_str("edges:\n");
            for (predicate, object, primary_source) in &self.edges {
                fm.push_str(&format!(
                    "- predicate: {predicate}\n  object: {object}\n  primary_source: {primary_source}\n"
                ));
            }
        }
        fm.push_str("---\n\n");
        fm.push_str(self.body);
        fm.push('\n');
        fm
    }
}

fn page(r#type: &str, identifier: &str, body: &str) -> String {
    PageSpec {
        r#type,
        identifier,
        raw_source: vec![],
        edges: vec![],
        body,
    }
    .render()
}

fn credibility() -> Credibility {
    Credibility {
        tier: CredibilityTier::Web,
        confidence: 0.5,
        publisher: None,
        venue: None,
        doi: None,
        retracted: false,
        reasoning: "fixture".into(),
        classifier_version: 1,
    }
}

/// A raw source with a **stated** sha256.
///
/// Written through `raw::write_raw` rather than `add_raw_source` on purpose: the
/// hash is the thing under test, and going through the ingest path would make it
/// a function of the converter and the credibility classifier — so a dedup test
/// would be measuring those instead.
fn put_raw(kb: &Path, id: &str, sha: &str, text: &str) {
    raw_store::write_raw(
        kb,
        Some(text.as_bytes()),
        Some("original.txt"),
        &format!("# {id}\n\n{text}\n"),
        SourceMeta {
            id: id.into(),
            title: id.into(),
            url: None,
            ingested_at: chrono::Utc::now(),
            sha256: sha.into(),
            mime: "text/plain".into(),
            original_filename: Some("original.txt".into()),
            credibility: credibility(),
        },
    )
    .unwrap();
    GitRepo::open(kb)
        .unwrap()
        .commit_all(ChangeKind::Manual, &format!("raw {id}"), None)
        .unwrap();
}

/// Everything in a knowledge base a merge could change, hashed.
///
/// `.git` is excluded because a transaction legitimately leaves objects behind
/// (an aborted branch's commits are unreferenced, not absent), and
/// `.biorouter-knowledge/` because it holds the graph cache and the transient
/// write lock — machine state, not knowledge. What remains is exactly the tree a
/// `.brkb` would carry.
fn fingerprint(kb: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    walk_fingerprint(kb, kb, &mut out);
    out
}

fn walk_fingerprint(root: &Path, dir: &Path, out: &mut BTreeMap<String, String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name == ".git" || name == ".biorouter-knowledge" {
            continue;
        }
        let path = entry.path();
        let rel = path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if path.is_dir() {
            walk_fingerprint(root, &path, out);
        } else {
            let bytes = std::fs::read(&path).unwrap_or_default();
            out.insert(rel, raw_store::hash_bytes(&bytes));
        }
    }
}

fn parse(kb: &Path, rel: &str) -> Page {
    Page::parse(&std::fs::read_to_string(kb.join(rel)).unwrap()).unwrap()
}

fn user() -> UserKbMerge {
    UserKbMerge::for_test()
}

/// Every identifier the merged base declares, for the dangling-reference check.
fn identifiers(kb: &Path) -> BTreeSet<String> {
    store::list_pages(kb, None)
        .unwrap()
        .into_iter()
        .filter_map(|p| page_identifier(&std::fs::read_to_string(kb.join(&p.path)).unwrap()))
        .map(|id| identity_key(&id))
        .collect()
}

/// Every `object` and `primary_source` in the base that names nothing.
///
/// This is the "break no links" property stated as a measurement rather than as
/// a claim about the rename map: it reads the merged base back and asks whether
/// each reference resolves, which a rewrite keyed on the wrong map would fail
/// even though every rename was recorded correctly.
fn dangling(kb: &Path) -> Vec<String> {
    let known = identifiers(kb);
    let mut out = Vec::new();
    for p in store::list_pages(kb, None).unwrap() {
        let Ok(parsed) = Page::parse(&std::fs::read_to_string(kb.join(&p.path)).unwrap()) else {
            continue;
        };
        for edge in &parsed.doc.edges {
            if !edge.object.is_empty() && !known.contains(&identity_key(&edge.object)) {
                out.push(format!("{}: object `{}`", p.path, edge.object));
            }
            if let Some(ps) = &edge.primary_source {
                if !ps.is_empty() && !known.contains(&identity_key(ps)) {
                    out.push(format!("{}: primary_source `{}`", p.path, ps));
                }
            }
        }
    }
    out
}

// ── (a) raw dedup on the sha256 already in meta.yaml ────────────────────────

/// The same source in both bases is **one** source afterwards, and every
/// reference to the incoming copy is repointed at the destination's.
///
/// The two bases give it different ids on purpose — `raw::new_source_id` mints a
/// uuid suffix, so two ingests of one PDF never agree on an id and an
/// id-keyed dedup would find nothing. The hash is the only thing that can
/// answer.
#[tokio::test]
async fn an_identical_source_in_both_bases_is_deduped_by_hash_not_duplicated() {
    let (_dir, svc) = service();
    let dst = base(&svc, "dst");
    let src = base(&svc, "src");

    put_raw(&dst, "chen-2020-aaaaaa", "sha-shared", "the shared paper");
    put_raw(&src, "chen-2020-zzzzzz", "sha-shared", "the shared paper");
    put_raw(&src, "novel-source-1", "sha-only-in-src", "only here");
    put_page(
        &src,
        "knowledge/publication/chen.md",
        &PageSpec {
            r#type: "Publication",
            identifier: "Chen 2020",
            raw_source: vec!["raw/chen-2020-zzzzzz/original.txt"],
            edges: vec![],
            body: "See [the original](raw/chen-2020-zzzzzz/original.txt).",
        }
        .render(),
    );

    let report = svc
        .merge_bases("dst", "src", &MergeAuthority::User(&user()), false)
        .await
        .unwrap();

    assert_eq!(
        report.raw_deduped,
        vec![RawDedup {
            source_id: "chen-2020-zzzzzz".into(),
            matched: "chen-2020-aaaaaa".into(),
            sha256: "sha-shared".into(),
        }],
        "the shared source was not recognised by content: {report:#?}"
    );
    assert!(
        !dst.join("raw/chen-2020-zzzzzz").exists(),
        "a second copy of the shared source was written anyway"
    );
    assert_eq!(
        report.raw_copied,
        vec![Rename {
            from: "novel-source-1".into(),
            to: "novel-source-1".into(),
        }],
        "the source only the incoming base had should have come across as itself"
    );
    assert!(dst.join("raw/novel-source-1/meta.yaml").exists());

    // …and the carried page now points at the DESTINATION's copy, in both the
    // frontmatter and the body. A dedup that dropped the directory and left the
    // references behind would be worse than no dedup at all.
    let carried = parse(&dst, "knowledge/publication/chen.md");
    let raw_source = carried.doc.extra.get("raw_source").unwrap();
    assert_eq!(
        serde_yaml::to_string(raw_source).unwrap().trim(),
        "- raw/chen-2020-aaaaaa/original.txt",
        "raw_source still names the deduped copy"
    );
    assert!(
        carried.body.contains("raw/chen-2020-aaaaaa/original.txt"),
        "the body link still names the deduped copy: {}",
        carried.body
    );
}

/// A raw id that collides is renamed, and `meta.yaml`'s own `id` follows it.
///
/// The reference implementation renames the directory alone. Here `meta.yaml`
/// states the id too, so a directory-only rename leaves `raw::read_meta`
/// answering with an id that names a directory somewhere else — a dangling
/// reference one level below the pages, invisible to every link check.
#[tokio::test]
async fn a_colliding_raw_id_is_renamed_and_its_meta_follows() {
    let (_dir, svc) = service();
    let dst = base(&svc, "dst");
    let src = base(&svc, "src");
    put_raw(&dst, "notes", "sha-dst", "the destination's notes");
    put_raw(&src, "notes", "sha-src", "the source's notes");

    let report = svc
        .merge_bases("dst", "src", &MergeAuthority::User(&user()), false)
        .await
        .unwrap();

    assert_eq!(
        report.raw_copied,
        vec![Rename {
            from: "notes".into(),
            to: "notes-src".into(),
        }]
    );
    assert_eq!(
        std::fs::read_to_string(dst.join("raw/notes/source.md")).unwrap(),
        "# notes\n\nthe destination's notes\n",
        "the destination's own raw source was overwritten"
    );
    assert_eq!(
        raw_store::read_meta(&dst, "notes-src").unwrap().id,
        "notes-src",
        "meta.yaml still claims the pre-rename id"
    );
}

// ── (b) the destination stays canonical ─────────────────────────────────────

/// The snapshot catches all three ways a merge could stop the destination being
/// canonical: an identifier that vanishes, one that moves, and a page that is
/// deleted.
///
/// Exercised directly against the primitive rather than through a merge,
/// because a merge that *did* one of these is the bug — a test that could only
/// observe the property through a correct merge could never fail.
#[test]
fn the_snapshot_catches_a_destination_identifier_that_was_removed_or_moved() {
    let (_dir, svc) = service();
    let kb = base(&svc, "dst");
    put_page(
        &kb,
        "knowledge/molecule/il-6.md",
        &page("Molecule", "IL-6", "b"),
    );
    put_page(
        &kb,
        "knowledge/molecule/tnf.md",
        &page("Molecule", "TNF", "b"),
    );
    let before = snapshot(&kb).unwrap();
    assert_eq!(before.page_count(), 2);
    assert!(verify_snapshot(&kb, &before).unwrap().is_empty());

    // 1. the identifier is rewritten in place
    put_page(
        &kb,
        "knowledge/molecule/il-6.md",
        &page("Molecule", "IL-6 (renamed)", "b"),
    );
    let issues = verify_snapshot(&kb, &before).unwrap();
    assert!(
        issues.iter().any(|i| i.contains("`IL-6`")),
        "a destination identifier was rewritten and nothing said so: {issues:?}"
    );

    // 2. the page moves
    put_page(
        &kb,
        "knowledge/molecule/il-6.md",
        &page("Molecule", "IL-6", "b"),
    );
    put_page(&kb, "knowledge/other/tnf.md", &page("Molecule", "TNF", "b"));
    std::fs::remove_file(kb.join("knowledge/molecule/tnf.md")).unwrap();
    let issues = verify_snapshot(&kb, &before).unwrap();
    assert!(
        issues.iter().any(|i| i.contains("moved")),
        "a destination page moved and nothing said so: {issues:?}"
    );

    // 3. the page is deleted outright — and a legacy page with no identifier at
    //    all has to be catchable too, which is why the snapshot carries paths.
    std::fs::remove_file(kb.join("knowledge/other/tnf.md")).unwrap();
    let issues = verify_snapshot(&kb, &before).unwrap();
    assert!(
        issues.iter().any(|i| i.contains("no longer exists")),
        "a destination page was deleted and nothing said so: {issues:?}"
    );
}

/// A destination raw source must survive a merge untouched — the third set in
/// the snapshot, and the one the reference implementation has no equivalent of.
#[test]
fn the_snapshot_catches_a_destination_raw_source_that_was_renamed() {
    let (_dir, svc) = service();
    let kb = base(&svc, "dst");
    put_raw(&kb, "notes", "sha-dst", "notes");
    let before = snapshot(&kb).unwrap();
    std::fs::rename(kb.join("raw/notes"), kb.join("raw/notes-2")).unwrap();
    let issues = verify_snapshot(&kb, &before).unwrap();
    assert!(issues.iter().any(|i| i.contains("raw/notes")), "{issues:?}");
}

// ── (c) + (d) carry-over, identifier collision, reference rewriting ─────────

/// The heart of it. An identifier that exists in both bases is **not** collapsed
/// — the incoming one is renamed — and every reference to it is repointed so
/// nothing dangles.
///
/// ⚠ The assertion that the destination's own `IL-6` page is **byte-identical**
/// afterwards is the one that separates this from a merge that "worked": the
/// wrong implementation of this feature combines the two pages, and it looks
/// entirely reasonable until a user notices their curated page has grown a
/// stranger's edges.
///
/// ⚠ **The plain `[[IL-6]]` in `chen.md` is the case this test was missing**,
/// and its absence was not visible from inside it: the fixture wrote the piped,
/// path-style and sugar forms, the comment said "all three grammars", and the
/// fourth — a bare name, the commonest link a legacy base contains — was
/// nowhere.
///
/// Here that link is repointed by *name*, which is the rung it was written on.
/// This fixture cannot show what happens when the name rung is the **only** one
/// that could reach it, because the page collided on its path too and the
/// basename rung would land on the same page by another road; the test that
/// separates those is
/// [`a_renamed_identifier_whose_page_did_not_move_still_repoints_plain_links`].
#[tokio::test]
async fn an_identifier_collision_renames_the_incoming_side_and_leaves_no_dangling_reference() {
    let (_dir, svc) = service();
    let dst = base(&svc, "dst");
    let src = base(&svc, "src");

    put_page(
        &dst,
        "knowledge/molecule/il-6.md",
        &page("Molecule", "IL-6", "The destination's own curated page."),
    );
    let dst_page_before = std::fs::read_to_string(dst.join("knowledge/molecule/il-6.md")).unwrap();

    put_page(
        &src,
        "knowledge/molecule/il-6.md",
        &PageSpec {
            r#type: "Molecule",
            identifier: "IL-6",
            raw_source: vec![],
            edges: vec![("reported_in", "Chen 2020", "Chen 2020")],
            body: "The incoming page, which cites [[Chen 2020]].",
        }
        .render(),
    );
    put_page(
        &src,
        "knowledge/publication/chen.md",
        &PageSpec {
            r#type: "Publication",
            identifier: "Chen 2020",
            raw_source: vec![],
            edges: vec![("mentions", "IL-6", "Chen 2020")],
            body: "Reports on [[IL-6]], [[knowledge/molecule/il-6.md|IL-6]] and \
                   [[associated_with:: IL-6 | k=v]].",
        }
        .render(),
    );

    let report = svc
        .merge_bases("dst", "src", &MergeAuthority::User(&user()), false)
        .await
        .unwrap();

    assert_eq!(
        report.identifiers_renamed,
        vec![Rename {
            from: "IL-6".into(),
            to: "IL-6 (src)".into(),
        }],
        "the incoming identifier was not renamed: {report:#?}"
    );
    assert_eq!(
        std::fs::read_to_string(dst.join("knowledge/molecule/il-6.md")).unwrap(),
        dst_page_before,
        "the destination's own page changed; the destination is canonical"
    );

    let carried_path = &report
        .paths_renamed
        .iter()
        .find(|r| r.from == "knowledge/molecule/il-6.md")
        .expect("the colliding path was not renamed")
        .to;
    let carried = parse(&dst, carried_path);
    assert_eq!(carried.doc.identifier.as_deref(), Some("IL-6 (src)"));

    // Every reference to the renamed page, in all four grammars.
    let chen = parse(&dst, "knowledge/publication/chen.md");
    assert_eq!(
        chen.doc.edges[0].object, "IL-6 (src)",
        "the typed edge still points at the destination's IL-6"
    );
    assert!(
        chen.body.contains("[[IL-6 (src)]]") && !chen.body.contains("[[IL-6]]"),
        "the plain [[IL-6]] link was not repointed, so it now names the \
         destination's own IL-6 page: {}",
        chen.body
    );
    // The link keeps the FORM it was written in — a `.md`-suffixed logical path
    // stays one. Turning every path-style link into a bare title would resolve
    // identically and produce a diff across the whole carried bundle.
    assert!(
        chen.body.contains(&format!("[[{carried_path}|IL-6]]")),
        "the piped path-style link was not repointed: {}",
        chen.body
    );
    assert!(
        chen.body.contains("[[associated_with:: IL-6 (src) | k=v]]"),
        "BioOKF inline edge sugar was not repointed: {}",
        chen.body
    );

    assert!(
        dangling(&dst).is_empty(),
        "the merged base has dangling references: {:?}",
        dangling(&dst)
    );
    // …and the provenance stamp, which is what makes "which pages arrived?"
    // answerable without the report.
    assert!(
        carried
            .doc
            .extra
            .get(MERGED_FROM_KEY)
            .and_then(|v| v.get("identifier"))
            .and_then(|v| v.as_str())
            == Some("IL-6"),
        "the renamed page does not record what it used to be called"
    );
}

/// Two bases that name one concept differently on disk: the identifier collides
/// and the **path does not**. A plain `[[IL-6]]` in an incoming page must still
/// be repointed — and this is the fixture where nothing else can do it for it.
///
/// ⚠ **This is the case that corrupts, and the reason it needed its own test.**
/// With the page path colliding too, a rewriter that consulted only the page map
/// still landed a bare `[[IL-6]]` on the right page, by the wrong road — the
/// stem had moved, so the link moved with it. Give the incoming page a path of
/// its own and that road disappears: the page map has no entry to find, the link
/// is left spelled exactly as it was, and in the merged base that spelling now
/// resolves to the **destination's** curated `IL-6`. Nothing dangles. The
/// incoming publication simply starts asserting things about someone else's
/// concept, silently, in the least reversible operation in the subsystem.
///
/// So the measurement is the graph, not the text: `dangling()` reads `edges:`
/// and a body link is not an edge, and a link that was left alone *resolves*.
/// The question a string comparison cannot ask is **which page** it resolves to.
#[tokio::test]
async fn a_renamed_identifier_whose_page_did_not_move_still_repoints_plain_links() {
    let (_dir, svc) = service();
    let dst = base(&svc, "dst");
    let src = base(&svc, "src");

    put_page(
        &dst,
        "knowledge/molecule/il-6.md",
        &page("Molecule", "IL-6", "The destination's own curated page."),
    );
    // Same concept, same identifier, a different file name — so the identifier
    // collides and the path does not.
    put_page(
        &src,
        "knowledge/molecule/interleukin-6.md",
        &page("Molecule", "IL-6", "The incoming page."),
    );
    put_page(
        &src,
        "knowledge/publication/chen.md",
        &page("Publication", "Chen 2020", "Reports on [[IL-6]]."),
    );

    let report = svc
        .merge_bases("dst", "src", &MergeAuthority::User(&user()), false)
        .await
        .unwrap();
    assert_eq!(
        report.identifiers_renamed,
        vec![Rename {
            from: "IL-6".into(),
            to: "IL-6 (src)".into(),
        }],
        "{report:#?}"
    );
    assert!(
        report.paths_renamed.is_empty(),
        "the incoming path did not collide; a rename here would give the page \
         map an entry and hide the very thing this test measures: {report:#?}"
    );

    let chen = parse(&dst, "knowledge/publication/chen.md");
    assert!(
        chen.body.contains("[[IL-6 (src)]]"),
        "the plain link was not repointed: {}",
        chen.body
    );

    let g = crate::knowledge::graph::derive(&dst).unwrap();
    let node_at = |path: &str| {
        g.nodes
            .iter()
            .find(|n| n.path == path)
            .unwrap_or_else(|| panic!("no node for {path}; nodes={:?}", g.nodes))
            .id
            .clone()
    };
    let chen_node = node_at("knowledge/publication/chen.md");
    let dst_own = node_at("knowledge/molecule/il-6.md");
    let carried = node_at("knowledge/molecule/interleukin-6.md");
    assert!(
        !g.edges
            .iter()
            .any(|e| e.from == chen_node && e.to == dst_own),
        "the incoming publication now points at the DESTINATION's IL-6: \
         edges={:?}",
        g.edges
    );
    assert!(
        g.edges
            .iter()
            .any(|e| e.from == chen_node && e.to == carried),
        "the incoming publication lost the link it arrived with: edges={:?}",
        g.edges
    );
}

/// A page that collides with nothing keeps its path, its identifier and its
/// links — the case that must stay boring.
#[tokio::test]
async fn a_page_that_collides_with_nothing_is_carried_over_unchanged() {
    let (_dir, svc) = service();
    let dst = base(&svc, "dst");
    let src = base(&svc, "src");
    put_page(
        &dst,
        "knowledge/molecule/tnf.md",
        &page("Molecule", "TNF", "b"),
    );
    put_page(
        &src,
        "knowledge/molecule/il-6.md",
        &page("Molecule", "IL-6", "Body with [[TNF]]."),
    );

    let report = svc
        .merge_bases("dst", "src", &MergeAuthority::User(&user()), false)
        .await
        .unwrap();

    assert!(report.identifiers_renamed.is_empty(), "{report:#?}");
    assert!(report.paths_renamed.is_empty(), "{report:#?}");
    let carried = parse(&dst, "knowledge/molecule/il-6.md");
    assert_eq!(carried.doc.identifier.as_deref(), Some("IL-6"));
    assert!(
        carried.body.contains("[[TNF]]"),
        "an untouched link was rewritten anyway: {}",
        carried.body
    );
    // The link now resolves against the DESTINATION's `TNF`, which is the merge
    // doing its job: the two bases have become one graph.
    assert!(dangling(&dst).is_empty());
}

// ── (e) atomicity ───────────────────────────────────────────────────────────

/// A failure part-way through leaves the destination byte-identical.
///
/// The failure is forced by a directory sitting where the last incoming page
/// wants to land — invisible to `store::list_pages`, so the planner cannot see
/// it and the guard in `write_everything` is what fires. By then a raw source
/// has been copied and two pages have been written, so this measures the
/// rollback rather than an early return.
///
/// ⚠ `raw/<id>/original.*` is **gitignored** and a page not yet committed on the
/// transaction branch is **untracked**. Neither is reachable by any checkout, so
/// deleting the `Created` list is what this test fails on — `abort_txn` alone
/// leaves both behind.
#[tokio::test]
async fn a_failure_mid_merge_leaves_the_destination_byte_identical() {
    let (_dir, svc) = service();
    let dst = base(&svc, "dst");
    let src = base(&svc, "src");
    put_page(
        &dst,
        "knowledge/molecule/tnf.md",
        &page("Molecule", "TNF", "b"),
    );
    put_raw(&dst, "notes", "sha-dst", "notes");

    put_raw(&src, "incoming", "sha-src", "incoming");
    put_page(&src, "knowledge/a.md", &page("Concept", "A", "b"));
    put_page(&src, "knowledge/b.md", &page("Concept", "B", "b"));
    put_page(&src, "knowledge/zz.md", &page("Concept", "ZZ", "b"));

    // The poison: a *directory* named like a page, at the path the last incoming
    // page will take. Committed, so the comparison below is over a stable tree.
    std::fs::create_dir_all(dst.join("knowledge/zz.md")).unwrap();
    std::fs::write(dst.join("knowledge/zz.md/keep.txt"), b"not a page").unwrap();
    GitRepo::open(&dst)
        .unwrap()
        .commit_all(ChangeKind::Manual, "poison", None)
        .unwrap();

    let before = fingerprint(&dst);
    let err = svc
        .merge_bases("dst", "src", &MergeAuthority::User(&user()), false)
        .await
        .expect_err("the merge should have failed on the poisoned path");
    assert!(
        format!("{err:#}").contains("knowledge/zz.md"),
        "the failure does not name what it refused: {err:#}"
    );

    assert_eq!(
        fingerprint(&dst),
        before,
        "the destination changed despite the merge failing"
    );
    assert!(
        !dst.join("raw/incoming").exists(),
        "a copied raw source survived"
    );
    assert!(
        !dst.join("knowledge/a.md").exists(),
        "a written page survived"
    );
    assert!(
        !dst.join("knowledge/b.md").exists(),
        "a written page survived"
    );

    // …and the base is still usable: HEAD is back on `main`, so the next write
    // does not land on an abandoned transaction branch.
    put_page(&dst, "knowledge/after.md", &page("Concept", "After", "b"));
    assert!(dst.join("knowledge/after.md").exists());
}

/// The source base is only read. Nothing about a merge moves, renames or empties
/// it — the deliberate deviation from `bokf-core::merge_raw`, which relocates.
#[tokio::test]
async fn a_merge_leaves_the_source_base_byte_identical() {
    let (_dir, svc) = service();
    let dst = base(&svc, "dst");
    let src = base(&svc, "src");
    put_page(
        &dst,
        "knowledge/molecule/il-6.md",
        &page("Molecule", "IL-6", "b"),
    );
    put_raw(&src, "incoming", "sha-src", "incoming");
    put_page(
        &src,
        "knowledge/molecule/il-6.md",
        &PageSpec {
            r#type: "Molecule",
            identifier: "IL-6",
            raw_source: vec!["raw/incoming/original.txt"],
            edges: vec![],
            body: "b",
        }
        .render(),
    );

    let before = fingerprint(&src);
    svc.merge_bases("dst", "src", &MergeAuthority::User(&user()), false)
        .await
        .unwrap();
    assert_eq!(
        fingerprint(&src),
        before,
        "the merge modified the source base"
    );
}

// ── the dry run ─────────────────────────────────────────────────────────────

/// A dry run writes nothing, and reports the same operation the merge performs.
///
/// Both halves matter. A preview that writes is a merge with a misleading name;
/// a preview that describes a *different* merge is worse than none, because the
/// user approves one thing and gets another.
#[tokio::test]
async fn the_dry_run_writes_nothing_and_describes_the_merge_that_would_run() {
    let (_dir, svc) = service();
    let dst = base(&svc, "dst");
    let src = base(&svc, "src");
    put_page(
        &dst,
        "knowledge/molecule/il-6.md",
        &page("Molecule", "IL-6", "b"),
    );
    put_raw(&dst, "shared", "sha-shared", "shared");
    put_raw(&src, "shared-elsewhere", "sha-shared", "shared");
    put_raw(&src, "incoming", "sha-src", "incoming");
    put_page(
        &src,
        "knowledge/molecule/il-6.md",
        &page("Molecule", "IL-6", "b"),
    );
    put_page(
        &src,
        "knowledge/molecule/tnf.md",
        &page("Molecule", "TNF", "b"),
    );

    let before = fingerprint(&dst);
    let preview = svc
        .merge_bases("dst", "src", &MergeAuthority::User(&user()), true)
        .await
        .unwrap();

    assert!(preview.dry_run);
    assert!(preview.commit_sha.is_none());
    assert_eq!(
        fingerprint(&dst),
        before,
        "the dry run wrote to the destination"
    );

    let applied = svc
        .merge_bases("dst", "src", &MergeAuthority::User(&user()), false)
        .await
        .unwrap();
    assert!(applied.commit_sha.is_some());

    // Field by field, so a preview that under-reports one axis is caught. The
    // two differ only in the fields an applied merge can fill.
    assert_eq!(preview.pages_carried, applied.pages_carried);
    assert_eq!(preview.identifiers_renamed, applied.identifiers_renamed);
    assert_eq!(preview.paths_renamed, applied.paths_renamed);
    assert_eq!(preview.raw_copied, applied.raw_copied);
    assert_eq!(preview.raw_deduped, applied.raw_deduped);
    assert_eq!(preview.references_rewritten, applied.references_rewritten);
    // Both halves of the count, because a preview that agreed on the numerator
    // and invented its own denominator would read as a different operation.
    assert_eq!(preview.references_seen, applied.references_seen);
    assert_eq!(preview.destination_tier, applied.destination_tier);
    assert_eq!(preview.owners_added, applied.owners_added);
    assert!(preview.canonical_violations.is_empty());
}

// ── the privacy lattice ─────────────────────────────────────────────────────

/// MAX on the tier axis. A private source raises a public destination, and the
/// raise happens whoever asked — the user's own merge included, because the
/// content is what carries the classification, not the caller.
#[tokio::test]
async fn a_private_source_base_raises_a_public_destination() {
    let (_dir, svc) = service();
    let dst = base(&svc, "dst");
    let src = base(&svc, "src");
    put_page(&src, "knowledge/a.md", &page("Concept", "A", "b"));
    crate::knowledge::tier::raise_unlocked(svc.root(), "src", true).unwrap();
    assert!(!crate::knowledge::tier::is_private(svc.root(), "dst"));

    let report = svc
        .merge_bases("dst", "src", &MergeAuthority::User(&user()), false)
        .await
        .unwrap();

    assert_eq!(report.destination_tier, "private");
    assert!(
        crate::knowledge::tier::is_private(svc.root(), "dst"),
        "a public base absorbed a private base's pages and stayed public"
    );
    // The source is untouched on this axis too: a merge raises, never lowers,
    // and it has nothing of the source's to lower.
    assert!(crate::knowledge::tier::is_private(svc.root(), "src"));
    assert!(dst.join("knowledge/a.md").exists());
}

/// UNION on the affiliation axis (DR-26) — the same rule `import_brkb` applies to
/// an incoming archive, because a merge is that transfer with the archive step
/// removed.
#[tokio::test]
async fn a_merge_unions_the_owning_institutions_and_drops_none() {
    let (_dir, svc) = service();
    base(&svc, "dst");
    let src = base(&svc, "src");
    put_page(&src, "knowledge/a.md", &page("Concept", "A", "b"));
    let owners =
        |names: &[&str]| -> BTreeSet<String> { names.iter().map(|s| s.to_string()).collect() };
    crate::knowledge::tier::add_owners_unlocked(svc.root(), "dst", owners(&["ucsf"])).unwrap();
    crate::knowledge::tier::add_owners_unlocked(svc.root(), "src", owners(&["stanford"])).unwrap();

    let report = svc
        .merge_bases("dst", "src", &MergeAuthority::User(&user()), false)
        .await
        .unwrap();

    assert_eq!(report.owners_added, vec!["stanford".to_string()]);
    assert_eq!(
        crate::knowledge::tier::affiliation(svc.root(), "dst").owners(),
        Some(&owners(&["stanford", "ucsf"])),
        "the destination must own the union; dropping either is a laundering path"
    );
    assert_eq!(
        crate::knowledge::tier::affiliation(svc.root(), "src").owners(),
        Some(&owners(&["stanford"])),
        "the source's own claim must not change"
    );
}

/// The barrier, over BOTH ids. A model that may not read the source may not
/// merge it, and a model that may not write the destination may not merge into
/// it — including through the **preview**, whose report quotes the source's page
/// paths and identifiers straight back to the caller.
#[tokio::test]
async fn a_public_model_can_merge_neither_a_private_source_nor_into_a_private_destination() {
    let (_dir, svc) = service();
    base(&svc, "public-a");
    base(&svc, "public-b");
    let secret = base(&svc, "secret");
    put_page(&secret, "knowledge/a.md", &page("Concept", "A", "b"));
    crate::knowledge::tier::raise_unlocked(svc.root(), "secret", true).unwrap();

    let public = KbCaller::restricted();
    for dry_run in [true, false] {
        let err = svc
            .merge_bases(
                "public-a",
                "secret",
                &MergeAuthority::Model(&public),
                dry_run,
            )
            .await
            .expect_err("a public model reached a private source");
        assert!(
            format!("{err:#}").contains("private"),
            "the refusal is not the barrier's own: {err:#}"
        );
        svc.merge_bases(
            "secret",
            "public-b",
            &MergeAuthority::Model(&public),
            dry_run,
        )
        .await
        .expect_err("a public model wrote into a private destination");
    }
    assert!(
        !svc.root().join("public-a/knowledge/a.md").exists(),
        "a refused merge still carried a page"
    );
}

/// A private model's merge is permitted and ratchets the destination on both
/// axes, which is what stops the merge being the laundering path `kb_export` +
/// `kb_import` was.
#[tokio::test]
async fn a_private_models_merge_ratchets_the_destination_on_both_axes() {
    let (_dir, svc) = service();
    base(&svc, "dst");
    let src = base(&svc, "src");
    put_page(&src, "knowledge/a.md", &page("Concept", "A", "b"));

    let ucsf = KbCaller::new(
        true,
        crate::knowledge::affiliation::CallerAffiliation::Institution("ucsf".into()),
    );
    let report = svc
        .merge_bases("dst", "src", &MergeAuthority::Model(&ucsf), false)
        .await
        .unwrap();

    assert_eq!(report.destination_tier, "private");
    assert_eq!(report.owners_added, vec!["ucsf".to_string()]);
}

/// A merge into itself is refused before anything is planned. It reads as a
/// harmless no-op and is not: the plan would see every page as a path collision
/// with itself and duplicate the whole base.
#[tokio::test]
async fn a_base_cannot_be_merged_into_itself() {
    let (_dir, svc) = service();
    let kb = base(&svc, "dst");
    put_page(&kb, "knowledge/a.md", &page("Concept", "A", "b"));
    let before = fingerprint(&kb);
    svc.merge_bases("dst", "dst", &MergeAuthority::User(&user()), false)
        .await
        .expect_err("a base was merged into itself");
    assert_eq!(fingerprint(&kb), before);
}

// ── the audits ──────────────────────────────────────────────────────────────

/// The merge proof-of-user is minted in exactly one place.
///
/// [`MergeAuthority::User`] skips the caller barrier, so this type is the whole
/// of what stops a model reaching that branch — Rust guarantees the tuple
/// literal cannot be written outside this module, and this is what caps the
/// number of `from_user_action` call sites at one.
///
/// ⚠ **The window is `/src/`, not `crates/`**, matching
/// `biorouter/tests/privacy_capability.rs`'s census and for the same reason: a
/// test binary that mints the proof is not a way to merge two bases, it is a
/// test *of* the merge — `tests/privacy_toggle_merge.rs` has to construct one to
/// exercise the fold with the master switch off, and it cannot reach the
/// `#[cfg(test)]` twin from another compilation unit. What the audit is about is
/// production code, where a second site would be a second door.
///
/// ⚠ A tripwire over one spelling, not a proof. What it reliably catches is the
/// realistic case: a second bypass added by someone who copied the first.
#[test]
fn the_merge_proof_of_user_is_constructed_in_exactly_one_place() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    // Composed at runtime so this file's own text cannot satisfy the search.
    let minted = concat!("UserKbMerge::", "from_user_action");
    let mut sites: Vec<String> = Vec::new();
    let mut scanned = 0usize;
    for entry in walkdir::WalkDir::new(root.join("crates")) {
        let entry = entry.expect("the audit must not silently skip an unreadable directory");
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let rel = p
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if !rel.contains("/src/") {
            continue;
        }
        scanned += 1;
        let src = std::fs::read_to_string(p)
            .unwrap_or_else(|e| panic!("the audit could not read {rel}: {e}"));
        for line in src.lines() {
            let code = line.trim_start();
            // Prose cannot construct a proof, and a header that explains why the
            // type exists is not a way to merge two bases.
            if code.starts_with("//") {
                continue;
            }
            if code.contains(minted) {
                sites.push(rel.clone());
            }
        }
    }
    assert!(
        scanned >= 400,
        "only {scanned} .rs files were scanned. A broken walk reports the same \
         empty set as a clean tree."
    );
    sites.sort();
    assert_eq!(
        sites,
        vec!["crates/biorouter-server/src/routes/knowledge.rs".to_string()],
        "the merge proof-of-user is minted somewhere new. Every construction site \
         is a way to merge two knowledge bases with no reachability check on \
         either, so only the route behind the user-action header may hold one."
    );
}

/// The plan-time canonical check is not decorative: hand it a plan that would
/// land on the destination and it says so.
///
/// Constructed by hand because a correct planner cannot produce one — which is
/// exactly why the check exists and exactly why it needs a test that does not go
/// through the planner.
#[test]
fn the_plan_check_names_a_plan_that_would_write_over_the_destination() {
    let (_dir, svc) = service();
    let kb = base(&svc, "dst");
    put_page(
        &kb,
        "knowledge/molecule/il-6.md",
        &page("Molecule", "IL-6", "b"),
    );
    put_raw(&kb, "notes", "sha", "notes");
    let snap = snapshot(&kb).unwrap();

    let issues = plan_violations(
        &snap,
        &[PlannedPage {
            source_path: "knowledge/molecule/il-6.md".into(),
            destination_path: "knowledge/molecule/il-6.md".into(),
            identifier: Some("IL-6".into()),
            renamed_identifier: None,
            content: String::new(),
        }],
        &[Rename {
            from: "notes".into(),
            to: "notes".into(),
        }],
    );
    assert_eq!(issues.len(), 3, "{issues:#?}");
    assert!(issues
        .iter()
        .any(|i| i.contains("overwrite the destination's own page")));
    assert!(issues.iter().any(|i| i.contains("must be renamed instead")));
    assert!(issues.iter().any(|i| i.contains("raw/notes")));
}

/// The link rewriter, over every grammar `okf::links` reads, plus the two
/// non-links that must survive it: an unterminated `[[`, and a link to a page
/// nothing renamed.
///
/// ⚠ **The maps are keyed apart on purpose, and the test could not fail while
/// they were not.** This fixture used to seed `identifiers["il-6"]` *and*
/// `page_stems["il-6"]`, and `link_key("IL-6")` slugs to `il-6` too — so
/// `[[IL-6]]` came out renamed through the PAGE map and the assertion passed
/// over a rewriter that never consulted the identifier map for a plain
/// `[[Name]]` at all. Every identifier below now reduces to a key **no page
/// stem holds**, so a bare link can only move if the identifier rung was asked;
/// and the page-stem key belongs to no identifier, so the path-style link is
/// equally proof that the basename rung still runs.
#[test]
fn the_rewriter_touches_every_link_grammar_and_only_what_moved() {
    let mut renames = Renames::default();
    // `identity_key` = `interleukin-6`; no page stem is keyed on it.
    renames
        .identifiers
        .insert("interleukin-6".into(), "Interleukin 6 (src)".into());
    // An identifier that READS as a path (`link_key` = `cd8-ratio`, which is not
    // a page stem here either). It is why the two rungs are ordered rather than
    // one being skipped — see `rewritten_reference`.
    renames
        .identifiers
        .insert("cd4-cd8-ratio".into(), "CD4/CD8 ratio (src)".into());
    renames.page_stems.insert("il-6".into(), "il-6-src".into());
    renames.raw_ids.insert("old".into(), "new".into());

    let body = "\
[[Interleukin 6]] and [[knowledge/molecule/il-6.md|the alias]] and [[treats:: Interleukin 6 | k=v]]\n\
[[CD4/CD8 ratio]]\n\
[a link](knowledge/molecule/il-6.md) and [raw](../raw/old/original.pdf)\n\
[[TNF]] and [other](knowledge/molecule/tnf.md) and [[unterminated\n";
    let (out, n) = rewrite_body(body, &renames);

    // The bare name: the identifier map is the ONLY map that can reach it.
    assert!(
        out.contains("[[Interleukin 6 (src)]]") && !out.contains("[[Interleukin 6]]"),
        "a plain [[Name]] was left spelled the way it was, which in the merged \
         base now names the DESTINATION's page of that name: {out}"
    );
    assert!(
        out.contains("[[CD4/CD8 ratio (src)]]"),
        "an identifier containing a slash reads as path-shaped; the path rung \
         must be tried FIRST for it, never instead of the name rung: {out}"
    );
    assert!(
        out.contains("[[knowledge/molecule/il-6-src.md|the alias]]"),
        "{out}"
    );
    assert!(
        out.contains("[[treats:: Interleukin 6 (src) | k=v]]"),
        "{out}"
    );
    assert!(
        out.contains("[a link](knowledge/molecule/il-6-src.md)"),
        "{out}"
    );
    assert!(out.contains("[raw](../raw/new/original.pdf)"), "{out}");
    assert!(
        out.contains("[[TNF]]"),
        "an untouched wiki link moved: {out}"
    );
    assert!(
        out.contains("[other](knowledge/molecule/tnf.md)"),
        "an untouched markdown link moved: {out}"
    );
    assert!(out.contains("[[unterminated"), "prose was eaten: {out}");
    assert_eq!(
        n,
        RefCounts {
            // 5 wiki payloads + 3 markdown destinations. The unterminated `[[`
            // is prose and is not a reference anyone looked at.
            seen: 8,
            rewritten: 6,
        },
        "the reference counts do not match what changed: {out}"
    );
}

/// The denominator is the count of what the rewriter SAW, so it has to be right
/// on the pages where nothing moved — which is most of them.
///
/// This is the shape of the bug the pair exists to make visible: a report of
/// "0 rewritten" over 4 references seen is a merge that carried a page whole,
/// and "0 rewritten" over 0 seen is a rewriter that never looked. The two used
/// to be indistinguishable, because an empty rename map skipped the scan.
#[test]
fn a_page_nothing_renamed_still_reports_the_references_it_was_read_for() {
    let body = "\
[[IL-6]] and [[knowledge/molecule/il-6.md|the alias]] and [[treats:: IL-6 | k=v]]\n\
[a link](knowledge/molecule/il-6.md)\n";
    let (out, n) = rewrite_body(body, &Renames::default());

    assert_eq!(out, body, "an empty rename map changed a byte: {out}");
    assert_eq!(
        n,
        RefCounts {
            seen: 4,
            rewritten: 0
        }
    );
}

/// A page whose frontmatter does not parse is carried rather than rejected
/// (DR-7) — and it is carried *without* a guessed rewrite of the part that could
/// not be read.
#[tokio::test]
async fn a_page_with_unreadable_frontmatter_is_carried_and_not_guessed_at() {
    let (_dir, svc) = service();
    let dst = base(&svc, "dst");
    let src = base(&svc, "src");
    // An unterminated frontmatter block: `frontmatter::split` errors rather than
    // reading it as prose, which is the case DR-7 is stated about.
    let broken = "---\ntype: Molecule\nidentifier: IL-6\n\nno closing delimiter\n";
    std::fs::create_dir_all(src.join("knowledge")).unwrap();
    std::fs::write(src.join("knowledge/broken.md"), broken).unwrap();
    GitRepo::open(&src)
        .unwrap()
        .commit_all(ChangeKind::Manual, "broken", None)
        .unwrap();

    svc.merge_bases("dst", "src", &MergeAuthority::User(&user()), false)
        .await
        .unwrap();

    assert_eq!(
        std::fs::read_to_string(dst.join("knowledge/broken.md")).unwrap(),
        broken,
        "an unparseable page was rewritten by guesswork instead of carried"
    );
}

/// The merged base gains an `index.md` section naming what arrived. It is a
/// section of its own rather than entries folded into the destination's type
/// headings, because *which* heading a page belongs under is the destination's
/// own vocabulary and deciding that is judgement work.
#[tokio::test]
async fn the_index_gains_a_section_naming_what_arrived() {
    let (_dir, svc) = service();
    let dst = base(&svc, "dst");
    let src = base(&svc, "src");
    put_page(
        &src,
        "knowledge/molecule/il-6.md",
        &page("Molecule", "IL-6", "b"),
    );

    svc.merge_bases("dst", "src", &MergeAuthority::User(&user()), false)
        .await
        .unwrap();

    let index = std::fs::read_to_string(dst.join("index.md")).unwrap();
    assert!(index.contains("# Merged from src"), "{index}");
    assert!(
        index.contains("* [IL-6](knowledge/molecule/il-6.md)"),
        "{index}"
    );
    // …and the change log says a merge happened, with the numbers.
    let log = std::fs::read_to_string(dst.join("log.md")).unwrap();
    assert!(log.contains("merge src"), "{log}");
    assert!(log.contains("+1 pages"), "{log}");
}

/// A page with no frontmatter block at all — a legacy page, or one a human wrote
/// by hand — is carried, keeps its body verbatim, and gains a block holding only
/// the provenance stamp.
///
/// The pair of assertions is the point: the body must survive byte-for-byte
/// (this is user prose, and re-serialising YAML must not reach it), and the
/// source's own file must be untouched (the change belongs to the copy).
#[tokio::test]
async fn a_page_with_no_frontmatter_keeps_its_body_and_gains_only_the_provenance_stamp() {
    let (_dir, svc) = service();
    let dst = base(&svc, "dst");
    let src = base(&svc, "src");
    let body = "# Hand-written\n\nNo frontmatter here, and a [[link]] to nothing.\n";
    put_page(&src, "knowledge/plain.md", body);
    let src_before = fingerprint(&src);

    svc.merge_bases("dst", "src", &MergeAuthority::User(&user()), false)
        .await
        .unwrap();

    let carried = parse(&dst, "knowledge/plain.md");
    assert_eq!(carried.body, body, "the body was reformatted");
    assert!(carried.had_frontmatter, "the stamp did not produce a block");
    assert_eq!(
        carried.doc.extra.len(),
        1,
        "the merge invented frontmatter beyond the stamp: {:?}",
        carried.doc.extra
    );
    assert_eq!(
        carried
            .doc
            .extra
            .get(MERGED_FROM_KEY)
            .and_then(|v| v.get("path"))
            .and_then(|v| v.as_str()),
        Some("knowledge/plain.md")
    );
    assert_eq!(
        fingerprint(&src),
        src_before,
        "the source's own copy gained the stamp too"
    );
}

/// Merging A, then B, then A again puts the third merge's pages under **A's**
/// heading, not under B's.
///
/// ⚠ This is a regression test for a bug the first implementation had: it found
/// A's heading already present, skipped creating it, and then appended at the
/// end of the file — which is inside whatever section happens to be last. The
/// symptom is silent and only appears with three merges from two sources, so a
/// two-base fixture cannot see it.
#[tokio::test]
async fn a_second_merge_from_the_same_base_lists_under_that_bases_own_heading() {
    let (_dir, svc) = service();
    let dst = base(&svc, "dst");
    let a = base(&svc, "alpha");
    let b = base(&svc, "beta");
    put_page(&a, "knowledge/a1.md", &page("Concept", "A1", "b"));
    put_page(&b, "knowledge/b1.md", &page("Concept", "B1", "b"));

    for source in ["alpha", "beta"] {
        svc.merge_bases("dst", source, &MergeAuthority::User(&user()), false)
            .await
            .unwrap();
    }
    put_page(&a, "knowledge/a2.md", &page("Concept", "A2", "b"));
    svc.merge_bases("dst", "alpha", &MergeAuthority::User(&user()), false)
        .await
        .unwrap();

    let index = std::fs::read_to_string(dst.join("index.md")).unwrap();
    let section_of = |bullet: &str| -> String {
        let mut current = String::new();
        for line in index.lines() {
            if line.starts_with("# ") {
                current = line.to_string();
            }
            if line.contains(bullet) {
                return current;
            }
        }
        panic!("{bullet} is not in the index:\n{index}");
    };
    assert_eq!(section_of("knowledge/a1.md"), "# Merged from alpha");
    assert_eq!(
        section_of("knowledge/a2.md"),
        "# Merged from alpha",
        "the second alpha merge listed its page under a later source's heading:\n{index}"
    );
    assert_eq!(section_of("knowledge/b1.md"), "# Merged from beta");
    // …and re-listing is idempotent: a1 appears once, not once per merge.
    assert_eq!(
        index.matches("knowledge/a1.md").count(),
        1,
        "a page was listed twice:\n{index}"
    );
}

// ── the profile gate ────────────────────────────────────────────────────────

/// A base in an explicit profile, at the OKF generation.
fn base_in(svc: &KnowledgeService, id: &str, format: crate::knowledge::types::KbFormat) -> PathBuf {
    svc.create_base_in(id, id, None, format).unwrap();
    svc.root().join(id)
}

/// A **legacy** base: created at the current generation and then stamped back to
/// the one below it, which is what `Manifest::profile` reads to answer `None`.
///
/// Written this way rather than by hand-building a directory because the merge
/// reads a great deal more of a base than its manifest — a fixture missing the
/// git repo or the scaffold would fail for a reason that has nothing to do with
/// the gate under test, and would keep failing after the gate was removed.
fn legacy_base(svc: &KnowledgeService, id: &str) -> PathBuf {
    let root = base(svc, id);
    let mut m = crate::knowledge::manifest::load(&root).unwrap();
    m.schema_version = crate::knowledge::types::AUTOMATIC_SCHEMA_CEILING;
    crate::knowledge::manifest::save(&root, &m).unwrap();
    assert!(
        crate::knowledge::manifest::load(&root)
            .unwrap()
            .is_legacy_format(),
        "the fixture did not produce a legacy base"
    );
    root
}

/// The table in [`super::assert_profiles_merge`], driven at every corner.
///
/// A pure-function test beside the on-disk ones for the reason
/// `session_reach::refuse_unless_reachable` gives for the same shape: the
/// on-disk tests can only afford to visit two or three cells, and the cell a
/// future edit gets wrong is whichever one nobody wrote a base for.
#[test]
fn the_profile_table_allows_exactly_the_directions_that_preserve_conformance() {
    use crate::knowledge::types::KbFormat::{Biookf, Okf};
    // (destination, source, allowed)
    let table = [
        (None, None, true),
        (Some(Okf), Some(Okf), true),
        (Some(Biookf), Some(Biookf), true),
        // The one asymmetric cell: a BioOKF bundle is a valid OKF bundle, so it
        // may go down the ladder and never up it.
        (Some(Okf), Some(Biookf), true),
        (Some(Biookf), Some(Okf), false),
        // Legacy is a different vocabulary, not a looser OKF, so neither
        // direction composes.
        (Some(Okf), None, false),
        (Some(Biookf), None, false),
        (None, Some(Okf), false),
        (None, Some(Biookf), false),
    ];
    for (destination, source, allowed) in table {
        assert_eq!(
            super::profiles_compose(destination, source),
            allowed,
            "merging {source:?} into {destination:?}"
        );
    }
}

/// The defect: a legacy base's `title`/`kind` pages carried into a base whose
/// manifest declares `format: biookf`, where every BioOKF rule is then applied
/// to them with no undo.
///
/// The dry run is asserted first and for the same weight as the merge, because
/// `POST /bases/{id}/merge` defaults `dry_run` to **true** — a gate that only
/// fired on the apply would let the preview walk the user through, page by page,
/// an operation that must never happen.
#[tokio::test]
async fn a_legacy_base_is_refused_a_merge_into_a_biookf_base_in_both_modes() {
    let (_dir, svc) = service();
    let dst = base_in(&svc, "dst", crate::knowledge::types::KbFormat::Biookf);
    let src = legacy_base(&svc, "src");
    put_page(
        &src,
        "knowledge/notes/hrv.md",
        "---\ntitle: HRV\nkind: note\n---\n\nbody\n",
    );

    let before = fingerprint(&dst);
    let src_before = fingerprint(&src);
    for dry_run in [true, false] {
        let err = svc
            .merge_bases("dst", "src", &MergeAuthority::User(&user()), dry_run)
            .await
            .expect_err("an incompatible merge was allowed");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("legacy (pre-OKF)") && msg.contains("biookf"),
            "the refusal must name both profiles (dry_run={dry_run}): {msg}"
        );
    }
    assert_eq!(
        fingerprint(&dst),
        before,
        "the refused merge still wrote to the destination"
    );
    assert_eq!(
        fingerprint(&src),
        src_before,
        "the refused merge wrote to the source, which is only ever read"
    );
}

/// The other refused direction, and the one a user is most likely to try: a
/// general-purpose base merged into a biomedical one. A plain-OKF page states no
/// BioOKF node type and no per-edge provenance triplet, so the destination's own
/// contract would be broken by pages that were never written to it.
#[tokio::test]
async fn an_okf_base_is_refused_a_merge_into_a_biookf_base() {
    let (_dir, svc) = service();
    base_in(&svc, "dst", crate::knowledge::types::KbFormat::Biookf);
    let src = base_in(&svc, "src", crate::knowledge::types::KbFormat::Okf);
    put_page(&src, "knowledge/concept/a.md", &page("Concept", "A", "b"));

    let err = svc
        .merge_bases("dst", "src", &MergeAuthority::User(&user()), true)
        .await
        .expect_err("OKF into BioOKF was allowed");
    let msg = format!("{err:#}");
    // ⚠ Matched on the whole clause, not on the two format words. `"biookf"`
    // CONTAINS `"okf"`, so `msg.contains("okf") && msg.contains("biookf")` is
    // satisfied by the second conjunct alone: a refusal naming `biookf` twice —
    // or naming only the destination and never the source — passes it, and the
    // assertion cannot tell that from the message it means to require. The
    // clauses below also pin each format to its own SLOT, which is the half that
    // makes the refusal actionable: "your source is X, this base is Y" is
    // advice, and the same two words in the wrong order is not.
    assert!(
        msg.contains("it is written in the okf format"),
        "the refusal must name the SOURCE's profile: {msg}"
    );
    assert!(
        msg.contains("this base is biookf"),
        "the refusal must name the DESTINATION's profile: {msg}"
    );
}

/// …and the direction that is safe stays open. Refusing this one would be the
/// opposite failure — a user with a BioOKF base and a general one told to keep
/// two graphs forever, for a merge whose carried pages already satisfy every
/// rule the destination will check them against.
#[tokio::test]
async fn a_biookf_base_merges_into_an_okf_base() {
    let (_dir, svc) = service();
    let dst = base_in(&svc, "dst", crate::knowledge::types::KbFormat::Okf);
    let src = base_in(&svc, "src", crate::knowledge::types::KbFormat::Biookf);
    put_page(
        &src,
        "knowledge/molecule/il-6.md",
        &page("Molecule", "IL-6", "b"),
    );

    let report = svc
        .merge_bases("dst", "src", &MergeAuthority::User(&user()), false)
        .await
        .expect("BioOKF into OKF is the subset direction and must be allowed");
    assert_eq!(report.pages_carried.len(), 1);
    assert!(dst.join("knowledge/molecule/il-6.md").exists());
}
