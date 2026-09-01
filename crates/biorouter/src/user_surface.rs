//! Whether the code running right now has a person it can put a question to.
//!
//! ⚠ **A rule, not a list.** Any MCP server can park a decision through
//! `create_elicitation`, and any tool can raise an approval, so the set of
//! calls that might need a human is unbounded and no static tool-name list can
//! express this refusal. What CAN be stated is the property of the *caller*:
//! `POST /agent/call_tool` has no admitted turn, no stream, and nothing to draw
//! a card on — so a decision raised beneath it has nowhere to go.
//!
//! Without this, such a card is inserted into a queue only the agent loop
//! drains: it parks for its whole time-to-live with nobody able to answer it,
//! and is then resurrected by the next chat turn, where it appears as a
//! question about something that happened minutes ago.
//!
//! Shaped after [`crate::session_context`]: a `task_local`, so it follows the
//! future rather than the thread, and defaults to "there is a person" — the
//! safe answer, since the cost of being wrong is a card that waits rather than
//! a decision made without anyone.

use tokio::task_local;

task_local! {
    static NO_HUMAN_SURFACE: bool;
}

/// Run `f` with no human surface: anything under it that would park a decision
/// is refused outright instead.
pub async fn without_human_surface<F>(f: F) -> F::Output
where
    F: std::future::Future,
{
    NO_HUMAN_SURFACE.scope(true, f).await
}

/// Is there no person this code could ask?
///
/// `false` outside any scope — the default is that a person exists.
pub fn no_human_surface() -> bool {
    NO_HUMAN_SURFACE.try_with(|flag| *flag).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_default_is_that_a_person_exists() {
        // ⚠ The direction of the default is the safety property. Defaulting to
        // "no human" would make every ordinary turn's approval card vanish.
        assert!(!no_human_surface());
    }

    #[tokio::test]
    async fn the_scope_covers_everything_awaited_inside_it() {
        without_human_surface(async {
            assert!(no_human_surface());
            // A nested await is still inside the scope — the point of using a
            // task-local rather than a parameter.
            tokio::task::yield_now().await;
            assert!(no_human_surface());
        })
        .await;
        assert!(!no_human_surface(), "the scope must not leak past its future");
    }
}
