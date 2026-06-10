//! Renders the Biorouter CLI's visual surfaces (greeting, session info, tool
//! section rules, context meter, task-execution display, status messages) so
//! the styling can be eyeballed without starting a live session.
//!
//! Run under a real (or emulated) terminal so colors are emitted:
//!
//! ```sh
//! script -q /dev/null cargo run -p biorouter-cli --example preview_tui
//! ```

fn main() {
    biorouter_cli::session::output::preview();
}
