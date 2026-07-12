pub(crate) mod chat_fts;
mod chat_history_search;
mod diagnostics;
pub mod extension_data;
mod legacy;
pub mod session_manager;

pub use diagnostics::{generate_diagnostics, get_system_info, SystemInfo};
pub use extension_data::{EnabledExtensionsState, ExtensionData, ExtensionState, TodoState};
pub use session_manager::{
    ActivityWindow, DailyActivity, Session, SessionInsights, SessionManager, SessionType,
    SessionUpdateBuilder,
};
