//! One answer to "does this tool-result block belong in the request we send the
//! model", shared by every provider format module.
//!
//! MCP lets a tool tag each content block it returns with an `audience`: an
//! array whose members are `"user"`, `"assistant"`, or both. The specification
//! says the field "describes who the intended customer of this object or data
//! is" and that "it can include multiple entries to indicate content useful for
//! multiple audiences (e.g., `["user", "assistant"]`)"
//! ([schema reference, 2025-06-18](https://modelcontextprotocol.io/specification/2025-06-18/schema);
//! the resources page words the same field as "an array indicating the intended
//! audience(s) for this resource ... For example, `["user", "assistant"]`
//! indicates content useful for both").
//!
//! So the field names a set of audiences, and every consumer's job is to ask
//! whether it is in that set. A provider formatter is building the message the
//! MODEL will read, so its question is "is the assistant one of the named
//! audiences", which is [`is_for_model`]. The desktop transcript, the CLI
//! renderer and the TUI ask the mirror-image question about the user, and their
//! filters are correctly the opposite shape rather than a copy of this one.
//!
//! ## The predicate this module exists to replace
//!
//! Asking "is the user NOT in the audience" looks equivalent and is not. It
//! disagrees on two of the five cases a tool can produce, and both disagreements
//! are wrong in a way that is invisible until a tool emits the combination:
//!
//! | `audience`                | assistant is a named audience | user is not a named audience |
//! |---------------------------|-------------------------------|------------------------------|
//! | absent                    | send                          | send                         |
//! | `["user"]`                | withhold                      | withhold                     |
//! | `["assistant"]`           | send                          | send                         |
//! | `["user", "assistant"]`   | send                          | **withhold**                 |
//! | `[]`                      | withhold                      | **send**                     |
//!
//! A block marked for both audiences is the tool saying the content is useful to
//! both, so withholding it starves the model of output the tool meant it to see.
//! A block marked for nobody names no audience at all, so sending it to the
//! model reads an empty set as permission.
//!
//! Bedrock carried the second column and the other three formatters carried the
//! first, which meant the same tool output reached the model on one provider and
//! not on another. They share this function now so the two columns cannot come
//! back.
//!
//! ## Who calls this
//!
//! Every renderer that builds a request for a model: Bedrock, Databricks,
//! Google and OpenAI, which keep a block list and handle each kind of block
//! themselves; Anthropic, the OpenAI Responses API, Snowflake and the toolshim
//! conversion, which flatten to one string via [`flattened_text`]. gcpvertexai
//! inherits the Anthropic and Google paths, openrouter and xiaomi_mimo inherit
//! OpenAI's, so none of the three has a filter of its own to keep in step.
//!
//! Toolshim is the one that cannot be deferred downstream. It rewrites a tool
//! response into a plain text block, so by the time the provider formatter runs
//! there is no annotation left to read.
//!
//! An absent `audience` is not an empty one. A tool that never set the field has
//! expressed no preference, and every consumer in this repo treats that as
//! visible to everyone, so it is sent.

use rmcp::model::{Content, RawContent, ResourceContents, Role};

/// Whether a tool-result content block should be included in the request sent
/// to the model.
///
/// True when the block carries no `audience` at all, or names `assistant` among
/// its audiences. See the module docs for why this is not the same as "the user
/// is not in the audience".
pub fn is_for_model(content: &Content) -> bool {
    content
        .audience()
        .is_none_or(|audience| audience.contains(&Role::Assistant))
}

/// The text one content block contributes when a tool result is flattened into
/// a single string, or `None` if it contributes nothing.
///
/// Four renderers flatten rather than carry a block list: Anthropic, the OpenAI
/// Responses API, Snowflake, and the toolshim text conversion. All four used to
/// read only [`RawContent::Text`], which silently discarded embedded text
/// resources.
///
/// That mattered the moment [`is_for_model`] was applied to them, because
/// `text_editor view` returns the file to the assistant as an embedded resource
/// and to the user as formatted text. Filtering alone would have left those
/// renderers with an empty tool result for every file view, so the two changes
/// belong together and in this order: drop what is not addressed to the model
/// first, then read the text out of what is left. Reading resources without
/// filtering first would do the opposite damage and push whole Auto Visualiser
/// figures, which are `audience: ["user"]` HTML documents, into the request.
///
/// Bedrock, Databricks, Google and OpenAI already read text resources this way
/// as part of their own richer per-block handling, so this restores agreement
/// rather than inventing a rule.
///
/// This does NOT check the audience. Every caller filters with [`is_for_model`]
/// on the line above, the way the other four formatters do.
pub fn flattened_text(content: &Content) -> Option<String> {
    match &content.raw {
        RawContent::Text(text) => Some(text.text.clone()),
        RawContent::Resource(embedded) => match &embedded.resource {
            ResourceContents::TextResourceContents { text, .. } => Some(text.clone()),
            ResourceContents::BlobResourceContents { .. } => None,
        },
        _ => None,
    }
}

/// One tool result carrying every audience a tool can put on a block, in the
/// order [`MODEL_VISIBLE`] and [`MODEL_HIDDEN`] describe.
///
/// Each provider format module asserts against THIS fixture rather than
/// hand-rolling its own, so every call site is held to one set of cases.
/// Private per-module fixtures would be free to drift apart, which is the shape
/// of the bug this module exists to close.
#[cfg(test)]
pub(crate) fn every_audience_case() -> Vec<Content> {
    vec![
        Content::text("alpha-untagged"),
        Content::text("bravo-user-only").with_audience(vec![Role::User]),
        Content::text("charlie-assistant-only").with_audience(vec![Role::Assistant]),
        Content::text("delta-both-audiences").with_audience(vec![Role::User, Role::Assistant]),
        Content::text("echo-empty-audience").with_audience(vec![]),
    ]
}

/// Blocks of [`every_audience_case`] that must reach the model, in order.
#[cfg(test)]
pub(crate) const MODEL_VISIBLE: [&str; 3] = [
    "alpha-untagged",
    "charlie-assistant-only",
    "delta-both-audiences",
];

/// Blocks of [`every_audience_case`] that must never reach the model.
#[cfg(test)]
pub(crate) const MODEL_HIDDEN: [&str; 2] = ["bravo-user-only", "echo-empty-audience"];

/// The body of the assistant-audience embedded resource in
/// [`text_editor_view_result`].
#[cfg(test)]
pub(crate) const VIEW_FOR_MODEL: &str = "fn main() { the file body the assistant reads }";

/// The body of the user-audience text block in [`text_editor_view_result`].
#[cfg(test)]
pub(crate) const VIEW_FOR_USER: &str = "1: the numbered rendering only the user reads";

/// The shape `text_editor view` returns: the file to the assistant as an
/// embedded text resource, a formatted rendering to the user as plain text.
///
/// Copied in structure from `text_editor.rs`'s `text_editor_view`, which is the
/// producer that makes filtering and resource reading a single change rather
/// than two. A flattening renderer that filters by audience but still ignores
/// resources returns nothing at all here, which is why each of the four
/// flattening sites asserts against this fixture as well as
/// [`every_audience_case`].
#[cfg(test)]
pub(crate) fn text_editor_view_result() -> Vec<Content> {
    vec![
        Content::embedded_text("str:///notes.rs", VIEW_FOR_MODEL)
            .with_audience(vec![Role::Assistant]),
        Content::text(VIEW_FOR_USER)
            .with_audience(vec![Role::User])
            .with_priority(0.0),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One text block carrying `audience`, or none if `audience` is `None`.
    ///
    /// `with_audience` is the only way to reach an `Annotations` with the field
    /// set, and skipping it is the only way to reach a block with no
    /// annotations, so the two shapes have to be built differently. That
    /// distinction is the point of the first and last rows below.
    fn block(audience: Option<Vec<Role>>) -> Content {
        match audience {
            Some(roles) => Content::text("payload").with_audience(roles),
            None => Content::text("payload"),
        }
    }

    /// Every audience a tool can emit, and whether the model sees it.
    ///
    /// The two rows that matter are `["user", "assistant"]` and `[]`: they are
    /// where the discarded "user is not in the audience" predicate gave the
    /// opposite answer, so a regression to it fails here twice.
    #[test]
    fn the_full_audience_truth_table() {
        let cases: [(&str, Option<Vec<Role>>, bool); 5] = [
            ("no annotation", None, true),
            ("user only", Some(vec![Role::User]), false),
            ("assistant only", Some(vec![Role::Assistant]), true),
            (
                "both audiences",
                Some(vec![Role::User, Role::Assistant]),
                true,
            ),
            ("empty audience", Some(vec![]), false),
        ];

        for (name, audience, expected) in cases {
            assert_eq!(
                is_for_model(&block(audience.clone())),
                expected,
                "{name} ({audience:?}) should {} the model",
                if expected {
                    "reach"
                } else {
                    "be withheld from"
                }
            );
        }
    }

    /// Order within the array is a set, not a sequence.
    #[test]
    fn both_audiences_reach_the_model_in_either_order() {
        assert!(is_for_model(&block(Some(vec![
            Role::Assistant,
            Role::User
        ]))));
        assert!(is_for_model(&block(Some(vec![
            Role::User,
            Role::Assistant
        ]))));
    }

    /// The two constants the formatter tests assert against must partition the
    /// fixture, or a formatter test could pass while ignoring a block.
    #[test]
    fn the_fixture_and_its_two_expectations_agree() {
        let fixture = every_audience_case();
        assert_eq!(fixture.len(), MODEL_VISIBLE.len() + MODEL_HIDDEN.len());
        for block in &fixture {
            let text = block
                .as_text()
                .expect("fixture blocks are text")
                .text
                .clone();
            let visible = MODEL_VISIBLE.contains(&text.as_str());
            let hidden = MODEL_HIDDEN.contains(&text.as_str());
            assert!(visible ^ hidden, "{text} is in neither list or in both");
            assert_eq!(is_for_model(block), visible, "{text}");
        }
    }

    /// A block with annotations but no `audience` key is an absent audience,
    /// not an empty one. `with_priority` sets the sibling field and leaves
    /// `audience` at `None`, which is the shape a tool that only ranked its
    /// output produces.
    #[test]
    fn annotations_without_an_audience_still_reach_the_model() {
        let ranked = Content::text("payload").with_priority(0.2);
        assert_eq!(ranked.audience(), None);
        assert!(is_for_model(&ranked));
    }

    /// What each kind of block contributes to a flattened tool result. A text
    /// resource carries its text; a binary one carries nothing, because a
    /// base64 blob spliced into a prompt is noise the model pays for.
    #[test]
    fn a_flattened_result_reads_text_and_text_resources_only() {
        assert_eq!(
            flattened_text(&Content::text("plain")),
            Some("plain".to_string())
        );
        assert_eq!(
            flattened_text(&Content::embedded_text("str:///f", "resource body")),
            Some("resource body".to_string())
        );
        assert_eq!(
            flattened_text(&Content::resource(
                rmcp::model::ResourceContents::BlobResourceContents {
                    uri: "str:///f".to_string(),
                    mime_type: Some("application/octet-stream".to_string()),
                    blob: "AAAA".to_string(),
                    meta: None,
                }
            )),
            None
        );
        assert_eq!(flattened_text(&Content::image("AAAA", "image/png")), None);
    }

    /// The `text_editor view` fixture is only interesting if its two halves
    /// really do land on opposite sides of the filter, and if the half the
    /// model needs is the one a text-only renderer would have dropped.
    #[test]
    fn the_view_fixture_puts_the_model_text_behind_a_resource() {
        let blocks = text_editor_view_result();
        assert!(is_for_model(&blocks[0]), "the resource is for the model");
        assert!(!is_for_model(&blocks[1]), "the rendering is for the user");
        assert_eq!(blocks[0].as_text(), None, "reading .as_text() loses it");
        assert_eq!(flattened_text(&blocks[0]), Some(VIEW_FOR_MODEL.to_string()));
        assert_eq!(flattened_text(&blocks[1]), Some(VIEW_FOR_USER.to_string()));
    }
}
