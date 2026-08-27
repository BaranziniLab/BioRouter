use chrono;
use serde::Deserialize;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Represents errors that can occur during Azure authentication.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    /// Error when loading credentials from the filesystem or environment
    #[error("Failed to load credentials: {0}")]
    Credentials(String),

    /// Error during token exchange
    #[error("Token exchange failed: {0}")]
    TokenExchange(String),
}

/// Represents an authentication token with its type and value.
#[derive(Debug, Clone)]
pub struct AuthToken {
    /// The type of the token (e.g., "Bearer")
    pub token_type: String,
    /// The actual token value
    pub token_value: String,
}

/// Represents the types of Azure credentials supported.
#[derive(Debug, Clone)]
pub enum AzureCredentials {
    /// API key based authentication
    ApiKey(String),
    /// Azure credential chain based authentication
    DefaultCredential,
}

/// Holds a cached token and its expiration time.
#[derive(Debug, Clone)]
struct CachedToken {
    token: AuthToken,
    expires_at: Instant,
}

/// Response from Azure token endpoint
#[derive(Debug, Clone, Deserialize)]
struct TokenResponse {
    #[serde(rename = "accessToken")]
    access_token: String,
    #[serde(rename = "tokenType")]
    token_type: String,
    #[serde(rename = "expires_on")]
    expires_on: u64,
}

/// Azure authentication handler that manages credentials and token caching.
#[derive(Debug)]
pub struct AzureAuth {
    credentials: AzureCredentials,
    cached_token: Arc<RwLock<Option<CachedToken>>>,
}

impl AzureAuth {
    /// Creates a new Azure authentication handler.
    ///
    /// Initializes the authentication handler by:
    /// 1. Loading credentials from environment
    /// 2. Setting up an HTTP client for token requests
    /// 3. Initializing the token cache
    ///
    /// # Returns
    /// * `Result<Self, AuthError>` - A new AzureAuth instance or an error if initialization fails
    pub fn new(api_key: Option<String>) -> Result<Self, AuthError> {
        let credentials = match api_key {
            Some(key) => AzureCredentials::ApiKey(key),
            None => AzureCredentials::DefaultCredential,
        };

        Ok(Self {
            credentials,
            cached_token: Arc::new(RwLock::new(None)),
        })
    }

    /// Returns the type of credentials being used.
    pub fn credential_type(&self) -> &AzureCredentials {
        &self.credentials
    }

    /// Retrieves a valid authentication token.
    ///
    /// This method implements an efficient token management strategy:
    /// 1. For API key auth, returns the API key directly
    /// 2. For Azure credential chain:
    ///    a. Checks the cache for a valid token
    ///    b. Returns the cached token if not expired
    ///    c. Obtains a new token if needed or expired
    ///    d. Uses double-checked locking for thread safety
    ///
    /// # Returns
    /// * `Result<AuthToken, AuthError>` - A valid authentication token or an error
    pub async fn get_token(&self) -> Result<AuthToken, AuthError> {
        match &self.credentials {
            AzureCredentials::ApiKey(key) => Ok(AuthToken {
                token_type: "Bearer".to_string(),
                token_value: key.clone(),
            }),
            AzureCredentials::DefaultCredential => self.get_default_credential_token().await,
        }
    }

    async fn get_default_credential_token(&self) -> Result<AuthToken, AuthError> {
        // Try read lock first for better concurrency
        if let Some(cached) = self.cached_token.read().await.as_ref() {
            if cached.expires_at > Instant::now() {
                return Ok(cached.token.clone());
            }
        }

        // Take write lock only if needed
        let mut token_guard = self.cached_token.write().await;

        // Double-check expiration after acquiring write lock
        if let Some(cached) = token_guard.as_ref() {
            if cached.expires_at > Instant::now() {
                return Ok(cached.token.clone());
            }
        }

        // Get new token using Azure CLI credential.
        //
        // ⚠ Resolved through `SearchPaths`, NOT spawned as the bare name `az`.
        // A bare name is looked up in the child's PATH, and a desktop app
        // launched from Finder or the Dock inherits `/usr/bin:/bin:/usr/sbin:
        // /sbin` -- not the user's shell PATH. So an `az` installed by Homebrew
        // into /opt/homebrew/bin is invisible, and the user is told
        //
        //     Failed to execute Azure CLI: No such file or directory (os error 2)
        //
        // which reads as "not installed" when it is installed and simply not on
        // the path this process was given. Same defect class as the coding-agent
        // providers, which resolve their CLI the same way.
        let az = crate::config::search_path::SearchPaths::builder()
            .resolve("az")
            .map_err(|_| AuthError::TokenExchange(azure_cli_missing_message()))?;
        let mut command = tokio::process::Command::new(&az);
        command.args([
            "account",
            "get-access-token",
            "--resource",
            "https://cognitiveservices.azure.com",
        ]);
        if let Ok(path) = crate::config::search_path::SearchPaths::builder().path() {
            command.env("PATH", path);
        }
        crate::subprocess::prepare_agent_child_command(&mut command);
        let output = command.output().await.map_err(|e| {
            AuthError::TokenExchange(format!(
                "Could not run the Azure CLI at {}: {e}",
                az.display()
            ))
        })?;

        if !output.status.success() {
            return Err(AuthError::TokenExchange(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        let token_response: TokenResponse = serde_json::from_slice(&output.stdout)
            .map_err(|e| AuthError::TokenExchange(format!("Invalid token response: {}", e)))?;

        let auth_token = AuthToken {
            token_type: token_response.token_type,
            token_value: token_response.access_token,
        };

        let expires_at = Instant::now()
            + Duration::from_secs(
                token_response
                    .expires_on
                    .saturating_sub(chrono::Utc::now().timestamp() as u64)
                    .saturating_sub(30),
            );

        *token_guard = Some(CachedToken {
            token: auth_token.clone(),
            expires_at,
        });

        Ok(auth_token)
    }
}

/// What to tell a user whose Azure CLI is missing.
///
/// Extracted so it can be asserted: the previous message was
/// `Failed to execute Azure CLI: No such file or directory (os error 2)`,
/// which names the symptom and none of the remedy -- and was ALSO produced when
/// `az` was installed but simply unreachable from a GUI process's PATH.
fn azure_cli_missing_message() -> String {
    "The Azure CLI (`az`) was not found.\n\n\
     This provider signs in with your own Azure login, so it needs the Azure CLI \
     installed and signed in:\n\
     \u{20}\u{20}1. Install it -- `brew install azure-cli` on macOS, or see \
     https://learn.microsoft.com/cli/azure/install-azure-cli\n\
     \u{20}\u{20}2. Sign in with `az login`\n\
     \u{20}\u{20}3. Start a new chat\n\n\
     If it IS installed, Biorouter looked in ~/.local/bin, /usr/local/bin, \
     /opt/homebrew/bin and /opt/local/bin as well as this process's PATH."
        .to_string()
}

#[cfg(test)]
mod cli_discovery_tests {
    use super::*;

    /// The message has to carry the REMEDY, not just the symptom. A user who
    /// sees only "No such file or directory" has no way to know what to install
    /// or that PATH is even involved.
    #[test]
    fn the_missing_cli_message_says_what_to_do() {
        let m = azure_cli_missing_message();
        assert!(
            m.contains("brew install azure-cli"),
            "no install command: {m}"
        );
        assert!(m.contains("az login"), "no sign-in step: {m}");
        assert!(
            m.contains("/opt/homebrew/bin"),
            "does not say where it looked: {m}"
        );
        assert!(
            !m.contains("os error 2"),
            "the raw errno is the old message, not the new one: {m}"
        );
    }

    /// ⚠ The regression this guards is not the message -- it is the LOOKUP.
    /// `Command::new("az")` resolves against the child's PATH, and a desktop app
    /// launched from Finder inherits `/usr/bin:/bin:/usr/sbin:/sbin`. An `az`
    /// installed by Homebrew is then invisible and the user is told it is not
    /// installed, so installing it does not fix anything. The search must
    /// include the standard user-tool directories.
    #[test]
    fn the_cli_is_looked_for_outside_a_gui_processs_path() {
        let path = crate::config::search_path::SearchPaths::builder()
            .path()
            .expect("joinable");
        let entries: Vec<_> = std::env::split_paths(&path).collect();
        for want in ["/usr/local/bin", "/opt/homebrew/bin"] {
            assert!(
                entries.iter().any(|e| e == std::path::Path::new(want)),
                "{want} missing from the Azure CLI search path: {entries:?}"
            );
        }
    }
}
