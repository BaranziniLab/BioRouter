use super::base::Config;
use crate::agents::extension::PLATFORM_EXTENSIONS;
use crate::agents::ExtensionConfig;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_yaml::Mapping;
use tracing::warn;
use utoipa::ToSchema;

pub const DEFAULT_EXTENSION: &str = "developer";
pub const DEFAULT_EXTENSION_TIMEOUT: u64 = 300;
pub const DEFAULT_EXTENSION_DESCRIPTION: &str = "";
pub const DEFAULT_DISPLAY_NAME: &str = "Developer";
const EXTENSIONS_CONFIG_KEY: &str = "extensions";

#[derive(Debug, Deserialize, Serialize, Clone, ToSchema)]
pub struct ExtensionEntry {
    pub enabled: bool,
    #[serde(flatten)]
    pub config: ExtensionConfig,
}

pub fn name_to_key(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_lowercase()
}

fn get_extensions_map() -> IndexMap<String, ExtensionEntry> {
    let raw: Mapping = Config::global()
        .get_param(EXTENSIONS_CONFIG_KEY)
        .unwrap_or_else(|err| {
            warn!(
                "Failed to load {}: {err}. Falling back to empty object.",
                EXTENSIONS_CONFIG_KEY
            );
            Default::default()
        });

    let mut extensions_map = IndexMap::with_capacity(raw.len());
    for (k, v) in raw {
        match (k, serde_yaml::from_value::<ExtensionEntry>(v)) {
            (serde_yaml::Value::String(key), Ok(entry)) => {
                extensions_map.insert(key, entry);
            }
            (k, v) => {
                warn!(
                    key = ?k,
                    value = ?v,
                    "Skipping malformed extension config entry"
                );
            }
        }
    }

    inject_platform_extensions(&mut extensions_map);
    extensions_map
}

fn inject_platform_extensions(extensions: &mut IndexMap<String, ExtensionEntry>) {
    for (key, def) in PLATFORM_EXTENSIONS.iter() {
        let configured_platform = extensions
            .get(*key)
            .filter(|entry| platform_entry_matches_key(entry, key))
            .cloned()
            .or_else(|| {
                extensions
                    .values()
                    .find(|entry| platform_entry_matches_key(entry, key))
                    .cloned()
            });
        let colliding_keys = extensions
            .iter()
            .filter(|(stored_key, entry)| *stored_key == key || entry.config.key() == *key)
            .map(|(stored_key, _)| stored_key.clone())
            .collect::<Vec<_>>();
        let has_non_platform_collision = colliding_keys.iter().any(|stored_key| {
            extensions
                .get(stored_key)
                .is_some_and(|entry| !platform_entry_matches_key(entry, key))
        });
        for stored_key in colliding_keys {
            extensions.shift_remove(&stored_key);
        }

        if has_non_platform_collision {
            warn!(
                key,
                "Ignoring extension that occupies a reserved platform extension key"
            );
        }
        extensions.insert(
            key.to_string(),
            configured_platform.unwrap_or_else(|| ExtensionEntry {
                config: ExtensionConfig::Platform {
                    name: def.name.to_string(),
                    description: def.description.to_string(),
                    bundled: Some(true),
                    available_tools: Vec::new(),
                },
                enabled: def.default_enabled,
            }),
        );
    }
}

fn platform_entry_matches_key(entry: &ExtensionEntry, key: &str) -> bool {
    matches!(
        &entry.config,
        ExtensionConfig::Platform { name, .. } if name_to_key(name) == key
    )
}

fn save_extensions_map(extensions: IndexMap<String, ExtensionEntry>) {
    let config = Config::global();
    if let Err(e) = config.set_param(EXTENSIONS_CONFIG_KEY, &extensions) {
        // TODO(jack) why is this just a debug statement?
        tracing::debug!("Failed to save extensions config: {}", e);
    }
}

fn retain_bundled_extensions(extensions: &mut IndexMap<String, ExtensionEntry>) -> usize {
    let before = extensions.len();
    extensions.retain(|_, entry| entry.config.is_bundled());
    before - extensions.len()
}

pub fn reset_to_bundled_extensions() -> anyhow::Result<usize> {
    let mut extensions = get_extensions_map();
    let removed = retain_bundled_extensions(&mut extensions);
    Config::global()
        .set_param(EXTENSIONS_CONFIG_KEY, &extensions)
        .map_err(anyhow::Error::from)?;
    Ok(removed)
}

pub fn get_extension_by_name(name: &str) -> Option<ExtensionConfig> {
    get_extension_entry_by_name(name).map(|entry| entry.config)
}

/// Names (`config.name()`) of every extension entry actually present in the
/// persisted config file — i.e. operator-authored entries, before
/// [`get_extensions_map`] injects absent platform extensions with their
/// defaults. Malformed entries are skipped, mirroring `get_extensions_map`.
///
/// This is the provenance signal for the `manage_extensions` enable gate and
/// the `search_available_extensions` labeling (#42): only an entry the
/// operator wrote with `enabled: false` counts as operator-disabled. An
/// injected default-off platform extension (e.g. `chatrecall`) is absent
/// here and stays agent-enableable.
pub fn persisted_extension_names() -> std::collections::HashSet<String> {
    let raw: Mapping = Config::global()
        .get_param(EXTENSIONS_CONFIG_KEY)
        .unwrap_or_default();
    persisted_names_from_raw(&raw)
}

fn persisted_names_from_raw(raw: &Mapping) -> std::collections::HashSet<String> {
    raw.iter()
        .filter_map(
            |(k, v)| match (k, serde_yaml::from_value::<ExtensionEntry>(v.clone())) {
                (serde_yaml::Value::String(_), Ok(entry)) => Some(entry.config.name().to_string()),
                _ => None,
            },
        )
        .collect()
}

/// True when `name` has an entry the operator actually wrote into the config
/// file, as opposed to an injected platform default. See
/// [`persisted_extension_names`].
pub fn extension_entry_is_persisted(name: &str) -> bool {
    persisted_extension_names().contains(name)
}

/// Like [`get_extension_by_name`], but returns the whole config entry so
/// callers can see the operator's `enabled` flag, not just the config.
pub fn get_extension_entry_by_name(name: &str) -> Option<ExtensionEntry> {
    find_entry_by_name(&get_extensions_map(), name).cloned()
}

fn find_entry_by_name<'a>(
    extensions: &'a IndexMap<String, ExtensionEntry>,
    name: &str,
) -> Option<&'a ExtensionEntry> {
    extensions
        .values()
        .find(|entry| entry.config.name() == name)
}

pub fn set_extension(entry: ExtensionEntry) {
    let mut extensions = get_extensions_map();
    let key = entry.config.key();
    extensions.insert(key, entry);
    save_extensions_map(extensions);
}

pub fn remove_extension(key: &str) {
    let mut extensions = get_extensions_map();
    extensions.shift_remove(key);
    save_extensions_map(extensions);
}

pub fn set_extension_enabled(key: &str, enabled: bool) {
    let mut extensions = get_extensions_map();
    if let Some(entry) = extensions.get_mut(key) {
        entry.enabled = enabled;
        save_extensions_map(extensions);
    }
}

pub fn get_all_extensions() -> Vec<ExtensionEntry> {
    let extensions = get_extensions_map();
    extensions.into_values().collect()
}

pub fn get_all_extension_names() -> Vec<String> {
    let extensions = get_extensions_map();
    extensions.keys().cloned().collect()
}

pub fn is_extension_enabled(key: &str) -> bool {
    let extensions = get_extensions_map();
    extensions.get(key).map(|e| e.enabled).unwrap_or(false)
}

pub fn get_enabled_extensions() -> Vec<ExtensionConfig> {
    get_all_extensions()
        .into_iter()
        .filter(|ext| ext.enabled)
        .map(|ext| ext.config)
        .collect()
}

pub fn get_warnings() -> Vec<String> {
    let raw: Mapping = Config::global()
        .get_param(EXTENSIONS_CONFIG_KEY)
        .unwrap_or_default();

    let mut warnings = Vec::new();
    for (k, v) in raw {
        if let (serde_yaml::Value::String(key), Ok(entry)) =
            (k, serde_yaml::from_value::<ExtensionEntry>(v))
        {
            if matches!(entry.config, ExtensionConfig::Sse { .. }) {
                warnings.push(format!(
                    "'{}': SSE is unsupported, migrate to streamable_http",
                    key
                ));
            }
        }
    }
    warnings
}

pub fn resolve_extensions_for_new_session(
    workflow_extensions: Option<&[ExtensionConfig]>,
    override_extensions: Option<Vec<ExtensionConfig>>,
) -> Vec<ExtensionConfig> {
    if let Some(exts) = workflow_extensions {
        return exts.to_vec();
    }

    if let Some(exts) = override_extensions {
        return exts;
    }

    get_enabled_extensions()
}

#[cfg(test)]
mod persisted_provenance_tests {
    use super::*;

    fn raw_mapping(yaml: &str) -> Mapping {
        serde_yaml::from_str(yaml).expect("valid yaml mapping")
    }

    // #42: provenance must come from the raw persisted mapping, not the
    // post-injection map — an injected default-off platform extension
    // (chatrecall) must NOT read as operator-authored.
    #[test]
    fn persisted_names_reflect_only_entries_written_to_the_config_file() {
        let raw = raw_mapping(
            r#"
developer:
  enabled: false
  type: builtin
  name: developer
custom-key:
  enabled: true
  type: stdio
  name: custom name
  cmd: run-me
  args: []
  description: A custom server
  timeout: 30
"#,
        );
        let names = persisted_names_from_raw(&raw);
        assert!(names.contains("developer"));
        assert!(
            names.contains("custom name"),
            "must record config.name(), matching get_extension_entry_by_name: {names:?}"
        );
        assert!(
            !names.contains("chatrecall"),
            "absent platform extensions are injected, never persisted"
        );
        assert!(!names.contains("custom-key"), "map keys are not names");
    }

    #[test]
    fn persisted_names_skip_malformed_entries_like_get_extensions_map() {
        let raw = raw_mapping(
            r#"
broken:
  type: no-such-type
developer:
  enabled: true
  type: builtin
  name: developer
"#,
        );
        let names = persisted_names_from_raw(&raw);
        assert_eq!(
            names,
            std::collections::HashSet::from(["developer".to_string()])
        );
        assert!(persisted_names_from_raw(&Mapping::new()).is_empty());
    }
}

#[cfg(test)]
mod entry_lookup_tests {
    use super::*;

    #[test]
    fn find_entry_by_name_matches_config_name_and_keeps_enabled_flag() {
        let mut extensions = IndexMap::new();
        extensions.insert(
            "developer".into(),
            ExtensionEntry {
                enabled: false,
                config: ExtensionConfig::default(),
            },
        );
        extensions.insert(
            "custom-key".into(),
            ExtensionEntry {
                enabled: true,
                config: ExtensionConfig::stdio("custom name", "cmd", "Custom", 30_u64),
            },
        );

        // Matches on config.name(), not the map key.
        let entry = find_entry_by_name(&extensions, "custom name").expect("found by name");
        assert!(entry.enabled);
        assert!(find_entry_by_name(&extensions, "custom-key").is_none());

        // The operator's enabled:false flag survives the lookup — this is
        // what the manage_extensions enable gate relies on (#42).
        let entry = find_entry_by_name(&extensions, "developer").expect("found");
        assert!(!entry.enabled);

        assert!(find_entry_by_name(&extensions, "missing").is_none());
    }
}

#[cfg(test)]
mod platform_extension_tests {
    use super::*;

    #[test]
    fn custom_extension_cannot_shadow_reserved_platform_key() {
        let mut extensions = IndexMap::from([(
            "custom-key".to_string(),
            ExtensionEntry {
                enabled: true,
                config: ExtensionConfig::stdio("skills", "untrusted", "custom", 30_u64),
            },
        )]);

        inject_platform_extensions(&mut extensions);

        assert!(matches!(
            extensions["skills"].config,
            ExtensionConfig::Platform { ref name, .. } if name == "skills"
        ));
        assert!(!extensions.contains_key("custom-key"));
    }

    #[test]
    fn persisted_platform_enabled_state_is_preserved() {
        let mut extensions = IndexMap::from([(
            "chatrecall".to_string(),
            ExtensionEntry {
                enabled: true,
                config: ExtensionConfig::Platform {
                    name: "Chat Recall".to_string(),
                    description: "configured".to_string(),
                    bundled: Some(true),
                    available_tools: Vec::new(),
                },
            },
        )]);

        inject_platform_extensions(&mut extensions);

        assert!(extensions["chatrecall"].enabled);
        assert_eq!(extensions["chatrecall"].config.name(), "Chat Recall");
    }
}

#[cfg(test)]
mod reset_tests {
    use super::*;

    #[test]
    fn reset_filter_keeps_only_bundled_extensions() {
        let mut extensions = IndexMap::new();
        extensions.insert(
            "developer".into(),
            ExtensionEntry {
                enabled: true,
                config: ExtensionConfig::default(),
            },
        );
        extensions.insert(
            "custom".into(),
            ExtensionEntry {
                enabled: true,
                config: ExtensionConfig::stdio("custom", "custom", "Custom", 30_u64),
            },
        );

        assert_eq!(retain_bundled_extensions(&mut extensions), 1);
        assert_eq!(extensions.len(), 1);
        assert!(extensions["developer"].config.is_bundled());
    }
}
