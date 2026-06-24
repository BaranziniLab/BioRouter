//! `{{vault:NAME}}` secret substitution for BRSDK app tool calls.
//!
//! The security-critical half of the encryption capability. An app stores
//! secrets encrypted at rest (see `biorouter_mcp::agent_drafter::vault`); the
//! server decrypts them with the app's keyring key and installs the plaintext
//! here, on the per-app [`Agent`](crate::agents::Agent). At **tool dispatch** —
//! after the model has produced a tool call, before any tool sees it — the
//! placeholders in the call's arguments are replaced with the real secrets.
//!
//! This ordering is the whole point: on the **request** side the plaintext secret
//! never enters the model's context — the model only ever emits the
//! `{{vault:NAME}}` placeholder, and substitution happens only on the leaf
//! MCP-dispatch path (never for subagent/frontend/final_output/schedule calls,
//! whose args would carry plaintext back to an LLM, the browser, or a store).
//!
//! Residual risk (documented, not eliminated here): the tool runs with the real
//! secret, so a tool that echoes its arguments back in its **result** can surface
//! the secret into history on the next turn. That's a property of the tool, not
//! of this substitution — apps should avoid secret-echoing tools.

use std::collections::HashMap;

use serde_json::Value;

const OPEN: &str = "{{vault:";
const CLOSE: &str = "}}";

/// Decrypted per-app secrets, resolved into tool-call arguments at dispatch.
#[derive(Clone, Default, Debug)]
pub struct VaultRefs {
    secrets: HashMap<String, String>,
}

impl VaultRefs {
    pub fn new(secrets: HashMap<String, String>) -> Self {
        Self { secrets }
    }

    pub fn is_empty(&self) -> bool {
        self.secrets.is_empty()
    }

    /// The allow-listed secret names (for advertising what an app may reference).
    pub fn names(&self) -> impl Iterator<Item = &String> {
        self.secrets.keys()
    }

    /// Resolve placeholders in every value of a tool-argument map, in place.
    pub fn resolve_args(&self, args: &mut serde_json::Map<String, Value>) {
        for v in args.values_mut() {
            self.resolve_value(v);
        }
    }

    fn resolve_value(&self, v: &mut Value) {
        match v {
            Value::String(s) => {
                if let Some(replaced) = self.resolve_str(s) {
                    *s = replaced;
                }
            }
            Value::Array(arr) => {
                for x in arr.iter_mut() {
                    self.resolve_value(x);
                }
            }
            Value::Object(obj) => {
                for x in obj.values_mut() {
                    self.resolve_value(x);
                }
            }
            // Numbers/bools/null carry no placeholders.
            _ => {}
        }
    }

    /// Replace each `{{vault:NAME}}` in a single string. One left-to-right pass:
    /// a substituted secret is NOT re-scanned, so a secret value can never inject
    /// another reference (no recursive expansion). Unknown names are left as the
    /// literal placeholder (harmless text, and visible for debugging). Returns
    /// `None` when nothing changed (so the caller can avoid a needless clone).
    fn resolve_str(&self, s: &str) -> Option<String> {
        if !s.contains(OPEN) {
            return None;
        }
        let mut out = String::with_capacity(s.len());
        let mut rest = s;
        let mut changed = false;
        while let Some(start) = rest.find(OPEN) {
            let after_open = &rest[start + OPEN.len()..];
            if let Some(end_rel) = after_open.find(CLOSE) {
                let raw_name = &after_open[..end_rel];
                let name = raw_name.trim();
                // A valid name has no nested braces (defends against
                // "{{vault:{{vault:X}}}}" style nesting probes).
                if !name.is_empty() && !raw_name.contains('{') {
                    out.push_str(&rest[..start]);
                    match self.secrets.get(name) {
                        Some(secret) => {
                            out.push_str(secret);
                            changed = true;
                        }
                        None => {
                            // Leave the original placeholder verbatim.
                            out.push_str(&rest[start..start + OPEN.len() + end_rel + CLOSE.len()]);
                        }
                    }
                    rest = &after_open[end_rel + CLOSE.len()..];
                    continue;
                }
            }
            // Malformed (no close, or invalid name): emit through the OPEN token
            // and keep scanning after it. Guarantees forward progress.
            out.push_str(&rest[..start + OPEN.len()]);
            rest = &rest[start + OPEN.len()..];
        }
        out.push_str(rest);
        if changed {
            Some(out)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn vault(pairs: &[(&str, &str)]) -> VaultRefs {
        VaultRefs::new(pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect())
    }

    fn args(v: Value) -> serde_json::Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn resolves_a_flat_placeholder() {
        let v = vault(&[("API_KEY", "sk-secret-123")]);
        let mut a = args(json!({ "header": "Bearer {{vault:API_KEY}}" }));
        v.resolve_args(&mut a);
        assert_eq!(a["header"], json!("Bearer sk-secret-123"));
    }

    #[test]
    fn resolves_multiple_refs_in_one_string() {
        let v = vault(&[("A", "x"), ("B", "y")]);
        let mut a = args(json!({ "u": "{{vault:A}}-{{vault:B}}-{{vault:A}}" }));
        v.resolve_args(&mut a);
        assert_eq!(a["u"], json!("x-y-x"));
    }

    #[test]
    fn resolves_nested_objects_and_arrays() {
        let v = vault(&[("TOK", "T0K")]);
        let mut a = args(json!({
            "outer": { "inner": "{{vault:TOK}}" },
            "list": ["plain", "v={{vault:TOK}}", { "deep": "{{vault:TOK}}" }]
        }));
        v.resolve_args(&mut a);
        assert_eq!(a["outer"]["inner"], json!("T0K"));
        assert_eq!(a["list"][1], json!("v=T0K"));
        assert_eq!(a["list"][2]["deep"], json!("T0K"));
    }

    #[test]
    fn unknown_name_is_left_as_literal_placeholder() {
        let v = vault(&[("KNOWN", "k")]);
        let mut a = args(json!({ "x": "{{vault:UNKNOWN}} but {{vault:KNOWN}}" }));
        v.resolve_args(&mut a);
        // Unknown stays literal; known resolves.
        assert_eq!(a["x"], json!("{{vault:UNKNOWN}} but k"));
    }

    #[test]
    fn secret_with_special_chars_stays_valid_json() {
        // A secret containing quotes/backslashes must survive: we operate on the
        // parsed Value, so re-serialization escapes it correctly.
        let v = vault(&[("P", "a\"b\\c\nd")]);
        let mut a = args(json!({ "pw": "{{vault:P}}" }));
        v.resolve_args(&mut a);
        assert_eq!(a["pw"], json!("a\"b\\c\nd"));
        // And the whole thing still serializes to valid JSON.
        let s = serde_json::to_string(&Value::Object(a)).unwrap();
        let _: Value = serde_json::from_str(&s).unwrap();
    }

    #[test]
    fn a_secret_cannot_inject_another_reference() {
        // A's value literally contains a B-reference; resolving A must NOT then
        // expand B (single pass). Otherwise a malicious secret could chain-leak.
        let v = vault(&[("A", "{{vault:B}}"), ("B", "REAL_SECRET")]);
        let mut a = args(json!({ "x": "{{vault:A}}" }));
        v.resolve_args(&mut a);
        assert_eq!(a["x"], json!("{{vault:B}}"), "must not recursively expand");
    }

    #[test]
    fn no_placeholder_is_a_noop() {
        let v = vault(&[("A", "x")]);
        let mut a = args(json!({ "plain": "nothing here", "n": 5, "b": true }));
        let before = a.clone();
        v.resolve_args(&mut a);
        assert_eq!(a, before);
    }

    #[test]
    fn malformed_placeholders_do_not_panic_or_loop() {
        let v = vault(&[("A", "x")]);
        // Unclosed, empty name, nested braces — all must terminate and be safe.
        let mut a = args(json!({
            "unclosed": "{{vault:A",
            "empty": "{{vault:}}",
            "nested": "{{vault:{{vault:A}}}}",
            "ok": "{{vault:A}}"
        }));
        v.resolve_args(&mut a);
        assert_eq!(a["unclosed"], json!("{{vault:A"));
        assert_eq!(a["empty"], json!("{{vault:}}"));
        assert_eq!(a["ok"], json!("x"));
        // The nested form's inner {{vault:A}} resolves on its own pass position;
        // the key invariant is no panic / no infinite loop and "ok" still works.
    }

    #[test]
    fn whitespace_in_name_is_trimmed() {
        let v = vault(&[("API", "v")]);
        let mut a = args(json!({ "x": "{{vault: API }}" }));
        v.resolve_args(&mut a);
        assert_eq!(a["x"], json!("v"));
    }
}
