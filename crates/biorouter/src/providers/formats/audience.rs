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
//! An absent `audience` is not an empty one. A tool that never set the field has
//! expressed no preference, and every consumer in this repo treats that as
//! visible to everyone, so it is sent.

use rmcp::model::{Content, Role};

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

/// One tool result carrying every audience a tool can put on a block, in the
/// order [`MODEL_VISIBLE`] and [`MODEL_HIDDEN`] describe.
///
/// Each provider format module asserts against THIS fixture rather than
/// hand-rolling its own, so the four call sites are held to one set of cases.
/// Four private fixtures would be free to drift apart, which is the shape of
/// the bug this module exists to close.
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
}
