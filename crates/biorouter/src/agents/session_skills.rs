//! BR-71 decision (c): per-SESSION skill enablement.
//!
//! `workspace_set_tools { add_skills, remove_skills }` scopes skills to one
//! conversation. It must never write `~/.config/biorouter/skills-config.json`
//! — that file is the machine-wide user preference shared by the GUI toggles
//! and `biorouter skill enable/disable`
//! (`biorouter-cli/src/commands/skill.rs`), and rewriting it from an agent
//! tool would change every other conversation, window and CLI invocation.
//!
//! The override is stored where every other per-session extension state lives:
//! `Session.extension_data` under `("workspace_skills", "v1")` — the
//! `set_extension_state` precedent of `agents/goal.rs` and
//! `guardrails/run_state.rs`.
//!
//! **The session row is the only copy.** An earlier draft kept a process-wide
//! `HashMap<session_id, override>` in front of it, hydrated once per session.
//! That is unsound in three ways, each of which this module now avoids by
//! construction:
//!
//! * Session ids are display ids allocated as `YYYYMMDD_<max+1>` and are handed
//!   back after a delete or a `/reset` History, so a cache keyed by the id
//!   follows that id into an unrelated new conversation — the previous
//!   occupant's grants with it.
//! * A cached read cannot distinguish "no override" from "could not read it",
//!   so a transient failure or a corrupt value is cached permanently as a
//!   grant of everything the session had revoked. Both reads here fail CLOSED
//!   instead, returning an error the caller must handle.
//! * A merge based on the cache rather than on the persisted value silently
//!   drops whatever another writer committed in between.
//!
//! Reading the row costs one indexed `SELECT` of a single column
//! ([`SessionManager::get_extension_state`]) on a path that runs a handful of
//! times per turn — three skill tools, called deliberately by the model — not
//! a hot loop.

use std::collections::HashSet;

use anyhow::{Context, Result};

use crate::session::SessionManager;

pub const STATE_KEY: &str = "workspace_skills";
pub const STATE_VERSION: &str = "v1";

/// One session's deviation from the machine-wide skill set.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SessionSkillOverride {
    /// Enabled for this session even if machine-wide disabled.
    #[serde(default)]
    pub add: Vec<String>,
    /// Disabled for this session even if machine-wide enabled.
    #[serde(default)]
    pub remove: Vec<String>,
}

impl SessionSkillOverride {
    /// The composition rule, in one place: an explicit session `add` wins over
    /// everything, then an explicit session `remove`, then the machine-wide
    /// disabled set.
    pub fn is_disabled(&self, skill_name: &str, machine_disabled: &HashSet<String>) -> bool {
        if self.add.iter().any(|s| s == skill_name) {
            return false;
        }
        if self.remove.iter().any(|s| s == skill_name) {
            return true;
        }
        machine_disabled.contains(skill_name)
    }

    pub fn is_empty(&self) -> bool {
        self.add.is_empty() && self.remove.is_empty()
    }

    fn merge(mut self, add_skills: &[String], remove_skills: &[String]) -> Self {
        for name in remove_skills {
            self.add.retain(|s| s != name);
            if !self.remove.iter().any(|s| s == name) {
                self.remove.push(name.clone());
            }
        }
        for name in add_skills {
            self.remove.retain(|s| s != name);
            if !self.add.iter().any(|s| s == name) {
                self.add.push(name.clone());
            }
        }
        self
    }
}

fn decode(value: serde_json::Value) -> Result<SessionSkillOverride> {
    serde_json::from_value(value).context(
        "this session's persisted skill override (workspace_skills/v1) is unreadable; \
         refusing to fall back to the machine-wide skill set, which would restore \
         skills this conversation revoked",
    )
}

/// This session's persisted override, read from the session row.
///
/// Fails CLOSED: a read error or a corrupt value is an `Err`, never a silent
/// [`SessionSkillOverride::default()`]. A session that has simply never been
/// touched by `workspace_set_tools` yields the default, which is correct.
pub async fn for_session(
    session_manager: &SessionManager,
    session_id: &str,
) -> Result<SessionSkillOverride> {
    match session_manager
        .get_extension_state(session_id, STATE_KEY, STATE_VERSION)
        .await
        .with_context(|| format!("reading session skill override for {session_id}"))?
    {
        Some(value) => decode(value),
        None => Ok(SessionSkillOverride::default()),
    }
}

/// Merge `add_skills` / `remove_skills` into the session's override and persist
/// it. Idempotent; a name appearing in both lists ends up in `add` only, which
/// matches [`SessionSkillOverride::is_disabled`]'s precedence.
///
/// The read-merge-write happens inside one transaction
/// ([`SessionManager::update_extension_state`]), so a concurrent grant — or a
/// concurrent write to any OTHER key of the same `extension_data` column, such
/// as a goal or a todo list — cannot be lost. A corrupt persisted value aborts
/// the transaction rather than being overwritten.
pub async fn apply(
    session_manager: &SessionManager,
    session_id: &str,
    add_skills: &[String],
    remove_skills: &[String],
) -> Result<SessionSkillOverride> {
    let add = add_skills.to_vec();
    let remove = remove_skills.to_vec();
    let stored = session_manager
        .update_extension_state(session_id, STATE_KEY, STATE_VERSION, move |current| {
            let base = match current {
                Some(value) => decode(value.clone())?,
                None => SessionSkillOverride::default(),
            };
            Ok(serde_json::to_value(base.merge(&add, &remove))?)
        })
        .await?
        .with_context(|| format!("session '{session_id}' not found"))?;
    decode(stored)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn manager_and_session(temp: &tempfile::TempDir, name: &str) -> (SessionManager, String) {
        let sm = SessionManager::new(temp.path().to_path_buf());
        let session = sm
            .create_session(
                temp.path().to_path_buf(),
                name.to_string(),
                crate::session::SessionType::User,
            )
            .await
            .unwrap();
        (sm, session.id)
    }

    #[test]
    fn overrides_compose_add_over_remove_over_machine_wide() {
        let machine_disabled: std::collections::HashSet<String> =
            ["a".to_string(), "b".to_string()].into_iter().collect();

        let empty = SessionSkillOverride::default();
        assert!(empty.is_disabled("a", &machine_disabled));
        assert!(!empty.is_disabled("c", &machine_disabled));

        // add re-enables a machine-disabled skill FOR THIS SESSION ONLY.
        let added = SessionSkillOverride {
            add: vec!["a".into()],
            remove: vec![],
        };
        assert!(!added.is_disabled("a", &machine_disabled));
        assert!(added.is_disabled("b", &machine_disabled));

        // remove disables a machine-enabled skill for this session.
        let removed = SessionSkillOverride {
            add: vec![],
            remove: vec!["c".into()],
        };
        assert!(removed.is_disabled("c", &machine_disabled));

        // An explicit add wins over an explicit remove: the last write in one
        // call is `add`, and a tool that both adds and removes the same name is
        // asking for it to be present.
        let both = SessionSkillOverride {
            add: vec!["c".into()],
            remove: vec!["c".into()],
        };
        assert!(!both.is_disabled("c", &machine_disabled));
    }

    #[tokio::test]
    async fn apply_persists_to_extension_data_and_never_touches_the_machine_file() {
        let temp = tempfile::TempDir::new().unwrap();
        let (sm, id) = manager_and_session(&temp, "skills").await;

        apply(
            &sm,
            &id,
            &["single-cell".to_string()],
            &["ralph".to_string()],
        )
        .await
        .unwrap();

        // Persisted in the session row, under the documented key.
        let reread = sm.get_session(&id, false).await.unwrap();
        let stored = reread
            .extension_data
            .get_extension_state(STATE_KEY, STATE_VERSION)
            .expect("override persisted");
        assert_eq!(stored["add"][0], "single-cell");
        assert_eq!(stored["remove"][0], "ralph");

        // And readable back through the documented reader.
        let live = for_session(&sm, &id).await.unwrap();
        assert!(live.add.contains(&"single-cell".to_string()));
        assert!(live.remove.contains(&"ralph".to_string()));
    }

    /// The session row IS the state — there is no process-wide cache to warm,
    /// so a cold process (here: a fresh manager over the same directory, after
    /// the writer's pool is closed) reads the override with no hydration step.
    /// This is also what makes a reused session id safe: ids are handed out as
    /// `YYYYMMDD_<max+1>` and come back after a delete or a History reset, and
    /// a cached override keyed by that id would follow the id into an unrelated
    /// new conversation.
    #[tokio::test]
    async fn the_override_survives_a_process_restart() {
        let temp = tempfile::TempDir::new().unwrap();
        let id = {
            let (sm, id) = manager_and_session(&temp, "skills-2").await;
            apply(&sm, &id, &["proteomics".to_string()], &[])
                .await
                .unwrap();
            sm.close().await;
            id
        };

        let cold = SessionManager::new(temp.path().to_path_buf());
        assert!(for_session(&cold, &id)
            .await
            .unwrap()
            .add
            .contains(&"proteomics".to_string()));
    }

    /// A recreated session that inherits a previously-used id must start with
    /// no override. (With a process-wide cache keyed by the display id this is
    /// exactly the leak: session 1 is granted a skill, History is reset, and
    /// the next unrelated conversation is handed the same id — and the grant.)
    #[tokio::test]
    async fn a_reused_session_id_does_not_inherit_the_previous_occupants_override() {
        let temp = tempfile::TempDir::new().unwrap();
        let (sm, id) = manager_and_session(&temp, "first-occupant").await;
        apply(&sm, &id, &["single-cell".to_string()], &[])
            .await
            .unwrap();

        // What `/reset` History does: empty `sessions`, then create afresh.
        sm.clear_all_sessions().await.unwrap();
        let recreated = sm
            .create_session(
                temp.path().to_path_buf(),
                "second-occupant".to_string(),
                crate::session::SessionType::User,
            )
            .await
            .unwrap();
        assert_eq!(
            recreated.id, id,
            "the id allocator hands the id straight back"
        );

        assert_eq!(
            for_session(&sm, &recreated.id).await.unwrap(),
            SessionSkillOverride::default(),
            "a new conversation must not inherit a deleted one's skill grants"
        );
    }

    /// Fail CLOSED, and never destroy what you cannot read: a corrupt override
    /// is refused by both the reader and the writer rather than silently reset
    /// to "no deviation" (which would restore every skill the session revoked).
    #[tokio::test]
    async fn a_corrupt_override_is_refused_by_both_the_reader_and_the_writer() {
        let temp = tempfile::TempDir::new().unwrap();
        let (sm, id) = manager_and_session(&temp, "skills-corrupt").await;
        sm.update_extension_state(&id, STATE_KEY, STATE_VERSION, |_| {
            Ok(serde_json::json!("not an override"))
        })
        .await
        .unwrap();

        assert!(for_session(&sm, &id).await.is_err());
        assert!(apply(&sm, &id, &["single-cell".to_string()], &[])
            .await
            .is_err());
        // ...and the unreadable value is still there, not overwritten.
        assert_eq!(
            sm.get_extension_state(&id, STATE_KEY, STATE_VERSION)
                .await
                .unwrap(),
            Some(serde_json::json!("not an override"))
        );
    }

    #[tokio::test]
    async fn applying_to_a_missing_session_is_an_error_not_a_silent_grant() {
        let temp = tempfile::TempDir::new().unwrap();
        let sm = SessionManager::new(temp.path().to_path_buf());
        assert!(
            apply(&sm, "no-such-session", &["single-cell".to_string()], &[])
                .await
                .is_err()
        );
    }
}
