//! Typed declaration parameters: the schema *is* the documentation.
//!
//! Before this, `surface` and `theme` had **no tool parameter at all** — the
//! authoring instructions told the model to "seed the surface in `create_app`", a
//! field that did not exist. The only way to declare a contract or pick a theme
//! was to rewrite the whole `manifest.json` through `update_app`, which:
//!
//!   * hard-failed on `missing field created_at` (metadata a model has no way to
//!     know and no business inventing);
//!   * surfaced raw serde errors like `invalid type: sequence, expected a map` for
//!     the internally-tagged and map-shaped fields, with no hint of the shape; and
//!   * wrote the model's bytes verbatim, silently destroying `built_at`,
//!     `sdk_hash` and `session_id`.
//!
//! The result was ~6 rejected manifest mutations per app: the model guessing Rust
//! types from serde errors. These params move the schema into the *tool schema the
//! model is handed*, so a shape the contract forbids cannot be emitted at all —
//! and `pack` becomes an enum, so an invalid theme is a schema rejection rather
//! than a silent fallback to the default.

use std::collections::HashMap;

use schemars::JsonSchema;
use serde::Deserialize;

use super::manifest::{
    ActionDecl, ActionEffect, ComponentDecl, ModelRoute, SignalDecl, SurfaceDecl, ThemeConfig,
};

/// One app verb the agent may invoke via `app_call`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ActionParam {
    /// Verb name, e.g. `apply_intervention`. Must match the name the app's
    /// `main.ts` registers with `br.actions.register(...)`.
    pub name: String,
    /// What calling it does, in one line. The agent picks actions from this.
    #[serde(default)]
    pub description: Option<String>,
    /// JSON Schema for the arguments. Omit for unconstrained.
    #[serde(default)]
    pub params: Option<serde_json::Value>,
    /// `"read"` (default) or `"mutate"`. Declare `mutate` for any action that
    /// CHANGES the app — an intervention, a parameter, a committed edit.
    #[serde(default)]
    pub effect: Option<ActionEffect>,
    /// JSON Pointers into the shared state this action's handler owns, e.g.
    /// `["/params/lion_vision"]`. Only meaningful for a `mutate` action.
    ///
    /// Declaring them makes the platform REFUSE any direct `ui_state` /
    /// `ui_patch_state` write to those paths, so the value on the page can only
    /// move by calling your handler. Without it the agent can write the number
    /// itself and narrate a change your app never made.
    #[serde(default)]
    pub writes: Vec<String>,
    /// Named inputs this action's output depends on, e.g. `["sumstats", "ld"]`.
    /// If a worker reports one of them missing, a non-synthetic call is REFUSED —
    /// which is what stops the agent from inventing the numbers instead.
    #[serde(default)]
    pub requires_evidence: Vec<String>,
    /// Require every call to declare where its numbers came from. Set this on any
    /// action that publishes statistics.
    #[serde(default)]
    pub provenance_required: Option<bool>,
}

/// One app→agent notification.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SignalParam {
    /// Signal name, e.g. `criterion_clicked`.
    pub name: String,
    /// JSON Schema for the payload. Omit for unconstrained.
    #[serde(default)]
    pub payload: Option<serde_json::Value>,
    /// Minimum ms between deliveries (server-side coalescing). Omit for the default.
    #[serde(default)]
    pub coalesce_ms: Option<u64>,
    /// Whether this signal may start a turn on its own. Default false (queue-only:
    /// it becomes context for the next turn). Autorun additionally requires the
    /// user-granted `ui.allow_autorun`.
    #[serde(default)]
    pub autorun: Option<bool>,
    /// Whether declaring this signal also SUBSCRIBES the agent to it. Default
    /// true — you almost never want otherwise. The user's first click happens
    /// before the agent's first tool call, so a signal that requires an explicit
    /// `ui_subscribe` is dropped on the floor exactly when it matters most.
    #[serde(default)]
    pub eager: Option<bool>,
}

/// One custom catalog component the app registers.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ComponentParam {
    pub name: String,
    /// JSON Schema for the props. Omit for unconstrained.
    #[serde(default)]
    pub props: Option<serde_json::Value>,
}

/// The app's declared contract.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct SurfaceParam {
    /// JSON Schema for the shared state document.
    #[serde(default)]
    pub state_schema: Option<serde_json::Value>,
    /// The shared state document's INITIAL value. Declare this whenever the app
    /// has `data-br-bind` bindings: without it every bound element renders blank
    /// until the first (paid) agent turn writes to the doc, and the natural
    /// workaround — a private local `state` object — silently diverges from the
    /// document the agent actually reads.
    #[serde(default)]
    pub state_initial: Option<serde_json::Value>,
    /// Verbs the AGENT may call on the app.
    #[serde(default)]
    pub actions: Vec<ActionParam>,
    /// Notifications the APP sends the agent.
    #[serde(default)]
    pub signals: Vec<SignalParam>,
    /// Custom render components the app registers.
    #[serde(default)]
    pub components: Vec<ComponentParam>,
}

impl SurfaceParam {
    /// Convert to the manifest shape, filling defaults.
    pub fn into_decl(self) -> SurfaceDecl {
        SurfaceDecl {
            state_schema: self.state_schema,
            state_initial: self.state_initial,
            actions: self
                .actions
                .into_iter()
                .map(|a| ActionDecl {
                    name: a.name,
                    description: a.description.unwrap_or_default(),
                    params: a.params.unwrap_or_else(|| serde_json::json!({})),
                    effect: a.effect.unwrap_or_default(),
                    writes: a.writes,
                    requires_evidence: a.requires_evidence,
                    provenance_required: a.provenance_required.unwrap_or(false),
                })
                .collect(),
            signals: self
                .signals
                .into_iter()
                .map(|s| {
                    let default = SignalDecl::default();
                    SignalDecl {
                        name: s.name,
                        payload: s.payload,
                        coalesce_ms: s.coalesce_ms.unwrap_or(default.coalesce_ms),
                        autorun: s.autorun.unwrap_or(default.autorun),
                        eager: s.eager.unwrap_or(default.eager),
                    }
                })
                .collect(),
            components: self
                .components
                .into_iter()
                .map(|c| ComponentDecl {
                    name: c.name,
                    props: c.props.unwrap_or_else(|| serde_json::json!({})),
                })
                .collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.state_schema.is_none()
            && self.state_initial.is_none()
            && self.actions.is_empty()
            && self.signals.is_empty()
            && self.components.is_empty()
    }
}

/// The curated theme packs, as an **enum** rather than a free string.
///
/// A misspelled pack used to fall through `ThemeConfig::resolved_pack()` to the
/// default silently, so the app rendered with a look the author never asked for
/// and nothing said why. As a schema enum the model cannot emit an unknown pack.
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ThemePack {
    /// The base Biorouter look.
    Biorouter,
    /// Clean clinical/EHR surfaces.
    Clinical,
    /// Warm paper, for notebook-style apps.
    LabNotebook,
    /// High-contrast monospace.
    Terminal,
    /// Typeset, for reading-heavy apps.
    Journal,
    /// Dark, for canvas/visualisation apps.
    Midnight,
}

impl ThemePack {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Biorouter => "biorouter",
            Self::Clinical => "clinical",
            Self::LabNotebook => "lab-notebook",
            Self::Terminal => "terminal",
            Self::Journal => "journal",
            Self::Midnight => "midnight",
        }
    }
}

/// Theme selection.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ThemeParam {
    /// The curated pack.
    pub pack: ThemePack,
    /// Accent colour override (a CSS colour; sanitized server-side).
    #[serde(default)]
    pub accent: Option<String>,
    /// Individual `--br-*` token overrides.
    #[serde(default)]
    pub tokens: Option<HashMap<String, String>>,
}

impl ThemeParam {
    pub fn into_config(self) -> ThemeConfig {
        ThemeConfig {
            pack: self.pack.as_str().to_string(),
            accent: self.accent,
            tokens: self.tokens.unwrap_or_default(),
        }
    }
}

/// One named model route a `call` may select per invocation.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct RouteParam {
    /// Route name, e.g. `fast` or `deep`.
    pub name: String,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
}

/// A worker-profile key must be a stable identifier, not a display name.
///
/// `consult(agent: "Prosecutor")` 404'd against a manifest keyed `prosecutor`,
/// because the lookup is an exact map hit. Rejecting display-name keys at
/// declaration time is half the fix (the other half is tolerant resolution in
/// `consult`); together they mean the mismatch cannot arise.
pub fn validate_profile_key(key: &str) -> Result<(), String> {
    if key.is_empty() {
        return Err("a worker-profile key may not be empty".to_string());
    }
    if !key
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        return Err(format!(
            "worker-profile key '{key}' must be a stable identifier: lowercase letters, digits \
             and underscores only (e.g. `prosecutor`, `fine_mapper`). Put the display name in \
             the profile's `description`; the key is what `consult(agent: …)` targets, and a \
             capitalised or spaced key silently fails to resolve."
        ));
    }
    Ok(())
}

/// Build the manifest's route map from the typed params.
pub fn routes_from_params(routes: Vec<RouteParam>) -> HashMap<String, ModelRoute> {
    routes
        .into_iter()
        .map(|r| {
            (
                r.name,
                ModelRoute {
                    provider: r.provider,
                    model: r.model,
                },
            )
        })
        .collect()
}

/// A one-line canonical example for each free-form manifest field, appended to
/// serde's own error. Serde says "invalid type: sequence, expected a map"; that
/// tells a model *that* it is wrong, never *what right looks like*.
pub fn shape_hint(field: &str) -> Option<&'static str> {
    match field {
        "orchestration" => Some(
            r#"{"agents":{"<key>":{"system_prompt":"…","model":{"provider":"…","model":"…"}}},"routes":{"fast":{"model":"…"}},"workflows":{"<name>":{"steps":[{"type":"agent","agent":"<key>"}]}}}"#,
        ),
        "capabilities" => Some(
            r#"{"ui":{"enabled":true},"data":{"sources":[{"name":"…","kind":"knowledge","ref_id":"…"}]},"events":["tool","handoff"]}"#,
        ),
        "guardrails" => Some(r#"{"goal":"…","require_approval":["…"],"deny":["…"]}"#),
        "reliability" => Some(r#"{"tool_timeout_s":60,"max_retries":2}"#),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_drafter::manifest::THEME_PACKS;

    #[test]
    fn every_theme_pack_variant_maps_to_a_real_pack() {
        for pack in [
            ThemePack::Biorouter,
            ThemePack::Clinical,
            ThemePack::LabNotebook,
            ThemePack::Terminal,
            ThemePack::Journal,
            ThemePack::Midnight,
        ] {
            assert!(
                THEME_PACKS.contains(&pack.as_str()),
                "'{}' is not in THEME_PACKS: the enum has drifted from the CSS",
                pack.as_str()
            );
        }
    }

    /// The enum is what makes an invalid pack unrepresentable.
    #[test]
    fn an_unknown_pack_fails_to_deserialize_rather_than_silently_defaulting() {
        let bad = serde_json::json!({"pack": "solarized"});
        assert!(serde_json::from_value::<ThemeParam>(bad).is_err());
    }

    /// Explicitly choosing the default pack must survive — this is the case whose
    /// omission from the serialized manifest produced a false bug report.
    #[test]
    fn explicitly_choosing_the_default_pack_is_accepted() {
        let param: ThemeParam = serde_json::from_value(serde_json::json!({"pack": "biorouter"}))
            .expect("the default pack is a valid choice");
        assert_eq!(param.into_config().resolved_pack(), "biorouter");
    }

    #[test]
    fn a_display_name_profile_key_is_rejected_with_the_reason() {
        let err = validate_profile_key("Prosecutor").unwrap_err();
        assert!(
            err.contains("consult"),
            "must explain the consequence: {err}"
        );
        assert!(
            err.contains("description"),
            "must offer the right home for a display name"
        );

        assert!(validate_profile_key("prosecutor").is_ok());
        assert!(validate_profile_key("fine_mapper").is_ok());
        assert!(validate_profile_key("agent 2").is_err());
        assert!(validate_profile_key("").is_err());
    }

    /// A surface declared in one typed call, with defaults filled in — no manifest
    /// rewrite, no guessing.
    #[test]
    fn a_surface_param_becomes_a_complete_decl() {
        let param: SurfaceParam = serde_json::from_value(serde_json::json!({
            "actions": [{"name": "apply_intervention", "description": "Apply it"}],
            "signals": [{"name": "criterion_clicked"}],
        }))
        .unwrap();

        let decl = param.into_decl();
        assert_eq!(decl.actions.len(), 1);
        assert_eq!(decl.actions[0].name, "apply_intervention");
        assert_eq!(decl.signals.len(), 1);
        // Defaults are filled from the manifest type, not invented by the caller.
        assert_eq!(
            decl.signals[0].coalesce_ms,
            SignalDecl::default().coalesce_ms
        );
        assert!(!decl.signals[0].autorun, "autorun must stay opt-in");
    }

    #[test]
    fn shape_hints_exist_for_the_free_form_fields_models_guess_at() {
        assert!(shape_hint("orchestration").unwrap().contains("agents"));
        assert!(shape_hint("capabilities").unwrap().contains("ui"));
        assert!(shape_hint("nonexistent").is_none());
    }
}
