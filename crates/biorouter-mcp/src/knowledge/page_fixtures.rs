//! One place that knows what a knowledge page looks like, for the tests that
//! have to write one.
//!
//! ## Why this is production code and not a `#[cfg(test)]` helper
//!
//! The fixtures it replaces are spread across three crates —
//! `knowledge::server`'s tool-probe table here, `biorouter`'s conversation-ingest
//! test, and `biorouter-server`'s `knowledge_routes` integration tests — and a
//! `#[cfg(test)]` item is not visible from another crate's tests. The same
//! reasoning already put [`crate::knowledge::test_mode`] in the tree.
//!
//! ## Why the indirection, before there is anything to indirect (DR-19)
//!
//! Roughly twenty test fixtures hand `kb_write_page` a bare string —
//! `"content": "body"` is the shape, five bytes and no frontmatter. Most of them
//! are testing something else entirely: privacy tiers, HTTP status codes, the
//! change log. When a validating writer lands (Stage 3), every one of them fails
//! with a message about missing frontmatter *under a test name about tiers*, and
//! the natural response to twenty red privacy tests is to loosen the validator —
//! which defeats the change that made them red.
//!
//! So the helper lands **first**, while it is a pure refactor and every test
//! stays green, and the format change afterwards touches this one function
//! instead of twenty call sites.
//!
//! ## What it emits today, and what it will emit later
//!
//! Today's format, deliberately: `title` + `kind` frontmatter, which is what
//! `store::split_frontmatter`, `graph::page_kind_of` and the sub-agent's own
//! `schema.md` all expect. It is **not** the OKF shape (`type` + `identifier`) —
//! moving to that is Stage 3's job, and doing it here would change what every
//! caller writes while claiming to be a seam.

/// A minimal conformant knowledge page: frontmatter plus a body.
///
/// `page_type` is the page's `kind` in today's format (`source`, `entity`,
/// `concept`, `note`, `hub`, `flag`) and becomes its OKF `type` at Stage 3 —
/// hence the parameter name, which is the one thing about this signature that
/// is aimed at where the format is going rather than where it is.
///
/// The frontmatter goes through `serde_yaml` rather than `format!` so a title
/// carrying a colon, a `#`, or a leading `-` is quoted correctly. A fixture
/// builder that emits invalid YAML for an awkward title would be a worse trap
/// than the literals it replaces, because it would fail somewhere else.
pub fn valid_page(page_type: &str, title: &str, body: &str) -> String {
    let mut frontmatter = serde_yaml::Mapping::new();
    frontmatter.insert("title".into(), title.into());
    frontmatter.insert("kind".into(), page_type.into());
    let yaml = serde_yaml::to_string(&serde_yaml::Value::Mapping(frontmatter))
        .expect("a two-key string mapping always serializes");
    format!("---\n{yaml}---\n\n{}\n", body.trim_end_matches('\n'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::store::split_frontmatter;

    #[test]
    fn what_it_emits_is_what_the_reader_reads_back() {
        let page = valid_page("entity", "HRV", "Heart rate variability.");
        let (fm, body) = split_frontmatter(&page);
        assert_eq!(fm.get("title").and_then(|v| v.as_str()), Some("HRV"));
        assert_eq!(fm.get("kind").and_then(|v| v.as_str()), Some("entity"));
        assert_eq!(body.trim(), "Heart rate variability.");
    }

    #[test]
    fn an_awkward_title_is_quoted_rather_than_breaking_the_frontmatter() {
        // `format!`-built frontmatter emits `title: Chen 2020: IL-6`, which is a
        // YAML mapping value containing a colon — a parse error, surfacing
        // wherever the page is next read rather than here.
        let page = valid_page("source", "Chen 2020: IL-6 and severe COVID-19", "Body.");
        let (fm, _) = split_frontmatter(&page);
        assert_eq!(
            fm.get("title").and_then(|v| v.as_str()),
            Some("Chen 2020: IL-6 and severe COVID-19")
        );
    }

    #[test]
    fn the_body_is_carried_through_verbatim_and_ends_in_exactly_one_newline() {
        // Assertions across the suite are `content.contains(…)` over what was
        // written, so the body must survive unedited.
        let page = valid_page("note", "N", "SENTINEL-UCSF\n\n## Sources\n");
        assert!(page.contains("SENTINEL-UCSF"));
        assert!(page.ends_with("## Sources\n"), "got: {page:?}");
    }
}
