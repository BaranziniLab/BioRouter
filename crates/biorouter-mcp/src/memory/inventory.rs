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
//! * [`MemoryServer::remove_specific_memory_internal`] deletes every entry that
//!   *contains* the given text as a substring. A user clicking delete on one row
//!   must lose that row, not every row it happens to be a prefix of.
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

use super::{validated_category, MemoryServer};

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
    /// [`MemoryServer::delete_entry`] takes the body back as a guard rather
    /// than trusting the index on its own.
    pub index: usize,
    /// Words from the entry's leading `# …` line; empty when it has none.
    pub tags: Vec<String>,
    /// The entry body, interior newlines preserved.
    pub content: String,
}

/// One category file, listed in full.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct MemoryCategoryInventory {
    pub name: String,
    pub entries: Vec<MemoryEntry>,
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
fn parse_entries(content: &str) -> Vec<MemoryEntry> {
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
            tags,
            content: body,
        });
    }
    entries
}

/// Re-serialize entries into the on-disk format, so a delete leaves a file the
/// memory tools still parse identically.
fn render_entries(entries: &[MemoryEntry]) -> String {
    let mut out = String::new();
    for entry in entries {
        if !entry.tags.is_empty() {
            out.push('#');
            out.push(' ');
            out.push_str(&entry.tags.join(" "));
            out.push('\n');
        }
        out.push_str(&entry.content);
        out.push_str("\n\n");
    }
    out
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

    /// Every entry in one category, in file order.
    pub fn list_entries(&self, category: &str, scope: MemoryScope) -> io::Result<Vec<MemoryEntry>> {
        let path = self.get_memory_file(category, scope.is_global())?;
        if !path.exists() {
            return Ok(Vec::new());
        }
        Ok(parse_entries(&fs::read_to_string(path)?))
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
            inventory.categories.push(MemoryCategoryInventory {
                name: category.to_string(),
                entries: self.list_entries(category, scope)?,
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

    /// Delete exactly one entry, identified by its position **and** by the body
    /// the caller was shown.
    ///
    /// The guard is the whole design. An index alone deletes whatever has since
    /// moved into that slot — and this store is appended to by an agent that
    /// may be running while the user is looking at the list. A substring match
    /// alone (what `remove_specific_memory` does) deletes every entry the text
    /// appears in. Position plus exact body deletes the row that was clicked,
    /// or nothing.
    pub fn delete_entry(
        &self,
        category: &str,
        scope: MemoryScope,
        index: usize,
        expected_content: &str,
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
        let mut entries = parse_entries(&fs::read_to_string(&path)?);
        let Some(found) = entries.get(index) else {
            return Ok(EntryDeletion::OutOfRange);
        };
        if found.content != expected_content {
            return Ok(EntryDeletion::ContentMismatch);
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

    /// Delete a whole category. Reports how many entries went with it, so the
    /// caller can tell the user what they actually lost.
    pub fn delete_category(&self, category: &str, scope: MemoryScope) -> io::Result<Option<usize>> {
        let path = self.get_memory_file(category, scope.is_global())?;
        // Counting and removing under one lock, so the count reported to the
        // user is the count that was actually destroyed.
        let Some(_lock) = self.lock_store_if_present(scope.is_global())? else {
            return Ok(None);
        };
        if !path.exists() {
            return Ok(None);
        }
        let removed = parse_entries(&fs::read_to_string(&path)?).len();
        fs::remove_file(&path)?;
        Ok(Some(removed))
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

    /// Deleting one row deletes that row. `remove_specific_memory` deletes
    /// every entry *containing* the text, which for a user clicking a trash
    /// icon on "black" would silently take "we use black for formatting" too.
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

        let outcome = server
            .delete_entry("development", MemoryScope::Local, 0, "black")
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

        server
            .delete_entry("personal", MemoryScope::Global, 0, "Wanjun")
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

        let outcome = server
            .delete_entry("clinical", MemoryScope::Global, 0, "the only note")
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

        // The user was shown "first" at index 0, but the agent has since
        // rewritten the file.
        write_store(&dir, "clinical", "something else entirely\n\nsecond\n\n");

        let outcome = server
            .delete_entry("clinical", MemoryScope::Global, 0, "first")
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

    #[test]
    fn an_index_past_the_end_deletes_nothing() {
        let temp = tempdir().unwrap();
        let server = server_at(temp.path());
        write_store(&temp.path().join("local"), "development", "only one\n\n");

        assert_eq!(
            server
                .delete_entry("development", MemoryScope::Local, 7, "only one")
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

        assert_eq!(
            server
                .delete_category("clinical", MemoryScope::Global)
                .unwrap(),
            Some(3)
        );
        assert!(!dir.join("clinical.txt").exists());
        assert_eq!(
            server
                .delete_category("clinical", MemoryScope::Global)
                .unwrap(),
            None,
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
                    .delete_category(escaping, MemoryScope::Global)
                    .is_err(),
                "delete_category accepted {escaping:?}"
            );
            assert!(
                server
                    .delete_entry(escaping, MemoryScope::Global, 0, "ORIGINAL")
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
        assert_eq!(
            server
                .delete_category("a.txt.b", MemoryScope::Global)
                .unwrap(),
            Some(1)
        );
    }
}
