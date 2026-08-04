use super::ProviderTier;

/// The single function implementing R11, both halves.
///
/// (i) **Nothing local can grant private.** The tier is resolved from the
///     compiled-in `registry_private::PRIVATE_EXTENSIONS` baseline, never from
///     `config.yaml` and never from the `.brxt` bundle — which self-declares
///     nothing the resolver reads. That baseline is a **generated** file:
///     `landing/scripts/build-registry.mjs` writes it from the `data-privacy` /
///     `data-extension-name` annotations on the BAAM cards, in the same run as
///     `landing/registry.json` and the desktop fallback snapshot, and its
///     `--check` mode fails CI when the three disagree.
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
/// the `is_private_key` below once a Rust-side reader exists.
///
/// # Unioned, never replaced — Task 43, DR-23
///
/// DR-23: an extension's tier is **re-derived** from the registry, keyed on a
/// stable identifier, and never written onto the local config entry. That
/// identifier is the BAAM registry `id` the install recorded in
/// [`super::provenance`]. The config entry's own name stays as the fallback for
/// everything installed before that store existed, and for a `.brxt` dropped in
/// by hand, which carries no registry id at all.
///
/// The sources are **unioned**, not tried in order, and that is the
/// load-bearing choice:
///
///  * It closes the bug. Renaming `cdwagent` to `mystuff` in `config.yaml` made
///    this function answer Public, and Gates C, E, F1 and F2 all read this
///    answer — so a rename removed ENFORCEMENT, not a label.
///  * It cannot be turned into a downgrade. A record only ever ADDS a reason to
///    be Private, so forging one, corrupting the store, or deleting it outright
///    leaves the answer at least as restrictive as the config-name join that
///    shipped before. That is why the provenance file needs no gated writer —
///    DR-23's own argument that re-deriving removes the problem instead of
///    guarding it — and it is the same "raises and never lowers" rule Task 37
///    states for registry freshness.
///
/// ⚠ **A stale or absent registry never lowers anything either**, and here that
/// holds by construction rather than by a retention cache: there is no network
/// path to BAAM from Rust, so the registry this consults IS the compiled
/// snapshot above, linked into the binary. A recorded id the snapshot does not
/// publish leaves the name-derived answer standing.
///
/// ⚠ **A caller holding an `ExtensionConfig` must use
/// [`classify_extension_entry`] instead.** This name-only form cannot find a
/// record for an entry that was renamed after it was installed, because the
/// name is exactly what the rename changed; the config carries the install
/// directory, which it did not.
pub fn classify_extension(name: &str) -> ProviderTier {
    classify_extension_entry(name, None)
}

/// The resolver, for a caller that has the config as well as the key.
///
/// `key` is the name the caller is asking about — the extension manager's map
/// key, or the name a model named in `manage_extensions`. `config` is the entry
/// behind it when there is one; passing it is what lets a renamed entry still be
/// found, via the install directory in its arguments (see
/// [`super::provenance::find`]).
///
/// Private if **any** of these says so: the key, the config's own declared name,
/// or the registry id of the record found by either of those or by the install
/// directory. Anything else is Public — R11(ii), fail-open.
pub fn classify_extension_entry(
    key: &str,
    config: Option<&crate::agents::extension::ExtensionConfig>,
) -> ProviderTier {
    use crate::config::extensions::name_to_key;
    let mut keys = vec![name_to_key(key)];
    if let Some(config) = config {
        let declared = name_to_key(&config.name());
        if declared != keys[0] {
            keys.push(declared);
        }
    }
    if keys.iter().any(|k| is_private_key(k)) {
        return ProviderTier::Private;
    }
    // Reached only for a name the snapshot does not know, so nothing already
    // private by name pays for the store lookup.
    let referenced = config.map(referenced_paths).unwrap_or_default();
    if super::provenance::registry_ids_for(&keys, &referenced)
        .iter()
        .any(|id| is_private_key(&name_to_key(id)))
    {
        return ProviderTier::Private;
    }
    ProviderTier::Public
}

/// The filesystem paths a config names, for matching against a recorded
/// `install_dir`. Only `Stdio` carries any: it is the shape every `.brxt`
/// install writes (`uv run --directory <install_dir> <entry_point>`). The
/// arguments are compared **whole**, never parsed for a `--directory` flag — a
/// flag-parsing heuristic in a security path is right until an install path
/// spells it differently.
fn referenced_paths(config: &crate::agents::extension::ExtensionConfig) -> Vec<String> {
    match config {
        crate::agents::extension::ExtensionConfig::Stdio { args, .. } => args.clone(),
        _ => Vec::new(),
    }
}

/// Membership of the compiled marketplace snapshot, on an already-reduced key.
fn is_private_key(key: &str) -> bool {
    super::registry_private::PRIVATE_EXTENSIONS.contains(&key)
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

    /// The set itself, stated once where a human reads it.
    ///
    /// The test below asserts two members and a list of non-members, which an
    /// **extra** private entry passes — a generator bug that swept, say,
    /// `playwrightagent` into the set would go unnoticed there, and so would a
    /// hand edit. The set is small, deliberate and reviewed by name, so its
    /// exact value belongs in an assertion rather than in a comment.
    #[test]
    fn the_generated_set_is_exactly_these_two_keys() {
        assert_eq!(
            private_extension_ids().collect::<Vec<_>>(),
            vec!["cdwagent", "ucsfomopagent"],
            "the compiled-in private set changed. It is generated from the \
             data-privacy annotations in landing/baam.html by \
             landing/scripts/build-registry.mjs — if that change is intended, \
             say so here; if it is not, the generator or the page is wrong"
        );
    }

    /// The key has to survive both reductions on the way in and come out the
    /// same, or the set holds a spelling the running app never produces.
    ///
    /// `classify_extension` applies `name_to_key`; the extension manager applies
    /// `normalize` to the installed config entry's name before storing it. The
    /// two agree only on ASCII letters, digits, `_` and `-`. The generator
    /// refuses a name outside that set — this is the same invariant asserted
    /// from the consuming side, so a hand-edited key cannot reintroduce it.
    #[test]
    fn every_key_survives_both_reductions_unchanged() {
        for key in private_extension_ids() {
            assert_eq!(
                crate::config::extensions::name_to_key(key),
                key,
                "{key} is not already in name_to_key form, so classify_extension can never match it"
            );
            assert_eq!(
                crate::agents::normalize(key),
                key,
                "the extension manager would store {key} under a different name, \
                 so the tier lookup would miss it and the extension would classify Public"
            );
        }
    }

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

    // ----------------------------------------------------------------------
    // Task 43 / DR-23: the tier is re-derived from the registry, keyed on the
    // stable id the install recorded — never on the local config name alone.
    //
    // Every test below states its provenance through
    // `provenance::insert_test_record`, which is additive and keyed on a name
    // no other test uses, so these do not have to be serialised against each
    // other or against the file-backed store.
    // ----------------------------------------------------------------------

    /// A `.brxt` bundle whose installed name is not the registry id it came
    /// from — the situation `spokeagent-0.4.1` already puts the real catalogue
    /// in — is classified from the id, not from the name.
    #[test]
    fn an_install_name_that_differs_from_the_registry_id_is_still_private() {
        super::super::provenance::insert_test_record("clinical-0.5.1", "cdwagent");
        assert_eq!(
            classify_extension("clinical-0.5.1"),
            ProviderTier::Private,
            "the installed name is not the registry id, and the id is what decides"
        );
    }

    /// The registry may publish the id in either spelling the bundle uses, so
    /// the recorded id goes through the same `name_to_key` reduction the
    /// compiled set is stated in.
    #[test]
    fn a_recorded_id_is_reduced_the_same_way_the_set_is() {
        super::super::provenance::insert_test_record("clinical-0.5.2", "CDWAgent");
        assert_eq!(classify_extension("clinical-0.5.2"), ProviderTier::Private);
    }

    /// **The bug DR-23 exists to close, at the resolver.**
    ///
    /// ⚠ The rename changes BOTH the map key and the entry's own `name`, so a
    /// record found by key alone is not found at all — which is why the record
    /// carries the install directory and the resolver is handed the config. The
    /// name-only [`classify_extension`] therefore still answers Public here, and
    /// that asymmetry is asserted rather than described: it is the reason every
    /// gate had to move to `classify_extension_entry`.
    #[test]
    fn a_renamed_private_extension_is_still_private_through_its_install_directory() {
        use crate::agents::extension::{Envs, ExtensionConfig};
        let install_dir = "/home/researcher/.config/biorouter/extensions/CDWAgent";
        super::super::provenance::insert_test_record_at(
            "cdwagent-before-the-rename",
            "cdwagent",
            Some(install_dir),
        );
        let renamed = ExtensionConfig::Stdio {
            name: "mystuff".to_string(),
            description: "renamed by hand in config.yaml".to_string(),
            cmd: "uv".to_string(),
            args: vec![
                "run".to_string(),
                "--directory".to_string(),
                install_dir.to_string(),
                "server.py".to_string(),
            ],
            envs: Envs::default(),
            env_keys: vec![],
            timeout: Some(300),
            bundled: None,
            available_tools: vec![],
        };

        assert_eq!(
            classify_extension_entry("mystuff", Some(&renamed)),
            ProviderTier::Private,
            "renaming the entry moved the name, not the directory the install unpacked into"
        );
        assert_eq!(
            classify_extension("mystuff"),
            ProviderTier::Public,
            "the name-only form cannot see the install directory — a caller holding a config \
             must pass it, and this is why"
        );
    }

    /// The install-directory match is EXACT, over whole arguments. A config that
    /// merely mentions a similar path — a sibling directory, or the extensions
    /// root itself — must not inherit a neighbour's registry id.
    #[test]
    fn a_neighbouring_path_does_not_inherit_the_recorded_id() {
        use crate::agents::extension::{Envs, ExtensionConfig};
        let install_dir = "/home/researcher/.config/biorouter/extensions/ClinicalNeighbour";
        super::super::provenance::insert_test_record_at(
            "clinical-neighbour",
            "cdwagent",
            Some(install_dir),
        );
        let unrelated = ExtensionConfig::Stdio {
            name: "somethingelse".to_string(),
            description: "a different extension entirely".to_string(),
            cmd: "uv".to_string(),
            args: vec![
                "run".to_string(),
                "--directory".to_string(),
                format!("{install_dir}-v2"),
                "server.py".to_string(),
            ],
            envs: Envs::default(),
            env_keys: vec![],
            timeout: Some(300),
            bundled: None,
            available_tools: vec![],
        };
        assert_eq!(
            classify_extension_entry("somethingelse", Some(&unrelated)),
            ProviderTier::Public
        );
    }

    /// **Step 2, first direction.** Provenance may only RAISE. A record naming
    /// a public registry id cannot un-private a name the compiled set knows —
    /// otherwise writing one line into the provenance store would be a
    /// downgrade, and the whole point of re-deriving is that there is no stored
    /// value to forge.
    #[test]
    fn a_public_recorded_id_cannot_lower_a_private_name() {
        super::super::provenance::insert_test_record("cdwagent", "playwrightagent");
        assert_eq!(classify_extension("cdwagent"), ProviderTier::Private);
    }

    /// **Step 2, second direction.** A registry with no entry for the recorded
    /// id retains what is already known rather than defaulting to public.
    #[test]
    fn an_unknown_recorded_id_retains_the_name_derived_tier() {
        super::super::provenance::insert_test_record("ucsfomopagent", "an-id-nobody-publishes");
        assert_eq!(classify_extension("ucsfomopagent"), ProviderTier::Private);
    }

    /// **Step 0's admitted gap, asserted rather than described.** An extension
    /// installed before this task recorded no provenance at all, so it can only
    /// be joined the old way — on its config name. Renaming one of those still
    /// loses the tier, and the fallback is a documented fallback rather than a
    /// silent one.
    #[test]
    fn without_a_record_the_join_is_still_the_name_join() {
        assert_eq!(
            classify_extension("renamed-with-no-record"),
            ProviderTier::Public
        );
    }
}
