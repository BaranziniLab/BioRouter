use reqwest::StatusCode;
use std::time::Duration;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ProviderError {
    #[error("Authentication error: {0}")]
    Authentication(String),

    #[error("Context length exceeded: {0}")]
    ContextLengthExceeded(String),

    #[error("Rate limit exceeded: {details}")]
    RateLimitExceeded {
        details: String,
        retry_delay: Option<Duration>,
    },

    #[error("Server error: {0}")]
    ServerError(String),

    #[error("Request failed: {0}")]
    RequestFailed(String),

    #[error("Execution error: {0}")]
    ExecutionError(String),

    #[error("Usage data error: {0}")]
    UsageError(String),

    #[error("Unsupported operation: {0}")]
    NotImplemented(String),
}

/// The coarse class of a provider failure — enough for a caller to decide
/// "retry", "fix your credentials", or "give up", without string-matching the
/// error message.
///
/// Exists because a 403 and a transient 502 were both flattened into the same
/// assistant chat message, so nothing downstream could tell a misconfigured key
/// from a blip.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderErrorKind {
    /// 401/403 — the credentials are wrong, absent, or not allowed here.
    Auth,
    /// 429 — back off and retry.
    RateLimit,
    /// The account has no remaining credit or has hit a billing limit.
    Quota,
    /// 5xx from the provider.
    Server,
    /// The request never got a usable answer (timeout, DNS, connection reset).
    Network,
    /// The prompt exceeded the model's context window.
    ContextLength,
    /// The request shape or one of its parameters is not supported.
    InvalidRequest,
    /// The requested model or deployment does not exist or is unavailable.
    ModelUnavailable,
    /// The provider rejected the request under a safety or content policy.
    Policy,
    /// Anything else.
    Other,
}

impl ProviderErrorKind {
    /// True when the operator must fix a credential — the one class that will
    /// never succeed on retry.
    pub fn is_auth(&self) -> bool {
        matches!(self, Self::Auth)
    }

    /// True when retrying the same request could plausibly succeed.
    pub fn is_transient(&self) -> bool {
        matches!(self, Self::RateLimit | Self::Server | Self::Network)
    }

    pub fn wire_code(&self) -> &'static str {
        match self {
            Self::Auth => "auth",
            Self::RateLimit => "rate_limit",
            Self::Quota => "quota",
            Self::Server => "server",
            Self::Network => "network",
            Self::ContextLength => "context_length",
            Self::InvalidRequest => "invalid_request",
            Self::ModelUnavailable => "model_unavailable",
            Self::Policy => "policy",
            Self::Other => "unknown",
        }
    }
}

impl ProviderError {
    /// Classify this error. Used to decide the CLI exit code and the wire frame
    /// for [`crate::agents::turn_abort::TurnAbortCode::ProviderFailure`].
    pub fn kind(&self) -> ProviderErrorKind {
        match self {
            ProviderError::Authentication(_) => ProviderErrorKind::Auth,
            ProviderError::RateLimitExceeded { .. } => ProviderErrorKind::RateLimit,
            ProviderError::ServerError(_) => ProviderErrorKind::Server,
            ProviderError::ContextLengthExceeded(_) => ProviderErrorKind::ContextLength,
            // `RequestFailed` is what `From<anyhow::Error>` produces for a
            // reqwest failure, so it covers timeouts/connection errors — but it
            // also carries HTTP status text for some providers. Sniff the status
            // so a 401/403 surfaced this way is still classified as auth rather
            // than as a retryable network blip.
            ProviderError::RequestFailed(details) => {
                let d = details.to_ascii_lowercase();
                if d.contains("401") || d.contains("403") || d.contains("unauthorized") {
                    ProviderErrorKind::Auth
                } else if d.contains("insufficient_quota")
                    || d.contains("quota_exceeded")
                    || d.contains("billing_hard_limit")
                    || d.contains("payment required")
                    || d.contains("credit balance")
                {
                    ProviderErrorKind::Quota
                } else if d.contains("429") || d.contains("rate_limit") {
                    ProviderErrorKind::RateLimit
                } else if d.contains("context_length")
                    || d.contains("context window")
                    || d.contains("maximum context")
                    || d.contains("too many tokens")
                {
                    ProviderErrorKind::ContextLength
                } else if d.contains("content_policy")
                    || d.contains("safety policy")
                    || d.contains("safety filter")
                    || d.contains("content blocked")
                {
                    ProviderErrorKind::Policy
                } else if d.contains("404")
                    || d.contains("model_not_found")
                    || d.contains("model not found")
                    || d.contains("resource not found")
                    || d.contains("deployment not found")
                {
                    ProviderErrorKind::ModelUnavailable
                } else if d.contains("400")
                    || d.contains("422")
                    || d.contains("bad request")
                    || d.contains("invalid_request")
                    || d.contains("invalid argument")
                    || d.contains("invalid_argument")
                    || d.contains("unsupported parameter")
                    || d.contains("not supported")
                {
                    ProviderErrorKind::InvalidRequest
                } else if d.contains("500")
                    || d.contains("502")
                    || d.contains("503")
                    || d.contains("504")
                {
                    ProviderErrorKind::Server
                } else if d.contains("timeout")
                    || d.contains("timed out")
                    || d.contains("connection")
                    || d.contains("dns")
                    || d.contains("network")
                    || d.contains("failed to fetch")
                    || d.contains("error sending request")
                {
                    ProviderErrorKind::Network
                } else {
                    ProviderErrorKind::Other
                }
            }
            ProviderError::ExecutionError(_) | ProviderError::UsageError(_) => {
                ProviderErrorKind::Other
            }
            ProviderError::NotImplemented(_) => ProviderErrorKind::InvalidRequest,
        }
    }

    pub fn telemetry_type(&self) -> &'static str {
        match self {
            ProviderError::Authentication(_) => "auth",
            ProviderError::ContextLengthExceeded(_) => "context_length",
            ProviderError::RateLimitExceeded { .. } => "rate_limit",
            ProviderError::ServerError(_) => "server",
            ProviderError::RequestFailed(_) => "request",
            ProviderError::ExecutionError(_) => "execution",
            ProviderError::UsageError(_) => "usage",
            ProviderError::NotImplemented(_) => "not_implemented",
        }
    }
}

impl From<anyhow::Error> for ProviderError {
    fn from(error: anyhow::Error) -> Self {
        if let Some(reqwest_err) = error.downcast_ref::<reqwest::Error>() {
            let mut details = vec![];

            if let Some(status) = reqwest_err.status() {
                details.push(format!("status: {}", status));
            }
            if reqwest_err.is_timeout() {
                details.push("timeout".to_string());
            }
            if reqwest_err.is_connect() {
                if let Some(url) = reqwest_err.url() {
                    if let Some(host) = url.host_str() {
                        let port_info = url.port().map(|p| format!(":{}", p)).unwrap_or_default();

                        details.push(format!("failed to connect to {}{}", host, port_info));

                        if url.port().is_some() {
                            details.push("check that the port is correct".to_string());
                        }
                    }
                } else {
                    details.push("connection failed".to_string());
                }
            }
            let msg = if details.is_empty() {
                reqwest_err.to_string()
            } else {
                format!("{} ({})", reqwest_err, details.join(", "))
            };
            return ProviderError::RequestFailed(msg);
        }
        ProviderError::ExecutionError(error.to_string())
    }
}

impl From<reqwest::Error> for ProviderError {
    fn from(error: reqwest::Error) -> Self {
        ProviderError::RequestFailed(error.to_string())
    }
}

#[derive(Debug)]
pub enum GoogleErrorCode {
    BadRequest = 400,
    Unauthorized = 401,
    Forbidden = 403,
    NotFound = 404,
    TooManyRequests = 429,
    InternalServerError = 500,
    ServiceUnavailable = 503,
}

impl GoogleErrorCode {
    pub fn to_status_code(&self) -> StatusCode {
        match self {
            Self::BadRequest => StatusCode::BAD_REQUEST,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::TooManyRequests => StatusCode::TOO_MANY_REQUESTS,
            Self::InternalServerError => StatusCode::INTERNAL_SERVER_ERROR,
            Self::ServiceUnavailable => StatusCode::SERVICE_UNAVAILABLE,
        }
    }

    pub fn from_code(code: u64) -> Option<Self> {
        match code {
            400 => Some(Self::BadRequest),
            401 => Some(Self::Unauthorized),
            403 => Some(Self::Forbidden),
            404 => Some(Self::NotFound),
            429 => Some(Self::TooManyRequests),
            500 => Some(Self::InternalServerError),
            503 => Some(Self::ServiceUnavailable),
            _ => Some(Self::InternalServerError),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_common_provider_rejections_without_treating_them_as_network_errors() {
        let cases = [
            ("insufficient_quota", ProviderErrorKind::Quota),
            (
                "Bad request (400): unsupported parameter reasoning_effort",
                ProviderErrorKind::InvalidRequest,
            ),
            ("model_not_found", ProviderErrorKind::ModelUnavailable),
            (
                "request rejected by safety filter",
                ProviderErrorKind::Policy,
            ),
            ("429 rate_limit_exceeded", ProviderErrorKind::RateLimit),
            ("connection timed out", ProviderErrorKind::Network),
        ];

        for (message, expected) in cases {
            assert_eq!(
                ProviderError::RequestFailed(message.into()).kind(),
                expected
            );
        }
    }

    #[test]
    fn unknown_rejections_are_explicitly_unknown_and_not_retryable() {
        let kind = ProviderError::RequestFailed("provider rejected the request".into()).kind();
        assert_eq!(kind, ProviderErrorKind::Other);
        assert_eq!(kind.wire_code(), "unknown");
        assert!(!kind.is_transient());
    }
}
