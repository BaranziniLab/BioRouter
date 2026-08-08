//! The macOS authentication prompt that Biorouter's background service cannot
//! raise for itself (issue #56, DR-20 / DR-24).
//!
//! # Why this exists as a separate program
//!
//! `LAContext.evaluatePolicy` works fine from an ordinary process. It does
//! **not** work from `biorouterd` when `biorouterd` was started by the desktop
//! app: the call's synchronous XPC to `coreauthd` never gets a reply, and the
//! request hangs until its timeout. Measured on this release, same binary each
//! time:
//!
//! | how the caller was started | prompt |
//! |---|---|
//! | from a shell | appears, answered in ~4s |
//! | spawned by Electron | never appears |
//! | Electron's own main process, `promptTouchID` | never appears |
//! | Electron's main process **with a visible focused window** | never appears |
//! | **this helper, launched via `open`** | **appears, answered in ~3s** |
//!
//! Note the fourth row: a foreground application with a real window fails too,
//! so "the daemon has no window" — the explanation this repo carried for a
//! while — is not the cause. What separates the last row is `open`: it hands
//! the launch to LaunchServices, which starts the app as a child of **launchd**
//! rather than of whatever asked. That severs the lineage that breaks the XPC.
//!
//! # The protocol, and what it is honestly worth
//!
//! `biorouterd` creates a private directory, writes a single-use nonce into it,
//! and launches this helper with the *paths*. The helper reads the nonce,
//! raises the prompt, and writes `<nonce> <verdict>` to the result file. The
//! daemon accepts the verdict only if the nonce comes back verbatim.
//!
//! ⚠ **This is not a security boundary, and the nonce does not make it one.**
//! Anything running as the same user can read the nonce file and forge an
//! approval. That is a real weakening compared to an in-process call, which
//! cannot be forged at all — and it was accepted deliberately, because the
//! in-process call does not work under the desktop app, so the alternative is
//! not a stronger check but no check. What the nonce buys is that forging
//! requires *reading the daemon's private state* rather than guessing a
//! predictable path or racing a world-writable file. Biorouter's privacy
//! barrier is documented as safety rather than security for exactly this class
//! of reason; see `docs/security/privacy-tiers.md`.
//!
//! ⚠ **The nonce travels in a FILE, never in argv or the environment.** Both of
//! those are readable by any process on the machine (`ps`, `KERN_PROCARGS2`),
//! which would hand the secret to precisely the attacker it exists to
//! inconvenience. Only paths are passed as arguments.

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("biorouter-authprompt is macOS-only");
    std::process::exit(64);
}

#[cfg(target_os = "macos")]
fn main() {
    use std::io::Write;

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!("usage: biorouter-authprompt <nonce-file> <result-file> <reason>");
        std::process::exit(64);
    }
    let (nonce_path, result_path, reason) = (&args[1], &args[2], &args[3]);

    // A missing or unreadable nonce means the daemon did not set this up, which
    // is not something to authenticate past.
    let Ok(nonce) = std::fs::read_to_string(nonce_path) else {
        std::process::exit(65);
    };
    let nonce = nonce.trim().to_string();
    if nonce.is_empty() {
        std::process::exit(65);
    }

    let verdict = match macos::evaluate(reason) {
        macos::Verdict::Approved => "approved",
        macos::Verdict::Denied => "denied",
        macos::Verdict::Unavailable => "unavailable",
    };

    // The nonce leads, so a truncated write cannot be read as a bare verdict,
    // and the daemon's comparison fails closed on anything it did not issue.
    let line = format!("{nonce} {verdict}\n");

    // Written to a sibling temp path and renamed, so the daemon can never read
    // a half-written result and mistake it for an answer.
    let staging = format!("{result_path}.partial");
    if let Ok(mut file) = std::fs::File::create(&staging) {
        let _ = file.write_all(line.as_bytes());
        let _ = file.sync_all();
        drop(file);
        let _ = std::fs::rename(&staging, result_path);
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use block2::RcBlock;
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject, Bool};
    use std::sync::mpsc;
    use std::time::Duration;

    /// Biometrics **or** the login password — not `…WithBiometrics`, which is
    /// Touch-ID-only and fails outright on a Mac without a sensor. DR-20 asks
    /// for the password fallback by name.
    const LA_POLICY_DEVICE_OWNER_AUTHENTICATION: isize = 2;

    /// Long enough for someone to notice the dialog, find their password and
    /// type it; short enough that a forgotten prompt does not leave this
    /// process sitting on screen forever. The daemon bounds itself separately
    /// and more tightly, so this is the backstop, not the deadline.
    const WAIT: Duration = Duration::from_secs(120);

    #[link(name = "LocalAuthentication", kind = "framework")]
    extern "C" {}

    pub enum Verdict {
        Approved,
        Denied,
        /// This Mac cannot ask at all — no passcode set, or the framework is
        /// missing. Distinct from `Denied` because it means something different
        /// to the user and leads to different advice.
        Unavailable,
    }

    pub fn evaluate(reason: &str) -> Verdict {
        let Some(class) = AnyClass::get(c"LAContext") else {
            return Verdict::Unavailable;
        };
        // SAFETY: `+[LAContext new]` takes no arguments and returns a +1 instance.
        let context: Retained<AnyObject> = unsafe { msg_send![class, new] };

        let mut probe_error: *mut AnyObject = std::ptr::null_mut();
        // SAFETY: `-canEvaluatePolicy:error:` takes an LAPolicy and an
        // out-pointer to an NSError, and returns BOOL.
        let can: Bool = unsafe {
            msg_send![
                &*context,
                canEvaluatePolicy: LA_POLICY_DEVICE_OWNER_AUTHENTICATION,
                error: &mut probe_error
            ]
        };
        if !can.as_bool() {
            return Verdict::Unavailable;
        }

        let (tx, rx) = mpsc::sync_channel::<bool>(1);
        let reply = RcBlock::new(move |success: Bool, _error: *mut AnyObject| {
            let _ = tx.send(success.as_bool());
        });

        let Some(localized) = ns_string(reason) else {
            return Verdict::Unavailable;
        };

        // SAFETY: `-evaluatePolicy:localizedReason:reply:` takes an LAPolicy, an
        // NSString and a `void (^)(BOOL, NSError *)`. `RcBlock` heap-copies the
        // block and the callee retains it for the call's duration, so it
        // outlives this frame.
        let _: () = unsafe {
            msg_send![
                &*context,
                evaluatePolicy: LA_POLICY_DEVICE_OWNER_AUTHENTICATION,
                localizedReason: &*localized,
                reply: &*reply
            ]
        };

        // A timeout is not an approval, and neither is a dropped sender.
        match rx.recv_timeout(WAIT) {
            Ok(true) => Verdict::Approved,
            Ok(false) => Verdict::Denied,
            Err(_) => Verdict::Denied,
        }
    }

    fn ns_string(value: &str) -> Option<Retained<AnyObject>> {
        let class = AnyClass::get(c"NSString")?;
        let bytes = value.as_bytes();
        // SAFETY: `+[NSString stringWithBytes:length:encoding:]` copies the
        // buffer; 4 is NSUTF8StringEncoding and `value` is valid UTF-8.
        let string: Retained<AnyObject> = unsafe {
            msg_send![
                class,
                stringWithBytes: bytes.as_ptr(),
                length: bytes.len(),
                encoding: 4usize
            ]
        };
        Some(string)
    }
}
