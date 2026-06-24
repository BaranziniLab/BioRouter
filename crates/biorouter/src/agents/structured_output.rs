//! BRSDK structured-output contracts.
//!
//! When an app declares `output_type` (a JSON Schema), the final answer must be
//! a JSON value that validates against it. This module is the self-contained
//! primitive: fence-stripping, parsing, JSON-Schema validation (via the
//! `jsonschema` crate already used by the final-output tool), and building the
//! corrective re-prompt the agent loop pushes back on a mismatch.
//!
//! The agent-loop wiring (validate the terminal message, re-prompt up to N
//! times, emit the `output` event) is a separate, carefully-sequenced change;
//! keeping the logic here makes it independently testable.

use serde_json::Value;

/// Strip a single surrounding Markdown code fence (```json … ``` or bare ```)
/// so a model that wraps its JSON in a fenced block still validates. If no fence
/// is present the input is returned trimmed.
pub fn strip_json_fence(text: &str) -> &str {
    let t = text.trim();
    if let Some(rest) = t.strip_prefix("```") {
        // Drop an optional language tag on the opening fence line.
        let after_lang = match rest.find('\n') {
            Some(nl) => &rest[nl + 1..],
            None => rest,
        };
        if let Some(body) = after_lang.strip_suffix("```") {
            return body.trim();
        }
        // Closing fence may have trailing whitespace/newline.
        let trimmed_end = after_lang.trim_end();
        if let Some(body) = trimmed_end.strip_suffix("```") {
            return body.trim();
        }
    }
    t
}

/// Parse the (fence-stripped) `text` as a JSON value.
pub fn parse_output(text: &str) -> Result<Value, String> {
    let body = strip_json_fence(text);
    serde_json::from_str(body).map_err(|e| format!("not valid JSON: {e}"))
}

/// Validate `value` against `schema`. Returns the list of human-readable
/// validation errors (empty = valid). A schema that itself fails to compile is
/// reported as a single error.
pub fn validate(value: &Value, schema: &Value) -> Vec<String> {
    let validator = match jsonschema::validator_for(schema) {
        Ok(v) => v,
        Err(e) => return vec![format!("invalid output_type schema: {e}")],
    };
    validator
        .iter_errors(value)
        .map(|err| {
            let path = err.instance_path.to_string();
            if path.is_empty() {
                format!("- {err}")
            } else {
                format!("- {path}: {err}")
            }
        })
        .collect()
}

/// Convenience: parse + validate `text` against `schema`. On success returns the
/// parsed `Value`; on failure returns the error list (parse error or schema
/// violations).
pub fn parse_and_validate(text: &str, schema: &Value) -> Result<Value, Vec<String>> {
    let value = parse_output(text).map_err(|e| vec![format!("- {e}")])?;
    let errors = validate(&value, schema);
    if errors.is_empty() {
        Ok(value)
    } else {
        Err(errors)
    }
}

/// The corrective message pushed back into the conversation when the model's
/// output doesn't satisfy the contract. Lists the errors and the schema, and
/// instructs JSON-only output. Bounded re-prompting is the caller's concern.
pub fn reprompt_message(errors: &[String], schema: &Value) -> String {
    let schema_pretty =
        serde_json::to_string_pretty(schema).unwrap_or_else(|_| schema.to_string());
    format!(
        "Your response did not match the required output schema.\n\nErrors:\n{}\n\n\
         Return ONLY a single JSON object conforming to this schema — no prose, no \
         Markdown fences:\n{}",
        errors.join("\n"),
        schema_pretty
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn schema() -> Value {
        json!({
            "type": "object",
            "properties": {
                "gene": { "type": "string" },
                "pathways": { "type": "array", "items": { "type": "string" } },
                "confidence": { "type": "number" }
            },
            "required": ["gene", "pathways"]
        })
    }

    #[test]
    fn strips_json_fence_with_lang_tag() {
        let t = "```json\n{\"gene\":\"CFTR\"}\n```";
        assert_eq!(strip_json_fence(t), "{\"gene\":\"CFTR\"}");
    }

    #[test]
    fn strips_bare_fence_and_plain_passthrough() {
        assert_eq!(strip_json_fence("```\n{\"a\":1}\n```"), "{\"a\":1}");
        assert_eq!(strip_json_fence("  {\"a\":1}  "), "{\"a\":1}");
    }

    #[test]
    fn valid_output_passes() {
        let v = json!({ "gene": "CFTR", "pathways": ["chloride transport"], "confidence": 0.9 });
        assert!(validate(&v, &schema()).is_empty());
    }

    #[test]
    fn missing_required_field_reports_error() {
        let v = json!({ "gene": "CFTR" }); // missing "pathways"
        let errs = validate(&v, &schema());
        assert!(!errs.is_empty());
        assert!(errs.iter().any(|e| e.contains("pathways")), "errors: {errs:?}");
    }

    #[test]
    fn wrong_type_reports_error_with_path() {
        let v = json!({ "gene": "CFTR", "pathways": "not-an-array" });
        let errs = validate(&v, &schema());
        assert!(errs.iter().any(|e| e.contains("/pathways")), "errors: {errs:?}");
    }

    #[test]
    fn parse_and_validate_handles_fenced_model_output() {
        // The realistic case: a model returns fenced JSON.
        let model_text = "```json\n{\"gene\":\"TP53\",\"pathways\":[\"apoptosis\",\"cell cycle\"]}\n```";
        let v = parse_and_validate(model_text, &schema()).expect("should validate");
        assert_eq!(v["gene"], "TP53");
        assert_eq!(v["pathways"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn parse_and_validate_rejects_prose() {
        let model_text = "Here is the answer: TP53 is involved in apoptosis.";
        let res = parse_and_validate(model_text, &schema());
        assert!(res.is_err());
    }

    #[test]
    fn reprompt_message_lists_errors_and_schema() {
        let errs = vec!["- /pathways: \"x\" is not of type \"array\"".to_string()];
        let msg = reprompt_message(&errs, &schema());
        assert!(msg.contains("/pathways"));
        assert!(msg.contains("required"));
        assert!(msg.contains("ONLY"));
    }

    #[test]
    fn invalid_schema_is_reported_not_panicked() {
        let bad_schema = json!({ "type": 12345 }); // not a valid schema
        let errs = validate(&json!({}), &bad_schema);
        assert!(!errs.is_empty());
    }
}
