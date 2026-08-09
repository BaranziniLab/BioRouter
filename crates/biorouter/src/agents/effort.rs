//! BR-63 (part 2): reasoning-effort control — the explore-vs-answer knob.
//!
//! Before this, the depth/latency tradeoff was left entirely to the model: the
//! loop had exactly one exploration budget (`max_turns`), reasoning effort was
//! inferred from a model-name suffix (`o3-high`), and Anthropic thinking was
//! only reachable through the process-wide `CLAUDE_THINKING_ENABLED` env var.
//! A user who wanted "just answer, don't go dig" or "take your time, this one
//! matters" had no control at all.
//!
//! [`ReasoningEffort`] is that control. It is a **per-turn** setting resolved
//! at the top of `reply_internal` and it moves three things together:
//!
//! | | provider effort | thinking budget | exploration caps |
//! |---|---|---|---|
//! | `Quick` | `low`  | off             | halved (floored) |
//! | `Normal`| —      | —               | — (unchanged)    |
//! | `Deep`  | `high` | 16k tokens      | doubled          |
//!
//! **`Normal` is the default and is a strict no-op**: it touches neither the
//! model config nor the caps, so a session that never sets an effort behaves
//! exactly as it did before this change. That is deliberate — new behavior
//! that changes defaults must be opt-in.
//!
//! Providers vary in what they support, so the mapping degrades gracefully:
//! the effort is recorded on [`ModelConfig`] and each provider format takes
//! what it understands (`reasoning_effort` for the OpenAI families, a
//! `thinking` block for Anthropic) and ignores the rest. A provider that
//! understands neither still gets the cap changes, which are provider-agnostic.
//!
//! The proposal also wants `Deep` to switch on the self-critique pass (BR-50)
//! and the done-ness gate (BR-48). Neither exists yet; when they land, they hang
//! off [`ReasoningEffort::Deep`] here rather than growing another flag.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use utoipa::ToSchema;

use crate::model::ModelConfig;

/// Thinking budget (tokens) requested in `Deep` mode for providers that take an
/// explicit budget (Anthropic). Overridable with `CLAUDE_THINKING_BUDGET`.
pub const DEEP_THINKING_BUDGET_TOKENS: u32 = 16_000;

/// Temperature `Quick` picks when the user has not pinned one — a short,
/// low-variance answer rather than a creative one.
pub const QUICK_TEMPERATURE: f32 = 0.0;

/// `Quick` halves the exploration caps but never squeezes them below this, so
/// "quick" still means "can run a couple of tools", not "can't do anything".
const QUICK_TURNS_FLOOR: u32 = 6;
const QUICK_TOOL_CALLS_FLOOR: u32 = 12;

/// How hard the agent should think this turn.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Default,
    Serialize,
    Deserialize,
    ToSchema,
    Hash,
    PartialOrd,
    Ord,
)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningEffort {
    /// Answer from what's already known; minimal exploration, no thinking.
    Quick,
    /// Whatever the provider/model does by default. A strict no-op.
    #[default]
    Normal,
    /// Take the time: max provider reasoning effort, a thinking budget, and a
    /// wider exploration budget.
    Deep,
}

impl ReasoningEffort {
    /// Parse a user-typed effort (`/effort deep`, a GUI value, a CLI flag).
    /// Accepts the provider-native synonyms too so `low|medium|high` works.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "quick" | "fast" | "low" => Some(Self::Quick),
            "normal" | "medium" | "default" | "balanced" => Some(Self::Normal),
            "deep" | "high" | "thorough" | "max" => Some(Self::Deep),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Quick => "quick",
            Self::Normal => "normal",
            Self::Deep => "deep",
        }
    }

    /// True for the default (`Normal`), which must not perturb anything.
    pub fn is_default(self) -> bool {
        self == Self::Normal
    }

    /// The OpenAI-family `reasoning_effort` value, if this effort pins one.
    /// `Normal` returns `None` so the provider's own default is left alone.
    pub fn provider_effort(self) -> Option<&'static str> {
        match self {
            Self::Quick => Some("low"),
            Self::Normal => None,
            Self::Deep => Some("high"),
        }
    }

    /// Anthropic-style extended-thinking budget in tokens; `None` = no thinking
    /// block (which is *not* the same as "thinking off" — only `Deep` asks for
    /// one, the others leave the provider default in place).
    pub fn thinking_budget(self) -> Option<u32> {
        match self {
            Self::Deep => Some(
                std::env::var("CLAUDE_THINKING_BUDGET")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(DEEP_THINKING_BUDGET_TOKENS),
            ),
            _ => None,
        }
    }

    /// Exploration cap for this turn, derived from the session's configured cap.
    /// Quick never *raises* a cap the user lowered, Deep never lowers one.
    pub fn scale_turns(self, base: u32) -> u32 {
        match self {
            Self::Quick => (base / 2).max(QUICK_TURNS_FLOOR).min(base),
            Self::Normal => base,
            Self::Deep => base.saturating_mul(2),
        }
    }

    pub fn scale_tool_calls(self, base: u32) -> u32 {
        match self {
            Self::Quick => (base / 2).max(QUICK_TOOL_CALLS_FLOOR).min(base),
            Self::Normal => base,
            Self::Deep => base.saturating_mul(2),
        }
    }

    /// Stamp this effort onto a model config. `Normal` returns the config
    /// untouched, so the provider is never rebuilt for the default.
    pub fn apply_to_model(self, mut config: ModelConfig) -> ModelConfig {
        if self.is_default() {
            return config;
        }
        config.reasoning_effort = Some(self);
        // A pinned temperature (BIOROUTER_TEMPERATURE / a provider preset) is a
        // deliberate user choice — only fill one in when there isn't one.
        if self == Self::Quick && config.temperature.is_none() {
            config.temperature = Some(QUICK_TEMPERATURE);
        }
        config
    }

    /// One-line summary for the `/effort` confirmation notification.
    pub fn describe(self) -> &'static str {
        match self {
            Self::Quick => {
                "quick: low reasoning effort, no extended thinking, halved exploration budget"
            }
            Self::Normal => "normal: the model's default depth (no overrides)",
            Self::Deep => {
                "deep: high reasoning effort, extended thinking, doubled exploration budget"
            }
        }
    }
}

/// Per-session sticky effort, set by the `/effort` slash command (CLI, TUI and
/// GUI all route slash commands through `Agent::execute_command`). A per-request
/// effort — the GUI's composer toggle, which travels on the chat request — takes
/// precedence over this; the registry is what a session falls back to.
#[derive(Default)]
pub struct EffortRegistry {
    efforts: Mutex<HashMap<String, ReasoningEffort>>,
}

impl EffortRegistry {
    pub async fn set(&self, session_id: &str, effort: ReasoningEffort) {
        let mut efforts = self.efforts.lock().await;
        if effort.is_default() {
            efforts.remove(session_id);
        } else {
            efforts.insert(session_id.to_string(), effort);
        }
    }

    pub async fn get(&self, session_id: &str) -> Option<ReasoningEffort> {
        self.efforts.lock().await.get(session_id).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_names_and_provider_synonyms() {
        assert_eq!(
            ReasoningEffort::parse("quick"),
            Some(ReasoningEffort::Quick)
        );
        assert_eq!(
            ReasoningEffort::parse(" Deep "),
            Some(ReasoningEffort::Deep)
        );
        assert_eq!(ReasoningEffort::parse("HIGH"), Some(ReasoningEffort::Deep));
        assert_eq!(ReasoningEffort::parse("low"), Some(ReasoningEffort::Quick));
        assert_eq!(
            ReasoningEffort::parse("medium"),
            Some(ReasoningEffort::Normal)
        );
        assert_eq!(ReasoningEffort::parse("sideways"), None);
    }

    #[test]
    fn default_is_normal_and_is_a_no_op() {
        let effort = ReasoningEffort::default();
        assert_eq!(effort, ReasoningEffort::Normal);
        assert!(effort.is_default());
        assert_eq!(effort.provider_effort(), None);
        assert_eq!(effort.thinking_budget(), None);
        assert_eq!(effort.scale_turns(100), 100);
        assert_eq!(effort.scale_tool_calls(200), 200);

        let base = ModelConfig::new_or_fail("gpt-4o");
        let applied = effort.apply_to_model(base.clone());
        assert!(applied.reasoning_effort.is_none());
        assert_eq!(applied.temperature, base.temperature);
    }

    #[test]
    fn quick_lowers_effort_and_caps() {
        let quick = ReasoningEffort::Quick;
        assert_eq!(quick.provider_effort(), Some("low"));
        assert_eq!(quick.thinking_budget(), None);
        assert_eq!(quick.scale_turns(100), 50);
        assert_eq!(quick.scale_tool_calls(200), 100);
    }

    #[test]
    fn quick_never_raises_a_cap_the_user_lowered() {
        // Floor must not push a deliberately tiny budget back up.
        assert_eq!(ReasoningEffort::Quick.scale_turns(3), 3);
        assert_eq!(ReasoningEffort::Quick.scale_tool_calls(4), 4);
        // …but a normal-sized budget is floored, not driven to 1.
        assert_eq!(ReasoningEffort::Quick.scale_turns(8), QUICK_TURNS_FLOOR);
    }

    #[test]
    fn deep_widens_caps_without_overflowing() {
        let deep = ReasoningEffort::Deep;
        assert_eq!(deep.provider_effort(), Some("high"));
        assert_eq!(deep.scale_turns(100), 200);
        assert_eq!(deep.scale_tool_calls(200), 400);
        assert_eq!(deep.scale_turns(u32::MAX), u32::MAX);
    }

    #[test]
    fn quick_sets_a_low_temperature_only_when_unpinned() {
        let mut config = ModelConfig::new_or_fail("gpt-4o");
        config.temperature = None;
        let applied = ReasoningEffort::Quick.apply_to_model(config);
        assert_eq!(applied.temperature, Some(QUICK_TEMPERATURE));
        assert_eq!(applied.reasoning_effort, Some(ReasoningEffort::Quick));

        let mut pinned = ModelConfig::new_or_fail("gpt-4o");
        pinned.temperature = Some(0.9);
        let applied = ReasoningEffort::Quick.apply_to_model(pinned);
        assert_eq!(applied.temperature, Some(0.9));
    }

    #[test]
    fn deep_stamps_the_effort_and_leaves_temperature_alone() {
        let mut config = ModelConfig::new_or_fail("claude-sonnet-4-5");
        config.temperature = None;
        let applied = ReasoningEffort::Deep.apply_to_model(config);
        assert_eq!(applied.reasoning_effort, Some(ReasoningEffort::Deep));
        assert_eq!(applied.temperature, None);
    }

    #[test]
    fn serde_round_trips_lowercase() {
        let json = serde_json::to_string(&ReasoningEffort::Deep).unwrap();
        assert_eq!(json, "\"deep\"");
        let parsed: ReasoningEffort = serde_json::from_str("\"quick\"").unwrap();
        assert_eq!(parsed, ReasoningEffort::Quick);
    }

    #[tokio::test]
    async fn registry_is_per_session_and_clears_on_normal() {
        let registry = EffortRegistry::default();
        assert_eq!(registry.get("s1").await, None);

        registry.set("s1", ReasoningEffort::Deep).await;
        assert_eq!(registry.get("s1").await, Some(ReasoningEffort::Deep));
        assert_eq!(registry.get("s2").await, None);

        // Back to the default → the session stops carrying an override.
        registry.set("s1", ReasoningEffort::Normal).await;
        assert_eq!(registry.get("s1").await, None);
    }
}
