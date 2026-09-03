//! Installing an extension: one transaction, one credential path (#117).
//!
//! Three surfaces install extensions — the desktop, `biorouter extension
//! install`, and (now) an agent asked to do it in chat — and before this module
//! they shared no code and disagreed on the one thing that matters: what to do
//! when a required credential is missing. The CLI printed a warning and
//! registered the extension anyway. The desktop had a working credential form
//! that only a human clicking a menu could reach. An agent had neither, so it
//! told the user to paste a password into the chat.
//!
//! - [`brxt`] reads a bundle: validate, manifest, extract, `uv sync`.
//! - [`credentials`] is the secret-safe collection path. Read its header before
//!   touching anything that carries a value.
//! - [`transaction`] is the state machine and its rollback.
//! - [`claim`] is what a stopped install leaves on disk, and the only record
//!   that outlives the process that made it.

pub mod brxt;
pub mod claim;
pub mod credentials;
pub mod transaction;

pub use brxt::{BrxtBundle, BrxtEnvVar, BrxtManifest, BundledSkill};
pub use claim::{ClaimPhase, ClaimSource, InstallClaim};
pub use credentials::{
    cancel_credentials, park_credentials, request_credentials, submit_credentials,
    CredentialRequests, CredentialSpec, ParkedCredentials, SubmitOutcome, DEFAULT_CREDENTIAL_TTL,
};
pub use transaction::{
    classify_supplied, compose_config, CredentialPolicy, ExtensionInstallTransaction,
    InstallReport, InstallSource, InstallState,
};
