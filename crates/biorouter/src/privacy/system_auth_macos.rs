//! The macOS prompter (DR-24): `LAContext.evaluatePolicy` with
//! `LAPolicyDeviceOwnerAuthentication`.
//!
//! That policy — **not** `…WithBiometrics` — is the one DR-20 asks for: it
//! accepts Touch ID *and falls back to the login password*, so it works on a Mac
//! with no sensor and on one whose user prefers to type.
//!
//! ⚠ **`systemPreferences.promptTouchID()` is explicitly not this**, and DR-24
//! says why: it is Touch-ID-only, fails outright without a sensor, and offers no
//! password fallback — which is precisely the fallback the operator's ruling
//! names. It also lives in Electron, so it could not serve the CLI at all.
//!
//! Only the *acquisition* of the OS's answer lives here. The decision — which
//! `LAError` code means refused and which means "this Mac cannot" — is
//! [`super::system_auth::outcome_for_macos`], which is compiled and tested on
//! every host.

#![cfg(target_os = "macos")]

use super::system_auth::{
    outcome_for_macos, AuthOutcome, AuthRequest, MacAuthSignal, SystemAuthenticator,
};
use block2::RcBlock;
use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Bool};
use std::sync::mpsc;

/// `LAPolicyDeviceOwnerAuthentication` — biometrics **or** the login password.
/// (`LAPolicyDeviceOwnerAuthenticationWithBiometrics` is 1 and is the wrong one.)
const LA_POLICY_DEVICE_OWNER_AUTHENTICATION: isize = 2;

// Pulls in LocalAuthentication.framework so the `LAContext` class is registered
// with the Objective-C runtime. Without it `AnyClass::get` returns `None` and
// the prompter degrades to `AuthOutcome::Unavailable` on every Mac — a silent,
// total loss of the feature that no test on another platform would catch, which
// is why the link is declared here beside the lookup and asserted by
// `localauthentication_is_linked_into_this_binary` below.
#[link(name = "LocalAuthentication", kind = "framework")]
extern "C" {}

pub struct MacOsPrompter;

pub fn prompter() -> &'static dyn SystemAuthenticator {
    &MacOsPrompter
}

#[async_trait::async_trait]
impl SystemAuthenticator for MacOsPrompter {
    async fn authenticate(&self, req: &AuthRequest) -> AuthOutcome {
        let reason = prompt_text(req);
        // `evaluatePolicy` calls back on an internal queue and the wait is a
        // human typing a password, so it goes on the blocking pool rather than
        // parking a tokio worker for however long the user takes.
        let signal = tokio::task::spawn_blocking(move || evaluate(&reason))
            .await
            // A panic or a shutdown in the blocking pool is not an approval.
            .unwrap_or(MacAuthSignal::EvaluationFailed(0));
        outcome_for_macos(signal)
    }

    fn platform(&self) -> &'static str {
        "macOS (LocalAuthentication)"
    }
}

/// What the OS shows the user.
///
/// DR-20 point 4: the prompt must state the set it authorises, so the reason and
/// the ids travel together into the dialog rather than the dialog saying
/// "BioRouter wants to make changes".
fn prompt_text(req: &AuthRequest) -> String {
    format!("{} ({})", req.reason, req.session_ids.join(", "))
}

/// Raise the prompt and block until the user answers.
fn evaluate(reason: &str) -> MacAuthSignal {
    let Some(context_class) = AnyClass::get(c"LAContext") else {
        return MacAuthSignal::FrameworkMissing;
    };
    // SAFETY: `+[LAContext new]` takes no arguments and returns a +1 instance.
    let context: Retained<AnyObject> = unsafe { msg_send![context_class, new] };

    // Ask first, so "this Mac has no passcode set" is distinguishable from "the
    // user cancelled" — they map to different `AuthOutcome`s and to different
    // advice.
    let mut probe_error: *mut AnyObject = std::ptr::null_mut();
    // SAFETY: `-canEvaluatePolicy:error:` takes an LAPolicy and an out-pointer
    // to an NSError, and returns BOOL.
    let can_evaluate: Bool = unsafe {
        msg_send![
            &*context,
            canEvaluatePolicy: LA_POLICY_DEVICE_OWNER_AUTHENTICATION,
            error: &mut probe_error
        ]
    };
    if !can_evaluate.as_bool() {
        return MacAuthSignal::PolicyUnavailable(error_code(probe_error));
    }

    let (tx, rx) = mpsc::sync_channel::<MacAuthSignal>(1);
    let reply = RcBlock::new(move |success: Bool, error: *mut AnyObject| {
        let signal = if success.as_bool() {
            MacAuthSignal::Evaluated
        } else {
            MacAuthSignal::EvaluationFailed(error_code(error))
        };
        // The receiver is alive until this thread's `recv` returns, and the
        // channel has capacity, so this cannot block the callback queue.
        let _ = tx.send(signal);
    });

    let Some(localized_reason) = ns_string(reason) else {
        return MacAuthSignal::FrameworkMissing;
    };

    // SAFETY: `-evaluatePolicy:localizedReason:reply:` takes an LAPolicy, an
    // NSString and a `void (^)(BOOL, NSError *)` block. The block is copied to
    // the heap by `RcBlock` and retained by the callee for the duration of the
    // call, so it outlives this frame's `reply` binding.
    let _: () = unsafe {
        msg_send![
            &*context,
            evaluatePolicy: LA_POLICY_DEVICE_OWNER_AUTHENTICATION,
            localizedReason: &*localized_reason,
            reply: &*reply
        ]
    };

    // ⚠ **Bounded, and this is the P-05 fix at its source.**
    //
    // `evaluatePolicy` hands the work to an internal `la_client` queue, which
    // talks to `coreauthd` over a *synchronous* XPC. In a process that is not a
    // foreground application — which `biorouterd` never is — that XPC gets no
    // reply, the reply block never fires, and a plain `recv()` here parks this
    // thread for the life of the daemon. Measured on the packaged 1.89.0 build:
    // the tokio blocking-pool thread sat in `_pthread_cond_wait` while the
    // `la_client` thread sat in `__NSXPCCONNECTION_IS_WAITING_FOR_A_SYNCHRONOUS_REPLY__`,
    // and the HTTP handler above never answered.
    //
    // `super::system_auth::authenticate_or_refuse` already bounds the *caller*,
    // so the request terminates either way. Bounding it here as well is what
    // keeps the blocking-pool thread from being lost with it: a `spawn_blocking`
    // task that never finishes still occupies its thread after its future is
    // dropped, so without this every attempt would retire one thread out of the
    // runtime's finite pool.
    //
    // Slightly longer than the caller's bound so the two cannot race to describe
    // the same failure differently — the caller's message is the one the user
    // sees, and this is the cleanup behind it.
    //
    // A timeout, like a dropped sender, is not an approval.
    rx.recv_timeout(super::system_auth::PROMPT_TIMEOUT + std::time::Duration::from_secs(5))
        .unwrap_or(MacAuthSignal::EvaluationFailed(0))
}

/// `-[NSError code]`, or 0 for a null error.
fn error_code(error: *mut AnyObject) -> i64 {
    if error.is_null() {
        return 0;
    }
    // SAFETY: the callee hands back an NSError, whose `-code` returns NSInteger.
    unsafe { msg_send![&*error, code] }
}

/// An autoreleased `NSString` for `reason`, without taking a dependency on
/// `objc2-foundation` for one selector.
fn ns_string(value: &str) -> Option<Retained<AnyObject>> {
    let class = AnyClass::get(c"NSString")?;
    let bytes = value.as_bytes();
    // SAFETY: `+[NSString stringWithBytes:length:encoding:]` copies the buffer;
    // 4 is NSUTF8StringEncoding, and `value` is valid UTF-8 by construction.
    let string: Retained<AnyObject> = unsafe {
        msg_send![
            class,
            stringWithBytes: bytes.as_ptr().cast::<std::ffi::c_void>(),
            length: bytes.len(),
            encoding: 4usize
        ]
    };
    Some(string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn localauthentication_is_linked_into_this_binary() {
        // The whole macOS prompter degrades to `Unavailable` if the framework is
        // not linked, and it does so silently. This is the assertion that turns
        // a dropped `#[link]` attribute into a red build rather than into a
        // feature that quietly stops existing on every Mac.
        assert!(
            AnyClass::get(c"LAContext").is_some(),
            "LocalAuthentication.framework is not linked, so LAContext is not registered"
        );
    }

    #[test]
    fn the_prompt_names_the_sessions_it_authorises() {
        let req = AuthRequest::new("Make 2 chats public.", &["a1".into(), "b2".into()]).unwrap();
        let text = prompt_text(&req);
        assert!(text.contains("a1") && text.contains("b2"), "got {text}");
        assert!(text.contains("Make 2 chats public."), "got {text}");
    }

    #[test]
    fn a_null_error_reads_as_code_zero_rather_than_dereferencing() {
        assert_eq!(error_code(std::ptr::null_mut()), 0);
    }

    #[test]
    fn a_reason_string_survives_the_round_trip_into_objective_c() {
        let s = ns_string("Make 1 chat public — ünicode ok").expect("NSString class");
        // SAFETY: `-length` on an NSString returns its length in UTF-16 units.
        let length: usize = unsafe { msg_send![&*s, length] };
        assert!(length > 0);
    }
}
