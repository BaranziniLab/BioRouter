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

/// Where the bundled prompt helper lives, if it is there at all.
///
/// `BIOROUTER_AUTHPROMPT_APP` overrides, for a dev tree or a test. Otherwise it
/// is looked for beside the running executable, which is where the packaged app
/// puts it (`Biorouter.app/Contents/Resources/`), and `biorouterd` itself lives
/// in `Contents/Resources/bin/`.
///
/// `None` means "not bundled", and the caller falls back to the in-process
/// call. That path is not dead: it is what the CLI and any shell-started daemon
/// use, and it works there.
fn helper_app() -> Option<std::path::PathBuf> {
    const BUNDLE: &str = "Biorouter Authentication.app";
    if let Ok(explicit) = std::env::var("BIOROUTER_AUTHPROMPT_APP") {
        let path = std::path::PathBuf::from(explicit);
        return path.exists().then_some(path);
    }
    let exe = std::env::current_exe().ok()?;
    // `…/Contents/Resources/bin/biorouterd` -> `…/Contents/Resources/<bundle>`
    let candidate = exe.parent()?.parent()?.join(BUNDLE);
    candidate.exists().then_some(candidate)
}

/// Ask the bundled helper to raise the prompt, and believe its answer only if
/// it comes back carrying the nonce we issued.
///
/// ⚠ **Why a separate process at all** is documented at length on
/// `biorouter-authprompt`'s crate root, with the measurements: an in-process
/// `evaluatePolicy` never returns when the daemon was started by the desktop
/// app, and neither does Electron's own `promptTouchID`, and neither does an
/// Electron process holding a visible focused window. Launching through `open`
/// — so LaunchServices starts the helper under **launchd** rather than under
/// us — is what makes the prompt appear.
///
/// ⚠ **The nonce is not a security boundary.** Anything running as this user
/// can read it and forge an approval; what it buys is that forging requires
/// reading the daemon's private directory rather than guessing a path. This was
/// chosen knowingly over an unforgeable check that never fires, and it is why
/// the nonce goes in a 0600 file and never in argv or the environment, both of
/// which any process on the machine can read.
#[cfg(target_os = "macos")]
fn evaluate_via_helper(app: &std::path::Path, reason: &str) -> Option<MacAuthSignal> {
    use rand::Rng;
    use std::io::Write;
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};

    let dir = tempfile::Builder::new()
        .prefix("biorouter-auth-")
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir()
        .ok()?;
    let _ = std::fs::DirBuilder::new().mode(0o700).create(dir.path());

    let nonce: String = {
        let bytes: [u8; 32] = rand::thread_rng().gen();
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    };
    let nonce_path = dir.path().join("nonce");
    let result_path = dir.path().join("result");

    let mut nonce_file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&nonce_path)
        .ok()?;
    nonce_file.write_all(nonce.as_bytes()).ok()?;
    drop(nonce_file);

    // `-n` so a helper left over from a previous prompt cannot absorb this one.
    let spawned = std::process::Command::new("/usr/bin/open")
        .arg("-n")
        .arg("-a")
        .arg(app)
        .arg("--args")
        .arg(&nonce_path)
        .arg(&result_path)
        .arg(reason)
        .status();
    if !matches!(spawned, Ok(status) if status.success()) {
        return None;
    }

    // Poll rather than watch: the helper is a separate process launched through
    // LaunchServices, so there is no child to wait on and no descriptor to
    // select over. Bounded by the same budget the caller uses.
    let deadline = std::time::Instant::now() + super::system_auth::PROMPT_TIMEOUT;
    while std::time::Instant::now() < deadline {
        if let Ok(contents) = std::fs::read_to_string(&result_path) {
            return Some(parse_helper_result(&nonce, &contents));
        }
        std::thread::sleep(std::time::Duration::from_millis(150));
    }
    // Ran out of time. Not an approval.
    Some(MacAuthSignal::EvaluationFailed(0))
}

/// Turn the helper's one line into a signal, checking the nonce first.
///
/// ⚠ **Split out so it can be tested at all.** Everything else on this path
/// needs a human at a Touch ID sensor, which no test has — so without this the
/// only security-relevant logic here (does an approval we did not ask for get
/// believed?) would ship unexercised.
///
/// ⚠ **Fails closed on everything.** A missing nonce, the wrong nonce, a
/// truncated line, an unrecognised verdict, empty content: all of them are
/// `EvaluationFailed`, which the caller reads as "not approved". The only input
/// that approves is our exact nonce followed by exactly `approved`.
fn parse_helper_result(nonce: &str, contents: &str) -> MacAuthSignal {
    let mut parts = contents.trim().splitn(2, ' ');
    let seen = parts.next().unwrap_or_default();
    let verdict = parts.next().unwrap_or_default();
    // Constant-time comparison would be theatre: an attacker able to time this
    // can already read the nonce file. An EXACT match is not optional though —
    // a result the daemon did not ask for is not an answer to it.
    if seen.is_empty() || seen != nonce {
        return MacAuthSignal::EvaluationFailed(0);
    }
    match verdict {
        "approved" => MacAuthSignal::Evaluated,
        "unavailable" => MacAuthSignal::PolicyUnavailable(0),
        // "denied", and anything at all we do not recognise.
        _ => MacAuthSignal::EvaluationFailed(0),
    }
}

/// Raise the prompt and block until the user answers.
fn evaluate(reason: &str) -> MacAuthSignal {
    // The bundled helper first, because the in-process call below cannot work
    // under the desktop app. Absent (CLI, dev tree, shell-started daemon) the
    // in-process call is correct and is what runs.
    if let Some(app) = helper_app() {
        if let Some(signal) = evaluate_via_helper(&app, reason) {
            return signal;
        }
        // The helper is present but could not be launched. Fall through rather
        // than refuse: the in-process attempt costs one bounded wait and may
        // still succeed on a host where it works.
    }

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
    // talks to `coreauthd` over a *synchronous* XPC. Under the desktop app that
    // XPC gets no reply, the reply block never fires, and a plain `recv()` here
    // parks this thread for the life of the daemon. Measured on the packaged
    // 1.89.0 build: the tokio blocking-pool thread sat in `_pthread_cond_wait`
    // while the `la_client` thread sat in
    // `__NSXPCCONNECTION_IS_WAITING_FOR_A_SYNCHRONOUS_REPLY__`, and the HTTP
    // handler above never answered.
    //
    // ⚠ **WHY it gets no reply is NOT known, and the explanation that used to
    // stand here was measured to be false.** That explanation was "a process
    // that is not a foreground application — which `biorouterd` never is". It
    // does not survive contact:
    //
    //   * An Electron app *with a visible, focused window* — a foreground
    //     application by any definition — cannot raise this prompt either. Its
    //     own `systemPreferences.promptTouchID()` times out identically, so the
    //     failure is not about windows and not about this crate's FFI.
    //   * `TransformProcessType` to a UI-capable process type succeeds from a
    //     windowless process and changes nothing here. It was implemented,
    //     tested against the packaged app, and reverted.
    //   * The same release binary run from a shell reaches `coreauthd` and
    //     returns in seconds. Only the descent from the desktop app fails, and
    //     it fails whether the app is adhoc-signed or Developer-ID signed with
    //     the hardened runtime, and whether it was exec'd directly or launched
    //     through LaunchServices.
    //
    // So the discriminator is something about running underneath Electron, not
    // the daemon's window-lessness. Do not restore the old sentence, and do not
    // replace it with a fresh guess: the guess is what made this look understood
    // and therefore not worth re-testing. The bound below is correct defensive
    // code either way, which is why it stays while the cause is still open.
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

    /// The helper's reply is the one thing on this path a test can reach: the
    /// rest needs a finger on a sensor. It is also the only place an approval
    /// gets *believed*, so it is the half worth pinning.
    #[test]
    fn only_our_own_nonce_with_an_explicit_approval_is_believed() {
        let nonce = "a".repeat(64);
        assert!(matches!(
            parse_helper_result(&nonce, &format!("{nonce} approved\n")),
            MacAuthSignal::Evaluated
        ));
    }

    /// ⚠ Every one of these is an APPROVAL that must not be granted. A helper
    /// reply carrying someone else's nonce is the forgery the nonce exists to
    /// catch; the rest are malformed input, and malformed input is not consent.
    #[test]
    fn everything_else_fails_closed() {
        let nonce = "a".repeat(64);
        let other = "b".repeat(64);
        for hostile in [
            format!("{other} approved"),       // a reply we never asked for
            "approved".to_string(),            // verdict with no nonce at all
            format!("{nonce}"),                // our nonce, no verdict
            format!("{nonce} "),               // our nonce, empty verdict
            format!("{nonce} APPROVED"),       // case is not a match
            format!("{nonce} approved extra"), // trailing junk after the verdict
            format!(" approved {nonce}"),      // fields swapped
            String::new(),                     // empty file, e.g. a crashed helper
            format!("{}", &nonce[..32]),       // truncated nonce
        ] {
            assert!(
                !matches!(
                    parse_helper_result(&nonce, &hostile),
                    MacAuthSignal::Evaluated
                ),
                "approved on {hostile:?}"
            );
        }
    }

    /// "This Mac cannot ask" is not "the user said no", and the two lead to
    /// different advice — so the helper's third answer must survive the trip.
    #[test]
    fn an_unavailable_verdict_is_not_collapsed_into_a_refusal() {
        let nonce = "c".repeat(64);
        assert!(matches!(
            parse_helper_result(&nonce, &format!("{nonce} unavailable")),
            MacAuthSignal::PolicyUnavailable(_)
        ));
        assert!(matches!(
            parse_helper_result(&nonce, &format!("{nonce} denied")),
            MacAuthSignal::EvaluationFailed(_)
        ));
    }

    #[test]
    fn a_reason_string_survives_the_round_trip_into_objective_c() {
        let s = ns_string("Make 1 chat public — ünicode ok").expect("NSString class");
        // SAFETY: `-length` on an NSString returns its length in UTF-16 units.
        let length: usize = unsafe { msg_send![&*s, length] };
        assert!(length > 0);
    }
}
