use biorouter::config::declarative_providers::load_provider;
use biorouter::config::Config;
use biorouter::providers::base::{ConfigKey, ProviderMetadata, ProviderType};
use std::env;

pub fn check_provider_configured(metadata: &ProviderMetadata, provider_type: ProviderType) -> bool {
    let config = Config::global();

    if provider_type == ProviderType::Custom || provider_type == ProviderType::Declarative {
        if let Ok(loaded_provider) = load_provider(metadata.name.as_str()) {
            return config
                .get_secret::<String>(&loaded_provider.config.api_key_env)
                .is_ok();
        }
    }
    // Special case: Zero-config providers (no config keys)
    if metadata.config_keys.is_empty() {
        // Check if the provider has been explicitly configured via the UI
        let configured_marker = format!("{}_configured", metadata.name);
        return config.get_param::<bool>(&configured_marker).is_ok();
    }

    // Get all required keys
    let required_keys: Vec<&ConfigKey> = metadata
        .config_keys
        .iter()
        .filter(|key| key.required)
        .collect();

    // Special case: If a provider has exactly one required key and that key
    // has a default value, check if it's explicitly set
    if required_keys.len() == 1 && required_keys[0].default.is_some() {
        let key = &required_keys[0];

        // Check if the key is explicitly set (either in env or config)
        let is_set_in_env = env::var(&key.name).is_ok();
        let is_set_in_config = config.get(&key.name, key.secret).is_ok();

        return is_set_in_env || is_set_in_config;
    }

    // Special case: If a provider has only optional keys with defaults,
    // check if a configuration marker exists
    if required_keys.is_empty() && !metadata.config_keys.is_empty() {
        let all_optional_with_defaults = metadata
            .config_keys
            .iter()
            .all(|key| !key.required && key.default.is_some());

        if all_optional_with_defaults {
            // Check if the provider has been explicitly configured via the UI
            let configured_marker = format!("{}_configured", metadata.name);
            return config.get_param::<bool>(&configured_marker).is_ok();
        }
    }

    // For providers with multiple keys or keys without defaults:
    // Find required keys that don't have default values
    let required_non_default_keys: Vec<&ConfigKey> = required_keys
        .iter()
        .filter(|key| key.default.is_none())
        .cloned()
        .collect();

    // If there are no non-default keys, check ONLY BioRouter's stored config (not env vars).
    // Config::get_param() checks env vars first by design, so we use all_values()/all_secrets()
    // which read directly from the config file and keychain. This prevents a false "Configured"
    // state after Remove: providers like Bedrock set AWS_ env vars during initialization via
    // std::env::set_var(), and those vars can also exist from the system environment
    // (e.g. ~/.zshrc), surviving even after the user deletes the stored config.
    if required_non_default_keys.is_empty() {
        let file_values = config.all_values().unwrap_or_default();
        let secret_values = config.all_secrets().unwrap_or_default();
        return required_keys.iter().any(|key| {
            if key.secret {
                secret_values.contains_key(&key.name)
            } else {
                file_values.contains_key(&key.name)
            }
        });
    }

    // Otherwise, all non-default keys must be set (env vars are a valid source here since
    // keys without defaults won't be set accidentally by provider initialization)
    required_non_default_keys.iter().all(|key| {
        let is_set_in_env = env::var(&key.name).is_ok();
        let is_set_in_config = config.get(&key.name, key.secret).is_ok();

        is_set_in_env || is_set_in_config
    })
}
