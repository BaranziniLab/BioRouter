pub mod affiliation;
// The BioOKF v0.5 profile over `okf` (Stage 1 of the OKF migration): the 28
// node types, the 24 positive predicates plus their 11 derived `not_<X>`
// negatives, the domain/range table, SPEC §14's aliases, and the §10/§11 lint
// rules. Additive like `okf`: nothing below reads it yet.
pub mod biookf;
pub mod brkb;
pub mod caller;
pub mod convert;
pub mod credibility;
pub mod git;
pub mod graph;
// The one `[[…]]` parser and the one resolver behind it (Stage 1.5, DR-14).
// Unlike `okf` and `biookf` this module is NOT additive: `graph`, `macros::query`
// and `macros::lint` each used to carry their own copy of the regex and their own
// resolver, and all three now call this.
pub mod links;
pub mod log;
pub mod macros;
pub mod manifest;
// KB-to-KB merge (the deterministic half): raw dedup by content hash, rename on
// collision, reference rewriting, and a pre/post canonical check on the
// destination. The judgement half — semantic candidate matching, true-match
// collapse, prose harmonization — is a later macro and deliberately absent.
pub mod merge;
// OKF v0.2 as a format module (Stage 0 of the OKF migration). Additive: nothing
// in the modules above reads it yet — Stage 2 (`graph`) and Stage 3 (`store`,
// `manifest`, `service`) are what wire it in.
pub mod okf;
// The one page-shaped fixture builder the tests in three crates write through
// (Stage 1.5, DR-19). Production code so `biorouter` and `biorouter-server`
// tests can reach it; nothing in production calls it.
pub mod page_fixtures;
pub mod paths;
pub mod raw;
pub mod registry;
pub mod server;
pub mod service;
pub mod source_anchor;
pub mod source_paths;
pub mod store;
pub mod subagent;
pub mod test_mode;
pub mod tier;
pub mod tier_user;
pub mod types;
// One serializable diagnostic type over `okf::Diagnostic` and `biookf::Finding`,
// plus the validate-before-write entry point behind `kb_validate_page` and the
// typed half of the lint report (Stage 4).
pub mod validate;

pub use server::KnowledgeServer;
