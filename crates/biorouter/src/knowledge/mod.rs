// Knowledge module lives in biorouter-mcp so both crates can use it
// without a circular dependency (biorouter depends on biorouter-mcp).
pub use biorouter_mcp::knowledge::*;

pub mod provider_completer;
pub use provider_completer::ProviderCompleter;

pub mod conversation_ingest;
pub mod soul;
pub mod source_ingest;
