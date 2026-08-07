//! The Windows prompter (DR-24):
//! `Windows.Security.Credentials.UI.UserConsentVerifier`, i.e. Windows Hello.
//!
//! Hello covers PIN, biometrics **and** the account password, so it satisfies
//! DR-20's requirement that the user be able to fall back to typing a password.
//!
//! ⚠ **Not `CredUIPromptForWindowsCredentials` + `LogonUser`.** That pair is the
//! other obvious route and DR-20 point 3 forbids it outright: it hands the
//! password to *this process* to verify, which is the exact property the ruling
//! buys by staying out of the OS's way.
//!
//! ⚠ **This file has never been compiled.** This campaign has no Windows host
//! and no `x86_64-pc-windows-gnu` std, and the workspace's Windows artifact is
//! cross-compiled in Docker at release time. The WinRT surface it uses was
//! verified against the vendored `windows` 0.61.3 sources rather than by a
//! build — `UserConsentVerifier::{CheckAvailabilityAsync, RequestVerificationAsync}`,
//! `IAsyncOperation::get`, and the numeric values of both result enums. The
//! decision half, where "approve" is actually spoken, is
//! [`super::system_auth::outcome_for_windows`], which is compiled and tested on
//! every host including this one.

#![cfg(target_os = "windows")]

use super::system_auth::{
    outcome_for_windows, AuthOutcome, AuthRequest, SystemAuthenticator, WindowsConsentSignal,
};
use windows::core::HSTRING;
use windows::Security::Credentials::UI::{UserConsentVerifier, UserConsentVerifierAvailability};

pub struct WindowsPrompter;

pub fn prompter() -> &'static dyn SystemAuthenticator {
    &WindowsPrompter
}

#[async_trait::async_trait]
impl SystemAuthenticator for WindowsPrompter {
    async fn authenticate(&self, req: &AuthRequest) -> AuthOutcome {
        let message = prompt_text(req);
        // `IAsyncOperation::get` blocks until the user answers, so it goes on
        // the blocking pool rather than parking a tokio worker.
        let signal = tokio::task::spawn_blocking(move || request_verification(&message))
            .await
            // A panic or a shutdown in the blocking pool is not an approval.
            .unwrap_or(WindowsConsentSignal::CallFailed(0));
        outcome_for_windows(signal)
    }

    fn platform(&self) -> &'static str {
        "Windows (Windows Hello)"
    }
}

/// What the consent dialog shows.
///
/// DR-20 point 4: the prompt states the set it authorises, rather than saying
/// "BioRouter wants to make changes".
fn prompt_text(req: &AuthRequest) -> String {
    format!("{} ({})", req.reason, req.session_ids.join(", "))
}

fn request_verification(message: &str) -> WindowsConsentSignal {
    // Asked first so that "Hello is not set up on this PC" is distinguishable
    // from "the user cancelled": they map to different `AuthOutcome`s and to
    // different advice.
    let availability = match UserConsentVerifier::CheckAvailabilityAsync().and_then(|op| op.get()) {
        Ok(availability) => availability,
        Err(e) => return WindowsConsentSignal::CallFailed(e.code().0),
    };
    if availability != UserConsentVerifierAvailability::Available {
        return WindowsConsentSignal::NotAvailable(availability.0);
    }

    match UserConsentVerifier::RequestVerificationAsync(&HSTRING::from(message))
        .and_then(|op| op.get())
    {
        Ok(result) => WindowsConsentSignal::VerificationResult(result.0),
        Err(e) => WindowsConsentSignal::CallFailed(e.code().0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_prompt_names_the_sessions_it_authorises() {
        let req = AuthRequest::new("Make 2 chats public.", &["a1".into(), "b2".into()]).unwrap();
        let text = prompt_text(&req);
        assert!(text.contains("a1") && text.contains("b2"), "got {text}");
        assert!(text.contains("Make 2 chats public."), "got {text}");
    }
}
