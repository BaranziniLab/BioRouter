use super::base::Config;
use serde::{Deserialize, Serialize};

/// Optional, display-only monthly usage budget read from the `usage` section of
/// `config.yaml`. Both fields are independent and either may be unset; nothing
/// here is enforced — it only drives the summary gauge and its percent-used
/// figures.
///
/// ```yaml
/// usage:
///   monthly_token_limit: 66000000
///   monthly_dollar_limit: 250.0
/// ```
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UsageLimits {
    /// Subsidized monthly token allowance (e.g. the UCSF Versa limit).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub monthly_token_limit: Option<i64>,
    /// Monthly dollar budget.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub monthly_dollar_limit: Option<f64>,
}

impl UsageLimits {
    /// Load the configured limits, defaulting to "no limits" when the section is
    /// absent or malformed — a bad budget knob must never break a usage report.
    pub fn load() -> Self {
        Config::global()
            .get_param::<UsageLimits>("usage")
            .unwrap_or_default()
    }
}

/// Percent of a limit consumed, or `None` when the limit is unset. Returns
/// `None` for a non-positive limit rather than dividing by zero.
pub fn percent_of(used: f64, limit: Option<f64>) -> Option<f64> {
    match limit {
        Some(limit) if limit > 0.0 => Some(used / limit * 100.0),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_both_limits() {
        let limits: UsageLimits =
            serde_yaml::from_str("monthly_token_limit: 66000000\nmonthly_dollar_limit: 250.0")
                .unwrap();
        assert_eq!(limits.monthly_token_limit, Some(66_000_000));
        assert_eq!(limits.monthly_dollar_limit, Some(250.0));
    }

    #[test]
    fn parses_partial_and_empty() {
        let only_tokens: UsageLimits = serde_yaml::from_str("monthly_token_limit: 1000").unwrap();
        assert_eq!(only_tokens.monthly_token_limit, Some(1000));
        assert_eq!(only_tokens.monthly_dollar_limit, None);

        let empty: UsageLimits = serde_yaml::from_str("{}").unwrap();
        assert_eq!(empty, UsageLimits::default());
        assert_eq!(empty.monthly_token_limit, None);
        assert_eq!(empty.monthly_dollar_limit, None);
    }

    #[test]
    fn percent_math_and_guards() {
        // 33M of a 66M allowance = 50%.
        assert_eq!(percent_of(33_000_000.0, Some(66_000_000.0)), Some(50.0));
        // No limit configured → no percent.
        assert_eq!(percent_of(10.0, None), None);
        // A zero/negative limit never divides by zero.
        assert_eq!(percent_of(10.0, Some(0.0)), None);
        assert_eq!(percent_of(10.0, Some(-5.0)), None);
    }
}
