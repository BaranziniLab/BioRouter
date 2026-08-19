//! KB-to-KB merge — the **deterministic** half.
//!
//! # The dead end this closes
//!
//! A collaborator sends a `.brkb`; [`crate::knowledge::brkb::import`] mints a
//! **fresh** id for it (its collision loop is explicitly written to land on one,
//! so an import can never re-tier an existing base). The user now owns two bases
//! describing the same domain and no path to one graph. This module is that
//! path.
//!
//! # What is here and what is deliberately not
//!
//! The reference implementation is BioOKF's `bokf-core::merge` plus the
//! `biookf-merge` skill, and the skill's loop has two halves:
//!
//! | half | example | where it lives |
//! | --- | --- | --- |
//! | **mechanical** | dedup a raw source by content hash, rename on collision, rewrite every reference to what was renamed, carry over what does not collide | **here** |
//! | **judgement** | "is the SKB's `IL-6` the same concept as the MKB's `IL6`?", collapsing a true match, harmonising prose and subtype names | a macro, a later pass |
//!
//! Only the first half ships. The consequence is stated rather than hidden: an
//! identifier that exists in both bases is **not collapsed** — the incoming one
//! is renamed and every reference to it is rewritten, so the merged base holds
//! two pages that a human (or a later macro) can decide about. That is the
//! conservative direction: a wrong collapse destroys a page, a wrong rename
//! leaves two pages and a rename record.
//!
//! # The governing rule: the destination is canonical
//!
//! Its identifiers, its paths, its `raw/` locations win on **every** collision;
//! the incoming side is what gets renamed. [`snapshot`] captures the
//! destination's identifier→path set *before* the merge and [`verify_snapshot`]
//! confirms it is unchanged *after* — which is what makes "the target stayed
//! canonical" checkable rather than asserted. A violation aborts the
//! transaction.
//!
//! ⚠ The snapshot is held in memory across the transaction and never written to
//! disk. The reference persists it as `.bokf-premerge.json` at the bundle root;
//! here that file would land inside a git-tracked knowledge base and be packed
//! into the next `.brkb`, so the *only* thing the merge adds to the destination
//! tree is knowledge.
//!
//! # Copy, never move
//!
//! `bokf-core::merge_raw` **moves** the secondary's `raw/` directories with
//! `fs::rename`. This one copies, and the deviation is deliberate:
//!
//! 1. The whole merge is one transaction on the **destination's** git repo. The
//!    source has its own repo and is not in that transaction, so a move could
//!    not be rolled back and the atomicity promise below would be a lie.
//! 2. A BioRouter source base is a first-class object — registry entry, tier
//!    entry, session pointers, git history. Emptying its `raw/` would leave
//!    every one of *its* pages' `raw_source` dangling.
//! 3. Deleting a base is a separate, user-initiated action. A merge that
//!    silently destroyed one would be the least reversible operation in the
//!    system.
//!
//! So the source base is left byte-identical, and its tier is therefore never
//! lowered by construction — there is nothing to lower.
//!
//! # Atomicity
//!
//! Everything runs inside one [`crate::knowledge::git::Txn`] on the destination.
//! On any failure the destination is left byte-identical: files this merge
//! created are removed from the list it kept, and `abort_txn` restores the
//! tracked files it modified. The two are both needed — a page written and not
//! yet committed on the transaction branch is *untracked*, and a copied
//! `raw/<id>/original.pdf` is *gitignored* (`raw/*/original.*`), so neither is
//! reachable by a checkout however forceful.
//!
//! # What a merge does not carry
//!
//! `manifest.yaml`, `schema.md` and `log.md` stay the destination's own: the
//! first two *are* the destination's identity (its id, its profile, the contract
//! its sub-agent is taught from), and the third is a record of what happened to
//! **this** base — the source's history belongs to the source. What the merge
//! adds to `log.md` is one entry saying a merge happened.

use crate::knowledge::{
    caller::KbCaller,
    git::{GitRepo, Txn},
    links::{self, identity_key, link_key},
    okf::{frontmatter, model::ConceptDoc, model::Page},
    raw as raw_store, store,
    types::ChangeKind,
};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The frontmatter key a carried-over page gains, recording where it came from.
///
/// `br_`-prefixed like DR-3's `br_page_id`, and legal by construction: OKF §4.1
/// lets producers add any key and §11 forbids a consumer rejecting one, so a
/// bundle carrying it stays conformant and a round-trip preserves it (it rides
/// in `ConceptDoc::extra`).
///
/// It is written on **every** carried page, not only on renamed ones, because
/// "which of these pages came from the merge?" is the first question asked of a
/// merged base and reconstructing it from a report the user no longer has is not
/// an answer.
///
/// ⚠ **A page that had no frontmatter block gains one**, holding this key alone.
/// That is a real change and it is accepted rather than overlooked: DR-17/DR-22
/// keep a format migration off every automatic path, and this is not one — the
/// page is being *copied into a different base* by an explicit act, and the copy
/// is the only thing touched. The source's own file is left byte-identical. The
/// alternative — silently skipping the stamp on exactly the pages that carry the
/// least identifying metadata — leaves the merged base least traceable where it
/// most needs to be.
pub const MERGED_FROM_KEY: &str = "br_merged_from";

// ────────────────────────────────────────────────────────────────────────────
// Proof of user
// ────────────────────────────────────────────────────────────────────────────

/// Proof that a **human** asked for a merge. A ZST with a private field, so the
/// tuple literal is unavailable outside this module and
/// [`UserKbMerge::from_user_action`] is the only door.
///
/// It exists for the same reason [`crate::knowledge::tier_user::UserKbTierChange`]
/// does, and it is a **separate type on purpose**: one proof must not be
/// spendable on the other subject. A merge is not a declassification and a
/// declassification is not a merge.
///
/// Why a merge needs one at all: [`MergeAuthority::User`] skips the caller
/// barrier, and it is right to — the user can already read both bases from the
/// Knowledge view and through `GET /knowledge/bases/{id}/export`, which carries
/// no barrier either for exactly this reason (DR-14 governs what a **model** can
/// reach). A model must never be able to reach that branch, and a type it cannot
/// construct is the only form of "never" that survives a refactor.
///
/// ⚠ What Rust enforces is that the literal cannot be written elsewhere; what
/// caps the construction sites at one is
/// [`tests::the_merge_proof_of_user_is_constructed_in_exactly_one_place`].
pub struct UserKbMerge(());

impl UserKbMerge {
    /// Mint the proof. Call this **only** after `auth::user_action_proof` has
    /// returned `Proven`; the merge route is the sole call site.
    pub fn from_user_action() -> Self {
        Self(())
    }

    #[cfg(test)]
    pub(crate) fn for_test() -> Self {
        Self(())
    }
}

/// Who asked for this merge, and therefore which barrier applies.
///
/// One enum rather than two entry points, so the classification fold below
/// cannot be reached by a path that skipped the barrier: there is exactly one
/// merge function and this is its first argument.
pub enum MergeAuthority<'a> {
    /// A model asked. **Both** bases must be reachable from it (DR-17): you
    /// cannot merge a base you cannot read into a base you cannot write.
    Model(&'a KbCaller),
    /// A human asked, through a surface behind the user-action proof. No model
    /// is bound, so there is no tier to compare and no institution to cross; the
    /// classification fold still runs, and that is what keeps the model side
    /// honest afterwards.
    User(&'a UserKbMerge),
}

impl MergeAuthority<'_> {
    /// The barrier, for both bases. `Ok(())` permits.
    pub fn assert_may_merge(&self, root: &Path, destination: &str, source: &str) -> Result<()> {
        match self {
            // Destination first: it is the id the caller named as the subject,
            // and a caller that may not write it learns nothing about whether
            // the source exists.
            MergeAuthority::Model(caller) => {
                caller.assert_reachable(root, destination)?;
                caller.assert_reachable(root, source)
            }
            MergeAuthority::User(_) => Ok(()),
        }
    }

    /// The caller's own tier, for the classification fold. A user is not a model
    /// and contributes nothing on this axis — what raises the destination on the
    /// user's path is the **source base's** tier, which is the half that matters.
    pub fn caller_is_private(&self) -> bool {
        match self {
            MergeAuthority::Model(caller) => caller.is_private(),
            MergeAuthority::User(_) => false,
        }
    }

    /// The caller's institution, for the owner union (DR-26). `Unstated`
    /// contributes nothing, for the reason `tier::raise_affiliation_unlocked`
    /// gives: recording a sentinel owner makes a base permanently unreachable
    /// with no declassification path.
    pub fn caller_affiliation(&self) -> crate::knowledge::affiliation::CallerAffiliation {
        match self {
            MergeAuthority::Model(caller) => caller.affiliation().clone(),
            MergeAuthority::User(_) => crate::knowledge::affiliation::CallerAffiliation::Unstated,
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// The report
// ────────────────────────────────────────────────────────────────────────────

#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rename {
    pub from: String,
    pub to: String,
}

#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawDedup {
    /// The raw-source id in the source base.
    pub source_id: String,
    /// The destination raw-source id it was found to be byte-identical to.
    pub matched: String,
    pub sha256: String,
}

#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CarriedPage {
    pub source_path: String,
    pub destination_path: String,
    /// The page's identifier as the source wrote it, when it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identifier: Option<String>,
    /// Set only when the identifier collided with the destination's and was
    /// renamed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub renamed_identifier: Option<String>,
}

/// What the merge did, or — for a dry run — what it *would* do.
///
/// The dry run and the merge produce this from the **same** [`plan`] call, so
/// the preview cannot describe a different operation from the one that runs.
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MergeReport {
    pub destination_kb_id: String,
    pub source_kb_id: String,
    /// True when nothing was written.
    pub dry_run: bool,
    /// Raw sources copied into the destination, `from` → `to` (equal when there
    /// was no collision).
    pub raw_copied: Vec<Rename>,
    /// Raw sources already present in the destination, matched on the sha256 in
    /// `raw/<id>/meta.yaml`. Not copied; every reference to them is repointed at
    /// the destination's copy.
    pub raw_deduped: Vec<RawDedup>,
    pub pages_carried: Vec<CarriedPage>,
    /// Identifier collisions. The **incoming** side was renamed; the
    /// destination's identifier is untouched.
    pub identifiers_renamed: Vec<Rename>,
    /// Page-path collisions, as logical paths.
    pub paths_renamed: Vec<Rename>,
    /// How many references were repointed — edge `object`, edge
    /// `primary_source`, `raw_source` paths and body links.
    pub references_rewritten: usize,
    /// How many references the rewriter **looked at**, of the same four kinds.
    ///
    /// The denominator, and it is here because its absence hid a corruption. A
    /// preview that says only "1 identifier renamed, 3 references rewritten"
    /// reads as complete, and read exactly that way while plain `[[Name]]`
    /// links went through a map that could not rename them — silently
    /// retargeted at the destination's own page of that name. `seen -
    /// rewritten` is what the merge saw and deliberately left alone, so a
    /// reader can ask why a number is large before approving the least
    /// reversible operation in the subsystem.
    #[serde(default)]
    pub references_seen: usize,
    /// The destination's page count before the merge — the size of the set
    /// [`verify_snapshot`] checked.
    pub destination_pages_before: usize,
    /// Empty when the destination stayed canonical. A non-empty list aborts the
    /// transaction, so it is only ever populated on a failure path or a dry run.
    pub canonical_violations: Vec<String>,
    /// The destination's tier after the merge, as the word the store holds.
    pub destination_tier: String,
    /// Institutions added to the destination by the fold (DR-26).
    pub owners_added: Vec<String>,
    /// The squash commit on the destination's `main`, or `None` for a dry run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_sha: Option<String>,
}

// ────────────────────────────────────────────────────────────────────────────
// The snapshot: what "the destination stayed canonical" means
// ────────────────────────────────────────────────────────────────────────────

/// The destination as it was before the merge: every identifier it declares,
/// every page path it holds, and every raw-source id it owns.
///
/// Three sets and not the reference's one, because BioRouter has pages the
/// reference's `Bundle` does not model. A legacy or plain-OKF page may declare
/// no `identifier` at all, so an identifier-only snapshot would say nothing
/// about whether it survived; and `raw/` is where a merge does most of its
/// moving, so a destination raw id that got renamed has to be catchable too.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
    /// `identity_key(identifier)` → (identifier as written, logical path).
    identifiers: BTreeMap<String, (String, String)>,
    /// Every `knowledge/…` logical path.
    paths: BTreeSet<String>,
    /// Every `raw/<id>` directory name.
    raw_ids: BTreeSet<String>,
}

impl Snapshot {
    pub fn page_count(&self) -> usize {
        self.paths.len()
    }
}

/// Capture the destination's canonical set.
pub fn snapshot(kb_root: &Path) -> Result<Snapshot> {
    let mut snap = Snapshot::default();
    for page in store::list_pages(kb_root, None)? {
        snap.paths.insert(page.path.clone());
        let text = std::fs::read_to_string(kb_root.join(&page.path))
            .with_context(|| format!("reading {}", page.path))?;
        if let Some(id) = page_identifier(&text) {
            snap.identifiers
                .entry(identity_key(&id))
                .or_insert((id, page.path.clone()));
        }
    }
    snap.raw_ids = raw_ids(kb_root);
    Ok(snap)
}

/// Compare the destination against a pre-merge [`snapshot`] and report every way
/// it stopped being canonical: an identifier that was removed or moved, a page
/// that vanished, a raw source that was renamed out from under its references.
///
/// An empty list is the pass. Anything else aborts the transaction, so a merge
/// can never *silently* rewrite the base it was merging into.
pub fn verify_snapshot(kb_root: &Path, before: &Snapshot) -> Result<Vec<String>> {
    let now = snapshot(kb_root)?;
    let mut issues = Vec::new();
    for (key, (identifier, path)) in &before.identifiers {
        match now.identifiers.get(key) {
            None => issues.push(format!(
                "the destination's identifier `{identifier}` was removed or renamed; \
                 a merge renames the INCOMING side, never the destination"
            )),
            Some((_, now_path)) if now_path != path => issues.push(format!(
                "the destination's identifier `{identifier}` moved `{path}` → `{now_path}`; \
                 destination paths must stay stable through a merge"
            )),
            _ => {}
        }
    }
    for path in &before.paths {
        if !now.paths.contains(path) {
            issues.push(format!("the destination's page `{path}` no longer exists"));
        }
    }
    for id in &before.raw_ids {
        if !now.raw_ids.contains(id) {
            issues.push(format!(
                "the destination's raw source `raw/{id}` no longer exists; \
                 a merge renames the INCOMING raw sources, never the destination's"
            ));
        }
    }
    Ok(issues)
}

/// A page's identifier, through the two rungs of DR-3's ladder that a page can
/// answer on its own (`identifier`, then the deprecated `title` alias).
///
/// `None` for a page that declares neither *and* for a page whose frontmatter
/// does not parse — the two are the same answer to this question, and DR-7
/// forbids turning the second into a rejection.
fn page_identifier(text: &str) -> Option<String> {
    let page = Page::parse(text).ok()?;
    page.doc.primary_key().map(ToOwned::to_owned)
}

fn raw_ids(kb_root: &Path) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    let Ok(entries) = std::fs::read_dir(kb_root.join("raw")) else {
        return ids;
    };
    for entry in entries.flatten() {
        if entry.path().is_dir() {
            ids.insert(entry.file_name().to_string_lossy().to_string());
        }
    }
    ids
}

// ────────────────────────────────────────────────────────────────────────────
// The plan
// ────────────────────────────────────────────────────────────────────────────

/// One page to carry over, with its destination content already computed.
///
/// The content is rewritten during planning, not during application, so the dry
/// run exercises every rewrite the real merge performs. A rewrite that panics or
/// errors does so in the preview, where nothing has been written.
#[derive(Debug, Clone)]
struct PlannedPage {
    source_path: String,
    destination_path: String,
    identifier: Option<String>,
    renamed_identifier: Option<String>,
    content: String,
}

/// Everything the merge will do, computed without touching the destination.
pub struct MergePlan {
    source_kb_id: String,
    raw_copies: Vec<Rename>,
    raw_deduped: Vec<RawDedup>,
    pages: Vec<PlannedPage>,
    identifiers_renamed: Vec<Rename>,
    paths_renamed: Vec<Rename>,
    references: RefCounts,
    snapshot: Snapshot,
    /// Ways this plan would stop the destination being canonical, found before
    /// anything is written. Empty is the pass, and an apply refuses on anything
    /// else.
    ///
    /// The pair with [`verify_snapshot`] is deliberate and they are not the same
    /// check. This one asks *would the plan overwrite the destination*, and it is
    /// the only one a **dry run** can answer — nothing has been written, so a
    /// post-merge comparison there is vacuously green and would report a
    /// dangerous plan as safe. `verify_snapshot` asks *did the write do what the
    /// plan said*, which catches an application bug the planner cannot see.
    plan_violations: Vec<String>,
}

impl MergePlan {
    /// The report, minus the fields only an applied merge can fill.
    fn report(&self, destination_kb_id: &str, dry_run: bool) -> MergeReport {
        MergeReport {
            destination_kb_id: destination_kb_id.to_string(),
            source_kb_id: self.source_kb_id.clone(),
            dry_run,
            raw_copied: self.raw_copies.clone(),
            raw_deduped: self.raw_deduped.clone(),
            pages_carried: self
                .pages
                .iter()
                .map(|p| CarriedPage {
                    source_path: p.source_path.clone(),
                    destination_path: p.destination_path.clone(),
                    identifier: p.identifier.clone(),
                    renamed_identifier: p.renamed_identifier.clone(),
                })
                .collect(),
            identifiers_renamed: self.identifiers_renamed.clone(),
            paths_renamed: self.paths_renamed.clone(),
            references_rewritten: self.references.rewritten,
            references_seen: self.references.seen,
            destination_pages_before: self.snapshot.page_count(),
            canonical_violations: self.plan_violations.clone(),
            destination_tier: String::new(),
            owners_added: Vec::new(),
            commit_sha: None,
        }
    }
}

/// Every way this plan would write over something the destination already owns.
///
/// It re-derives the answer from the finished plan rather than trusting the
/// bookkeeping that produced it: [`plan_pages`] and [`plan_raw`] each track a
/// `taken` set as they go, and a `taken` set that is seeded wrong, or updated on
/// one branch and not the other, produces a plan that looks internally
/// consistent and lands on a destination page. This asks the destination.
fn plan_violations(
    snapshot: &Snapshot,
    pages: &[PlannedPage],
    raw_copies: &[Rename],
) -> Vec<String> {
    let mut issues = Vec::new();
    for page in pages {
        if snapshot.paths.contains(&page.destination_path) {
            issues.push(format!(
                "carrying `{}` would overwrite the destination's own page `{}`",
                page.source_path, page.destination_path
            ));
        }
        let landing = page
            .renamed_identifier
            .as_ref()
            .or(page.identifier.as_ref());
        if let Some(id) = landing {
            if let Some((existing, path)) = snapshot.identifiers.get(&identity_key(id)) {
                issues.push(format!(
                    "carrying `{}` would land the identifier `{id}` on the destination's \
                     `{existing}` ({path}); the incoming side must be renamed instead",
                    page.source_path
                ));
            }
        }
    }
    for copy in raw_copies {
        if snapshot.raw_ids.contains(&copy.to) {
            issues.push(format!(
                "copying `raw/{}` would overwrite the destination's own `raw/{}`",
                copy.from, copy.to
            ));
        }
    }
    issues
}

/// The rename maps every reference in a carried page is rewritten through.
#[derive(Debug, Default)]
struct Renames {
    /// `identity_key(old identifier)` → new identifier.
    identifiers: BTreeMap<String, String>,
    /// `link_key(old logical path)` → new file stem.
    page_stems: BTreeMap<String, String>,
    /// source raw id → destination raw id.
    raw_ids: BTreeMap<String, String>,
}

/// Compute the whole merge without writing anything.
///
/// The one entry point for both the dry run and the merge, so "the preview
/// describes the operation that will run" is a property of the code rather than
/// a promise in a doc comment.
pub fn plan(dst_root: &Path, src_root: &Path, src_kb_id: &str) -> Result<MergePlan> {
    let snapshot = snapshot(dst_root)?;
    let raw = plan_raw(dst_root, src_root, src_kb_id)?;
    let (mut pages, identifiers_renamed, paths_renamed) =
        plan_pages(&snapshot, src_root, src_kb_id)?;

    let mut renames = Renames {
        raw_ids: raw.id_map,
        ..Renames::default()
    };
    for r in &identifiers_renamed {
        // First wins. Two source pages whose identifiers reduce to the same
        // `identity_key` are already indistinguishable to the graph deriver's
        // own index (`links::LinkIndex`), so inventing a second answer here
        // would make the merge disagree with the picture it produces.
        renames
            .identifiers
            .entry(identity_key(&r.from))
            .or_insert_with(|| r.to.clone());
    }
    for r in &paths_renamed {
        renames
            .page_stems
            .insert(link_key(&r.from), file_stem(&r.to).to_string());
    }
    // The raw map holds an entry for every source raw id, including the ones
    // whose id did not change; rewriting through those is a no-op, so they are
    // dropped rather than counted as work.
    renames.raw_ids.retain(|from, to| from != to);

    let references = rewrite_pages(&mut pages, &renames, src_kb_id)?;
    let violations = plan_violations(&snapshot, &pages, &raw.copies);

    Ok(MergePlan {
        source_kb_id: src_kb_id.to_string(),
        raw_copies: raw.copies,
        raw_deduped: raw.deduped,
        pages,
        identifiers_renamed,
        paths_renamed,
        references,
        snapshot,
        plan_violations: violations,
    })
}

// ── raw/ ────────────────────────────────────────────────────────────────────

struct RawPlan {
    id_map: BTreeMap<String, String>,
    copies: Vec<Rename>,
    deduped: Vec<RawDedup>,
}

/// Dedup on the sha256 **already in** `raw/<id>/meta.yaml`, rename on id
/// collision, and record the remapping so every `raw/…` reference can be
/// rewritten through it.
///
/// The hash is read, never recomputed: it is written at ingest by
/// `raw::write_raw` over the original bytes, and recomputing it here would
/// answer a different question — the derived `source.md` can differ between two
/// bases that ingested the same PDF with different converter versions, while the
/// bytes that were ingested are the thing "the same source" means.
///
/// A source directory with no readable `meta.yaml` has no hash and is therefore
/// never deduped: it is copied, under a renamed id if it collides. Treating
/// "unreadable" as "not a duplicate" is the direction that keeps content.
fn plan_raw(dst_root: &Path, src_root: &Path, src_kb_id: &str) -> Result<RawPlan> {
    let mut plan = RawPlan {
        id_map: BTreeMap::new(),
        copies: Vec::new(),
        deduped: Vec::new(),
    };
    let by_sha = raw_sha_index(dst_root);
    let mut taken = raw_ids(dst_root);

    for id in raw_ids(src_root) {
        if let Some(meta) = raw_store::read_meta(src_root, &id).ok().filter(|m| {
            // A blank hash is not a hash. Two sources that both failed to record
            // one are not the same source, and collapsing them would be the one
            // dedup that silently deletes content.
            !m.sha256.trim().is_empty()
        }) {
            if let Some(existing) = by_sha.get(&meta.sha256) {
                plan.deduped.push(RawDedup {
                    source_id: id.clone(),
                    matched: existing.clone(),
                    sha256: meta.sha256.clone(),
                });
                plan.id_map.insert(id, existing.clone());
                continue;
            }
        }
        let target = disambiguate_id(&id, src_kb_id, |candidate| taken.contains(candidate));
        taken.insert(target.clone());
        plan.copies.push(Rename {
            from: id.clone(),
            to: target.clone(),
        });
        plan.id_map.insert(id, target);
    }
    Ok(plan)
}

/// `sha256` → raw id, for the destination. Directories whose `meta.yaml` cannot
/// be read simply do not participate in dedup.
fn raw_sha_index(kb_root: &Path) -> BTreeMap<String, String> {
    let mut by_sha = BTreeMap::new();
    for id in raw_ids(kb_root) {
        if let Ok(meta) = raw_store::read_meta(kb_root, &id) {
            if !meta.sha256.trim().is_empty() {
                by_sha.entry(meta.sha256).or_insert(id);
            }
        }
    }
    by_sha
}

// ── knowledge/ ──────────────────────────────────────────────────────────────

type PagePlan = (Vec<PlannedPage>, Vec<Rename>, Vec<Rename>);

/// Decide where every source page lands and what it is called there.
///
/// Two independent collisions, deliberately kept apart: a **path** collision is
/// two files wanting the same name, and an **identifier** collision is two pages
/// claiming to be the same concept. A base can have either without the other —
/// two `knowledge/entities/x.md` files declaring different identifiers, or one
/// identifier written at two different paths — and collapsing the two checks
/// would rename on one and miss the other.
fn plan_pages(before: &Snapshot, src_root: &Path, src_kb_id: &str) -> Result<PagePlan> {
    let mut pages = Vec::new();
    let mut identifiers_renamed = Vec::new();
    let mut paths_renamed = Vec::new();
    let mut taken_paths = before.paths.clone();
    let mut taken_identities: BTreeSet<String> = before.identifiers.keys().cloned().collect();

    for page in store::list_pages(src_root, None)? {
        let content = std::fs::read_to_string(src_root.join(&page.path))
            .with_context(|| format!("reading {} from the source base", page.path))?;
        let identifier = page_identifier(&content);

        let renamed_identifier = identifier.as_ref().and_then(|id| {
            let key = identity_key(id);
            if !taken_identities.contains(&key) {
                taken_identities.insert(key);
                return None;
            }
            let next = disambiguate_identifier(id, src_kb_id, |candidate| {
                taken_identities.contains(&identity_key(candidate))
            });
            taken_identities.insert(identity_key(&next));
            identifiers_renamed.push(Rename {
                from: id.clone(),
                to: next.clone(),
            });
            Some(next)
        });

        let destination_path = disambiguate_path(&page.path, src_kb_id, |candidate| {
            taken_paths.contains(candidate)
        });
        taken_paths.insert(destination_path.clone());
        if destination_path != page.path {
            paths_renamed.push(Rename {
                from: page.path.clone(),
                to: destination_path.clone(),
            });
        }

        pages.push(PlannedPage {
            source_path: page.path,
            destination_path,
            identifier,
            renamed_identifier,
            content,
        });
    }
    Ok((pages, identifiers_renamed, paths_renamed))
}

// ────────────────────────────────────────────────────────────────────────────
// Disambiguation
// ────────────────────────────────────────────────────────────────────────────

/// `<preferred>`, then `<preferred>-<src-kb>`, then `<preferred>-<src-kb>-2`, …
///
/// The source base's id is in the suffix rather than a bare counter because the
/// user reading `raw/chen-2020-omop` afterwards can see where it came from,
/// which a `raw/chen-2020-2` cannot tell them.
fn disambiguate_id(preferred: &str, src_kb_id: &str, taken: impl Fn(&str) -> bool) -> String {
    disambiguate(preferred, taken, |n| match n {
        0 => preferred.to_string(),
        1 => format!("{preferred}-{src_kb_id}"),
        n => format!("{preferred}-{src_kb_id}-{n}"),
    })
}

/// The same ladder for an identifier, which is prose and takes the readable
/// form: `IL-6`, `IL-6 (omop)`, `IL-6 (omop 2)`.
fn disambiguate_identifier(
    preferred: &str,
    src_kb_id: &str,
    taken: impl Fn(&str) -> bool,
) -> String {
    disambiguate(preferred, taken, |n| match n {
        0 => preferred.to_string(),
        1 => format!("{preferred} ({src_kb_id})"),
        n => format!("{preferred} ({src_kb_id} {n})"),
    })
}

/// The same ladder for a logical page path: only the file stem moves, so the
/// page stays in the directory its `type` put it in.
fn disambiguate_path(logical: &str, src_kb_id: &str, taken: impl Fn(&str) -> bool) -> String {
    let (dir, file) = match logical.rsplit_once('/') {
        Some((dir, file)) => (dir.to_string(), file.to_string()),
        None => (String::new(), logical.to_string()),
    };
    let stem = file
        .strip_suffix(".md")
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| file.clone());
    let rebuild = |stem: &str| {
        if dir.is_empty() {
            format!("{stem}.md")
        } else {
            format!("{dir}/{stem}.md")
        }
    };
    disambiguate(logical, taken, |n| match n {
        0 => rebuild(&stem),
        1 => rebuild(&format!("{stem}-{src_kb_id}")),
        n => rebuild(&format!("{stem}-{src_kb_id}-{n}")),
    })
}

/// Walk a candidate ladder until one is free. Bounded by `u32::MAX` in
/// principle; in practice the first or second rung always answers.
fn disambiguate(
    preferred: &str,
    taken: impl Fn(&str) -> bool,
    candidate: impl Fn(u32) -> String,
) -> String {
    for n in 0..1_000 {
        let c = candidate(n);
        if !taken(&c) {
            return c;
        }
    }
    // Unreachable short of a thousand collisions on one name. A uuid rather than
    // a panic: a merge that gives up loudly is worse than one that produces an
    // ugly but correct id, and the report names it either way.
    format!("{preferred}-{}", uuid::Uuid::new_v4())
}

fn file_stem(logical: &str) -> &str {
    let file = logical.rsplit('/').next().unwrap_or(logical);
    file.strip_suffix(".md").unwrap_or(file)
}

// ────────────────────────────────────────────────────────────────────────────
// Rewriting: nothing dangles
// ────────────────────────────────────────────────────────────────────────────

/// What the rewriter looked at, and how much of it moved.
///
/// The pair and not the tally alone. `references_rewritten` on its own is a
/// numerator with no denominator: "3 references repointed" reads as *done* on a
/// bundle where thirty more were seen and left, and that is precisely the shape
/// of the reader that missed a whole link grammar — the preview said a rename
/// happened and a nonzero number of references moved, while plain `[[Name]]`
/// links were being silently retargeted at the destination's pages. `seen`
/// makes the gap visible: it is what the rewriter *examined*, so
/// `seen - rewritten` is what it deliberately left alone.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct RefCounts {
    /// References examined against the rename maps.
    seen: usize,
    /// Of those, the ones repointed.
    rewritten: usize,
}

impl RefCounts {
    /// Record one reference the rewriter looked at, and whether it moved.
    fn saw(&mut self, rewritten: bool) {
        self.seen += 1;
        self.rewritten += usize::from(rewritten);
    }

    fn add(&mut self, other: RefCounts) {
        self.seen += other.seen;
        self.rewritten += other.rewritten;
    }
}

/// Rewrite every reference in every carried page, and stamp its provenance.
/// Returns what was seen and what was repointed.
fn rewrite_pages(
    pages: &mut [PlannedPage],
    renames: &Renames,
    src_kb_id: &str,
) -> Result<RefCounts> {
    let mut total = RefCounts::default();
    for page in pages.iter_mut() {
        let (content, counts) = rewrite_page(page, renames, src_kb_id)?;
        page.content = content;
        total.add(counts);
    }
    Ok(total)
}

/// One page: its own identifier, its edges' `object` and `primary_source`, every
/// `raw/…` path anywhere in its frontmatter, and every link in its body.
///
/// ⚠ A page whose frontmatter does not parse is carried **verbatim** apart from
/// its body links. DR-7's rule is that nothing rejects a page on read, and the
/// honest consequence is that this merge cannot repoint a reference it cannot
/// see. Rewriting such a page by string substitution instead would be the one
/// failure mode worse than a dangling edge: a silent corruption of a file the
/// user can still open.
fn rewrite_page(
    page: &PlannedPage,
    renames: &Renames,
    src_kb_id: &str,
) -> Result<(String, RefCounts)> {
    let Ok(split) = frontmatter::split(&page.content) else {
        return Ok(rewrite_body(&page.content, renames));
    };
    let mut doc = ConceptDoc::from_mapping(split.frontmatter);
    let mut n = RefCounts::default();

    if let Some(new) = &page.renamed_identifier {
        let old = page.identifier.as_deref().unwrap_or_default();
        if doc.identifier.is_some() {
            doc.identifier = Some(new.clone());
            n.saw(true);
        }
        // BioOKF §14 makes `title` a deprecated alias for `identifier`. Leaving
        // it behind would hand the merged page two conflicting primary keys and
        // every `object` would resolve against one of them arbitrarily.
        if doc.title.as_deref() == Some(old) {
            doc.title = Some(new.clone());
            n.saw(true);
        }
    }

    for edge in doc.edges.iter_mut() {
        n.saw(rename_in_place(&mut edge.object, &renames.identifiers));
        if let Some(ps) = edge.primary_source.as_mut() {
            n.saw(rename_in_place(ps, &renames.identifiers));
        }
    }

    // Every `raw/…` string anywhere in the frontmatter, including keys this
    // build does not model. `raw_source` is the named one, but `sources[]
    // .resource` carries the same path shape and a producer may put one in a key
    // that only rides in `extra` — a rewrite that enumerated key names would
    // repoint the two it knew about and leave the third dangling.
    let mut value = serde_yaml::Value::Mapping(doc.to_mapping());
    rewrite_raw_paths(&mut value, &renames.raw_ids, &mut n);
    let mut mapping = match value {
        serde_yaml::Value::Mapping(m) => m,
        // Unreachable: `to_mapping` produced it. Falling back to the typed view
        // loses the rewrite, never the document.
        _ => doc.to_mapping(),
    };
    mapping.insert(
        serde_yaml::Value::String(MERGED_FROM_KEY.to_string()),
        merged_from_value(page, src_kb_id),
    );

    let (body, body_n) = rewrite_body(&split.body, renames);
    n.add(body_n);
    Ok((frontmatter::join(&mapping, &body), n))
}

fn merged_from_value(page: &PlannedPage, src_kb_id: &str) -> serde_yaml::Value {
    let mut m = serde_yaml::Mapping::new();
    m.insert("kb_id".into(), src_kb_id.into());
    m.insert("path".into(), page.source_path.as_str().into());
    if let (Some(old), Some(_)) = (&page.identifier, &page.renamed_identifier) {
        m.insert("identifier".into(), old.as_str().into());
    }
    serde_yaml::Value::Mapping(m)
}

/// Replace `s` when it names a renamed identifier. Compared through
/// [`identity_key`] so `IL-6` and `il-6` are the same name, exactly as the graph
/// deriver resolves them. True when it moved.
fn rename_in_place(s: &mut String, identifiers: &BTreeMap<String, String>) -> bool {
    match identifiers.get(&identity_key(s)) {
        Some(next) => {
            *s = next.clone();
            true
        }
        None => false,
    }
}

/// Rewrite every `raw/<id>/…` string in a YAML value, in place, through the raw
/// id map. Recursive so it reaches paths inside `extra`.
///
/// Only strings that *are* raw paths are counted as seen: the walk visits every
/// scalar in the frontmatter, and counting a `type:` or an abstract as a
/// reference the merge chose not to repoint would make the denominator noise.
fn rewrite_raw_paths(
    value: &mut serde_yaml::Value,
    raw_ids: &BTreeMap<String, String>,
    c: &mut RefCounts,
) {
    match value {
        serde_yaml::Value::String(s) => {
            if raw_path_parts(s).is_none() {
                return;
            }
            match rewritten_raw_path(s, raw_ids) {
                Some(next) => {
                    *s = next;
                    c.saw(true);
                }
                None => c.saw(false),
            }
        }
        serde_yaml::Value::Sequence(seq) => {
            for v in seq.iter_mut() {
                rewrite_raw_paths(v, raw_ids, c);
            }
        }
        serde_yaml::Value::Mapping(map) => {
            for (_, v) in map.iter_mut() {
                rewrite_raw_paths(v, raw_ids, c);
            }
        }
        _ => {}
    }
}

/// `(lead-in, raw id, rest)` for a string that names something under `raw/`,
/// tolerating a `./` or `../` lead-in (a source page's body legitimately links
/// to `../raw/pmid-1/original.pdf`). `None` when the string is not a raw path at
/// all — which is a different answer from "a raw path this merge did not
/// rename", and the two have to be told apart for the reference counts to mean
/// anything.
fn raw_path_parts(s: &str) -> Option<(&str, &str, Option<&str>)> {
    let (prefix, rest) = s.split_once("raw/")?;
    if !prefix.is_empty() && !prefix.ends_with('/') {
        return None;
    }
    Some(match rest.split_once('/') {
        Some((id, tail)) => (prefix, id, Some(tail)),
        None => (prefix, rest, None),
    })
}

/// `raw/<old>/rest` → `raw/<new>/rest`. `None` when the string does not name a
/// renamed raw source.
fn rewritten_raw_path(s: &str, raw_ids: &BTreeMap<String, String>) -> Option<String> {
    let (prefix, id, tail) = raw_path_parts(s)?;
    let next = raw_ids.get(id)?;
    Some(match tail {
        Some(tail) => format!("{prefix}raw/{next}/{tail}"),
        None => format!("{prefix}raw/{next}"),
    })
}

/// Both body link grammars, in two passes.
///
/// Kept as its own scanner rather than reusing `okf::links::extract_links`
/// because that reader deliberately hands back targets *as written* with no byte
/// offsets — it answers "what does this page link to", and rewriting needs "and
/// where exactly did it say so".
///
/// It runs even when nothing was renamed, where it is a byte-for-byte copy: the
/// early return that used to skip it also skipped the *count*, and a page whose
/// forty links were all left alone would have reported having none. A
/// denominator that is only right when something moved is not a denominator.
fn rewrite_body(body: &str, renames: &Renames) -> (String, RefCounts) {
    let mut n = RefCounts::default();
    let wiki = rewrite_wiki_links(body, renames, &mut n);
    let markdown = rewrite_markdown_links(&wiki, renames, &mut n);
    (markdown, n)
}

fn rewrite_wiki_links(body: &str, renames: &Renames, n: &mut RefCounts) -> String {
    let mut out = String::with_capacity(body.len());
    let mut rest = body;
    while let Some((before, after)) = rest.split_once("[[") {
        out.push_str(before);
        out.push_str("[[");
        match after.split_once("]]") {
            Some((payload, tail)) => {
                out.push_str(&rewrite_wiki_payload(payload, renames, n));
                out.push_str("]]");
                rest = tail;
            }
            // An unterminated `[[` is prose, not a link. Copied through.
            None => rest = after,
        }
    }
    out.push_str(rest);
    out
}

/// The payload between `[[` and `]]`. Two grammars, told apart by the one rule
/// `okf::links` states: the segment before the first `|` contains `::` for
/// BioOKF's inline edge sugar and never for an ordinary title.
fn rewrite_wiki_payload(payload: &str, renames: &Renames, c: &mut RefCounts) -> String {
    let (head, alias) = match payload.split_once('|') {
        Some((head, alias)) => (head, Some(alias)),
        None => (payload, None),
    };
    let rewritten = match head.split_once("::") {
        // Sugar: the object is an identifier (BioOKF §7.2). Both branches go
        // through `rewritten_reference`, which is the point — the sugar object
        // and a plain head are the same grammar wearing different punctuation,
        // and they were resolved by two different rules until one of them was
        // wrong.
        Some((predicate, object)) => rewrite_sugar(predicate, object, renames, c),
        None => match rewritten_reference(head, renames) {
            Some(next) => {
                c.saw(true);
                next
            }
            None => {
                c.saw(false);
                head.to_string()
            }
        },
    };
    match alias {
        Some(alias) => format!("{rewritten}|{alias}"),
        None => rewritten,
    }
}

fn rewrite_sugar(predicate: &str, object: &str, renames: &Renames, c: &mut RefCounts) -> String {
    match rewritten_reference(object, renames) {
        Some(next) => {
            c.saw(true);
            // The trailing whitespace is preserved because the sugar's attribute
            // list follows on the other side of a `|` that this function never
            // sees: `[[treats:: X | k=v]]` is split before the pipe, so dropping
            // the space here silently reformats every attributed edge in the
            // bundle. A merge should be legible as a diff.
            format!("{predicate}:: {next}{}", trailing_whitespace(object))
        }
        None => {
            c.saw(false);
            format!("{predicate}::{object}")
        }
    }
}

/// A reference written as a **name or a path**, rewritten through whichever
/// rename map its grammar belongs to.
///
/// ⚠ **This is the fix for a silent retarget, and the reason it is not simply
/// `rewritten_target`.** A `[[…]]` head is two grammars, and the one that reads
/// as a bare name — `[[IL-6]]` — names an *identifier*, which is the very thing
/// a merge renames on collision. Sending it through the page map alone (which
/// keys on `link_key`) leaves it spelled the way it always was, and in the
/// merged base that spelling now resolves to the **destination's** page of that
/// name. The incoming page then asserts something about someone else's concept:
/// worse than a dangling link, because nothing is broken enough to notice.
///
/// So the two rungs are **ordered, never skipped** — `links::written_as_path`
/// says which one to ask first, and the other is always the fallback:
///
/// * a name (`IL-6`) → identifier map, then the page map, so a legacy page
///   addressed by its bare filename still moves with its file;
/// * a path (`knowledge/molecule/il-6.md`) → page map first, which keeps the
///   form it was written in (see [`rewritten_target`]), then the identifier map,
///   because `CD4/CD8 ratio` is an identifier that reads as a path and a hard
///   skip there would reintroduce exactly the retarget above.
///
/// The order is DR-3's ladder — identifier above basename — applied to
/// rewriting instead of resolution, so a link the graph deriver resolves by
/// name is repointed by name.
fn rewritten_reference(written: &str, renames: &Renames) -> Option<String> {
    let target = written.trim();
    let by_name = || renames.identifiers.get(&identity_key(target)).cloned();
    if links::written_as_path(target) {
        rewritten_target(target, renames).or_else(by_name)
    } else {
        by_name().or_else(|| rewritten_target(target, renames))
    }
}

fn trailing_whitespace(s: &str) -> String {
    let mut tail: Vec<char> = s.chars().rev().take_while(|c| c.is_whitespace()).collect();
    tail.reverse();
    tail.into_iter().collect()
}

/// OKF §6.1 markdown links: `[label](destination)`.
fn rewrite_markdown_links(body: &str, renames: &Renames, n: &mut RefCounts) -> String {
    let mut out = String::with_capacity(body.len());
    let mut rest = body;
    while let Some((before, after)) = rest.split_once("](") {
        out.push_str(before);
        out.push_str("](");
        match after.split_once(')') {
            Some((dest, tail)) => {
                match rewritten_target(dest, renames) {
                    Some(next) => {
                        n.saw(true);
                        out.push_str(&next);
                    }
                    None => {
                        n.saw(false);
                        out.push_str(dest);
                    }
                }
                out.push(')');
                rest = tail;
            }
            None => rest = after,
        }
    }
    out.push_str(rest);
    out
}

/// A link target that names a renamed page or a renamed raw source, rewritten in
/// the **form it was written in**: a bare title stays a bare title, a logical
/// path keeps its directory and its `.md`.
///
/// Keeping the form matters because `links::link_key` reduces both to the same
/// key but a human reads the file, and silently turning every path-style link
/// into a bare title would be a diff across the whole carried bundle for no
/// reason.
fn rewritten_target(target: &str, renames: &Renames) -> Option<String> {
    if let Some(next) = rewritten_raw_path(target, &renames.raw_ids) {
        return Some(next);
    }
    let stem = renames.page_stems.get(&link_key(target))?;
    let (dir, file) = match target.rsplit_once('/') {
        Some((dir, file)) => (Some(dir), file),
        None => (None, target),
    };
    let suffix = if file.ends_with(".md") { ".md" } else { "" };
    Some(match dir {
        Some(dir) => format!("{dir}/{stem}{suffix}"),
        None => format!("{stem}{suffix}"),
    })
}

// ────────────────────────────────────────────────────────────────────────────
// Application
// ────────────────────────────────────────────────────────────────────────────

/// What the merge created, so a failure can undo it.
///
/// Not a nicety: a page written and not yet committed on the transaction branch
/// is **untracked**, and a copied `raw/<id>/original.pdf` is **gitignored**
/// (`raw/*/original.*`). Neither is reachable by any checkout, so `abort_txn`
/// alone would leave the destination changed — which is precisely the promise
/// this module makes.
#[derive(Default)]
struct Created(Vec<PathBuf>);

impl Created {
    fn push(&mut self, path: PathBuf) {
        self.0.push(path);
    }

    fn undo(&self) {
        for path in self.0.iter().rev() {
            let _ = if path.is_dir() {
                std::fs::remove_dir_all(path)
            } else {
                std::fs::remove_file(path)
            };
        }
    }
}

/// Run the plan inside one transaction on the destination.
///
/// The order is load-bearing. Content first, then the canonical check, then the
/// squash commit: verifying *before* the commit is what lets a violation abort
/// rather than be reported about a base that has already changed.
pub fn apply(dst_root: &Path, src_root: &Path, plan: &MergePlan) -> Result<String> {
    anyhow::ensure!(
        plan.plan_violations.is_empty(),
        "refusing to merge: the plan would change the destination, which must stay \
         canonical:\n  {}",
        plan.plan_violations.join("\n  ")
    );
    let repo = GitRepo::open(dst_root)?;
    let txn = repo.begin_txn(&format!("merge-{}", plan.source_kb_id))?;
    let mut created = Created::default();

    match write_everything(dst_root, src_root, plan, &repo, &txn, &mut created) {
        Ok(sha) => Ok(sha),
        Err(e) => {
            created.undo();
            let _ = repo.abort_txn(&txn);
            Err(e)
        }
    }
}

fn write_everything(
    dst_root: &Path,
    src_root: &Path,
    plan: &MergePlan,
    repo: &GitRepo,
    txn: &Txn,
    created: &mut Created,
) -> Result<String> {
    for copy in &plan.raw_copies {
        let to = dst_root.join("raw").join(&copy.to);
        // The last line of defence for "the destination is canonical", and it
        // guards a case the planner cannot see: `raw_ids` counts DIRECTORIES, so
        // a stray *file* at `raw/<id>` is not in the taken set and the copy would
        // have landed on it.
        anyhow::ensure!(
            !to.exists(),
            "refusing to copy raw/{} onto the destination's existing raw/{}",
            copy.from,
            copy.to
        );
        created.push(to.clone());
        copy_dir(&src_root.join("raw").join(&copy.from), &to)?;
        // The directory name is the source id, and `meta.yaml` states it too. A
        // rename that moved only the directory would leave `raw::read_meta`
        // answering with an id that names a directory somewhere else — the exact
        // shape of a dangling reference, one level below the pages.
        restamp_meta_id(&to, &copy.to)?;
    }

    for page in &plan.pages {
        anyhow::ensure!(
            store::is_writable_page_path(&page.destination_path),
            "refusing to carry {} to {}: only knowledge/ pages are writable",
            page.source_path,
            page.destination_path
        );
        let abs = dst_root.join(&page.destination_path);
        // The same last line of defence, for pages. `snapshot.paths` holds what
        // `store::list_pages` found, so anything at that path the *walker* skips
        // — a directory whose name ends in `.md`, a symlink — is invisible to the
        // planner and would be written over here.
        anyhow::ensure!(
            !abs.exists(),
            "refusing to carry {} onto the destination's existing {}",
            page.source_path,
            page.destination_path
        );
        if let Some(parent) = abs.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Both paths, and the staging file is not paranoia: a write that fails
        // between `write` and `rename` leaves a `<page>.md.tmp` in the
        // destination's tree, and `store::list_pages` would not show it while
        // the next `.brkb` export would carry it.
        let tmp = abs.with_extension("md.tmp");
        created.push(tmp.clone());
        created.push(abs.clone());
        std::fs::write(&tmp, &page.content)?;
        std::fs::rename(&tmp, &abs)?;
    }

    append_index(dst_root, plan, created)?;

    // ONE commit on the transaction branch, staging everything above — so the
    // whole merge is a single tree that `commit_txn` can squash or `abort_txn`
    // can discard.
    repo.commit_on_txn_in_progress(&format!("merge {} into this base", plan.source_kb_id))?;

    let violations = verify_snapshot(dst_root, &plan.snapshot)?;
    anyhow::ensure!(
        violations.is_empty(),
        "the merge would have changed the destination, which must stay canonical:\n  {}",
        violations.join("\n  ")
    );

    crate::knowledge::log::append(
        dst_root,
        ChangeKind::Manual,
        &format!("merge {}", plan.source_kb_id),
        Some(&merge_delta(plan)),
        Some(&txn.branch),
    )?;

    repo.commit_txn(
        txn,
        ChangeKind::Manual,
        &format!("merge {}", plan.source_kb_id),
        Some(&merge_delta(plan)),
    )
}

fn merge_delta(plan: &MergePlan) -> String {
    format!(
        "+{} pages · +{} raw sources ({} deduped) · {} renamed · {} of {} references rewritten",
        plan.pages.len(),
        plan.raw_copies.len(),
        plan.raw_deduped.len(),
        plan.identifiers_renamed.len() + plan.paths_renamed.len(),
        plan.references.rewritten,
        plan.references.seen,
    )
}

/// The `id` inside a copied `raw/<id>/meta.yaml` follows the directory rename.
fn restamp_meta_id(raw_dir: &Path, id: &str) -> Result<()> {
    let path = raw_dir.join("meta.yaml");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(());
    };
    let Ok(mut meta) = serde_yaml::from_str::<crate::knowledge::types::SourceMeta>(&text) else {
        return Ok(());
    };
    if meta.id == id {
        return Ok(());
    }
    meta.id = id.to_string();
    std::fs::write(&path, serde_yaml::to_string(&meta)?)?;
    Ok(())
}

/// Add the carried pages to the destination's `index.md`, under one heading
/// naming the base they came from.
///
/// A section of its own, rather than folding each page into the destination's
/// existing type sections: which heading a page belongs under is the
/// destination's own vocabulary, and deciding that is judgement work. A section
/// is also what a user can act on — it is the list of what arrived.
fn append_index(dst_root: &Path, plan: &MergePlan, created: &mut Created) -> Result<()> {
    if plan.pages.is_empty() {
        return Ok(());
    }
    let path = dst_root.join("index.md");
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let heading = format!("# Merged from {}", plan.source_kb_id);
    let bullets: Vec<String> = plan
        .pages
        .iter()
        .map(|page| {
            let label = page
                .renamed_identifier
                .clone()
                .or_else(|| page.identifier.clone())
                .unwrap_or_else(|| file_stem(&page.destination_path).to_string());
            format!("* [{label}]({})", page.destination_path)
        })
        .collect();

    let tmp = path.with_extension("md.tmp");
    created.push(tmp.clone());
    std::fs::write(&tmp, insert_into_section(&text, &heading, &bullets))?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

/// Put `bullets` at the end of `heading`'s section, creating the section at the
/// end of the document if it is not there.
///
/// ⚠ **Not "append to the file", which is what this was.** A second merge from
/// the same source found its heading already present and then appended at the
/// end anyway — so with two sources merged in the order A, B, A, the third
/// merge's pages were listed under **B's** heading. The section is found and the
/// insertion point is the last non-blank line inside it.
///
/// A bullet already present anywhere in the file is skipped, so re-running a
/// merge does not double-list a page.
fn insert_into_section(text: &str, heading: &str, bullets: &[String]) -> String {
    let mut lines: Vec<String> = text.lines().map(ToOwned::to_owned).collect();
    let fresh: Vec<String> = bullets
        .iter()
        .filter(|b| !lines.contains(b))
        .cloned()
        .collect();
    if fresh.is_empty() {
        return join_lines(&lines);
    }
    match lines.iter().position(|l| l.trim_end() == heading) {
        Some(start) => {
            // `# ` and not `#`, so a sub-heading inside the section does not end
            // it: index sections are H1 (OKF §8), and BioOKF's own example puts
            // one type per `#`.
            let end = lines
                .iter()
                .enumerate()
                .skip(start + 1)
                .find(|(_, l)| l.starts_with("# "))
                .map(|(i, _)| i)
                .unwrap_or(lines.len());
            let mut at = end;
            while at > start + 1 && lines[at - 1].trim().is_empty() {
                at -= 1;
            }
            for (n, bullet) in fresh.into_iter().enumerate() {
                lines.insert(at + n, bullet);
            }
        }
        None => {
            if !lines.is_empty() {
                lines.push(String::new());
            }
            lines.push(heading.to_string());
            lines.push(String::new());
            lines.extend(fresh);
        }
    }
    join_lines(&lines)
}

fn join_lines(lines: &[String]) -> String {
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)?.flatten() {
        let path = entry.path();
        let dest = to.join(entry.file_name());
        if path.is_dir() {
            copy_dir(&path, &dest)?;
        } else {
            std::fs::copy(&path, &dest)?;
        }
    }
    Ok(())
}

/// The report a caller returns, once the classification fold has answered.
pub fn report(
    plan: &MergePlan,
    destination_kb_id: &str,
    dry_run: bool,
    tier: &str,
    owners_added: Vec<String>,
    commit_sha: Option<String>,
) -> MergeReport {
    let mut report = plan.report(destination_kb_id, dry_run);
    report.destination_tier = tier.to_string();
    report.owners_added = owners_added;
    report.commit_sha = commit_sha;
    report
}

#[cfg(test)]
mod tests;
