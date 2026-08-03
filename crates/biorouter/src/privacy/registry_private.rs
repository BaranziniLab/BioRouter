//! **Generated file — do not edit by hand.**
//!
//! `landing/scripts/build-registry.mjs` writes this in the same run as
//! `landing/registry.json` and the desktop fallback snapshot, from the
//! `data-privacy` / `data-extension-name` annotations on the extension cards
//! in `landing/baam.html`. Regenerate all three with:
//!
//!     node landing/scripts/build-registry.mjs
//!
//! The set has to live in Rust: there is no network path to the registry from
//! here (the only fetch is the Electron `main.ts` `registry:fetch` handler), so
//! without this file the CLI and the daemon can enforce nothing.
//!
//! Drift between the three is detectable, not merely discouraged:
//!
//!     node landing/scripts/build-registry.mjs --check
//!
//! regenerates all three in memory and fails if any committed copy differs. It
//! runs in CI (the Frontend workflow) and in `just check-everything`, so a hand
//! edit here — or an interrupted run that updated only some of the three — is
//! caught rather than trusted not to happen.

/// The BAAM extensions whose cards declare `data-privacy="private"`, and which
/// so must never be admitted to a public session.
///
/// Values are `name_to_key` **keys** — whitespace-stripped and lowercased —
/// which is the form `classify_extension` reduces its argument to before the
/// lookup. That makes the entry match either spelling the registry publishes:
/// the id (`cdwagent`) or the bundle `manifest.name` (`CDWAgent`).
pub const PRIVATE_EXTENSIONS: &[&str] = &["cdwagent", "ucsfomopagent"];
