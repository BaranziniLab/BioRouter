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
//! opens a unix socket there, and launches this helper with the *paths*. The
//! helper reads the nonce, raises the prompt, and sends `<nonce> <verdict>`
//! down the socket. The daemon accepts it only if **both** hold: the connecting
//! process is signed by our Developer ID team, and the nonce comes back
//! verbatim.
//!
//! ⚠ **The nonce alone was not enough, and that is why the socket exists.** A
//! result FILE can be written by anything running as the same user — including
//! an agent with shell access, which can also read the nonce out of the
//! daemon's private directory. Since the operations behind this prompt include
//! *turning privacy enforcement off*, a forged approval there is an escalation
//! rather than a skipped dialog. Connecting over a socket is what lets the
//! daemon ask the kernel **which process** is answering
//! (`super::system_auth_peer`), so forging now requires a binary signed by
//! `F3YYBXAFJ8` rather than merely a process running as you.
//!
//! ⚠ **It is still not unforgeable.** Someone who can inject into a process we
//! signed, or who holds the key, defeats it. Biorouter's privacy barrier remains
//! documented as safety rather than security; see `docs/security/privacy-tiers.md`.
//! The nonce is kept alongside the peer check because it answers a different
//! question: the peer check says *who*, the nonce says *which prompt*.
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
        eprintln!("usage: biorouter-authprompt <nonce-file> <result-file> <reason> [socket]");
        std::process::exit(64);
    }
    let (nonce_path, result_path, reason) = (&args[1], &args[2], &args[3]);
    // Present since the peer check landed; absent only for an older daemon.
    let socket_path = args.get(4).cloned();

    // A missing or unreadable nonce means the daemon did not set this up, which
    // is not something to authenticate past.
    let Ok(nonce) = std::fs::read_to_string(nonce_path) else {
        std::process::exit(65);
    };
    let nonce = nonce.trim().to_string();
    if nonce.is_empty() {
        std::process::exit(65);
    }

    // ⚠ **Connect BEFORE prompting.** The daemon verifies who we are as soon as
    // the connection lands, so an unauthorised binary is turned away without a
    // dialog ever appearing. Prompting first meant an impostor could still make
    // the user authenticate and only then be refused — the user pays for the
    // attacker's attempt, and a refusal after a successful fingerprint reads as
    // a broken app rather than a working defence.
    // `.ok()` swallows the error deliberately: the daemon being gone or never
    // having listened is the older-daemon case, and the file fallback below
    // handles it.
    let mut stream = socket_path
        .as_deref()
        .and_then(|path| std::os::unix::net::UnixStream::connect(path).ok());

    let verdict = match macos::evaluate(reason) {
        macos::Verdict::Approved => "approved",
        macos::Verdict::Denied => "denied",
        macos::Verdict::Unavailable => "unavailable",
    };

    // The nonce leads, so a truncated write cannot be read as a bare verdict,
    // and the daemon's comparison fails closed on anything it did not issue.
    let line = format!("{nonce} {verdict}\n");

    // ⚠ **The socket is the real channel now.** Connecting is what lets the
    // daemon ask the kernel who we are and Security.framework whether we are
    // signed by its team — a file cannot carry that, which is why a file alone
    // could be written by anything running as the user. Closing the stream is
    // what ends the daemon's `read_to_string`.
    if let Some(mut stream) = stream.take() {
        let _ = stream.write_all(line.as_bytes());
        let _ = stream.flush();
        // ⚠ **Stay alive until the daemon closes.** The daemon identifies us by
        // asking the kernel for the pid on its end of this socket, and a pid
        // only names a process while that process exists. macOS can approve
        // `evaluatePolicy` INSTANTLY from a recent authentication — no dialog at
        // all — so without this the helper writes and exits in milliseconds,
        // the daemon's next poll finds a dead pid, the signature lookup fails,
        // and a perfectly good approval is rejected as an impostor.
        //
        // Reading is how we wait: the daemon drops its end once it has the
        // verdict, which lands here as EOF. Bounded by the daemon's own timeout
        // on the other side, so a daemon that dies cannot pin this process.
        use std::io::Read;
        let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(90)));
        let mut ignored = Vec::new();
        let _ = stream.read_to_end(&mut ignored);
        drop(stream);
        return;
    }

    // Fallback for a daemon older than the socket: write the file it is
    // waiting on. Staged and renamed so it can never read a half-written
    // result and mistake it for an answer.
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

        // ⚠ **Refuse a cached authentication (F-13).** DR-20 point 2 admits no
        // cached grant, and the Linux prompter honours that literally: it turns
        // down `pkexec` because `auth_admin_keep` caches the authorization for
        // minutes, which would let a second declassification through with no
        // prompt at all. macOS reaches the same end by a different road — this
        // repo has observed `evaluatePolicy` returning success *instantly, with
        // no dialog*, off a recent authentication. On the prompt that gates
        // turning enforcement OFF, that is an approval the user did not
        // knowingly give for this operation.
        //
        // Zero is already the documented default for this property, so this
        // call is belt-and-braces: it stops a future macOS (or a stray
        // configuration) from reusing an earlier Touch ID, and it states the
        // requirement in code rather than relying on a default staying put.
        //
        // ⚠ It is **not** proven sufficient. If the instant approval comes from
        // a system-level grace period rather than this context's reuse window,
        // `LAContext` cannot override it and no setting here will. In that case
        // the honest repair is the disclosure text, not this line — do not read
        // the presence of this call as a freshness guarantee.
        //
        // SAFETY: `-setTouchIDAuthenticationAllowableReuseDuration:` takes an
        // NSTimeInterval (a `double`) and returns void.
        let _: () = unsafe {
            msg_send![
                &*context,
                setTouchIDAuthenticationAllowableReuseDuration: 0.0f64
            ]
        };

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

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject, Bool, Sel};

    /// ⚠ **`msg_send!` is dynamic dispatch, so a misspelled selector compiles
    /// cleanly and then kills the process at runtime** — "unrecognized selector
    /// sent to instance" — inside the one code path whose whole job is to be
    /// trustworthy. A crash there is not a soft failure: the helper dies without
    /// writing a verdict, and the operation is refused for a reason that has
    /// nothing to do with the user's answer.
    ///
    /// This asserts the freshness setter (F-13) is a real LAContext method
    /// rather than a plausible-looking string. It does **not** prompt.
    #[test]
    fn the_reuse_duration_setter_is_a_real_selector() {
        let Some(class) = AnyClass::get(c"LAContext") else {
            // No LocalAuthentication on this host: nothing to assert against,
            // and failing here would be a false alarm about the code.
            return;
        };
        // SAFETY: `+[LAContext new]` takes no arguments and returns a +1 instance.
        let context: Retained<AnyObject> = unsafe { msg_send![class, new] };
        let sel = Sel::register(c"setTouchIDAuthenticationAllowableReuseDuration:");
        // SAFETY: `-respondsToSelector:` takes a SEL and returns BOOL.
        let responds: Bool = unsafe { msg_send![&*context, respondsToSelector: sel] };
        assert!(
            responds.as_bool(),
            "LAContext does not respond to setTouchIDAuthenticationAllowableReuseDuration: — \
             the F-13 hardening in evaluate() would crash the helper at runtime"
        );
    }
}
