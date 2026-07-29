//! #68 regression, end-to-end: in Auto mode a file *outside* the project must
//! not be refused because its name happens to match one of the project's own
//! `.biorouterignore` patterns.
//!
//! Re-rooting the `SecretGuard` onto the session working directory (the fix for
//! the guard half of #68) is right, but the guard's matcher was then applied to
//! every candidate path regardless of where it sits. A bare gitignore pattern
//! matches a basename at any depth, so `notes.txt` in the project's ignore file
//! started denying `/somewhere/else/notes.txt` too — a path Auto mode
//! deliberately lets past the containment jail, because sensitive writes there
//! are gated upstream by the agent's `SensitiveOpsInspector` instead.
//!
//! Driven through the public `text_editor` tool, in Auto mode, with the process
//! cwd left alone: the only thing that can deny this read is the guard.
//!
//! `set_path_jail_relaxed` is process-wide, so this is a single test in its own
//! integration binary.

use biorouter_mcp::developer::rmcp_developer::TextEditorParams;
use biorouter_mcp::{set_path_jail_relaxed, DeveloperServer};
use rmcp::handler::server::wrapper::Parameters;

fn view_params(path: &str) -> Parameters<TextEditorParams> {
    Parameters(TextEditorParams {
        path: path.to_string(),
        command: "view".to_string(),
        diff: None,
        view_range: None,
        file_text: None,
        old_str: None,
        new_str: None,
        insert_line: None,
    })
}

/// Re-engages the jail on drop, so a panicking assertion cannot leave the
/// process-wide Auto-mode flag set for anything that runs after it.
struct RelaxedJail;

impl RelaxedJail {
    fn enter() -> Self {
        set_path_jail_relaxed(true);
        RelaxedJail
    }
}

impl Drop for RelaxedJail {
    fn drop(&mut self) {
        set_path_jail_relaxed(false);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn auto_mode_read_outside_the_project_survives_a_project_ignore_pattern() {
    let project = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();

    // The project protects its own `notes.txt`...
    std::fs::write(project.path().join(".biorouterignore"), "notes.txt\n").unwrap();
    let project_secret = project.path().join("notes.txt");
    std::fs::write(&project_secret, "internal").unwrap();

    // ...while an unrelated directory happens to hold a file of the same name.
    let stranger = outside.path().join("notes.txt");
    std::fs::write(&stranger, "READ ALLOWED").unwrap();

    let server = DeveloperServer::new().with_working_dir(project.path().to_path_buf());
    let _relaxed = RelaxedJail::enter();

    // Sanity: the project's own rule is still enforced, so this test cannot pass
    // by the guard simply doing nothing.
    let refused = server
        .text_editor(view_params(project_secret.to_str().unwrap()))
        .await
        .expect_err("the project's own .biorouterignore must still deny its file");
    assert!(
        refused.message.contains(".biorouterignore"),
        "the refusal must name the reason, got: {}",
        refused.message
    );

    // THE POINT: an ordinary file outside the project is not the project's to
    // deny. Auto mode already let it past the containment jail.
    let result = server
        .text_editor(view_params(stranger.to_str().unwrap()))
        .await;
    let out = result.unwrap_or_else(|e| {
        panic!(
            "a project .biorouterignore pattern denied an unrelated file outside \
             the project: {}",
            e.message
        )
    });
    let text = out
        .content
        .iter()
        .find_map(|c| c.as_text())
        .expect("view returns text")
        .text
        .clone();
    assert!(
        text.contains("READ ALLOWED"),
        "the outside file should have been read, got: {text}"
    );
}
