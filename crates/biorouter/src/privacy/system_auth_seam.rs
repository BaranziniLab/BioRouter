//! The test seam that stands in for the OS prompt (DR-20 point 6, DR-24 Step 3).
//!
//! The operator's ruling asks for it in as many words — *"to test it, please set
//! up this process so that when password is asked, a programatic msg is sent"* —
//! because otherwise every test of every declassification path, and every
//! operator session driving the dev GUI, would mean typing a password again.
//!
//! ⚠ **This is the part of DR-20 that must never ship, and the file-level `cfg`
//! below is the boundary.** Both halves of it are load-bearing:
//!
//! * `debug_assertions` is off in `release`, `release-dist` and `quick` — every
//!   profile `scripts/release.sh` and every `just release-*` recipe builds.
//! * `privacy-test-auth` is a non-default feature, reached from test builds
//!   through `[dev-dependencies]` so that `resolver = "2"` unifies it into a
//!   test build and not into a plain `cargo build`.
//!
//! A `compile_error!` in `crates/biorouter/src/lib.rs` turns the combination
//! "feature on, debug assertions off" into a build failure, so the guard is the
//! compiler rather than a script someone must remember to run.
//!
//! ⚠ **An environment variable is not, and may not become, that boundary.**
//! AR-11 measured the daemon's own environment to be recoverable in-process, so
//! an env-gated bypass is a bypass a compromised agent sets for itself.
//! `BIOROUTER_PRIVACY_TEST_AUTH` selects *which canned answer* an
//! already-compiled seam gives; it never makes the seam exist.

#![cfg(all(debug_assertions, feature = "privacy-test-auth"))]

use super::system_auth::{AuthOutcome, AuthRequest, SystemAuthenticator, TEST_SEAM_PLATFORM};
use std::sync::Mutex;
use tracing::warn;

/// The canned answer for the **next** prompt, consumed by it.
static NEXT: Mutex<Option<AuthOutcome>> = Mutex::new(None);
/// The last request the seam was handed, so a test can assert what the prompt
/// would have named — DR-20 point 4 is untestable otherwise.
static LAST: Mutex<Option<AuthRequest>> = Mutex::new(None);

/// Arm the seam for exactly one prompt.
///
/// One-shot on purpose: the answer is consumed by the prompt it answers, so an
/// armed test cannot go on to approve a second declassification it never
/// intended to authorise. That is DR-20 point 2 — no cached grant — expressed
/// inside the seam, so the seam cannot be *weaker* than the thing it replaces.
pub fn answer_next_prompt(outcome: AuthOutcome) {
    *NEXT.lock().expect("the test seam's arming lock") = Some(outcome);
}

/// What the seam was last asked to authorise.
pub fn last_request() -> Option<AuthRequest> {
    LAST.lock().expect("the test seam's record lock").clone()
}

/// Forget both the arming and the record, for a test that wants a clean slate.
pub fn reset() {
    *NEXT.lock().expect("the test seam's arming lock") = None;
    *LAST.lock().expect("the test seam's record lock") = None;
}

pub struct Seam;

pub fn prompter() -> &'static dyn SystemAuthenticator {
    &Seam
}

#[async_trait::async_trait]
impl SystemAuthenticator for Seam {
    async fn authenticate(&self, req: &AuthRequest) -> AuthOutcome {
        *LAST.lock().expect("the test seam's record lock") = Some(req.clone());

        // The operator's "a programatic msg is sent", taken literally. A live
        // seam must be obvious in a log at a glance, and that visibility is
        // itself a safety property: it is how someone notices a build that
        // should not have one.
        warn!(
            event = "privacy_test_auth_used",
            ids = ?req.session_ids,
            "[TEST-AUTH] the DR-20 system prompt was answered by the test seam, not by a user: {}",
            req.reason
        );
        eprintln!(
            "[TEST-AUTH] system authentication bypassed by the privacy-test-auth seam ({} session(s))",
            req.session_ids.len()
        );

        let armed = NEXT.lock().expect("the test seam's arming lock").take();
        // DEFAULT REFUSE. A test that forgets to arm the seam fails closed and
        // reads as "the user cancelled" — which is the correct thing for it to
        // read as.
        armed.or_else(env_answer).unwrap_or(AuthOutcome::Denied)
    }

    fn platform(&self) -> &'static str {
        TEST_SEAM_PLATFORM
    }
}

/// `BIOROUTER_PRIVACY_TEST_AUTH=approve|deny|unavailable`, for driving the dev
/// GUI where there is no test to call [`answer_next_prompt`].
///
/// Consulted only *after* the one-shot arming, and only inside an
/// already-compiled seam. An unrecognised value arms nothing, so a typo refuses.
fn env_answer() -> Option<AuthOutcome> {
    match std::env::var("BIOROUTER_PRIVACY_TEST_AUTH")
        .ok()?
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "approve" | "approved" | "allow" => Some(AuthOutcome::Approved),
        "deny" | "denied" => Some(AuthOutcome::Denied),
        "unavailable" => Some(AuthOutcome::Unavailable),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> AuthRequest {
        AuthRequest::new("Make 1 chat public.", &["20260804_120000".into()]).unwrap()
    }

    #[tokio::test]
    #[serial_test::serial(privacy_test_auth_seam)]
    async fn an_unarmed_seam_refuses() {
        reset();
        let _env = env_lock::lock_env([("BIOROUTER_PRIVACY_TEST_AUTH", None::<&str>)]);
        assert_eq!(Seam.authenticate(&request()).await, AuthOutcome::Denied);
    }

    #[tokio::test]
    #[serial_test::serial(privacy_test_auth_seam)]
    async fn arming_is_one_shot_so_a_second_prompt_is_not_covered() {
        reset();
        let _env = env_lock::lock_env([("BIOROUTER_PRIVACY_TEST_AUTH", None::<&str>)]);
        answer_next_prompt(AuthOutcome::Approved);
        assert_eq!(Seam.authenticate(&request()).await, AuthOutcome::Approved);
        assert_eq!(
            Seam.authenticate(&request()).await,
            AuthOutcome::Denied,
            "the canned answer must be consumed by the prompt it answered"
        );
    }

    #[tokio::test]
    #[serial_test::serial(privacy_test_auth_seam)]
    async fn the_seam_records_the_exact_id_set_it_was_asked_about() {
        reset();
        let req = AuthRequest::new("Make 2 chats public.", &["b".into(), "a".into()]).unwrap();
        answer_next_prompt(AuthOutcome::Approved);
        Seam.authenticate(&req).await;
        assert_eq!(
            last_request()
                .expect("the seam records what it was asked")
                .session_ids,
            vec!["a".to_string(), "b".to_string()]
        );
    }

    #[tokio::test]
    #[serial_test::serial(privacy_test_auth_seam)]
    async fn the_env_var_selects_an_answer_but_a_typo_still_refuses() {
        for (value, expected) in [
            ("approve", AuthOutcome::Approved),
            ("unavailable", AuthOutcome::Unavailable),
            ("deny", AuthOutcome::Denied),
            // An unrecognised value arms nothing, so the default refuse stands.
            ("yes please", AuthOutcome::Denied),
        ] {
            reset();
            let _env = env_lock::lock_env([("BIOROUTER_PRIVACY_TEST_AUTH", Some(value))]);
            assert_eq!(
                Seam.authenticate(&request()).await,
                expected,
                "BIOROUTER_PRIVACY_TEST_AUTH={value}"
            );
        }
    }

    #[tokio::test]
    #[serial_test::serial(privacy_test_auth_seam)]
    async fn the_env_var_never_beats_an_armed_answer() {
        // The arming is the test's explicit intent; the env var is a convenience
        // for driving the dev GUI. If the two disagree the explicit one wins, or
        // a stray export in a developer's shell would silently rewrite what
        // every test in the suite authorised.
        reset();
        let _env = env_lock::lock_env([("BIOROUTER_PRIVACY_TEST_AUTH", Some("approve"))]);
        answer_next_prompt(AuthOutcome::Denied);
        assert_eq!(Seam.authenticate(&request()).await, AuthOutcome::Denied);
    }

    #[test]
    fn the_seam_names_itself_as_the_seam() {
        assert_eq!(Seam.platform(), TEST_SEAM_PLATFORM);
    }
}
