use super::ProviderTier;

/// The single function implementing R11, both halves.
///
/// (i) **Nothing local can grant private.** The tier is resolved from the
///     compiled-in `registry_private::PRIVATE_EXTENSIONS` baseline, never from
///     `config.yaml` and never from the `.brxt` bundle — which self-declares
///     nothing the resolver reads, and whose install records no provenance at
///     all (`BrxtInstallModal.tsx` writes name/cmd/args/envs and no registry
///     id, source URL or hash). That baseline is hand-maintained, **not**
///     generated; see that module's header for the drift hazard it creates.
/// (ii) **Anything not on BAAM is PUBLIC.** Fail-open, operator ruling. This is
///     the opposite fail direction from `Provider::tier`'s default and the
///     asymmetry is deliberate: an unknown model is a place data might *go*
///     (restrict it); an unknown extension is a place data might *come from*.
///
/// Reversing ruling (ii) later is a one-line change here, by design.
///
/// **The second term of the design's union is not here, and that is not an
/// oversight.** §10.2 states `private_set = PRIVATE_EXTENSIONS ∪
/// private(last_good_fetch)` — freshness raises, never lowers. The last-good
/// fetch is written and read entirely on the Electron side (the `registry:fetch`
/// handler in `main.ts` and `components/baam/registry.ts`); no task in this
/// series gives the CLI or the daemon a reader for it, and inventing an
/// always-empty one here would read as enforcement that does not exist. In Rust
/// the compiled baseline governs alone — which is exactly the documented
/// first-run-after-upgrade behaviour (§15.4), and can only *under*-report
/// private, never over-report public. Adding the union is a one-line change at
/// the `||` below once a Rust-side reader exists.
pub fn classify_extension(name: &str) -> ProviderTier {
    let key = crate::config::extensions::name_to_key(name);
    if super::registry_private::PRIVATE_EXTENSIONS.contains(&key.as_str()) {
        ProviderTier::Private
    } else {
        ProviderTier::Public
    }
}

/// The compiled-in private set, by `name_to_key` key.
///
/// `registry_private` is a private module on purpose — nothing outside this
/// folder may *decide* a tier from the raw list, it must go through
/// [`classify_extension`]. This accessor exists for the one legitimate reader
/// that is not a classifier: the disclosure test in [`super::refusal`], which
/// asserts a refusal never names a member of the set the caller did not ask
/// about. A hand-written list there would stop tracking this one and the
/// assertion would go quietly vacuous.
pub fn private_extension_ids() -> impl Iterator<Item = &'static str> {
    super::registry_private::PRIVATE_EXTENSIONS.iter().copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::privacy::ProviderTier;

    #[test]
    fn the_private_set_is_exactly_the_two_the_registry_publishes() {
        use crate::privacy::ProviderTier::{Private, Public};
        assert_eq!(classify_extension("ucsfomopagent"), Private);
        assert_eq!(classify_extension("cdwagent"), Private);

        // R11(ii): anything not on BAAM is PUBLIC. Fail-open, by operator ruling.
        // `medcp` is enabled on the operator's own machine with CLINICAL_RECORDS_*
        // against a clinical MSSQL backend and stays fully callable — the badge is
        // a statement about provenance, not about the data behind the connector.
        for name in [
            "medcp",
            "msbaseagent",
            "spokeagent",
            "spokeagent-0.4.1",
            "developer",
            "memory",
            "knowledge",
            "autovisualiser",
            "computercontroller",
            "tutorial",
            "agent_drafter",
            "todo",
            "chatrecall",
            "extensionmanager",
            "skills",
            "code_execution",
            "appcontrol",
            "datasql",
            "files",
            "compute",
            "evidence",
            "something-nobody-has-published",
        ] {
            assert_eq!(classify_extension(name), Public, "{name}");
        }
    }

    #[test]
    fn classification_is_case_and_whitespace_insensitive_the_way_the_key_is() {
        // `name_to_key` (config/extensions.rs:23) strips whitespace and lowercases,
        // then `normalize()` (extension_manager.rs) preserves `_`. The tier
        // must be resolved on the SAME key the manager stores, or a config entry
        // named "UCSFOMOPAgent" installs Private under one rule and Public under
        // the other.
        assert_eq!(classify_extension("UCSFOMOPAgent"), ProviderTier::Private);
        assert_eq!(classify_extension(" ucsfomopagent "), ProviderTier::Private);
    }
}
