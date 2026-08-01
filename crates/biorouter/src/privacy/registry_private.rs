//! Hand-maintained. **Not generated** — an earlier header here said it was, and
//! all three of that header's claims were false in this tree:
//! `landing/scripts/build-registry.mjs` writes `registry.json` and nothing else
//! (its only `writeFileSync` is line 163); no `just check-privacy-registry`
//! target exists in the `Justfile`; and no extension entry in
//! `landing/registry.json` carries a `privacy` field for a generator to read.
//!
//! The set still has to live in Rust: there is no network path to the registry
//! from here (the only fetch is Electron's `main.ts` `registry:fetch` handler),
//! so without this file the CLI and the daemon can enforce nothing.
//!
//! ⚠ Because it is hand-maintained, the drift is silent and runs in the *leak*
//! direction: a newly published private extension that nobody adds here stays
//! Public in Rust, and no test or lint notices. Phase 1 is inert so nothing
//! depends on it yet — but before Phase 2 hangs Gate C on this set, either give
//! `landing/registry.json` a `privacy` field and generate this file from it, or
//! add a check that fails CI when the two disagree.

/// The two BAAM extensions that read UCSF clinical EHR systems, and so must
/// never be admitted to a public session.
///
/// Values are `name_to_key` **keys** — whitespace-stripped and lowercased —
/// which is the form `classify_extension` reduces its argument to before the
/// lookup. That makes the entry match either spelling the registry publishes:
/// the `id` (`cdwagent`) or the bundle's `manifest.name` (`CDWAgent`).
pub const PRIVATE_EXTENSIONS: &[&str] = &["cdwagent", "ucsfomopagent"];
