use crate::error::{to_env_var, ConfigError};
use config::{Config, Environment};
use serde::Deserialize;
use std::net::SocketAddr;

#[derive(Debug, Default, Deserialize)]
pub struct Settings {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    /// Directory holding the built web interface, or `None` to serve none.
    ///
    /// Set as `BIOROUTER_SERVE_UI`. When it is absent the daemon behaves
    /// exactly as it always has and answers only its API — which is what the
    /// desktop application wants, since Electron loads the bundle itself. When
    /// it is present the daemon additionally serves that directory as the
    /// application shell, which is how `biorouter serve` reaches a browser.
    ///
    /// It is a setting rather than a command-line flag because the daemon takes
    /// no flags: `Settings` is the whole of its configuration surface, read from
    /// the environment with a `BIOROUTER_` prefix.
    #[serde(default)]
    pub serve_ui: Option<String>,
}

impl Settings {
    pub fn socket_addr(&self) -> SocketAddr {
        format!("{}:{}", self.host, self.port)
            .parse()
            .expect("Failed to parse socket address")
    }

    pub fn new() -> Result<Self, ConfigError> {
        Self::load_and_validate()
    }

    fn load_and_validate() -> Result<Self, ConfigError> {
        // Start with default configuration
        let config = Config::builder()
            // Server defaults
            .set_default("host", default_host())?
            .set_default("port", default_port())?
            // Layer on the environment variables
            .add_source(
                Environment::with_prefix("BIOROUTER")
                    .prefix_separator("_")
                    .separator("__")
                    .try_parsing(true),
            )
            .build()?;

        // Try to deserialize the configuration
        let result: Result<Self, config::ConfigError> = config.try_deserialize();

        // Handle missing field errors specially
        match result {
            Ok(settings) => Ok(settings),
            Err(err) => {
                tracing::debug!("Configuration error: {:?}", &err);

                // Handle both NotFound and missing field message variants
                let error_str = err.to_string();
                if error_str.starts_with("missing field") {
                    // Extract field name from error message "missing field `type`"
                    let field = error_str
                        .trim_start_matches("missing field `")
                        .trim_end_matches("`");
                    let env_var = to_env_var(field);
                    Err(ConfigError::MissingEnvVar { env_var })
                } else if let config::ConfigError::NotFound(field) = &err {
                    let env_var = to_env_var(field);
                    Err(ConfigError::MissingEnvVar { env_var })
                } else {
                    Err(ConfigError::Other(err))
                }
            }
        }
    }
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    3000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_socket_addr_conversion() {
        let server_settings = Settings {
            host: "127.0.0.1".to_string(),
            port: 3000,
            serve_ui: None,
        };
        let addr = server_settings.socket_addr();
        assert_eq!(addr.to_string(), "127.0.0.1:3000");
    }
}
