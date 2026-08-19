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
pub mod log;
pub mod macros;
pub mod manifest;
// OKF v0.2 as a format module (Stage 0 of the OKF migration). Additive: nothing
// in the modules above reads it yet — Stage 2 (`graph`) and Stage 3 (`store`,
// `manifest`, `service`) are what wire it in.
pub mod okf;
pub mod paths;
pub mod raw;
pub mod registry;
pub mod server;
pub mod service;
pub mod source_paths;
pub mod store;
pub mod subagent;
pub mod test_mode;
pub mod tier;
pub mod tier_user;
pub mod types;

pub use server::KnowledgeServer;
