//! The scrubber and the validator: the harness that stands between what the
//! agent wrote and what becomes a public GitHub issue.
//!
//! ## Why this exists as its own layer
//!
//! The bug-report tool never posts a raw transcript — it posts a *distilled*
//! report the model wrote. That is the primary defence and it is not
//! sufficient, because the model writes that report **from** the transcript.
//! It will quote the error it is reporting, and a real error message is exactly
//! where a home path, a username, a bearer token or a signed URL lives:
//!
//! ```text
//! Error: failed to read /Users/jsmith/Documents/IRB-2019-441/cohort.csv
//! Error: 401 from https://api.example.org?token=sk-live-9f2a…
//! ```
//!
//! So every byte that leaves for GitHub passes through [`scrub`], and then
//! through [`validate_issue`], which **fails the report** rather than trusting
//! the scrub to have been complete. The two are deliberately different shapes
//! of check: `scrub` rewrites what it recognises, `validate_issue` refuses what
//! it still recognises afterwards. A single pass that did both would report
//! success for every pattern it forgot.
//!
//! ⚠ Nothing here is a *guarantee*. A secret that looks like prose survives any
//! pattern set. This raises the floor and makes the residue visible on the
//! approval card, where a person reads the exact body before it is posted;
//! the person is the last check and the design assumes it.

use std::path::Path;

use once_cell::sync::Lazy;
use regex::Regex;

/// What the model wrote about `/Users/jsmith/...`, after scrubbing.
pub const HOME_PLACEHOLDER: &str = "~";
/// What replaces anything credential-shaped.
pub const SECRET_PLACEHOLDER: &str = "[redacted]";
/// What replaces a username lifted out of a path.
pub const USER_PLACEHOLDER: &str = "<user>";

/// GitHub rejects an issue body over 65,536 characters. The margin is for the
/// receipt lines the filer appends after validation.
pub const MAX_ISSUE_BODY_CHARS: usize = 60_000;

/// The practical ceiling on a prefilled `…/issues/new?body=` URL.
///
/// GitHub answers 414 well before any browser's own limit, and the body is
/// percent-encoded on the way in — a body of mostly punctuation and newlines
/// triples. So the cap is applied to the ENCODED url, not the raw body, and the
/// raw budget below is the conservative pre-image of it.
pub const MAX_COMPOSE_URL_CHARS: usize = 8_000;

/// One thing the scrubber changed, for the receipt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// What kind of thing it was. Never the value.
    pub kind: &'static str,
    /// How many were replaced.
    pub count: usize,
}

/// The result of a scrub: the rewritten text and what was rewritten.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scrubbed {
    pub text: String,
    pub findings: Vec<Finding>,
}

impl Scrubbed {
    pub fn changed(&self) -> bool {
        !self.findings.is_empty()
    }

    /// A line for the tool's receipt. Names kinds and counts, never a value —
    /// this string reaches the model, and a "redacted" value quoted back into
    /// the conversation is not redacted.
    pub fn summary(&self) -> String {
        if self.findings.is_empty() {
            return "nothing needed redacting".to_string();
        }
        self.findings
            .iter()
            .map(|f| format!("{}×{}", f.count, f.kind))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// Credential shapes, most specific first.
///
/// Ordering matters: `GENERIC_ASSIGNMENT` would swallow the tail of a
/// `token=ghp_…` before `VENDOR_TOKEN` ever saw it, and the report would then
/// say "an assignment" where it should say "a GitHub token". Both redact, but
/// the receipt is what tells a user how bad the near-miss was.
static PATTERNS: Lazy<Vec<(&'static str, Regex)>> = Lazy::new(|| {
    let compile = |source: &str| Regex::new(source).expect("bug-report scrub pattern is valid");
    vec![
        // A JWT: three base64url segments. Whole thing, because the payload is
        // the identifying half.
        (
            "jwt",
            compile(r"\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}"),
        ),
        // GitHub's own prefixes, AWS access key ids, OpenAI/Anthropic style keys,
        // Slack, Google.
        (
            "vendor token",
            compile(
                r"\b(?:gh[pousr]_[A-Za-z0-9]{16,}|github_pat_[A-Za-z0-9_]{20,}|AKIA[0-9A-Z]{16}|ASIA[0-9A-Z]{16}|sk-[A-Za-z0-9_-]{16,}|sk_live_[A-Za-z0-9]{16,}|xox[baprs]-[A-Za-z0-9-]{10,}|AIza[0-9A-Za-z_-]{30,}|glpat-[A-Za-z0-9_-]{16,})",
            ),
        ),
        // `Authorization: Bearer …`, and the bare `Bearer …` an error message
        // quotes back.
        (
            "bearer token",
            compile(r"(?i)\bbearer\s+[A-Za-z0-9._~+/=-]{12,}"),
        ),
        // `Authorization: Basic dXNlcjpwYXNzd29yZA==` — HTTP's other credential
        // header, and the one shape that slipped through BOTH halves of this
        // harness. The `bearer token` rule above does not match it, and the
        // generic assignment rule below cannot: its value group needs six
        // characters and
        // `Basic` is five, so the match dies on the scheme word and never
        // reaches the base64 behind it. Scrub and validator agreed, and were
        // both wrong — which is precisely the failure the two-shape design
        // exists to prevent and cannot, when the gap is a shape neither knows.
        //
        // Anchored on the header name rather than on a bare `Basic`, on
        // purpose: `(?i)basic\s+\w{8,}` also matches "basic understanding",
        // and a false positive here is not cosmetic — `validate_issue` REFUSES
        // a report on anything the rescrub still finds, so it would reject a
        // bug report for containing ordinary English.
        (
            "basic auth",
            compile(r"(?i)\b((?:proxy-)?authorization\s*[=:]\s*basic)\s+[A-Za-z0-9+/=_-]{8,}"),
        ),
        // `curl -u alice:hunter2` / `--user=alice:hunter2` — the same credential
        // one layer earlier, in the command a user pastes into a bug report to
        // show what they ran. Not covered by `url credential`, which needs a
        // `scheme://`, and not by the assignment rule, which needs a keyword.
        //
        // The `\bcurl\b[^\n]*?` prefix is load-bearing and stays on ONE line:
        // a bare `-u user:pass` rule would redact `docker run -u 1000:1000` and
        // `id -u`, and a false positive is a refused report (see above).
        (
            "command-line credential",
            compile(r#"(?i)(\bcurl\b[^\n]*?\s--?u(?:ser)?[=\s])[^\s'"]+:[^\s'"]+"#),
        ),
        // A credential-shaped assignment in prose, a URL query, an env dump or a
        // YAML line. The needle set matches `diagnostics::is_secret_key` on
        // purpose: two redactors disagreeing about what a credential is means
        // one of them is wrong.
        (
            "credential assignment",
            compile(
                r#"(?i)\b([A-Za-z0-9_.-]*(?:api[_-]?key|secret|password|passwd|passcode|token|credential|private[_-]?key|access[_-]?key|auth)[A-Za-z0-9_.-]*)\s*[=:]\s*["']?([^\s"'&,;)}\]]{6,})"#,
            ),
        ),
        // `https://user:password@host` — the password is the whole point.
        (
            "url credential",
            compile(r"(?i)\b([a-z][a-z0-9+.-]*://)[^\s/:@]+:[^\s/@]+@"),
        ),
        (
            "email address",
            compile(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b"),
        ),
    ]
});

/// `/Users/<name>/…`, `/home/<name>/…`, `C:\Users\<name>\…`.
///
/// Matched independently of whose home it is: another account's name on a
/// shared machine identifies a person just as well as the reporter's does, and
/// a bundle read on a lab workstation routinely contains both.
static USER_HOME_PATH: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(/Users/|/home/|[A-Z]:\\Users\\)([A-Za-z0-9._-]+)")
        .expect("home path pattern is valid")
});

/// Rewrite everything recognisably private in `text`.
///
/// `home` is the current user's home directory when one is known. It is
/// replaced FIRST and with `~`, so the common case reads naturally
/// (`~/Desktop/BioRouter`) instead of collapsing to `<user>`; every other
/// account's home falls through to the generic rule below it.
pub fn scrub(text: &str, home: Option<&Path>) -> Scrubbed {
    let mut out = text.to_string();
    let mut findings: Vec<Finding> = Vec::new();

    if let Some(home) = home.map(|h| h.to_string_lossy().into_owned()) {
        // A home of `/` (or empty) would rewrite every absolute path in the
        // report into nonsense. `dirs::home_dir` can return odd values in a
        // container, so this is a real guard, not defensiveness.
        if home.len() > 1 {
            let count = out.matches(home.as_str()).count();
            if count > 0 {
                out = out.replace(home.as_str(), HOME_PLACEHOLDER);
                findings.push(Finding {
                    kind: "home path",
                    count,
                });
            }
        }
    }

    let mut user_paths = 0usize;
    out = USER_HOME_PATH
        .replace_all(&out, |caps: &regex::Captures<'_>| {
            user_paths += 1;
            format!("{}{USER_PLACEHOLDER}", &caps[1])
        })
        .into_owned();
    if user_paths > 0 {
        findings.push(Finding {
            kind: "username in path",
            count: user_paths,
        });
    }

    for (kind, pattern) in PATTERNS.iter() {
        let mut count = 0usize;
        out = pattern
            .replace_all(&out, |caps: &regex::Captures<'_>| {
                count += 1;
                match *kind {
                    // Keep the KEY, drop the value: "GITHUB_TOKEN was empty" is
                    // the whole content of some bug reports, and a report that
                    // cannot name the setting it is about is useless.
                    "credential assignment" => format!("{}={SECRET_PLACEHOLDER}", &caps[1]),
                    "url credential" => format!("{}{SECRET_PLACEHOLDER}@", &caps[1]),
                    // Keep the header/flag, drop the value — same reason as
                    // `credential assignment`: a report that cannot say WHICH
                    // request was rejected is not a report. Both replacements
                    // are also idempotent, which matters more than it looks:
                    // `validate_issue` re-runs this scrub and refuses anything
                    // it still finds, so a rewrite that re-matched its own
                    // output would refuse every report it had just cleaned.
                    "basic auth" => format!("{} {SECRET_PLACEHOLDER}", &caps[1]),
                    "command-line credential" => format!("{}{SECRET_PLACEHOLDER}", &caps[1]),
                    _ => SECRET_PLACEHOLDER.to_string(),
                }
            })
            .into_owned();
        if count > 0 {
            findings.push(Finding { kind, count });
        }
    }

    Scrubbed {
        text: out,
        findings,
    }
}

/// A reason the report must not be posted as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub rule: &'static str,
    pub detail: String,
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.rule, self.detail)
    }
}

/// The sections a body must carry, matched case-insensitively on the bolded
/// heading the repository's own `bug_report.md` uses.
///
/// Derived from that file at build time would be better and is not possible:
/// the template is markdown prose with no machine-readable section list. It is
/// instead asserted against the real file by a test, so a template edit that
/// renames a section fails here rather than silently producing reports that no
/// longer match it.
pub const REQUIRED_SECTIONS: &[&str] = &[
    "Describe the bug",
    "To Reproduce",
    "Expected behavior",
    "Please provide the following information",
];

/// The last gate before anything is posted.
///
/// Every rule is a refusal, not a warning. The tool has already asked a person
/// to approve a specific body; a check that "notices" a token and posts anyway
/// would make the approval a formality.
pub fn validate_issue(title: &str, body: &str, home: Option<&Path>) -> Vec<Violation> {
    let mut violations = Vec::new();

    let trimmed_title = title.trim();
    if trimmed_title.len() < 8 {
        violations.push(Violation {
            rule: "title",
            detail: format!(
                "the title is {} character(s); a bug report needs a title someone can \
                 recognise in a list",
                trimmed_title.chars().count()
            ),
        });
    }
    if trimmed_title.len() > 200 {
        violations.push(Violation {
            rule: "title",
            detail: "the title is over 200 characters; put the detail in the body".to_string(),
        });
    }
    if trimmed_title.contains('\n') {
        violations.push(Violation {
            rule: "title",
            detail: "the title spans more than one line".to_string(),
        });
    }

    for section in REQUIRED_SECTIONS {
        if !body.contains(section) {
            violations.push(Violation {
                rule: "template",
                detail: format!("the body has no `{section}` section"),
            });
        }
    }

    if body.chars().count() > MAX_ISSUE_BODY_CHARS {
        violations.push(Violation {
            rule: "size",
            detail: format!(
                "the body is {} characters; GitHub's limit is 65,536 and this tool's is {}",
                body.chars().count(),
                MAX_ISSUE_BODY_CHARS
            ),
        });
    }

    // The scrub already ran. Anything it still finds is a pattern the scrub
    // missed on its first pass — an overlap, a value reconstructed by
    // formatting — and the honest answer is to stop.
    let rescrub = scrub(body, home);
    for finding in &rescrub.findings {
        violations.push(Violation {
            rule: "disclosure",
            detail: format!(
                "the body still contains {} {}(s) after redaction",
                finding.count, finding.kind
            ),
        });
    }
    let title_scrub = scrub(title, home);
    for finding in &title_scrub.findings {
        violations.push(Violation {
            rule: "disclosure",
            detail: format!(
                "the title still contains {} {}(s) after redaction",
                finding.count, finding.kind
            ),
        });
    }

    violations
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_home_path_becomes_a_tilde_and_other_accounts_lose_their_name() {
        let scrubbed = scrub(
            "failed to read /Users/jsmith/Desktop/cohort.csv and /Users/klee/tmp/x",
            Some(Path::new("/Users/jsmith")),
        );
        assert!(
            scrubbed.text.contains("~/Desktop/cohort.csv"),
            "{scrubbed:?}"
        );
        assert!(
            scrubbed.text.contains("/Users/<user>/tmp/x"),
            "another account's name identifies a person too: {scrubbed:?}"
        );
        assert!(!scrubbed.text.contains("jsmith"), "{scrubbed:?}");
        assert!(!scrubbed.text.contains("klee"), "{scrubbed:?}");
    }

    #[test]
    fn a_linux_and_a_windows_home_are_both_recognised() {
        let scrubbed = scrub(
            r"/home/wgu/.config/biorouter and C:\Users\WGu\AppData\Roaming",
            None,
        );
        assert!(
            scrubbed.text.contains("/home/<user>/.config"),
            "{scrubbed:?}"
        );
        assert!(
            scrubbed.text.contains(r"C:\Users\<user>\AppData"),
            "{scrubbed:?}"
        );
    }

    #[test]
    fn vendor_tokens_are_redacted_whole() {
        for secret in [
            "ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZ012345",
            "github_pat_11ABCDEFG0abcdefghijklmnop",
            "AKIAIOSFODNN7EXAMPLE",
            "sk-proj-abcdefghijklmnopqrstuvwxyz0123",
            "xoxb-1234567890-abcdefghij",
            "glpat-abcdefghijklmnopqrst",
        ] {
            let scrubbed = scrub(&format!("the error was: {secret} rejected"), None);
            assert!(
                !scrubbed.text.contains(secret),
                "`{secret}` survived: {scrubbed:?}"
            );
            assert!(scrubbed.changed());
        }
    }

    #[test]
    fn a_jwt_is_redacted_including_its_payload() {
        let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dBjftJeZ4CVPmB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let scrubbed = scrub(&format!("Authorization failed for {jwt}"), None);
        assert!(!scrubbed.text.contains("eyJzdWIi"), "{scrubbed:?}");
        assert_eq!(scrubbed.findings[0].kind, "jwt");
    }

    /// The key survives, the value does not. A report that cannot say WHICH
    /// setting was empty is not a report.
    #[test]
    fn a_credential_assignment_keeps_its_key_and_loses_its_value() {
        let scrubbed = scrub(
            "config had GITHUB_TOKEN=ghs_supersecretvalue123456 set",
            None,
        );
        assert!(scrubbed.text.contains("GITHUB_TOKEN"), "{scrubbed:?}");
        assert!(!scrubbed.text.contains("supersecret"), "{scrubbed:?}");
    }

    /// The shape that slipped through BOTH halves: neither the bearer rule nor
    /// the assignment rule can reach the base64 behind `Basic`.
    #[test]
    fn basic_auth_credentials_are_redacted_and_the_header_survives() {
        let secret = "dXNlcjpodW50ZXIyc3VwZXJzZWNyZXQ=";
        for line in [
            format!("Authorization: Basic {secret}"),
            format!("authorization:basic {secret}"),
            format!("Proxy-Authorization: Basic {secret}"),
            format!("-H 'Authorization: Basic {secret}'"),
        ] {
            let scrubbed = scrub(&line, None);
            assert!(!scrubbed.text.contains(secret), "`{line}` survived: {scrubbed:?}");
            assert!(
                scrubbed.text.to_ascii_lowercase().contains("basic"),
                "the header names WHICH request was rejected: {scrubbed:?}"
            );
        }
    }

    /// A false positive here is not cosmetic: `validate_issue` refuses a report
    /// on anything the rescrub still finds, so an over-eager `Basic` rule would
    /// reject a bug report for containing ordinary English.
    #[test]
    fn ordinary_prose_about_basics_is_left_alone() {
        for line in [
            "a basic understanding of the graph schema",
            "Basic authentication is not configured",
            "the basic dashboard renders blank",
        ] {
            let scrubbed = scrub(line, None);
            assert_eq!(scrubbed.text, line, "prose was rewritten: {scrubbed:?}");
        }
    }

    /// `curl -u user:pass` — the same credential one layer earlier, in the
    /// command a user pastes in to show what they ran.
    #[test]
    fn a_curl_user_flag_loses_its_credential_and_keeps_its_flag() {
        for line in [
            "curl -u alice:hunter2 https://api.example.org/v1",
            "curl --user=alice:hunter2 https://api.example.org/v1",
            "curl -sS -H 'Accept: application/json' -u alice:hunter2 https://x.example",
        ] {
            let scrubbed = scrub(line, None);
            assert!(!scrubbed.text.contains("hunter2"), "`{line}` survived: {scrubbed:?}");
            assert!(!scrubbed.text.contains("alice"), "the username identifies a person too: {scrubbed:?}");
            assert!(scrubbed.text.contains("curl"), "{scrubbed:?}");
        }
    }

    /// The `\bcurl\b` prefix is what keeps this rule off every other `-u`.
    #[test]
    fn a_uid_gid_pair_is_not_a_credential() {
        for line in [
            "docker run -u 1000:1000 biorouter/ci",
            "id -u",
            "sort -u results.tsv",
        ] {
            let scrubbed = scrub(line, None);
            assert_eq!(scrubbed.text, line, "a non-credential was rewritten: {scrubbed:?}");
        }
    }

    /// `validate_issue` re-runs the scrub and refuses anything it still finds,
    /// so a rewrite that re-matched its own output would refuse every report it
    /// had just cleaned.
    #[test]
    fn the_new_replacements_do_not_match_their_own_output() {
        let once = scrub(
            "Authorization: Basic dXNlcjpodW50ZXIy and curl -u alice:hunter2 https://x.example",
            None,
        );
        assert!(once.changed());
        let twice = scrub(&once.text, None);
        assert!(
            twice.findings.is_empty(),
            "the second pass found something, so validate_issue would refuse: {twice:?}"
        );
    }

    /// Ordering: the vendor pattern must win, so the receipt names the right
    /// severity.
    #[test]
    fn a_vendor_token_inside_an_assignment_is_reported_as_a_vendor_token() {
        let scrubbed = scrub("token=ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZ012345", None);
        assert!(
            scrubbed.findings.iter().any(|f| f.kind == "vendor token"),
            "{scrubbed:?}"
        );
        assert!(!scrubbed.text.contains("ABCDEFGH"), "{scrubbed:?}");
    }

    #[test]
    fn a_url_password_and_an_email_are_removed() {
        let scrubbed = scrub(
            "cloned https://wgu:hunter2@git.example.org/x and mailed a@b.example.com",
            None,
        );
        assert!(!scrubbed.text.contains("hunter2"), "{scrubbed:?}");
        assert!(!scrubbed.text.contains("a@b.example.com"), "{scrubbed:?}");
        assert!(
            scrubbed.text.contains("https://[redacted]@git.example.org"),
            "the host is diagnostic and stays: {scrubbed:?}"
        );
    }

    /// A degenerate home must not rewrite every path in the report.
    #[test]
    fn a_root_home_is_ignored() {
        let scrubbed = scrub("/etc/hosts is unreadable", Some(Path::new("/")));
        assert_eq!(scrubbed.text, "/etc/hosts is unreadable");
    }

    #[test]
    fn ordinary_prose_is_left_alone() {
        let text = "The chart renders blank when the dataset has one row. \
                    Reproduced on macOS 15.4 with the Chart.js panel.";
        let scrubbed = scrub(text, Some(Path::new("/Users/jsmith")));
        assert_eq!(scrubbed.text, text);
        assert!(!scrubbed.changed());
        assert_eq!(scrubbed.summary(), "nothing needed redacting");
    }

    fn full_body(extra: &str) -> String {
        format!(
            "**Describe the bug**\n{extra}\n\n**To Reproduce**\n1. x\n\n\
             **Expected behavior**\ny\n\n**Please provide the following information**\n- OS: mac\n"
        )
    }

    #[test]
    fn a_complete_scrubbed_report_passes() {
        assert!(validate_issue(
            "Chart panel renders blank for a single-row dataset",
            &full_body("It renders blank."),
            Some(Path::new("/Users/jsmith")),
        )
        .is_empty());
    }

    #[test]
    fn a_missing_section_is_a_violation_naming_the_section() {
        let violations = validate_issue(
            "Chart panel renders blank for a single-row dataset",
            "**Describe the bug**\nblank\n",
            None,
        );
        assert!(
            violations
                .iter()
                .any(|v| v.rule == "template" && v.detail.contains("To Reproduce")),
            "{violations:?}"
        );
    }

    /// ⚠ The load-bearing test. The scrub is not trusted: validation re-runs it
    /// and refuses anything still recognisable, so a body assembled AFTER the
    /// scrub (a receipt line, a template splice) cannot smuggle a path through.
    #[test]
    fn a_body_that_skipped_the_scrub_is_refused_rather_than_posted() {
        let violations = validate_issue(
            "Ingest fails on a PDF with no text layer",
            &full_body("failed at /Users/jsmith/IRB-2019-441/notes.pdf"),
            Some(Path::new("/Users/jsmith")),
        );
        assert!(
            violations.iter().any(|v| v.rule == "disclosure"),
            "{violations:?}"
        );
    }

    #[test]
    fn a_secret_in_the_title_is_refused_too() {
        let violations = validate_issue(
            "Install fails with ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZ012345",
            &full_body("x"),
            None,
        );
        assert!(
            violations
                .iter()
                .any(|v| v.rule == "disclosure" && v.detail.contains("title")),
            "{violations:?}"
        );
    }

    #[test]
    fn an_empty_or_enormous_title_is_refused() {
        assert!(validate_issue("bug", &full_body("x"), None)
            .iter()
            .any(|v| v.rule == "title"));
        assert!(validate_issue(&"x".repeat(300), &full_body("x"), None)
            .iter()
            .any(|v| v.rule == "title"));
    }

    #[test]
    fn an_over_long_body_is_refused() {
        let body = format!("{}{}", full_body("x"), "y".repeat(MAX_ISSUE_BODY_CHARS));
        assert!(validate_issue("A perfectly ordinary title", &body, None)
            .iter()
            .any(|v| v.rule == "size"));
    }

    /// The section list this module enforces is the one the repository's own
    /// template actually uses. They live in different files and neither refers
    /// to the other, so a template rename would otherwise produce reports that
    /// silently stop matching it.
    #[test]
    fn the_required_sections_are_the_ones_the_repository_template_declares() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("the crate sits two levels under the repository root")
            .join(".github/ISSUE_TEMPLATE/bug_report.md");
        let Ok(template) = std::fs::read_to_string(&path) else {
            // A consumer vendoring this crate has no `.github/`. Skipping is
            // right; passing vacuously in the REPOSITORY is not, and the
            // repository always has the file.
            return;
        };
        for section in REQUIRED_SECTIONS {
            assert!(
                template.contains(&format!("**{section}**")),
                "`{section}` is not a heading in {}",
                path.display()
            );
        }
    }
}
