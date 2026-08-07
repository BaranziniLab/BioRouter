pub mod auth;
pub mod configuration;
pub mod error;
pub mod openapi;
pub mod routes;
pub mod state;
pub mod tunnel;
pub mod workspace;

/// Redirects the data/config/state dirs at a throwaway root before `main`, so
/// tests never open the developer's real `sessions.db`.
#[cfg(test)]
mod test_sandbox;

// Re-export commonly used items
pub use openapi::*;
pub use state::*;
