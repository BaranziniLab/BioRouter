// Knowledge module lives in biorouter-mcp so both crates can use it
// without a circular dependency (biorouter depends on biorouter-mcp).
pub use biorouter_mcp::knowledge::*;
