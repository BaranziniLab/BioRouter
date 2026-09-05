//! The failure extractor, run against a **real exported session**.
//!
//! ⚠ The unit tests next to the extractor build their conversations in Rust,
//! which means they can only ever exercise the shapes the author already
//! believed in. This one deserializes an actual `session.json` produced by
//! `generate_diagnostics` on a running desktop app — 30 real messages of
//! `toolRequest`/`toolResponse` traffic from four different extensions and a
//! bridged coding-agent provider, with every scrap of the user's own content
//! replaced and the two failure spellings injected in the exact serialized form
//! `tool_result_serde` writes.
//!
//! What it proves that a hand-built fixture cannot: that `Message` still
//! deserializes an export written by the shipping code, including the
//! provider-metadata and `_meta` fields the in-memory constructors leave unset.

use biorouter::agents::bug_report::evidence::{conversation_from_export, failures_in};
use biorouter::agents::tool_errors::ToolErrorKind;

fn export() -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/exported_session_with_failures.json"),
    )
    .expect("the fixture must be readable, or this test proves nothing")
}

#[test]
fn a_real_exported_session_parses_and_yields_both_failure_spellings() {
    let conversation = conversation_from_export(&export()).expect("a real export deserializes");
    assert!(
        conversation.messages().len() >= 30,
        "the fixture lost its body: {} messages",
        conversation.messages().len()
    );

    let (failures, total_failed, total_calls, _externalized) = failures_in(&conversation);

    assert!(
        total_calls >= 13,
        "the real session's successful calls must be counted too, or the report \
         cannot say `2 of 15 calls failed`: {total_calls}"
    );
    // Three, not two: the real session already contained one failed call, and
    // finding it is the point. It is a Gate C privacy refusal, which the
    // extractor must LABEL rather than treat as a defect.
    assert_eq!(
        total_failed, 3,
        "the two injected failures plus the real session's own, and nothing \
         invented from the successful calls around them: {failures:#?}"
    );
    let real = failures
        .iter()
        .find(|f| f.tool_name.as_deref() == Some("workspace__workspace_read_conversation"))
        .expect("the real session's own failed call is found");
    assert!(
        real.looks_deliberate,
        "a privacy refusal reads as a hard bug on every coarse signal — not \
         retryable, `ToolFailure`, a long message — so it must be labelled or the \
         tool files `the security boundary worked` as a defect: {real:#?}"
    );

    let shell = failures
        .iter()
        .find(|f| f.tool_name.as_deref() == Some("developer__shell"))
        .expect("the transport-level failure is found");
    assert_eq!(shell.kind, ToolErrorKind::NotFound);
    assert!(!shell.retryable);
    assert!(
        shell
            .arguments
            .as_deref()
            .is_some_and(|args| args.contains("cargo build")),
        "the call's own arguments are frequently the bug: {shell:#?}"
    );

    // ⚠ This is the one a `status == "error"` scan misses. In the export it is
    // spelled `{"status":"success","value":{...,"isError":true}}` — a success,
    // as far as the serializer is concerned.
    let editor = failures
        .iter()
        .find(|f| f.tool_name.as_deref() == Some("developer__text_editor"))
        .expect(
            "the `isError: true` spelling must be found; it is the common one, and a \
             scan that misses it reports a clean session while the user looks at a \
             red error",
        );
    assert!(
        editor.message.contains("the build failed"),
        "the tool's own text is the failure message: {editor:#?}"
    );
}

/// Every string the report could carry out of this session survives the
/// scrubber unchanged only because the fixture has nothing left to redact —
/// which is itself worth pinning, since a fixture that leaked would make the
/// suite complicit.
#[test]
fn the_fixture_carries_nothing_the_scrubber_would_have_to_remove() {
    let raw = export();
    let scrubbed = biorouter::agents::bug_report::redact::scrub(&raw, None);
    assert!(
        !scrubbed.changed(),
        "the checked-in fixture still contains identifying material: {}",
        scrubbed.summary()
    );
}
