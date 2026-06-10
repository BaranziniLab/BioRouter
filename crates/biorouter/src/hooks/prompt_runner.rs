//! Prompt hooks: an LLM judge receives the event payload and decides
//! {"ok": bool, "reason": string}. `ok: false` blocks (for blockable events).

use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use tracing::debug;

use super::event::HookEvent;
use super::outcome::{HookDecision, HookOutcome};
use crate::conversation::message::Message;
use crate::providers::base::Provider;

const JUDGE_SYSTEM_PROMPT: &str = r#"You are a hook judge for the Biorouter agent. You are given a rule and a JSON event describing what the agent is about to do (or just did). Decide whether the event complies with the rule.

Respond with ONLY a JSON object, no prose, no code fences:
{"ok": true} if the event complies with the rule, or
{"ok": false, "reason": "<short explanation>"} if it violates the rule.

If the rule does not apply to this event, respond {"ok": true}."#;

#[derive(Deserialize)]
struct JudgeVerdict {
    ok: bool,
    #[serde(default)]
    reason: Option<String>,
}

/// Extract the first JSON object from model output, tolerating code fences
/// and surrounding prose.
fn parse_verdict(text: &str) -> Option<JudgeVerdict> {
    let trimmed = text.trim();
    if let Ok(verdict) = serde_json::from_str::<JudgeVerdict>(trimmed) {
        return Some(verdict);
    }
    let start = trimmed.find('{')?;
    let end = trimmed.rfind('}')?;
    let candidate = trimmed.get(start..=end)?;
    serde_json::from_str::<JudgeVerdict>(candidate).ok()
}

/// Run a prompt hook against the given provider. Uses the provider's fast
/// model when one is configured. Failures are non-blocking (failure-open).
pub async fn run_prompt_hook(
    provider: Arc<dyn Provider>,
    event: HookEvent,
    judge_prompt: &str,
    payload_json: &str,
    timeout: Duration,
) -> HookOutcome {
    let system = format!("{JUDGE_SYSTEM_PROMPT}\n\nRule:\n{judge_prompt}");
    let user_message = Message::user().with_text(format!("Event:\n{payload_json}"));

    let completion = tokio::time::timeout(
        timeout,
        provider.complete_fast(&system, &[user_message], &[]),
    )
    .await;

    let response = match completion {
        Ok(Ok((message, _usage))) => message.as_concat_text(),
        Ok(Err(e)) => {
            return HookOutcome {
                error: Some(format!("prompt hook provider error: {e}")),
                ..Default::default()
            }
        }
        Err(_) => {
            return HookOutcome {
                error: Some(format!("prompt hook timed out after {timeout:?}")),
                ..Default::default()
            }
        }
    };

    debug!("hooks: prompt hook verdict for {}: {}", event, response);
    match parse_verdict(&response) {
        Some(JudgeVerdict { ok: true, .. }) => HookOutcome::default(),
        Some(JudgeVerdict { ok: false, reason }) => HookOutcome {
            decision: Some(HookDecision::Deny {
                reason: reason.unwrap_or_else(|| format!("{event} blocked by prompt hook")),
            }),
            ..Default::default()
        },
        None => HookOutcome {
            error: Some(format!(
                "prompt hook returned unparseable verdict: {}",
                response.chars().take(200).collect::<String>()
            )),
            ..Default::default()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_verdict() {
        let verdict =
            parse_verdict(r#"{"ok": false, "reason": "writes outside project"}"#).unwrap();
        assert!(!verdict.ok);
        assert_eq!(verdict.reason.as_deref(), Some("writes outside project"));
    }

    #[test]
    fn parses_fenced_verdict() {
        let verdict = parse_verdict("```json\n{\"ok\": true}\n```").unwrap();
        assert!(verdict.ok);
    }

    #[test]
    fn parses_verdict_with_prose() {
        let verdict = parse_verdict(
            "Sure! Here's my answer: {\"ok\": false, \"reason\": \"no\"} hope that helps",
        )
        .unwrap();
        assert!(!verdict.ok);
    }

    #[test]
    fn garbage_is_none() {
        assert!(parse_verdict("I think it's fine").is_none());
    }
}
