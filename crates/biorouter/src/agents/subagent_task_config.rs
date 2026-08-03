use crate::agents::ExtensionConfig;
use crate::privacy::SessionClassification;
use crate::providers::base::Provider;
use std::env;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Default maximum number of turns for task execution
pub const DEFAULT_SUBAGENT_MAX_TURNS: usize = 25;

/// Environment variable name for configuring max turns
pub const BIOROUTER_SUBAGENT_MAX_TURNS_ENV_VAR: &str = "BIOROUTER_SUBAGENT_MAX_TURNS";

/// Configuration for task execution with all necessary dependencies
///
/// **R5 (issue #56) needs no mechanism here, and that is worth stating.** A
/// child inherits "the same worker/leader mode the parent is operating under"
/// literally rather than by copying settings: `provider` is the parent's *same*
/// `Arc<dyn Provider>` (see `Agent::dispatch`'s `TaskConfig::new` call site), so
/// a child of a lead/worker parent runs the identical composite with the
/// identical split, and `tier()` on it is the same `least()` over the same
/// components.
///
/// One consequence, written down so a future reader does not "fix" it: sharing
/// the instance also shares its mutable `turn_count` / `failure_count` /
/// `in_fallback_mode`, so a subagent's turns advance the parent's lead→worker
/// transition. Pre-existing and orthogonal — cloning the wrapper to separate
/// them would split the tier computation this task depends on.
#[derive(Clone)]
pub struct TaskConfig {
    pub provider: Arc<dyn Provider>,
    pub parent_session_id: String,
    pub parent_working_dir: PathBuf,
    pub extensions: Vec<ExtensionConfig>,
    pub max_turns: Option<usize>,
    /// Issue #56 §8.2: the classification the child's row is BORN with — the
    /// floor established by the capability of the provider the child actually
    /// resolved, not by the parent's classification.
    ///
    /// Established by `subagent_tool::apply_settings_overrides`, which both
    /// spawn paths run **before** `create_subagent_session`; the value seeded by
    /// [`TaskConfig::new`] is never the one that reaches a row. Public is the
    /// seed because it is what the `sessions.privacy_tier` column defaults to
    /// and because the stamp is a `raise_privacy` — a seed of Private would
    /// permanently over-classify any child whose stamp were ever skipped, and a
    /// ratchet is not revisited.
    pub privacy_tier: SessionClassification,
    //
    // There is deliberately NO downgrade-confirmation flag on this struct
    // (DR-19), and its former name is not written here either — a Step 5 gate
    // greps the crate for it and expects silence, so that a re-add is visible
    // in review rather than buried in prose that has always mentioned it.
    // A private parent naming a public model for its child is refused in
    // `subagent_tool::apply_settings_overrides`, not flagged: the field this
    // struct used to carry had no reader anywhere in the tree — no surface
    // rendered it, no handler branched on it — and a field nothing reads is
    // indistinguishable, in review and in a later audit, from a control that
    // exists. If a future ruling reinstates an approval, it arrives together
    // with its consumer, not before one.
    //
    /// Issue #56 §8.2: private extensions the tier filter removed from the
    /// child's inherited set, by name, so the drop can be reported in the tool
    /// result. A capability that vanishes silently is one the model will keep
    /// planning around.
    pub dropped_private_extensions: Vec<String>,
}

impl fmt::Debug for TaskConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TaskConfig")
            .field("provider", &"<dyn Provider>")
            .field("parent_session_id", &self.parent_session_id)
            .field("parent_working_dir", &self.parent_working_dir)
            .field("max_turns", &self.max_turns)
            .field("extensions", &self.extensions)
            .field("privacy_tier", &self.privacy_tier)
            .field(
                "dropped_private_extensions",
                &self.dropped_private_extensions,
            )
            .finish()
    }
}

impl TaskConfig {
    pub fn new(
        provider: Arc<dyn Provider>,
        parent_session_id: &str,
        parent_working_dir: &Path,
        extensions: Vec<ExtensionConfig>,
    ) -> Self {
        Self {
            provider,
            parent_session_id: parent_session_id.to_owned(),
            parent_working_dir: parent_working_dir.to_owned(),
            extensions,
            max_turns: Some(
                env::var(BIOROUTER_SUBAGENT_MAX_TURNS_ENV_VAR)
                    .ok()
                    .and_then(|val| val.parse::<usize>().ok())
                    .unwrap_or(DEFAULT_SUBAGENT_MAX_TURNS),
            ),
            // Seeds, not decisions. `subagent_tool::apply_settings_overrides`
            // establishes both, and it runs before the child's row exists.
            // Deliberately NOT `floor(provider.tier())` here: `floor` is the one
            // crossing between the capability and classification lattices and its
            // caller set is asserted by a repo-grep audit, so the crossing lives
            // at the single site that decides it.
            privacy_tier: SessionClassification::Public,
            dropped_private_extensions: Vec::new(),
        }
    }
}
