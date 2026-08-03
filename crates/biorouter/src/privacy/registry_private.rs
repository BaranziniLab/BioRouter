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
//! ⚠ Nothing fails CI when this file and the registry disagree. One command
//! rewrites both from one source, so the only way to drift is to hand-edit this
//! file — which is what the first line asks you not to do.

/// The BAAM extensions whose cards declare `data-privacy="private"`, and which
/// so must never be admitted to a public session.
///
/// Values are `name_to_key` **keys** — whitespace-stripped and lowercased —
/// which is the form `classify_extension` reduces its argument to before the
/// lookup. That makes the entry match either spelling the registry publishes:
/// the id (`cdwagent`) or the bundle `manifest.name` (`CDWAgent`).
pub const PRIVATE_EXTENSIONS: &[&str] = &["cdwagent", "ucsfomopagent"];
