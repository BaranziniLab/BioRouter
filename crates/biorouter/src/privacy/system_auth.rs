//! The system-authentication seam (issue #56, DR-20) and DR-24's three
//! prompters behind it.
//!
//! DR-20 puts an **operating system** authentication — the class of the Keychain
//! authorization macOS raises at app start — in front of any declassification.
//! The password is typed into the OS, verified by the OS, and never seen by
//! BioRouter; this module returns a decision and nothing else. DR-24 then rules
//! that all three shipped platforms get a real prompt: macOS
//! `LAContext.evaluatePolicy(.deviceOwnerAuthentication)`, Windows
//! `UserConsentVerifier`, Linux polkit.
//!
//! ⚠ **Who calls [`prompter`], and what that does NOT include.** Task 44 built
//! this module with no consumer at all; Task 55 wired it to the two operations
//! DR-20 names. There are exactly two callers today, and both raise the prompt
//! as the **last** thing before their write, so a user is never asked for a
//! password to be told afterwards that the request was malformed:
//!
//! * [`crate::privacy::declassify::authenticate_declassification`] — the chats
//!   §12.4 grades onto the typed phrase. The phrase **stays**: it proves *which*
//!   chat, which a password cannot, and two proofs answering two different
//!   questions is not redundancy. A `turn:*` chat keeps its single click and is
//!   never prompted, because making the common case expensive is how you teach
//!   people to stop privatising at all.
//! * `/config/upsert`'s master-switch arm, when the write turns the tier system
//!   **off**. Turning it back **on** is not prompted: that is the safe
//!   direction, and an [`AuthOutcome::Unavailable`] there would strand a machine
//!   with protection disabled.
//!
//! ⚠ **Nothing here names the §12.4 proof-of-user, and the omission is enforced
//! rather than stylistic.** `the_proof_of_user_is_constructed_in_exactly_two_places`
//! in `declassify.rs` asserts by *exact equality* that the only files in the
//! workspace so much as mentioning it are the CLI command and the HTTP route —
//! comments included. A prompter that reached for it, or a doc comment that
//! merely spelled it, turns that audit red. The wiring above is deliberately
//! shaped so it never has to: `declassify.rs` owns both the authentication and
//! the proof, and this module knows about neither.
//!
//! # Why the decision logic is split from the platform call
//!
//! Each platform's prompter is two halves: an *acquisition* half that talks to
//! the OS, and a *decision* half that turns the OS's raw answer into an
//! [`AuthOutcome`]. The decision half is the security-relevant one — it is where
//! "approve" is spoken — and it is the half that would otherwise be untestable
//! anywhere but on that platform. So all three decision functions
//! ([`outcome_for_macos`], [`outcome_for_windows`], [`outcome_for_polkit`]) live
//! here, are compiled on **every** target, and are unit-tested on whatever host
//! runs `cargo test`. Only the acquisition halves are `cfg(target_os)`-gated.
//!
//! Every one of them fails **closed**: an unrecognised code is a refusal, never
//! an approval. DR-24 states the asymmetry outright — a build whose
//! declassification refuses is a missing feature, and one whose declassification
//! silently approves is the worst outcome this feature can produce.

use anyhow::{bail, Result};

/// What the user is about to be asked, built **before** the prompt and rendered
/// **inside** it.
///
/// DR-20 point 4 requires the id set to be fixed before the prompt and stated in
/// it: one authentication may cover several chats, but only the ones it named. A
/// prompt that says *"BioRouter wants to make changes"* satisfies the letter of
/// the ruling and defeats its purpose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthRequest {
    /// The sentence the OS shows the user. macOS renders it under "BioRouter is
    /// trying to …", Windows as the `UserConsentVerifier` message, polkit as the
    /// action's message.
    pub reason: String,
    /// Sorted, deduplicated and non-empty — enforced by [`AuthRequest::new`], so
    /// the set a proof is later spendable on has one canonical form.
    pub session_ids: Vec<String>,
}

impl AuthRequest {
    /// Sorts, deduplicates, and refuses an empty set.
    ///
    /// An empty set is rejected rather than accepted-and-ignored: a prompt that
    /// names no chats is a prompt the user cannot audit, and a proof minted from
    /// one would be spendable on nothing — which reads at the call site as a
    /// successful authentication that does nothing.
    pub fn new(reason: impl Into<String>, session_ids: &[String]) -> Result<Self> {
        let mut ids: Vec<String> = session_ids.to_vec();
        ids.sort();
        ids.dedup();
        if ids.is_empty() {
            bail!("a system authentication must name at least one session");
        }
        Ok(Self {
            reason: reason.into(),
            session_ids: ids,
        })
    }

    /// A prompt about something that is **not** a set of chats.
    ///
    /// The master switch is the only one today: turning the whole tier system
    /// off has no rows to name, and DR-20 point 4 still requires the dialog to
    /// say what it authorises rather than *"BioRouter wants to make changes"*.
    /// So `subject` takes the id slot and is what each platform renders where
    /// the ids would go.
    ///
    /// ⚠ **`subject` is not an id and is never matched against one.** The master
    /// switch's caller branches on the [`AuthOutcome`] inside the same function
    /// that raises the prompt, so no proof is minted from this request and there
    /// is nothing for a stray session id to be compared with. Infallible for the
    /// same reason [`AuthRequest::new`] is not: there is exactly one subject, and
    /// it is a constant in the caller's source.
    pub fn about(reason: impl Into<String>, subject: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            session_ids: vec![subject.into()],
        }
    }
}

/// The three answers a prompter can give. There is no fourth, and in particular
/// there is no "assume yes".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthOutcome {
    /// The operating system verified the user.
    Approved,
    /// The user said no, dismissed the prompt, or failed the check. Also the
    /// resting place for every code this module does not recognise.
    Denied,
    /// No prompt could be raised here at all — no prompter on this platform, no
    /// interactive session, or the platform's verifier is not configured. DR-24
    /// Step 2: this refuses, **naming the platform**, so it reads as a missing
    /// capability rather than as a bug.
    Unavailable,
}

/// One prompter per platform, plus the test seam.
///
/// ⚠ **Nothing in here ever receives, stores or logs a password** (DR-20 point
/// 3). The OS asks, the OS verifies, and an implementation returns one of three
/// values. A prompter that took a password as an argument would put it inside
/// this process, which is the exact property the ruling buys by staying out of
/// the OS's way.
#[async_trait::async_trait]
pub trait SystemAuthenticator: Send + Sync {
    /// Raise the prompt and wait for the user. Called once per operation — DR-20
    /// point 2 admits no cached grant, so an implementation must never answer
    /// from a previous prompt's result.
    async fn authenticate(&self, req: &AuthRequest) -> AuthOutcome;

    /// How this prompter names itself in a refusal, e.g. `"macOS
    /// (LocalAuthentication)"`. DR-24 Step 2 requires an [`AuthOutcome::Unavailable`]
    /// to name the platform.
    fn platform(&self) -> &'static str;

    /// The sentence a caller shows for [`AuthOutcome::Unavailable`].
    ///
    /// The default names the platform and says the operation was refused rather
    /// than half-done; a prompter with something more specific to say (polkit's
    /// missing action file, say) overrides it.
    fn unavailable_reason(&self) -> String {
        format!(
            "Biorouter could not raise a system authentication prompt on {}, so this \
             operation was refused and nothing was changed. Run it from a Biorouter \
             session on a machine whose operating system can raise one.",
            self.platform()
        )
    }
}

/// Why a prompt did not approve, and the sentence that says so.
///
/// **One type for both callers** — declassification and the master switch — so
/// the two cannot drift into describing the same refusal differently. It carries
/// the [`AuthOutcome`] as well as the message because the surfaces answer the
/// two differently: the CLI reports a [`AuthOutcome::Denied`] as "you did not
/// confirm, nothing changed" and an [`AuthOutcome::Unavailable`] as an error
/// with the platform's advice, which is the difference between the user's answer
/// and the machine's incapability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemAuthRefusal {
    /// Never [`AuthOutcome::Approved`]: [`refusal_for`] is the only constructor
    /// in the tree, and its `Approved` arm returns `None` rather than a refusal.
    pub outcome: AuthOutcome,
    /// What to show. For [`AuthOutcome::Unavailable`] this is the prompter's own
    /// [`SystemAuthenticator::unavailable_reason`], so it names the platform and
    /// says what would have to be true for the prompt to exist.
    pub message: String,
}

impl std::fmt::Display for SystemAuthRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

/// What a user who cancelled, dismissed or failed the prompt is told.
///
/// It states the *consequence* rather than the cause, because a prompter cannot
/// tell the three apart and guessing would be worse than saying less. It closes
/// with what did not happen: a refusal a user reads as "it probably went through
/// anyway" is the failure this whole surface exists to prevent.
pub const DENIED_MESSAGE: &str =
    "The system authentication was not completed, so nothing was changed.";

/// Turn a prompter's answer into either *go ahead* (`None`) or the refusal.
///
/// The **decision half** of every caller, split from the platform call for the
/// same reason [`outcome_for_macos`] and its siblings are: it is where "approve"
/// is spoken, so it is compiled everywhere and unit-tested on whatever host runs
/// `cargo test`. `Approved` is the only arm that returns `None`, which is what
/// makes a caller's `if let Some(refusal) = …` fail closed for every future
/// variant this enum could grow.
pub fn refusal_for(
    outcome: AuthOutcome,
    prompter: &dyn SystemAuthenticator,
) -> Option<SystemAuthRefusal> {
    match outcome {
        AuthOutcome::Approved => None,
        AuthOutcome::Denied => Some(SystemAuthRefusal {
            outcome,
            message: DENIED_MESSAGE.to_string(),
        }),
        AuthOutcome::Unavailable => Some(SystemAuthRefusal {
            outcome,
            message: prompter.unavailable_reason(),
        }),
    }
}

/// How the test seam names itself.
///
/// Declared here, outside the seam's own `cfg`, so a test can assert the seam is
/// **not** selected without itself carrying the cfg pair — which is the whole
/// point of the audit below.
pub const TEST_SEAM_PLATFORM: &str = "test seam (privacy-test-auth)";

// ---------------------------------------------------------------------------
// macOS — LAContext.evaluatePolicy(.deviceOwnerAuthentication)
// ---------------------------------------------------------------------------

/// `LAError` codes this module distinguishes. Values from
/// `LocalAuthentication/LAError.h`; every other code lands on
/// [`AuthOutcome::Denied`] by the catch-all, so the list needs to be correct,
/// not exhaustive.
pub const LA_ERROR_AUTHENTICATION_FAILED: i64 = -1;
/// The user pressed Cancel.
pub const LA_ERROR_USER_CANCEL: i64 = -2;
/// The user chose "Enter Password" — with `.deviceOwnerAuthentication` the
/// password *is* the fallback, so reaching this means they backed out of it.
pub const LA_ERROR_USER_FALLBACK: i64 = -3;
/// The system cancelled the prompt (screen lock, app backgrounded).
pub const LA_ERROR_SYSTEM_CANCEL: i64 = -4;
/// No passcode is set on the device, so the OS has nothing to verify against.
pub const LA_ERROR_PASSCODE_NOT_SET: i64 = -5;
/// There is no interactive session to draw a prompt in — a daemon, an ssh
/// shell, a headless build agent.
pub const LA_ERROR_NOT_INTERACTIVE: i64 = -1004;

/// What macOS's LocalAuthentication actually returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacAuthSignal {
    /// `evaluatePolicy` called back with `success == YES`.
    Evaluated,
    /// `evaluatePolicy` called back with `success == NO` and this `LAError` code.
    EvaluationFailed(i64),
    /// `canEvaluatePolicy` refused the policy outright — checked before
    /// prompting so that "this Mac cannot do it" is distinguishable from "the
    /// user said no".
    PolicyUnavailable(i64),
    /// The `LAContext` class is not registered, i.e. LocalAuthentication.framework
    /// did not load.
    FrameworkMissing,
}

/// macOS's raw answer → an [`AuthOutcome`].
///
/// The split between `Denied` and `Unavailable` is a claim about *why* the
/// refusal happened, and it changes the advice the user is given: `Unavailable`
/// tells them to go and use a different machine. So only the codes that really
/// are "this Mac cannot raise the prompt" map there — an unrecognised code maps
/// to `Denied`, which says no more than "it did not succeed" and is true
/// whatever the code meant.
pub fn outcome_for_macos(signal: MacAuthSignal) -> AuthOutcome {
    match signal {
        MacAuthSignal::Evaluated => AuthOutcome::Approved,
        MacAuthSignal::FrameworkMissing => AuthOutcome::Unavailable,
        MacAuthSignal::PolicyUnavailable(_) => AuthOutcome::Unavailable,
        MacAuthSignal::EvaluationFailed(LA_ERROR_PASSCODE_NOT_SET) => AuthOutcome::Unavailable,
        MacAuthSignal::EvaluationFailed(LA_ERROR_NOT_INTERACTIVE) => AuthOutcome::Unavailable,
        MacAuthSignal::EvaluationFailed(_) => AuthOutcome::Denied,
    }
}

// ---------------------------------------------------------------------------
// Windows — Windows.Security.Credentials.UI.UserConsentVerifier
// ---------------------------------------------------------------------------

/// `UserConsentVerificationResult` values, from the WinRT enum.
pub const CONSENT_VERIFIED: i32 = 0;
/// No Windows Hello device is present.
pub const CONSENT_DEVICE_NOT_PRESENT: i32 = 1;
/// Windows Hello is present but this user has not enrolled.
pub const CONSENT_NOT_CONFIGURED_FOR_USER: i32 = 2;
/// Group policy has turned Windows Hello off.
pub const CONSENT_DISABLED_BY_POLICY: i32 = 3;
/// The device is busy.
pub const CONSENT_DEVICE_BUSY: i32 = 4;
/// The user ran out of retries.
pub const CONSENT_RETRIES_EXHAUSTED: i32 = 5;
/// The user dismissed the prompt.
pub const CONSENT_CANCELED: i32 = 6;

/// What Windows's `UserConsentVerifier` actually returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowsConsentSignal {
    /// A `UserConsentVerificationResult` from `RequestVerificationAsync`.
    VerificationResult(i32),
    /// `CheckAvailabilityAsync` returned something other than `Available`; the
    /// payload is the `UserConsentVerifierAvailability` value.
    NotAvailable(i32),
    /// The WinRT call itself failed; the payload is the `HRESULT`. On Windows a
    /// process that cannot host the consent UI fails here, which really is "no
    /// prompter in this process".
    CallFailed(i32),
}

/// Windows's raw answer → an [`AuthOutcome`].
pub fn outcome_for_windows(signal: WindowsConsentSignal) -> AuthOutcome {
    match signal {
        WindowsConsentSignal::VerificationResult(CONSENT_VERIFIED) => AuthOutcome::Approved,
        WindowsConsentSignal::VerificationResult(CONSENT_DEVICE_NOT_PRESENT)
        | WindowsConsentSignal::VerificationResult(CONSENT_NOT_CONFIGURED_FOR_USER)
        | WindowsConsentSignal::VerificationResult(CONSENT_DISABLED_BY_POLICY) => {
            AuthOutcome::Unavailable
        }
        // Busy, retries exhausted, cancelled, and anything unrecognised: the
        // prompt exists and did not approve. Refuse, but do not tell the user
        // their machine cannot do this — it can, and retrying may work.
        WindowsConsentSignal::VerificationResult(_) => AuthOutcome::Denied,
        WindowsConsentSignal::NotAvailable(_) => AuthOutcome::Unavailable,
        WindowsConsentSignal::CallFailed(_) => AuthOutcome::Unavailable,
    }
}

// ---------------------------------------------------------------------------
// Linux — polkit
// ---------------------------------------------------------------------------

/// The polkit action id BioRouter authenticates against.
///
/// A **dedicated** action rather than polkit's built-in
/// `org.freedesktop.policykit.exec` (which is what `pkexec` uses), for one
/// decisive reason: the built-in ships as `auth_admin_keep`, and `_keep` caches
/// the authorization for several minutes. DR-20 point 2 admits no cached grant,
/// so borrowing that action would make the second declassification in a
/// five-minute window silent. This action is declared `auth_self` — the user's
/// own password, every time.
pub const POLKIT_ACTION_ID: &str = "edu.ucsf.biorouter.declassify";

/// The polkit action file that must be installed at
/// `/usr/share/polkit-1/actions/edu.ucsf.biorouter.declassify.policy` for the
/// Linux prompter to work.
///
/// ⚠ **Not yet installed by any packaging path.** Until the deb/rpm makers ship
/// it, `pkcheck` reports the action as unknown and the Linux prompter returns
/// [`AuthOutcome::Unavailable`], which is DR-24's stated honest posture rather
/// than a silent approval. It is kept here, beside the id it declares, so the
/// two cannot drift; a test below asserts they agree.
pub const POLKIT_ACTION_POLICY: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE policyconfig PUBLIC
 "-//freedesktop//DTD PolicyKit Policy Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/PolicyKit/1/policyconfig.dtd">
<policyconfig>
  <vendor>Baranzini Lab, UCSF</vendor>
  <vendor_url>https://biorouter.ucsf.edu</vendor_url>
  <action id="edu.ucsf.biorouter.declassify">
    <description>Make private Biorouter chats public</description>
    <message>Authentication is required to make these private Biorouter chats public: $(chats)</message>
    <defaults>
      <allow_any>auth_self</allow_any>
      <allow_inactive>auth_self</allow_inactive>
      <allow_active>auth_self</allow_active>
    </defaults>
  </action>
</policyconfig>
"#;

/// What polkit's `pkcheck` actually returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolkitSignal {
    /// `pkcheck` exited with this status code.
    Exited(i32),
    /// `pkcheck` was killed by a signal.
    Signalled,
    /// `pkcheck` is not on `PATH` — polkit is not installed, which is the normal
    /// state of the headless hosts this product already ships CLI-only packages
    /// for.
    NotInstalled,
    /// polkit is installed but nothing can draw the prompt: no graphical session
    /// and no controlling terminal for the text agent.
    NoAuthenticationAgent,
}

/// polkit's raw answer → an [`AuthOutcome`].
///
/// **Only exit 0 approves.** `pkcheck`'s non-zero codes vary across polkit
/// versions and distributions, so the mapping is written so that every code it
/// does not know about refuses; getting a code wrong can cost a user a retry and
/// can never cost them a disclosure.
pub fn outcome_for_polkit(signal: PolkitSignal) -> AuthOutcome {
    match signal {
        PolkitSignal::Exited(0) => AuthOutcome::Approved,
        // 2 is pkcheck's "an error occurred", which is what an unregistered
        // action id looks like — i.e. the action file below was never installed.
        // 127 is the shell/exec failure. Both are "no prompt happened here".
        PolkitSignal::Exited(2) | PolkitSignal::Exited(127) => AuthOutcome::Unavailable,
        PolkitSignal::Exited(_) | PolkitSignal::Signalled => AuthOutcome::Denied,
        PolkitSignal::NotInstalled | PolkitSignal::NoAuthenticationAgent => {
            AuthOutcome::Unavailable
        }
    }
}

// ---------------------------------------------------------------------------
// Resolution
// ---------------------------------------------------------------------------

/// The prompter this build uses — **the only place the test seam is reachable
/// from**.
///
/// The seam arm and the platform arm are mutually exclusive `cfg`s rather than
/// an early `return`, so there is no build in which both are compiled and the
/// ordering between them decides which wins.
pub fn prompter() -> &'static dyn SystemAuthenticator {
    #[cfg(all(debug_assertions, feature = "privacy-test-auth"))]
    {
        crate::privacy::system_auth_seam::prompter()
    }
    #[cfg(not(all(debug_assertions, feature = "privacy-test-auth")))]
    {
        platform_prompter()
    }
}

/// The real prompter for this target, never the seam.
///
/// Kept separate from [`prompter`] and always compiled so that a build which
/// *has* the seam can still assert what it would have used without it — the seam
/// must not be able to hide a platform arm that no longer compiles.
pub fn platform_prompter() -> &'static dyn SystemAuthenticator {
    #[cfg(target_os = "macos")]
    {
        crate::privacy::system_auth_macos::prompter()
    }
    #[cfg(target_os = "windows")]
    {
        crate::privacy::system_auth_windows::prompter()
    }
    #[cfg(target_os = "linux")]
    {
        crate::privacy::system_auth_polkit::prompter()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        &NoPrompter
    }
}

/// The fail-closed prompter for a target DR-24 does not name.
///
/// It exists so that adding a platform to the build is a *refusal* until someone
/// writes its prompter, rather than a compile error someone might be tempted to
/// silence with an approval.
pub struct NoPrompter;

#[async_trait::async_trait]
impl SystemAuthenticator for NoPrompter {
    async fn authenticate(&self, _req: &AuthRequest) -> AuthOutcome {
        AuthOutcome::Unavailable
    }

    fn platform(&self) -> &'static str {
        std::env::consts::OS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- DR-24 Step 4: per platform, an approve, a deny and an unavailable ---

    #[test]
    fn macos_signals_map_to_approve_deny_and_unavailable() {
        assert_eq!(
            outcome_for_macos(MacAuthSignal::Evaluated),
            AuthOutcome::Approved
        );
        assert_eq!(
            outcome_for_macos(MacAuthSignal::EvaluationFailed(LA_ERROR_USER_CANCEL)),
            AuthOutcome::Denied
        );
        assert_eq!(
            outcome_for_macos(MacAuthSignal::FrameworkMissing),
            AuthOutcome::Unavailable
        );
        // A Mac with no passcode has nothing to verify against, and a daemon has
        // nowhere to draw the prompt. Both are "this machine cannot", not "the
        // user said no", and the message the user gets differs accordingly.
        assert_eq!(
            outcome_for_macos(MacAuthSignal::EvaluationFailed(LA_ERROR_PASSCODE_NOT_SET)),
            AuthOutcome::Unavailable
        );
        assert_eq!(
            outcome_for_macos(MacAuthSignal::EvaluationFailed(LA_ERROR_NOT_INTERACTIVE)),
            AuthOutcome::Unavailable
        );
        assert_eq!(
            outcome_for_macos(MacAuthSignal::PolicyUnavailable(LA_ERROR_PASSCODE_NOT_SET)),
            AuthOutcome::Unavailable
        );
    }

    #[test]
    fn windows_signals_map_to_approve_deny_and_unavailable() {
        assert_eq!(
            outcome_for_windows(WindowsConsentSignal::VerificationResult(CONSENT_VERIFIED)),
            AuthOutcome::Approved
        );
        assert_eq!(
            outcome_for_windows(WindowsConsentSignal::VerificationResult(CONSENT_CANCELED)),
            AuthOutcome::Denied
        );
        assert_eq!(
            outcome_for_windows(WindowsConsentSignal::NotAvailable(
                CONSENT_DEVICE_NOT_PRESENT
            )),
            AuthOutcome::Unavailable
        );
        // Enrolment and policy are capability facts about the machine…
        assert_eq!(
            outcome_for_windows(WindowsConsentSignal::VerificationResult(
                CONSENT_NOT_CONFIGURED_FOR_USER
            )),
            AuthOutcome::Unavailable
        );
        assert_eq!(
            outcome_for_windows(WindowsConsentSignal::VerificationResult(
                CONSENT_DISABLED_BY_POLICY
            )),
            AuthOutcome::Unavailable
        );
        // …while busy and retries-exhausted are transient, and telling the user
        // to go and find another machine would be wrong advice.
        assert_eq!(
            outcome_for_windows(WindowsConsentSignal::VerificationResult(
                CONSENT_DEVICE_BUSY
            )),
            AuthOutcome::Denied
        );
        assert_eq!(
            outcome_for_windows(WindowsConsentSignal::VerificationResult(
                CONSENT_RETRIES_EXHAUSTED
            )),
            AuthOutcome::Denied
        );
    }

    #[test]
    fn polkit_signals_map_to_approve_deny_and_unavailable() {
        assert_eq!(
            outcome_for_polkit(PolkitSignal::Exited(0)),
            AuthOutcome::Approved
        );
        assert_eq!(
            outcome_for_polkit(PolkitSignal::Exited(1)),
            AuthOutcome::Denied
        );
        assert_eq!(
            outcome_for_polkit(PolkitSignal::NotInstalled),
            AuthOutcome::Unavailable
        );
        // An unregistered action id — the state of every Linux install until the
        // packaging ships the action file — must read as "no prompter here",
        // not as "the user refused".
        assert_eq!(
            outcome_for_polkit(PolkitSignal::Exited(2)),
            AuthOutcome::Unavailable
        );
        assert_eq!(
            outcome_for_polkit(PolkitSignal::NoAuthenticationAgent),
            AuthOutcome::Unavailable
        );
    }

    /// The property that actually matters, swept rather than sampled: across the
    /// whole plausible code space of all three platforms, exactly one value per
    /// platform approves.
    #[test]
    fn exactly_one_signal_per_platform_approves() {
        let mut mac_approvals = 0;
        for code in -2000i64..64 {
            for signal in [
                MacAuthSignal::EvaluationFailed(code),
                MacAuthSignal::PolicyUnavailable(code),
            ] {
                if outcome_for_macos(signal) == AuthOutcome::Approved {
                    mac_approvals += 1;
                }
            }
        }
        assert_eq!(
            mac_approvals, 0,
            "no LAError code may approve; only a successful evaluation does"
        );
        assert_eq!(
            outcome_for_macos(MacAuthSignal::Evaluated),
            AuthOutcome::Approved
        );

        let windows_approvals = (-64i32..1024)
            .flat_map(|code| {
                [
                    WindowsConsentSignal::VerificationResult(code),
                    WindowsConsentSignal::NotAvailable(code),
                    WindowsConsentSignal::CallFailed(code),
                ]
            })
            .filter(|s| outcome_for_windows(*s) == AuthOutcome::Approved)
            .count();
        assert_eq!(
            windows_approvals, 1,
            "only UserConsentVerificationResult::Verified may approve on Windows"
        );

        let polkit_approvals = (-64i32..1024)
            .map(PolkitSignal::Exited)
            .chain([
                PolkitSignal::Signalled,
                PolkitSignal::NotInstalled,
                PolkitSignal::NoAuthenticationAgent,
            ])
            .filter(|s| outcome_for_polkit(*s) == AuthOutcome::Approved)
            .count();
        assert_eq!(
            polkit_approvals, 1,
            "only a pkcheck exit status of 0 may approve on Linux"
        );
    }

    // --- DR-24 Step 2: unavailable names the platform ---

    #[test]
    fn an_unavailable_refusal_names_the_platform() {
        let p = platform_prompter();
        assert!(
            p.unavailable_reason().contains(p.platform()),
            "the refusal for AuthOutcome::Unavailable must name the platform; got {:?}",
            p.unavailable_reason()
        );
        assert!(
            NoPrompter
                .unavailable_reason()
                .contains(NoPrompter.platform()),
            "the fallback prompter's refusal must name the platform too"
        );
    }

    #[test]
    fn the_polkit_action_file_declares_the_id_the_prompter_asks_for() {
        assert!(
            POLKIT_ACTION_POLICY.contains(&format!("id=\"{POLKIT_ACTION_ID}\"")),
            "the shipped polkit action file must declare the action id pkcheck is called with"
        );
        // `auth_admin_keep` would cache the grant for several minutes, which is
        // exactly what DR-20 point 2 forbids.
        assert!(
            !POLKIT_ACTION_POLICY.contains("_keep"),
            "a polkit action that caches its authorization defeats DR-20's per-operation rule"
        );
    }

    // --- Request shape ---

    #[test]
    fn an_auth_request_is_sorted_deduplicated_and_non_empty() {
        let req = AuthRequest::new("why", &["b".into(), "a".into(), "b".into()]).unwrap();
        assert_eq!(req.session_ids, vec!["a".to_string(), "b".to_string()]);
        assert!(AuthRequest::new("why", &[]).is_err());
    }

    #[test]
    fn a_request_about_a_subject_still_names_what_it_authorises() {
        // DR-20 point 4 holds for the master switch too: the dialog has to say
        // what it is authorising, and "the whole install" is a thing to say.
        let req = AuthRequest::about("Turn the tiers off.", "every private chat");
        assert_eq!(req.session_ids, vec!["every private chat".to_string()]);
        assert_eq!(req.reason, "Turn the tiers off.");
    }

    // --- Task 55: the decision every caller shares ---

    /// The property that makes `if let Some(refusal) = refusal_for(..)` safe at
    /// every call site: exactly one outcome is permission to proceed.
    #[test]
    fn only_an_approval_is_permission_to_proceed() {
        assert_eq!(refusal_for(AuthOutcome::Approved, &NoPrompter), None);
        for outcome in [AuthOutcome::Denied, AuthOutcome::Unavailable] {
            let refusal = refusal_for(outcome, &NoPrompter)
                .unwrap_or_else(|| panic!("{outcome:?} must be a refusal"));
            assert_eq!(refusal.outcome, outcome);
            assert!(!refusal.message.is_empty());
        }
    }

    /// The two refusals say different things, and the one that means "this
    /// machine cannot" carries the platform's own advice — telling a user to go
    /// and find another machine when they simply pressed Cancel is wrong advice,
    /// and so is the reverse.
    #[test]
    fn a_denial_and_an_unavailable_are_not_interchangeable() {
        let denied = refusal_for(AuthOutcome::Denied, &NoPrompter).unwrap();
        let unavailable = refusal_for(AuthOutcome::Unavailable, &NoPrompter).unwrap();
        assert_ne!(denied.message, unavailable.message);
        assert_eq!(denied.message, DENIED_MESSAGE);
        assert_eq!(unavailable.message, NoPrompter.unavailable_reason());
        assert!(unavailable.message.contains(NoPrompter.platform()));
        // `Display` is what the CLI and the routes print; it must be the message
        // and not a derived-`Debug` dump of the enum beside it.
        assert_eq!(denied.to_string(), DENIED_MESSAGE);
    }

    // --- Resolution ---

    #[test]
    fn the_resolver_reaches_the_seam_only_under_the_test_feature() {
        let selected = prompter().platform();
        #[cfg(all(debug_assertions, feature = "privacy-test-auth"))]
        assert_eq!(
            selected, TEST_SEAM_PLATFORM,
            "the seam is compiled in, so it must be what the resolver hands back"
        );
        #[cfg(not(all(debug_assertions, feature = "privacy-test-auth")))]
        {
            assert_ne!(
                selected, TEST_SEAM_PLATFORM,
                "the seam is not compiled in and must not be reachable"
            );
            assert_eq!(selected, platform_prompter().platform());
        }
    }

    #[test]
    fn this_host_resolves_to_its_own_platform_prompter() {
        let named = platform_prompter().platform();
        if cfg!(target_os = "macos") {
            assert!(named.contains("macOS"), "got {named}");
        } else if cfg!(target_os = "windows") {
            assert!(named.contains("Windows"), "got {named}");
        } else if cfg!(target_os = "linux") {
            assert!(named.contains("Linux"), "got {named}");
        } else {
            assert_eq!(named, std::env::consts::OS);
        }
    }

    // --- DR-24 Step 3: the bypass must be compiled out of shipped builds ---

    /// The test DR-24 Step 4 singles out: *"the one that fails the plausible
    /// wrong implementation, since every functional test passes just as well
    /// with the bypass still reachable."*
    ///
    /// It asserts the four independent things that together make the seam
    /// unreachable from a shipped binary, and it runs under a **bare** `cargo
    /// test` — i.e. in exactly the build where the seam is absent and every
    /// behavioural test of it is filtered out.
    #[test]
    fn the_test_seam_cannot_be_compiled_into_a_shipped_profile() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();

        // (1) The cfg pair that makes the seam exist appears in exactly two
        // files. Anything else carrying it is a second door into the bypass.
        let needle = "debug_assertions, feature = \"privacy-test-auth\"";
        let mut carrying: Vec<String> = vec![];
        let mut scanned = 0usize;
        for entry in walkdir::WalkDir::new(root.join("crates")) {
            let entry = entry.expect("the audit must not silently skip an unreadable directory");
            let p = entry.path();
            if p.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            scanned += 1;
            let src = std::fs::read_to_string(p).expect("the audit could not read a source file");
            if src.contains(needle) {
                carrying.push(
                    p.strip_prefix(&root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
        assert!(scanned >= 400, "only {scanned} .rs files were scanned");
        carrying.sort();
        assert_eq!(
            carrying,
            vec![
                // The resolver arm, and this audit's own needle.
                "crates/biorouter/src/privacy/system_auth.rs".to_string(),
                // The seam module's file-level inner attribute.
                "crates/biorouter/src/privacy/system_auth_seam.rs".to_string(),
            ],
            "a third file gates something on the test-seam cfg pair. The claim that a release \
             binary has no path to the bypass rests on this set staying closed."
        );

        // (2) The compiler refuses any build that turns the feature on with
        // debug assertions off — which is every profile this workspace ships.
        let lib_rs =
            std::fs::read_to_string(root.join("crates/biorouter/src/lib.rs")).expect("lib.rs");
        assert!(
            lib_rs.contains("#[cfg(all(feature = \"privacy-test-auth\", not(debug_assertions)))]")
                && lib_rs.contains("compile_error!"),
            "lib.rs must carry the unconditional compile_error! guard from DR-20 requirement (2)"
        );

        // (3) No shipped profile turns debug assertions back on, and the two
        // derived profiles inherit from release rather than redefining it.
        let workspace = std::fs::read_to_string(root.join("Cargo.toml")).expect("root Cargo.toml");
        assert!(
            !workspace.contains("debug-assertions = true")
                && !workspace.contains("debug_assertions = true"),
            "a shipped profile re-enables debug assertions, which would let the feature compile"
        );
        for derived in ["release-dist", "quick"] {
            let section = workspace
                .split(&format!("[profile.{derived}]"))
                .nth(1)
                .unwrap_or_else(|| panic!("no [profile.{derived}] in the workspace manifest"));
            assert!(
                section
                    .split("\n[")
                    .next()
                    .unwrap()
                    .contains("inherits = \"release\""),
                "[profile.{derived}] must inherit release, or it could differ on debug assertions"
            );
        }

        // (4) The feature is never enabled by a build dependency. A dev-dependency
        // may arm it (that is how a test build reaches the seam under
        // resolver = "2"); a normal dependency doing so would put the seam in
        // `cargo build`.
        for entry in walkdir::WalkDir::new(root.join("crates")).max_depth(2) {
            let entry = entry.expect("the audit must not silently skip a manifest");
            if entry.file_name() != "Cargo.toml" {
                continue;
            }
            let manifest = std::fs::read_to_string(entry.path()).expect("a crate manifest");
            let mut section = String::new();
            for line in manifest.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with('[') {
                    section = trimmed.to_string();
                    continue;
                }
                if !trimmed.contains("privacy-test-auth") {
                    continue;
                }
                assert!(
                    section.contains("features") || section.contains("dev-dependencies"),
                    "{} enables privacy-test-auth from {section}, which would compile the seam \
                     into a plain `cargo build`: {trimmed}",
                    entry.path().display()
                );
                if section == "[features]" {
                    assert!(
                        !trimmed.starts_with("default"),
                        "{} makes privacy-test-auth a DEFAULT feature: {trimmed}",
                        entry.path().display()
                    );
                }
            }
        }
    }
}
