//! Write-boundary validation: an id that does not exist cannot be saved.
//!
//! The discovery tool ([`super::catalog`]) tells the model what exists; *this*
//! is what makes the catalog binding rather than advisory. Prose ("only configure
//! real skills") cannot help a model that has no way to know what is real, and
//! prose ("`br.kb` is an API namespace, not an id") only lowers the probability
//! of the mistake — it cannot make an invalid manifest un-saveable.
//!
//! Every rejection names the *available* ids, so the model's next attempt is
//! grounded instead of being another guess, and points at the `requires` block as
//! the honest way to record an unmet need.

use super::catalog::Catalog;
use super::store::{Requirement, RequirementKind};
use crate::knowledge::paths::validate_kb_id;

/// Reject a knowledge-base id that is malformed or not installed.
pub fn check_knowledge_base(kb: &str, catalog: &Catalog) -> Result<(), String> {
    let kb = kb.trim();
    if kb.is_empty() {
        return Ok(());
    }

    // `br.kb` is the client-side API namespace (`br.kb.search(…)`). The model
    // reached for it as an identifier more than once; say so explicitly, because
    // the generic "invalid characters" message did not stop it.
    if kb == "br.kb" || kb.starts_with("br.") {
        return Err(format!(
            "'{kb}' is not a knowledge-base id. `br.kb` is the CLIENT API namespace \
             your app calls at runtime, never an id. Installed knowledge bases: {}. \
             If this app needs a knowledge base that is not installed here, leave \
             `knowledge_base` unset and record the need in `requires` instead.",
            Catalog::render_list(&catalog.kb_ids())
        ));
    }

    if let Err(e) = validate_kb_id(kb) {
        return Err(format!(
            "knowledge_base '{kb}' is not a valid id: {e}. Installed knowledge bases: {}. \
             If this app needs one that is not installed here, leave `knowledge_base` \
             unset and record the need in `requires` instead.",
            Catalog::render_list(&catalog.kb_ids())
        ));
    }

    if !catalog.has_kb(kb) {
        return Err(format!(
            "knowledge base '{kb}' is not installed on this Biorouter. Installed: {}. \
             Do not invent an id: an app configured with a knowledge base that does not \
             exist arms KB tools scoped to nothing and fails on its first turn. Leave \
             `knowledge_base` unset and record the need in `requires` instead.",
            Catalog::render_list(&catalog.kb_ids())
        ));
    }

    Ok(())
}

/// Reject skill ids that are not installed.
pub fn check_skills(skills: &[String], catalog: &Catalog) -> Result<(), String> {
    let missing: Vec<&str> = skills
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && !catalog.has_skill(s))
        .collect();

    if missing.is_empty() {
        return Ok(());
    }

    Err(format!(
        "these skills are not installed on this Biorouter: {}. Installed skills: {}. \
         Configuring a skill that does not exist makes the agent's first turn fail \
         (it is told to load a skill that cannot load). Leave it out and record the \
         need in `requires` instead. An app may legitimately want a skill this \
         machine does not have.",
        missing.join(", "),
        Catalog::render_list(&catalog.skill_ids())
    ))
}

/// Reject extension names that are neither built in nor installed.
pub fn check_extensions(extensions: &[String], catalog: &Catalog) -> Result<(), String> {
    let missing: Vec<&str> = extensions
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && !catalog.has_extension(s))
        .collect();

    if missing.is_empty() {
        return Ok(());
    }

    Err(format!(
        "these extensions are not available on this Biorouter: {}. Available: {}. \
         Record an unavailable extension in `requires` instead of configuring it.",
        missing.join(", "),
        Catalog::render_list(&catalog.extension_names())
    ))
}

/// Whether unknown ids are rejected at the write boundary.
///
/// On by default. `BIOROUTER_APPS_CATALOG_STRICT=0` turns it off, for the one
/// legitimate case where a hard rejection would be wrong: an app whose knowledge
/// base genuinely exists on the author's machine, being rebuilt or exported on a
/// CI box that has no knowledge bases at all. Strict mode is what makes the
/// catalog binding; the escape hatch is what keeps it from bricking a valid app
/// in an environment that simply can't see its grants.
///
/// The runtime protections in `configure_agent` do **not** depend on this — a
/// grant that cannot be satisfied is never armed, whatever this returns.
pub fn strict_mode() -> bool {
    !matches!(
        std::env::var("BIOROUTER_APPS_CATALOG_STRICT")
            .unwrap_or_default()
            .trim(),
        "0" | "false" | "off"
    )
}

/// Validate every id an app configures, in one place.
pub fn check_all(
    knowledge_base: Option<&str>,
    skills: &[String],
    extensions: &[String],
    catalog: &Catalog,
) -> Result<(), String> {
    if !strict_mode() {
        return Ok(());
    }
    if let Some(kb) = knowledge_base {
        check_knowledge_base(kb, catalog)?;
    }
    check_skills(skills, catalog)?;
    check_extensions(extensions, catalog)?;
    Ok(())
}

/// Mark which of the app's declared requirements this install can actually
/// satisfy. Used by the runtime capability report and by lint.
pub fn unmet_requirements<'a>(
    requires: &'a [Requirement],
    catalog: &Catalog,
) -> Vec<&'a Requirement> {
    requires
        .iter()
        .filter(|r| match r.kind {
            RequirementKind::KnowledgeBase => !catalog.has_kb(&r.id),
            RequirementKind::Skill => !catalog.has_skill(&r.id),
            RequirementKind::Extension => !catalog.has_extension(&r.id),
            // A data source is app-defined; the platform can't check it.
            RequirementKind::DataSource => false,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_drafter::catalog::{drafter_catalog_root_with_kbs, KbEntry, SkillEntry};
    use crate::knowledge::affiliation::CallerAffiliation;
    use crate::knowledge::caller::KbCaller;
    use crate::knowledge::tier;

    /// A private, LOCAL model — the caller DR-26 clears everywhere. `Unstated`
    /// would be the restrictive value and would assert the opposite of what
    /// these fixtures mean by "a private caller".
    fn private_local() -> KbCaller {
        KbCaller::new(true, CallerAffiliation::Local)
    }

    /// Issue #56, CP5. `:33`, `:42` and `:52` render
    /// `Catalog::render_list(&catalog.kb_ids())` into `INVALID_PARAMS` strings the
    /// model reads back, so a deliberately-invalid `configure_app` is an
    /// enumeration oracle that needs no valid input at all. `br.kb` is the exact
    /// string the 100-app test drive produced, so this is the live path.
    ///
    /// The fix is upstream — a private base is absent from the catalog these
    /// three read — which is why nothing in this file changes.
    #[test]
    fn a_rejection_message_cannot_be_used_to_enumerate_private_bases() {
        let (_d, root, _env) = drafter_catalog_root_with_kbs(&["default", "omop"]);
        tier::raise_unlocked(&root, "omop", true).unwrap();
        let public = Catalog::discover(&KbCaller::restricted());
        for probe in ["br.kb", "NOT A VALID ID", "clinvar"] {
            let e = check_knowledge_base(probe, &public).unwrap_err();
            assert!(
                !e.contains("omop"),
                "{probe} enumerated a private base: {e}"
            );
            assert!(
                e.contains("default"),
                "{probe} lost the public bases too: {e}"
            );
        }
    }

    /// Omission, not refusal: the message must read "not installed", which is
    /// what a public caller can truthfully be told. A message that said "private"
    /// would confirm the base exists — the leak in a politer sentence.
    #[test]
    fn a_public_session_cannot_configure_an_app_against_a_private_base() {
        let (_d, root, _env) = drafter_catalog_root_with_kbs(&["omop"]);
        tier::raise_unlocked(&root, "omop", true).unwrap();
        let e =
            check_knowledge_base("omop", &Catalog::discover(&KbCaller::restricted())).unwrap_err();
        assert!(e.contains("not installed"), "{e}");
        assert!(!e.to_lowercase().contains("private"), "{e}");
        assert!(check_knowledge_base("omop", &Catalog::discover(&private_local())).is_ok());
    }

    fn catalog_with(kbs: &[&str], skills: &[&str]) -> Catalog {
        Catalog {
            knowledge_bases: kbs
                .iter()
                .map(|id| KbEntry {
                    id: (*id).to_string(),
                    name: (*id).to_string(),
                })
                .collect(),
            skills: skills
                .iter()
                .map(|id| SkillEntry {
                    id: (*id).to_string(),
                    description: String::new(),
                })
                .collect(),
            extensions: Catalog::discover(&KbCaller::restricted()).extensions,
        }
    }

    /// The literal failure from the test drive: the model configured `br.kb`,
    /// which is the client API namespace.
    #[test]
    fn br_kb_is_rejected_with_an_explanation_of_what_it_actually_is() {
        let err = check_knowledge_base("br.kb", &Catalog::empty()).unwrap_err();
        assert!(err.contains("CLIENT API namespace"), "{err}");
        assert!(
            err.contains("requires"),
            "must point at the honest alternative"
        );
    }

    /// A well-formed id that simply is not installed here must still be rejected —
    /// and the error must list what *is* installed, so the retry is grounded.
    #[test]
    fn a_wellformed_but_uninstalled_kb_is_rejected_and_the_error_lists_what_exists() {
        let catalog = catalog_with(&["msknowledge"], &[]);
        let err = check_knowledge_base("clinvar", &catalog).unwrap_err();
        assert!(err.contains("not installed"), "{err}");
        assert!(
            err.contains("msknowledge"),
            "must name the installed KBs: {err}"
        );
    }

    #[test]
    fn an_installed_kb_is_accepted() {
        let catalog = catalog_with(&["clinvar"], &[]);
        assert!(check_knowledge_base("clinvar", &catalog).is_ok());
    }

    #[test]
    fn an_empty_kb_is_not_a_configuration_at_all() {
        assert!(check_knowledge_base("", &Catalog::empty()).is_ok());
        assert!(check_knowledge_base("   ", &Catalog::empty()).is_ok());
    }

    /// The test drive configured 7 skill lists against a runtime with zero
    /// installed skills; every one of them failed on turn 1.
    #[test]
    fn uninstalled_skills_are_rejected_and_named() {
        let catalog = catalog_with(&[], &["r-scripting"]);
        let skills = vec!["pathway-analysis".to_string(), "r-scripting".to_string()];
        let err = check_skills(&skills, &catalog).unwrap_err();
        assert!(
            err.contains("pathway-analysis"),
            "must name the missing one"
        );
        assert!(
            !err.contains(
                "these skills are not installed on this Biorouter: pathway-analysis, r-scripting"
            ),
            "must not name the installed one as missing: {err}"
        );
        assert!(err.contains("requires"));
    }

    #[test]
    fn installed_skills_are_accepted() {
        let catalog = catalog_with(&[], &["r-scripting"]);
        assert!(check_skills(&["r-scripting".to_string()], &catalog).is_ok());
        assert!(check_skills(&[], &catalog).is_ok());
    }

    /// Built-ins must never be rejected — every app names some of them.
    #[test]
    fn builtin_extensions_are_always_available() {
        let catalog = Catalog::discover(&KbCaller::restricted());
        let exts = vec![
            "developer".to_string(),
            "autovisualiser".to_string(),
            "knowledge".to_string(),
        ];
        assert!(check_extensions(&exts, &catalog).is_ok());
    }

    #[test]
    fn an_invented_extension_is_rejected() {
        let catalog = Catalog::discover(&KbCaller::restricted());
        let err = check_extensions(&["spoke_agent".to_string()], &catalog).unwrap_err();
        assert!(err.contains("spoke_agent"));
        assert!(
            err.contains("developer"),
            "must list what IS available: {err}"
        );
    }

    /// The whole point of `requires`: wanting something absent is legal.
    #[test]
    fn a_requirement_for_an_absent_capability_is_legal_and_reported_as_unmet() {
        let requires = vec![
            Requirement {
                kind: RequirementKind::KnowledgeBase,
                id: "clinvar".into(),
                reason: "variant annotations".into(),
            },
            Requirement {
                kind: RequirementKind::Skill,
                id: "r-scripting".into(),
                reason: "plots".into(),
            },
        ];
        let catalog = catalog_with(&[], &["r-scripting"]);

        let unmet = unmet_requirements(&requires, &catalog);
        assert_eq!(unmet.len(), 1);
        assert_eq!(unmet[0].id, "clinvar");
    }
}
