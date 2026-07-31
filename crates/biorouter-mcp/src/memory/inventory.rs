//! Reading and pruning the memory stores **as the user**, not as the model.
//!
//! Issue #63 closed the consent gate on machine-wide memory: a global read is
//! now put to the user for approval. That is only half a fix while the user
//! cannot see what is in the store they are approving reads of — the approval
//! card names a category, and nothing anywhere shows what that category holds
//! or lets them throw it away.
//!
//! This module is the other half: the inventory the Settings surface lists, and
//! the two deletions it offers. It is deliberately *not* built on the four MCP
//! tools. Those are the model's interface, shaped for a model:
//!
//! * [`MemoryServer::retrieve`] returns a `HashMap` keyed by the entry's joined
//!   tags, so two entries carrying the same tags collapse into one and an
//!   untagged entry's lines are concatenated with every other untagged entry's.
//!   That is lossy in exactly the way a management view must not be — the user
//!   is being shown what is on their disk, and what would be disclosed if they
//!   approved a read.
//! * [`MemoryServer::remove_specific_memory_internal`] identifies a memory by
//!   its *body*, which is what a model has to hand. Two rows can share a body
//!   and differ only in their tags, and a user clicking delete on one of them
//!   must lose that row — so this door identifies a row by a digest of the whole
//!   serialized entry, and by the revision of the category it was listed in.
//!
//! What it does share is [`MemoryServer::get_memory_file`], and that is the
//! point: the containment checks issue #73 put there — a category is one plain
//! path segment, re-checked after canonicalization — are the only thing between
//! a category name and the filesystem, and a second door into the store must go
//! through the same lock rather than grow its own.
//!
//! ## What the store actually records
//!
//! A category is a flat text file, `<store>/<category>.txt`, appended to by
//! [`MemoryServer::remember`] as an optional `# tags` line, the body, and a
//! blank line. So the provenance that genuinely exists is:
//!
//! * the category name (the filename),
//! * the scope — which of the two directories it is in — and that directory's
//!   absolute path,
//! * the entry's tags, when the model supplied any,
//! * the category file's size and modification time.
//!
//! That is the whole list. Nothing records **when an individual memory was
//! written, which conversation wrote it, or which model**, so the inventory
//! reports a modification time per *category* and says so, rather than
//! inventing a per-entry timestamp the file cannot support.

use std::fs;
use std::io;
use std::path::Path;
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{validated_category, MemoryServer};

/// A content digest, used two ways: over one serialized entry (which row is
/// this?) and over a whole category file (which state of the category is this?).
///
/// It has to cover the *complete* serialized entry, tag line included. Two
/// entries can share a body and differ only in their tags — `# phi\npatient A`
/// and `patient A` are two rows a user must be able to tell apart, and a guard
/// that compared bodies alone would happily satisfy itself against the wrong one
/// (#63 review, finding 6).
fn digest_of(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

/// The revision reported for a category that is not there. Distinct from any
/// real file's digest, so it cannot be presented back as "this is the state I
/// listed" for a category that has since been created.
const NO_SUCH_CATEGORY: &str = "absent";

/// Which of the two stores an entry lives in.
///
/// The distinction is the whole subject of issue #63: `Local` is this project's
/// `.biorouter/memory`, reachable only by a session opened in that directory;
/// `Global` is the machine-wide store every Biorouter session on the computer
/// shares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum MemoryScope {
    Global,
    Local,
}

impl MemoryScope {
    /// The `is_global` flag the memory tools take.
    pub fn is_global(self) -> bool {
        matches!(self, MemoryScope::Global)
    }
}

/// One stored memory, exactly as it sits in the category file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct MemoryEntry {
    /// Position within the category file, counting from zero.
    ///
    /// Stable only for as long as the file is untouched, which is why
    /// [`MemoryServer::delete_entry`] takes [`MemoryEntry::digest`] back as a
    /// guard rather than trusting the index on its own.
    pub index: usize,
    /// Words from the entry's leading `# …` line; empty when it has none.
    pub tags: Vec<String>,
    /// The entry body, interior newlines preserved.
    pub content: String,
    /// Digest of this entry exactly as it is serialized on disk — tag line and
    /// body together.
    ///
    /// This is the row's identity, and it is what a delete has to name. The body
    /// alone is not an identity: two entries can carry the same text under
    /// different tags, and a body-only guard is satisfied by whichever of them
    /// happens to sit at the index (#63 review, finding 6).
    pub digest: String,
}

/// One category file, listed in full.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct MemoryCategoryInventory {
    pub name: String,
    pub entries: Vec<MemoryEntry>,
    /// Digest of the whole category file as it was listed.
    ///
    /// The compare-and-set token for every delete: any write to the category —
    /// an agent appending, another window deleting — changes it, so a delete
    /// that still carries the listed revision is a delete of the category the
    /// user was actually looking at. Without it the user confirms a list that
    /// has since moved on and destroys something they were never shown.
    pub revision: String,
    /// The category file's size on disk.
    pub size_bytes: u64,
    /// The category file's modification time, Unix seconds.
    ///
    /// **Per category, not per entry.** Appending any memory restamps the whole
    /// file, so this dates the most recent write to the category and says
    /// nothing about when the other entries arrived. `None` when the filesystem
    /// does not report one.
    pub modified: Option<i64>,
}

/// One store: where it is, and everything in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct MemoryStoreInventory {
    pub scope: MemoryScope,
    /// Absolute path of the store directory, shown to the user so "global" and
    /// "local" are not the only thing they have to go on.
    pub path: String,
    /// Whether that directory exists yet. The store is created lazily on first
    /// write, so "no directory" is the ordinary empty state, not an error.
    pub exists: bool,
    pub categories: Vec<MemoryCategoryInventory>,
}

/// The outcome of deleting a single entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryDeletion {
    Deleted {
        /// Entries left in the category afterwards.
        remaining: usize,
        /// Whether the category file was removed because it emptied.
        category_removed: bool,
    },
    /// The index is past the end of the category — the listing was stale.
    OutOfRange,
    /// There is an entry at that index, but it is not the one the caller was
    /// shown. Something wrote to the category in between.
    ContentMismatch,
    /// The row at that index is still the right one, but the category as a whole
    /// is not the one that was listed — something else in it was added, edited
    /// or removed since. The delete refuses rather than acting on a view the
    /// user has not seen.
    CategoryChanged,
}

/// The outcome of deleting a whole category.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CategoryDeletion {
    Deleted {
        /// How many entries went with it, so the caller can say what was lost.
        removed_entries: usize,
    },
    /// There is no such category.
    Missing,
    /// The category changed since it was listed. "Delete everything in
    /// `clinical`" is consent to lose what the user was shown, not whatever
    /// arrived afterwards.
    CategoryChanged,
}

/// Split a category file into entries the way [`MemoryServer::retrieve`] splits
/// it — on a blank line — but keeping order, duplicates, and the difference
/// between "no tag line" and "an empty tag line".
///
/// The blank-line split is inherited from the file format
/// [`MemoryServer::remember`] writes and is not a choice made here: a memory
/// body that itself contains a blank line was already two entries as far as
/// every existing reader is concerned, and a management view that silently
/// re-joined them would show the user something the store does not contain.
pub(super) fn parse_entries(content: &str) -> Vec<MemoryEntry> {
    let mut entries = Vec::new();
    for chunk in content.split("\n\n") {
        let mut lines = chunk.lines();
        let Some(first) = lines.next() else {
            // The trailing blank line every write leaves behind.
            continue;
        };
        let (tags, body) = match first.strip_prefix('#') {
            Some(tag_line) => (
                tag_line.split_whitespace().map(String::from).collect(),
                lines.collect::<Vec<_>>().join("\n"),
            ),
            None => (
                Vec::new(),
                std::iter::once(first)
                    .chain(lines)
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
        };
        entries.push(MemoryEntry {
            index: entries.len(),
            digest: digest_of(&render_entry(&tags, &body)),
            tags,
            content: body,
        });
    }
    entries
}

/// One entry in the on-disk format. The unit both [`render_entries`] and
/// [`MemoryEntry::digest`] are built from, so a row's identity is by
/// construction a digest of what a rewrite would put back on disk.
fn render_entry(tags: &[String], content: &str) -> String {
    let mut out = String::new();
    if !tags.is_empty() {
        out.push('#');
        out.push(' ');
        out.push_str(&tags.join(" "));
        out.push('\n');
    }
    out.push_str(content);
    out.push_str("\n\n");
    out
}

/// Re-serialize entries into the on-disk format, so a delete leaves a file the
/// memory tools still parse identically.
pub(super) fn render_entries(entries: &[MemoryEntry]) -> String {
    entries
        .iter()
        .map(|entry| render_entry(&entry.tags, &entry.content))
        .collect()
}

fn modified_secs(meta: &fs::Metadata) -> Option<i64> {
    meta.modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs() as i64)
}

impl MemoryServer {
    /// The directory backing one scope.
    pub fn store_dir(&self, scope: MemoryScope) -> &Path {
        match scope {
            MemoryScope::Global => &self.global_memory_dir,
            MemoryScope::Local => &self.local_memory_dir,
        }
    }

    /// One category as it stands on disk right now: its entries, and the
    /// revision naming exactly this state of it.
    ///
    /// One read, so the two cannot describe different moments — computing the
    /// revision in a second pass would report a digest of a file the entries did
    /// not come from.
    fn read_category(
        &self,
        category: &str,
        scope: MemoryScope,
    ) -> io::Result<(Vec<MemoryEntry>, String)> {
        let path = self.get_memory_file(category, scope.is_global())?;
        if !path.exists() {
            return Ok((Vec::new(), NO_SUCH_CATEGORY.to_string()));
        }
        let raw = fs::read_to_string(path)?;
        Ok((parse_entries(&raw), digest_of(&raw)))
    }

    /// Every entry in one category, in file order.
    pub fn list_entries(&self, category: &str, scope: MemoryScope) -> io::Result<Vec<MemoryEntry>> {
        Ok(self.read_category(category, scope)?.0)
    }

    /// The revision of one category — the compare-and-set token a delete has to
    /// carry back. See [`MemoryCategoryInventory::revision`].
    pub fn category_revision(&self, category: &str, scope: MemoryScope) -> io::Result<String> {
        Ok(self.read_category(category, scope)?.1)
    }

    /// Everything in one store: what the Settings surface lists.
    ///
    /// Total by construction — a file the memory tools would refuse to read is
    /// skipped rather than failing the whole listing, matching
    /// [`MemoryServer::retrieve_all`]. A store that cannot be listed at all
    /// (permissions) is an error, because that is not an empty store and must
    /// not be shown as one.
    pub fn inventory(&self, scope: MemoryScope) -> io::Result<MemoryStoreInventory> {
        let base = self.store_dir(scope).to_path_buf();
        let mut inventory = MemoryStoreInventory {
            scope,
            path: base.display().to_string(),
            exists: base.exists(),
            categories: Vec::new(),
        };
        if !inventory.exists {
            return Ok(inventory);
        }

        for entry in fs::read_dir(&base)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let file_name = entry.file_name();
            let Some(category) = file_name.to_str().and_then(|n| n.strip_suffix(".txt")) else {
                continue;
            };
            if validated_category(category).is_err() {
                continue;
            }
            let meta = entry.metadata()?;
            let (entries, revision) = self.read_category(category, scope)?;
            inventory.categories.push(MemoryCategoryInventory {
                name: category.to_string(),
                entries,
                revision,
                size_bytes: meta.len(),
                modified: modified_secs(&meta),
            });
        }

        // `read_dir` order is whatever the filesystem feels like; the user gets
        // a stable list.
        inventory
            .categories
            .sort_unstable_by(|a, b| a.name.cmp(&b.name));
        Ok(inventory)
    }

    /// Delete exactly one entry, identified by its position, by the **digest of
    /// the row** the caller was shown, and by the **revision of the category**
    /// they were shown it in.
    ///
    /// The three-part guard is the whole design, and each part answers a way the
    /// listing can have gone stale between rendering and clicking — this store
    /// is appended to by an agent that may be running while the user reads it.
    ///
    /// * The *index* alone deletes whatever has since moved into that slot.
    /// * The *row digest* pins which entry that is, over the complete serialized
    ///   entry rather than its body: two rows can share a body and differ only
    ///   in tags, and a body-only guard is satisfied by the wrong one.
    /// * The *category revision* makes it a compare-and-set. The row can still
    ///   be the right row while the category around it has changed, and a user
    ///   who confirmed a delete against a list that has since moved on is
    ///   deleting from a state they were never shown.
    ///
    /// Together with the store lock, the check and the write are atomic, so the
    /// CAS cannot be raced.
    pub fn delete_entry(
        &self,
        category: &str,
        scope: MemoryScope,
        index: usize,
        expected_digest: &str,
        expected_revision: &str,
    ) -> io::Result<EntryDeletion> {
        let path = self.get_memory_file(category, scope.is_global())?;
        // The read, the guard and the rewrite are one critical section. Without
        // it an agent's append lands between the read and the write and is
        // silently discarded — and on the last-entry path below, the whole
        // category file (including that append) is removed (#63 review, 6).
        let Some(_lock) = self.lock_store_if_present(scope.is_global())? else {
            return Ok(EntryDeletion::OutOfRange);
        };
        if !path.exists() {
            return Ok(EntryDeletion::OutOfRange);
        }
        let raw = fs::read_to_string(&path)?;
        let revision = digest_of(&raw);
        let mut entries = parse_entries(&raw);
        let Some(found) = entries.get(index) else {
            return Ok(EntryDeletion::OutOfRange);
        };
        // Row identity first: it is the more specific answer, and the one the
        // caller can act on ("that is not the memory you clicked").
        if found.digest != expected_digest {
            return Ok(EntryDeletion::ContentMismatch);
        }
        if revision != expected_revision {
            return Ok(EntryDeletion::CategoryChanged);
        }

        entries.remove(index);
        if entries.is_empty() {
            // An emptied category file would keep its name in the system prompt
            // for every future session — the category index #58 kept is exactly
            // what a global category name still leaks. Deleting the last memory
            // in a category has to take the category with it.
            fs::remove_file(&path)?;
            return Ok(EntryDeletion::Deleted {
                remaining: 0,
                category_removed: true,
            });
        }

        for (position, entry) in entries.iter_mut().enumerate() {
            entry.index = position;
        }
        super::replace_category_file(&path, &render_entries(&entries))?;
        Ok(EntryDeletion::Deleted {
            remaining: entries.len(),
            category_removed: false,
        })
    }

    /// Delete a whole category, guarded by the revision it was listed at.
    ///
    /// Reports how many entries went with it, so the caller can tell the user
    /// what they actually lost — and refuses if that number would not be the
    /// number they were shown. "Delete everything in `clinical`" is consent to
    /// lose the memories on the screen, not whatever an agent appended while the
    /// confirmation dialog was open.
    pub fn delete_category(
        &self,
        category: &str,
        scope: MemoryScope,
        expected_revision: &str,
    ) -> io::Result<CategoryDeletion> {
        let path = self.get_memory_file(category, scope.is_global())?;
        // Counting and removing under one lock, so the count reported to the
        // user is the count that was actually destroyed.
        let Some(_lock) = self.lock_store_if_present(scope.is_global())? else {
            return Ok(CategoryDeletion::Missing);
        };
        if !path.exists() {
            return Ok(CategoryDeletion::Missing);
        }
        let raw = fs::read_to_string(&path)?;
        if digest_of(&raw) != expected_revision {
            return Ok(CategoryDeletion::CategoryChanged);
        }
        let removed_entries = parse_entries(&raw).len();
        fs::remove_file(&path)?;
        Ok(CategoryDeletion::Deleted { removed_entries })
    }
}

#[cfg(test)]
mod tests {
    use super::super::GlobalMemoryConsent;
    use super::*;
    use rmcp::handler::server::router::tool::ToolRouter;
    use tempfile::tempdir;

    fn server_at(base: &Path) -> MemoryServer {
        MemoryServer {
            tool_router: ToolRouter::new(),
            instructions: String::new(),
            global_memory_dir: base.join("global"),
            local_memory_dir: base.join("local"),
            consent: GlobalMemoryConsent::Gated,
        }
    }

    fn write_store(dir: &Path, category: &str, body: &str) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join(format!("{category}.txt")), body).unwrap();
    }

    /// One category exactly as the Settings surface would render it — the rows
    /// and the revision a delete then has to carry back.
    fn listing(
        server: &MemoryServer,
        category: &str,
        scope: MemoryScope,
    ) -> MemoryCategoryInventory {
        server
            .inventory(scope)
            .unwrap()
            .categories
            .into_iter()
            .find(|c| c.name == category)
            .unwrap_or_else(|| panic!("category {category:?} is not in the {scope:?} inventory"))
    }

    /// The reason this module exists rather than reusing `retrieve`: two
    /// memories written under the same tags are two memories on disk, and a
    /// user pruning their store has to be shown both. `retrieve` keys a
    /// `HashMap` on the joined tags and the second `insert` overwrites the
    /// first, so the older memory is invisible while still being on disk and
    /// still being disclosed by a `retrieve_memories` call.
    #[test]
    fn two_memories_sharing_a_tag_are_both_listed() {
        let temp = tempdir().unwrap();
        let server = server_at(temp.path());
        write_store(
            &temp.path().join("global"),
            "clinical",
            "# phi cohort\nfirst memory\n\n# phi cohort\nsecond memory\n\n",
        );

        let entries = server
            .list_entries("clinical", MemoryScope::Global)
            .unwrap();
        let bodies: Vec<&str> = entries.iter().map(|e| e.content.as_str()).collect();
        assert_eq!(
            bodies,
            vec!["first memory", "second memory"],
            "both same-tag memories must be listed; retrieve() collapses them"
        );
        assert_eq!(entries[0].tags, vec!["phi", "cohort"]);
        assert_eq!(entries[1].index, 1);
    }

    /// Untagged entries stay separate rows too — `retrieve` concatenates every
    /// untagged entry's lines into one `"untagged"` bucket.
    #[test]
    fn untagged_memories_stay_separate_rows() {
        let temp = tempdir().unwrap();
        let server = server_at(temp.path());
        write_store(
            &temp.path().join("local"),
            "development",
            "we use black\n\nwe pin rust 1.92\n\n",
        );

        let entries = server
            .list_entries("development", MemoryScope::Local)
            .unwrap();
        assert_eq!(entries.len(), 2, "two memories, two rows");
        assert!(entries.iter().all(|e| e.tags.is_empty()));
        assert_eq!(entries[1].content, "we pin rust 1.92");
    }

    /// A memory body spanning several lines keeps them.
    #[test]
    fn a_multi_line_memory_keeps_its_newlines() {
        let temp = tempdir().unwrap();
        let server = server_at(temp.path());
        write_store(
            &temp.path().join("global"),
            "notes",
            "# ops\nline one\nline two\n\n",
        );

        let entries = server.list_entries("notes", MemoryScope::Global).unwrap();
        assert_eq!(entries[0].content, "line one\nline two");
    }

    /// The inventory is what Settings renders, so it has to carry the store's
    /// real provenance: where the file is, how big it is, when it last changed.
    #[test]
    fn the_inventory_reports_the_store_path_and_per_category_metadata() {
        let temp = tempdir().unwrap();
        let server = server_at(temp.path());
        let global = temp.path().join("global");
        write_store(&global, "personal", "# name\nWanjun\n\n");
        write_store(&global, "clinical", "a cohort note\n\n");

        let inventory = server.inventory(MemoryScope::Global).unwrap();
        assert_eq!(inventory.scope, MemoryScope::Global);
        assert!(inventory.exists);
        assert_eq!(inventory.path, global.display().to_string());
        assert_eq!(
            inventory
                .categories
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>(),
            vec!["clinical", "personal"],
            "categories are sorted so the list does not reshuffle between loads"
        );
        let personal = &inventory.categories[1];
        assert_eq!(personal.entries.len(), 1);
        assert!(personal.size_bytes > 0, "size on disk is real provenance");
        assert!(
            personal.modified.is_some_and(|t| t > 1_600_000_000),
            "the category file's mtime is the only timestamp the store has"
        );
    }

    /// A store that has never been written to is empty, not broken — it is
    /// created lazily on first write.
    #[test]
    fn a_store_that_does_not_exist_yet_lists_as_empty() {
        let temp = tempdir().unwrap();
        let server = server_at(temp.path());

        let inventory = server.inventory(MemoryScope::Local).unwrap();
        assert!(!inventory.exists);
        assert!(inventory.categories.is_empty());
        assert!(
            inventory.path.ends_with("local"),
            "the path is shown even when nothing is there yet"
        );
    }

    /// The two scopes are separate stores and must never bleed into each other:
    /// this whole feature exists so the user can tell machine-wide memory from
    /// project-local memory.
    #[test]
    fn the_two_scopes_do_not_bleed_into_each_other() {
        let temp = tempdir().unwrap();
        let server = server_at(temp.path());
        write_store(&temp.path().join("global"), "shared", "machine wide\n\n");
        write_store(&temp.path().join("local"), "project", "this project\n\n");

        let global = server.inventory(MemoryScope::Global).unwrap();
        let local = server.inventory(MemoryScope::Local).unwrap();
        assert_eq!(
            global
                .categories
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>(),
            vec!["shared"]
        );
        assert_eq!(
            local
                .categories
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>(),
            vec!["project"]
        );
    }

    /// Deleting one row deletes that row, not the rows it is a prefix of — a
    /// user clicking the trash icon on "black" must not lose "we use black for
    /// formatting" with it.
    #[test]
    fn deleting_one_entry_leaves_the_entries_that_merely_contain_it() {
        let temp = tempdir().unwrap();
        let server = server_at(temp.path());
        let dir = temp.path().join("local");
        write_store(
            &dir,
            "development",
            "black\n\nwe use black for formatting\n\n",
        );

        let listed = listing(&server, "development", MemoryScope::Local);
        let outcome = server
            .delete_entry(
                "development",
                MemoryScope::Local,
                0,
                &listed.entries[0].digest,
                &listed.revision,
            )
            .unwrap();
        assert_eq!(
            outcome,
            EntryDeletion::Deleted {
                remaining: 1,
                category_removed: false
            }
        );

        let left = server
            .list_entries("development", MemoryScope::Local)
            .unwrap();
        assert_eq!(
            left.iter().map(|e| e.content.as_str()).collect::<Vec<_>>(),
            vec!["we use black for formatting"],
            "a substring match would have deleted this one too"
        );
    }

    /// A delete keeps the file in the format the memory tools read, so a
    /// pruned category is still readable by the model afterwards.
    #[test]
    fn a_pruned_category_is_still_readable_by_the_memory_tools() {
        let temp = tempdir().unwrap();
        let server = server_at(temp.path());
        write_store(
            &temp.path().join("global"),
            "personal",
            "# name\nWanjun\n\n# city\nSan Francisco\n\n",
        );

        let listed = listing(&server, "personal", MemoryScope::Global);
        server
            .delete_entry(
                "personal",
                MemoryScope::Global,
                0,
                &listed.entries[0].digest,
                &listed.revision,
            )
            .unwrap();

        let retrieved = server.retrieve("personal", true).unwrap();
        assert_eq!(
            retrieved.get("city").map(Vec::as_slice),
            Some(["San Francisco".to_string()].as_slice()),
            "the surviving memory must still parse for the model"
        );
        assert!(
            !retrieved.contains_key("name"),
            "the deleted memory must be gone from the model's view too"
        );
    }

    /// Deleting the last memory in a category takes the category with it —
    /// otherwise the *name* survives in every future session's system prompt
    /// (the global category index) with nothing behind it.
    #[test]
    fn emptying_a_category_removes_the_category_itself() {
        let temp = tempdir().unwrap();
        let server = server_at(temp.path());
        let dir = temp.path().join("global");
        write_store(&dir, "clinical", "the only note\n\n");

        let listed = listing(&server, "clinical", MemoryScope::Global);
        let outcome = server
            .delete_entry(
                "clinical",
                MemoryScope::Global,
                0,
                &listed.entries[0].digest,
                &listed.revision,
            )
            .unwrap();
        assert_eq!(
            outcome,
            EntryDeletion::Deleted {
                remaining: 0,
                category_removed: true
            }
        );
        assert!(
            !dir.join("clinical.txt").exists(),
            "an emptied category must not linger as a name in the prompt index"
        );
        assert!(server
            .inventory(MemoryScope::Global)
            .unwrap()
            .categories
            .is_empty());
    }

    /// The store is appended to by a running agent. If the file changed between
    /// the listing and the click, the delete must refuse rather than take out
    /// whatever moved into that slot.
    #[test]
    fn a_stale_index_refuses_instead_of_deleting_the_wrong_memory() {
        let temp = tempdir().unwrap();
        let server = server_at(temp.path());
        let dir = temp.path().join("global");
        write_store(&dir, "clinical", "first\n\nsecond\n\n");
        let listed = listing(&server, "clinical", MemoryScope::Global);

        // The user was shown "first" at index 0, but the agent has since
        // rewritten the file.
        write_store(&dir, "clinical", "something else entirely\n\nsecond\n\n");

        let outcome = server
            .delete_entry(
                "clinical",
                MemoryScope::Global,
                0,
                &listed.entries[0].digest,
                &listed.revision,
            )
            .unwrap();
        assert_eq!(outcome, EntryDeletion::ContentMismatch);
        assert_eq!(
            server
                .list_entries("clinical", MemoryScope::Global)
                .unwrap()
                .len(),
            2,
            "nothing may be deleted when the guard fails"
        );
    }

    /// Two memories can carry the same words under different tags. They are two
    /// rows on screen and two rows on disk, so deleting one must not be
    /// satisfiable by the other — which is exactly what a guard comparing bodies
    /// does (#63 review, finding 6). The digest covers the serialized entry, tag
    /// line included.
    #[test]
    fn a_row_is_identified_by_its_tags_as_well_as_its_body() {
        let temp = tempdir().unwrap();
        let server = server_at(temp.path());
        let dir = temp.path().join("global");
        write_store(
            &dir,
            "clinical",
            "# draft\npatient A responded\n\n# confirmed\npatient A responded\n\n",
        );
        let listed = listing(&server, "clinical", MemoryScope::Global);
        assert_eq!(listed.entries[0].content, listed.entries[1].content);
        assert_ne!(
            listed.entries[0].digest, listed.entries[1].digest,
            "two rows differing only in tags must not share an identity"
        );

        // The user clicked the *confirmed* row (index 1) but the digest they
        // carry back names the draft one. A body comparison cannot tell these
        // apart; the delete must refuse rather than destroy the confirmed note.
        let outcome = server
            .delete_entry(
                "clinical",
                MemoryScope::Global,
                1,
                &listed.entries[0].digest,
                &listed.revision,
            )
            .unwrap();
        assert_eq!(outcome, EntryDeletion::ContentMismatch);
        assert_eq!(
            server
                .list_entries("clinical", MemoryScope::Global)
                .unwrap()
                .len(),
            2,
            "nothing may be deleted when the row named is not the row at that index"
        );

        // Naming the row it really is deletes exactly that row.
        let outcome = server
            .delete_entry(
                "clinical",
                MemoryScope::Global,
                1,
                &listed.entries[1].digest,
                &listed.revision,
            )
            .unwrap();
        assert_eq!(
            outcome,
            EntryDeletion::Deleted {
                remaining: 1,
                category_removed: false
            }
        );
        let left = server
            .list_entries("clinical", MemoryScope::Global)
            .unwrap();
        assert_eq!(left[0].tags, vec!["draft"], "the wrong row was taken");
    }

    /// The row the user clicked can still be that row while the *category* has
    /// moved on — an agent appended to it, or another window deleted from it,
    /// between the listing and the click. Deleting then acts on a state the user
    /// was never shown, so it refuses and the caller reloads.
    ///
    /// This is the case the old suite could not reach: it changed the file
    /// *before* the delete began, which the read inside `delete_entry` simply
    /// picked up. Here nothing about the clicked row changes at all.
    #[test]
    fn an_append_since_the_listing_refuses_the_delete() {
        let temp = tempdir().unwrap();
        let server = server_at(temp.path());
        write_store(&temp.path().join("global"), "clinical", "first\n\n");
        let listed = listing(&server, "clinical", MemoryScope::Global);

        // A conversation saves a memory while the user reads the list.
        server
            .remember("context", "clinical", "arrived afterwards", &[], true)
            .unwrap();

        let outcome = server
            .delete_entry(
                "clinical",
                MemoryScope::Global,
                0,
                &listed.entries[0].digest,
                &listed.revision,
            )
            .unwrap();
        assert_eq!(
            outcome,
            EntryDeletion::CategoryChanged,
            "the row is unchanged but the category is not the one that was listed"
        );
        let left = server
            .list_entries("clinical", MemoryScope::Global)
            .unwrap();
        assert_eq!(
            left.iter().map(|e| e.content.as_str()).collect::<Vec<_>>(),
            vec!["first", "arrived afterwards"],
            "a refused delete removes nothing"
        );

        // Reloading gives the current revision, and the delete goes through.
        let reloaded = listing(&server, "clinical", MemoryScope::Global);
        assert_eq!(
            server
                .delete_entry(
                    "clinical",
                    MemoryScope::Global,
                    0,
                    &reloaded.entries[0].digest,
                    &reloaded.revision,
                )
                .unwrap(),
            EntryDeletion::Deleted {
                remaining: 1,
                category_removed: false
            },
            "a refusal must be recoverable by reloading, or the button never works"
        );
    }

    /// The same compare-and-set on the whole-category delete. "Delete everything
    /// in `clinical`" is consent to lose the memories that were on the screen.
    #[test]
    fn an_append_since_the_listing_refuses_the_category_delete() {
        let temp = tempdir().unwrap();
        let server = server_at(temp.path());
        let dir = temp.path().join("global");
        write_store(&dir, "clinical", "one\n\ntwo\n\n");
        let listed = listing(&server, "clinical", MemoryScope::Global);

        server
            .remember("context", "clinical", "arrived afterwards", &[], true)
            .unwrap();

        assert_eq!(
            server
                .delete_category("clinical", MemoryScope::Global, &listed.revision)
                .unwrap(),
            CategoryDeletion::CategoryChanged
        );
        assert!(
            dir.join("clinical.txt").exists(),
            "a refused category delete removes nothing"
        );

        let reloaded = listing(&server, "clinical", MemoryScope::Global);
        assert_eq!(
            server
                .delete_category("clinical", MemoryScope::Global, &reloaded.revision)
                .unwrap(),
            CategoryDeletion::Deleted { removed_entries: 3 }
        );
    }

    #[test]
    fn an_index_past_the_end_deletes_nothing() {
        let temp = tempdir().unwrap();
        let server = server_at(temp.path());
        write_store(&temp.path().join("local"), "development", "only one\n\n");
        let listed = listing(&server, "development", MemoryScope::Local);

        assert_eq!(
            server
                .delete_entry(
                    "development",
                    MemoryScope::Local,
                    7,
                    &listed.entries[0].digest,
                    &listed.revision
                )
                .unwrap(),
            EntryDeletion::OutOfRange
        );
        assert_eq!(
            server
                .list_entries("development", MemoryScope::Local)
                .unwrap()
                .len(),
            1
        );
    }

    /// Deleting a category reports what it cost, so the confirmation can say
    /// "3 memories" rather than "some memories".
    #[test]
    fn deleting_a_category_reports_how_many_memories_it_held() {
        let temp = tempdir().unwrap();
        let server = server_at(temp.path());
        let dir = temp.path().join("global");
        write_store(&dir, "clinical", "one\n\ntwo\n\nthree\n\n");
        let listed = listing(&server, "clinical", MemoryScope::Global);

        assert_eq!(
            server
                .delete_category("clinical", MemoryScope::Global, &listed.revision)
                .unwrap(),
            CategoryDeletion::Deleted { removed_entries: 3 }
        );
        assert!(!dir.join("clinical.txt").exists());
        assert_eq!(
            server
                .delete_category("clinical", MemoryScope::Global, &listed.revision)
                .unwrap(),
            CategoryDeletion::Missing,
            "deleting a category that is already gone is not an error"
        );
    }

    /// The daemon is one process serving every window, so the store a
    /// management call operates on has to be the one it was handed — never the
    /// daemon's own working directory, and never a prompt-composing
    /// constructor that reads both stores to build a system prompt nobody sends.
    #[test]
    fn with_stores_manages_exactly_the_directories_it_was_given() {
        let temp = tempdir().unwrap();
        let global = temp.path().join("elsewhere-global");
        let local = temp.path().join("some-project/.biorouter/memory");
        write_store(&global, "clinical", "machine wide\n\n");
        write_store(&local, "development", "this project\n\n");

        let server = MemoryServer::with_stores(global.clone(), local.clone());
        assert_eq!(server.store_dir(MemoryScope::Global), global.as_path());
        assert_eq!(server.store_dir(MemoryScope::Local), local.as_path());
        assert_eq!(
            server
                .inventory(MemoryScope::Local)
                .unwrap()
                .categories
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>(),
            vec!["development"]
        );
        assert!(
            server.get_instructions().is_empty(),
            "a management server must not compose a system prompt out of the stores"
        );
    }

    /// #73's containment rules govern this door too. A category is a name; the
    /// management surface must not be the way round the checks the tools got.
    #[test]
    fn a_traversing_category_cannot_be_listed_or_deleted_here_either() {
        let temp = tempdir().unwrap();
        let outside = temp.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        let victim = outside.join("victim.txt");
        fs::write(&victim, "ORIGINAL\n").unwrap();

        let server = server_at(&temp.path().join("store"));
        for escaping in ["../../outside/victim", "/etc/hosts", ".."] {
            assert!(
                server.list_entries(escaping, MemoryScope::Global).is_err(),
                "list_entries accepted {escaping:?}"
            );
            assert!(
                server
                    .delete_category(escaping, MemoryScope::Global, "any")
                    .is_err(),
                "delete_category accepted {escaping:?}"
            );
            assert!(
                server
                    .delete_entry(escaping, MemoryScope::Global, 0, "any", "any")
                    .is_err(),
                "delete_entry accepted {escaping:?}"
            );
        }
        assert_eq!(
            fs::read_to_string(&victim).unwrap(),
            "ORIGINAL\n",
            "a file outside the store was touched"
        );
    }

    /// A stray non-memory file in the store directory is not a category and
    /// must not appear as a phantom, permanently empty one — the same rule
    /// `retrieve_all` follows after the #73 suffix-strip fix.
    #[test]
    fn a_file_without_the_txt_suffix_is_not_a_category() {
        let temp = tempdir().unwrap();
        let server = server_at(temp.path());
        let dir = temp.path().join("global");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("notes.md"), "not a memory").unwrap();
        fs::write(dir.join("real.txt"), "a memory\n\n").unwrap();

        let inventory = server.inventory(MemoryScope::Global).unwrap();
        assert_eq!(
            inventory
                .categories
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>(),
            vec!["real"]
        );
    }

    /// A category whose own name contains `.txt` survives the suffix strip
    /// (#73) on this path too, and is deletable by the name it is listed under.
    #[test]
    fn a_category_named_with_txt_inside_it_round_trips() {
        let temp = tempdir().unwrap();
        let server = server_at(temp.path());
        let dir = temp.path().join("global");
        write_store(&dir, "a.txt.b", "tricky\n\n");

        let inventory = server.inventory(MemoryScope::Global).unwrap();
        assert_eq!(
            inventory
                .categories
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>(),
            vec!["a.txt.b"]
        );
        assert_eq!(inventory.categories[0].entries.len(), 1);
        let listed = &inventory.categories[0];
        assert_eq!(
            server
                .delete_category("a.txt.b", MemoryScope::Global, &listed.revision)
                .unwrap(),
            CategoryDeletion::Deleted { removed_entries: 1 }
        );
    }
}
