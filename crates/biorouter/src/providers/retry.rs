use super::errors::ProviderError;
use crate::providers::base::Provider;
use async_trait::async_trait;
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::sleep;

/// Unix-millis until which a provider was last observed rate-limited (HTTP 429).
/// Set by [`retry_operation`] on any rate-limit error; read by the scheduler
/// ([`crate::scheduler`]) to defer scheduled (non-interactive) jobs so background
/// work doesn't pile onto a throttled provider while the per-request retry budget
/// is already absorbing the 429.
pub static RATE_LIMITED_UNTIL_MS: AtomicU64 = AtomicU64::new(0);

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Record that the provider is rate-limited for at least `dur` from now.
pub fn note_rate_limited(dur: Duration) {
    let until = now_ms().saturating_add(dur.as_millis() as u64);
    RATE_LIMITED_UNTIL_MS.fetch_max(until, Ordering::Relaxed);
}

/// Whether a provider is currently within an observed rate-limit backoff window.
pub fn is_rate_limited() -> bool {
    now_ms() < RATE_LIMITED_UNTIL_MS.load(Ordering::Relaxed)
}

pub const DEFAULT_MAX_RETRIES: usize = 3;
pub const DEFAULT_INITIAL_RETRY_INTERVAL_MS: u64 = 1000;
pub const DEFAULT_BACKOFF_MULTIPLIER: f64 = 2.0;
pub const DEFAULT_MAX_RETRY_INTERVAL_MS: u64 = 30_000;

/// Rate-limit (HTTP 429) responses are always transient, but sustained
/// throttling (e.g. several concurrent sessions against one key) routinely lasts
/// longer than the generic `max_retries` window (3 retries ≈ 7s). Give rate-limit
/// errors a deeper dedicated budget so a transient 429 doesn't abort the turn.
/// With the default 1s→2s backoff capped at 30s, 8 attempts span ~2 minutes.
pub const RATE_LIMIT_MAX_RETRIES: usize = 8;

/// The effective retry ceiling for a given error: rate-limit errors get the
/// larger of the configured `max_retries` and [`RATE_LIMIT_MAX_RETRIES`].
fn effective_max_retries(error: &ProviderError, config: &RetryConfig) -> usize {
    if matches!(error, ProviderError::RateLimitExceeded { .. }) {
        config.max_retries.max(RATE_LIMIT_MAX_RETRIES)
    } else {
        config.max_retries
    }
}

#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub(crate) max_retries: usize,
    /// Initial interval between retries in milliseconds
    pub(crate) initial_interval_ms: u64,
    /// Multiplier for backoff (exponential)
    pub(crate) backoff_multiplier: f64,
    /// Maximum interval between retries in milliseconds
    pub(crate) max_interval_ms: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: DEFAULT_MAX_RETRIES,
            initial_interval_ms: DEFAULT_INITIAL_RETRY_INTERVAL_MS,
            backoff_multiplier: DEFAULT_BACKOFF_MULTIPLIER,
            max_interval_ms: DEFAULT_MAX_RETRY_INTERVAL_MS,
        }
    }
}

impl RetryConfig {
    pub fn new(
        max_retries: usize,
        initial_interval_ms: u64,
        backoff_multiplier: f64,
        max_interval_ms: u64,
    ) -> Self {
        Self {
            max_retries,
            initial_interval_ms,
            backoff_multiplier,
            max_interval_ms,
        }
    }

    pub fn max_retries(&self) -> usize {
        self.max_retries
    }

    pub fn delay_for_attempt(&self, attempt: usize) -> Duration {
        if attempt == 0 {
            return Duration::from_millis(0);
        }

        let exponent = (attempt - 1) as u32;
        let base_delay_ms = (self.initial_interval_ms as f64
            * self.backoff_multiplier.powi(exponent as i32)) as u64;

        let capped_delay_ms = std::cmp::min(base_delay_ms, self.max_interval_ms);

        let jitter_factor_to_avoid_thundering_herd = 0.8 + (rand::random::<f64>() * 0.4);
        let jitter_delay_ms =
            (capped_delay_ms as f64 * jitter_factor_to_avoid_thundering_herd) as u64;

        Duration::from_millis(jitter_delay_ms)
    }
}

pub fn should_retry(error: &ProviderError) -> bool {
    error.kind().is_transient()
}

pub async fn retry_operation<F, Fut, T>(
    config: &RetryConfig,
    operation: F,
) -> Result<T, ProviderError>
where
    F: Fn() -> Fut + Send,
    Fut: Future<Output = Result<T, ProviderError>> + Send,
    T: Send,
{
    let mut attempts = 0;

    loop {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(error) => {
                // Surface rate-limiting to the scheduler so background jobs back
                // off, whether or not we retry this request.
                if let ProviderError::RateLimitExceeded { retry_delay, .. } = &error {
                    let window = retry_delay
                        .unwrap_or(Duration::from_secs(60))
                        .max(Duration::from_secs(30));
                    note_rate_limited(window);
                }
                if should_retry(&error) && attempts < effective_max_retries(&error, config) {
                    attempts += 1;
                    tracing::warn!(
                        attempt = attempts,
                        max_retries = effective_max_retries(&error, config),
                        error_type = error.telemetry_type(),
                        "Request failed, retrying"
                    );

                    let delay = match &error {
                        ProviderError::RateLimitExceeded {
                            retry_delay: Some(d),
                            ..
                        } => *d,
                        _ => config.delay_for_attempt(attempts),
                    };

                    sleep(delay).await;
                    continue;
                }
                return Err(error);
            }
        }
    }
}

/// Trait for retry functionality to keep Provider dyn-compatible
#[async_trait]
pub trait ProviderRetry {
    fn retry_config(&self) -> RetryConfig {
        RetryConfig::default()
    }

    async fn with_retry<F, Fut, T>(&self, operation: F) -> Result<T, ProviderError>
    where
        F: Fn() -> Fut + Send,
        Fut: Future<Output = Result<T, ProviderError>> + Send,
        T: Send,
    {
        let mut attempts = 0;
        let config = self.retry_config();

        loop {
            return match operation().await {
                Ok(result) => Ok(result),
                Err(error) => {
                    if should_retry(&error) && attempts < effective_max_retries(&error, &config) {
                        attempts += 1;
                        tracing::warn!(
                            attempt = attempts,
                            max_retries = effective_max_retries(&error, &config),
                            error_type = error.telemetry_type(),
                            "Request failed, retrying"
                        );

                        let delay = match &error {
                            ProviderError::RateLimitExceeded {
                                retry_delay: Some(provider_delay),
                                ..
                            } => *provider_delay,
                            _ => config.delay_for_attempt(attempts),
                        };

                        let skip_backoff = std::env::var("BIOROUTER_PROVIDER_SKIP_BACKOFF")
                            .unwrap_or_default()
                            .parse::<bool>()
                            .unwrap_or(false);

                        if skip_backoff {
                            tracing::info!(
                                "Skipping backoff due to BIOROUTER_PROVIDER_SKIP_BACKOFF"
                            );
                        } else {
                            tracing::info!("Backing off for {:?} before retry", delay);
                            sleep(delay).await;
                        }
                        continue;
                    }

                    Err(error)
                }
            };
        }
    }
}

impl<P: Provider> ProviderRetry for P {
    fn retry_config(&self) -> RetryConfig {
        Provider::retry_config(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limit_gets_deeper_retry_budget_than_generic() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, DEFAULT_MAX_RETRIES);

        // A transient 429 should be retried far more than the generic ceiling,
        // because sustained throttling outlasts ~7s of generic retries.
        let rate_limit = ProviderError::RateLimitExceeded {
            details: "Too many requests".to_string(),
            retry_delay: None,
        };
        assert_eq!(
            effective_max_retries(&rate_limit, &config),
            RATE_LIMIT_MAX_RETRIES
        );
        const { assert!(RATE_LIMIT_MAX_RETRIES > DEFAULT_MAX_RETRIES) };

        // Non-rate-limit retryable errors keep the generic ceiling.
        let server = ProviderError::ServerError("boom".to_string());
        assert_eq!(effective_max_retries(&server, &config), DEFAULT_MAX_RETRIES);

        // should_retry still classifies rate limits as retryable.
        assert!(should_retry(&rate_limit));
    }

    #[test]
    fn rate_limit_budget_respects_a_higher_configured_max() {
        // If a provider configures an even larger max_retries, keep theirs.
        let config = RetryConfig {
            max_retries: 20,
            ..RetryConfig::default()
        };
        let rate_limit = ProviderError::RateLimitExceeded {
            details: "x".to_string(),
            retry_delay: None,
        };
        assert_eq!(effective_max_retries(&rate_limit, &config), 20);
    }

    #[test]
    fn permanent_request_rejections_are_not_retried() {
        for message in [
            "insufficient_quota",
            "Bad request (400): unsupported parameter",
            "model_not_found",
            "content blocked by safety filter",
            "provider rejected the request",
        ] {
            assert!(!should_retry(&ProviderError::RequestFailed(message.into())));
        }

        assert!(should_retry(&ProviderError::RequestFailed(
            "connection timed out".into()
        )));
    }
}
