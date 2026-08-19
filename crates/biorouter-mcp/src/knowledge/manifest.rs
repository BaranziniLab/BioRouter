use crate::knowledge::types::Manifest;
use anyhow::{Context, Result};
use std::path::Path;

const MANIFEST_FILE: &str = "manifest.yaml";

pub fn manifest_path(kb_root: &Path) -> std::path::PathBuf {
    kb_root.join(MANIFEST_FILE)
}

pub fn load(kb_root: &Path) -> Result<Manifest> {
    let p = manifest_path(kb_root);
    let s = std::fs::read_to_string(&p).with_context(|| format!("reading {}", p.display()))?;
    Ok(serde_yaml::from_str(&s)?)
}

pub fn save(kb_root: &Path, m: &Manifest) -> Result<()> {
    std::fs::create_dir_all(kb_root)?;
    let yaml = serde_yaml::to_string(m)?;
    let tmp = manifest_path(kb_root).with_extension("yaml.tmp");
    std::fs::write(&tmp, yaml)?;
    std::fs::rename(tmp, manifest_path(kb_root))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::types::{KbFormat, AUTOMATIC_SCHEMA_CEILING, CURRENT_SCHEMA_VERSION};
    use chrono::TimeZone;

    /// The legacy shape: exactly the six keys every `manifest.yaml` on disk
    /// has, with Stage 3's three new keys at the values a file that does not
    /// mention them must read back as.
    fn sample() -> Manifest {
        Manifest {
            id: "ms".into(),
            name: "MS Patient Analysis".into(),
            color: "#5a6394".into(),
            created_at: chrono::Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
            schema_version: 1,
            default_model: None,
            format: KbFormat::Okf,
            okf_version: None,
            biookf_version: None,
        }
    }

    #[test]
    fn roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        save(dir.path(), &sample()).unwrap();
        assert_eq!(load(dir.path()).unwrap(), sample());
    }

    /// The exact bytes on every `manifest.yaml` written to date. DR-12's (c):
    /// this is the file the next added field must not break, and the only way
    /// to know it does not is to keep a copy of it here that no future writer
    /// can quietly regenerate.
    const V1_ON_DISK: &str = "id: ms\n\
         name: MS Patient Analysis\n\
         color: '#5a6394'\n\
         created_at: 2023-11-14T22:13:20Z\n\
         schema_version: 1\n";

    #[test]
    fn a_manifest_with_only_todays_keys_still_loads() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(manifest_path(dir.path()), V1_ON_DISK).unwrap();
        assert_eq!(load(dir.path()).unwrap(), sample());
    }

    #[test]
    fn all_six_of_todays_keys_load_including_the_optional_one() {
        // `default_model` is the only field a manifest may legitimately omit,
        // so the five-key file above and this six-key one are both shapes on
        // disk right now. Stage 3 adds a seventh, and it must break neither.
        let dir = tempfile::tempdir().unwrap();
        let with_model =
            format!("{V1_ON_DISK}default_model:\n  provider: anthropic\n  model: claude-opus-5\n");
        std::fs::write(manifest_path(dir.path()), with_model).unwrap();
        let m = load(dir.path()).unwrap();
        assert_eq!(
            m.default_model.as_ref().map(|r| r.model.as_str()),
            Some("claude-opus-5")
        );
        assert_eq!(m.schema_version, 1);
    }

    #[test]
    fn a_manifest_missing_schema_version_loads_at_the_oldest_generation() {
        // Not 0. A missing key means "written before anyone was counting", and
        // the migration ladder has to run for such a base, not skip it.
        let dir = tempfile::tempdir().unwrap();
        let without = V1_ON_DISK.replace("schema_version: 1\n", "");
        std::fs::write(manifest_path(dir.path()), &without).unwrap();
        assert_eq!(load(dir.path()).unwrap().schema_version, 1);
    }

    #[test]
    fn a_manifest_missing_created_at_loads_at_the_epoch_not_at_now() {
        // `Utc::now()` as the default would invent a fact and re-invent it on
        // every read, so the base would sort as the newest one in the list
        // forever. The epoch is honest about not knowing.
        let dir = tempfile::tempdir().unwrap();
        let without = V1_ON_DISK.replace("created_at: 2023-11-14T22:13:20Z\n", "");
        std::fs::write(manifest_path(dir.path()), &without).unwrap();
        let m = load(dir.path()).unwrap();
        assert_eq!(m.created_at, chrono::DateTime::UNIX_EPOCH);
        assert_eq!(m.id, "ms", "the rest of the manifest still read");
    }

    #[test]
    fn an_unknown_future_key_is_not_a_load_failure() {
        // What a downgrade looks like: a manifest written by a build that knew
        // about a key this one has never heard of.
        let dir = tempfile::tempdir().unwrap();
        let with_future = format!("{V1_ON_DISK}attestations: []\n");
        std::fs::write(manifest_path(dir.path()), with_future).unwrap();
        assert_eq!(load(dir.path()).unwrap(), sample());
    }

    // -----------------------------------------------------------------------
    // Stage 3: `format`, `okf_version`, `biookf_version` (DR-6 / DR-12)
    // -----------------------------------------------------------------------

    /// DR-12 (c), for the three keys Stage 3 adds. The file above is the exact
    /// bytes on every `manifest.yaml` written to date and it mentions none of
    /// them; it must still load, and it must read back as the legacy shape.
    ///
    /// The cascade this guards is silent and ends in *persisted* data loss —
    /// `list_bases` drops the base, its id leaves the installed universe,
    /// `repair_decision` clears the stored primary and `apply_selection_unlocked`
    /// writes the cleared pointer to disk — so a load failure here would not
    /// surface as an error, it would surface as the user's knowledge bases
    /// disappearing.
    #[test]
    fn a_manifest_with_none_of_the_stage_3_keys_still_loads_as_the_legacy_shape() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(manifest_path(dir.path()), V1_ON_DISK).unwrap();
        let m = load(dir.path()).unwrap();
        assert_eq!(m, sample(), "the whole struct, not just the new keys");
        assert_eq!(m.format, KbFormat::Okf, "the field defaults");
        assert_eq!(m.okf_version, None, "and declares no revision");
        assert_eq!(m.biookf_version, None);
        // The load succeeding is only half of it: the base must still read as
        // legacy, or DR-6's trap has been walked into at the reader instead of
        // at the writer.
        assert_eq!(
            m.profile(),
            None,
            "a v1 manifest must not read as an OKF base merely because `format` \
             defaults to okf"
        );
        assert!(m.is_legacy_format());
    }

    /// The same file at the generation every base on disk is stamped to by the
    /// automatic ladder. Still legacy: the ceiling is below the OKF generation
    /// precisely so this cannot come out any other way.
    #[test]
    fn a_manifest_at_the_ladder_ceiling_is_still_legacy() {
        let dir = tempfile::tempdir().unwrap();
        let at_ceiling = V1_ON_DISK.replace(
            "schema_version: 1\n",
            &format!("schema_version: {AUTOMATIC_SCHEMA_CEILING}\n"),
        );
        std::fs::write(manifest_path(dir.path()), at_ceiling).unwrap();
        let m = load(dir.path()).unwrap();
        assert_eq!(m.schema_version, AUTOMATIC_SCHEMA_CEILING);
        assert_eq!(m.profile(), None);
    }

    #[test]
    fn an_okf_generation_manifest_round_trips_with_both_versions() {
        let dir = tempfile::tempdir().unwrap();
        let m = Manifest {
            schema_version: CURRENT_SCHEMA_VERSION,
            format: KbFormat::Biookf,
            okf_version: Some("0.2".into()),
            biookf_version: Some("0.5".into()),
            ..sample()
        };
        save(dir.path(), &m).unwrap();
        assert_eq!(load(dir.path()).unwrap(), m);

        let yaml = std::fs::read_to_string(manifest_path(dir.path())).unwrap();
        assert!(yaml.contains("format: biookf"), "{yaml}");
        // Quoted, or YAML resolves it to the float 0.2 and a later 0.10 would
        // silently become 0.1 — a revision that sorts BELOW 0.2.
        assert!(yaml.contains("okf_version: '0.2'"), "{yaml}");
        assert!(yaml.contains("biookf_version: '0.5'"), "{yaml}");
        assert_eq!(load(dir.path()).unwrap().profile(), Some(KbFormat::Biookf));
    }

    /// The two optional revision keys are omitted rather than written as
    /// `null`, so a legacy manifest that is re-saved (the schema ladder stamps
    /// one forward on the first macro call) does not gain keys claiming an OKF
    /// revision it does not have.
    #[test]
    fn re_saving_a_legacy_manifest_does_not_invent_an_okf_revision() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(manifest_path(dir.path()), V1_ON_DISK).unwrap();
        let mut m = load(dir.path()).unwrap();
        m.schema_version = AUTOMATIC_SCHEMA_CEILING;
        save(dir.path(), &m).unwrap();

        let yaml = std::fs::read_to_string(manifest_path(dir.path())).unwrap();
        assert!(!yaml.contains("okf_version"), "{yaml}");
        assert!(!yaml.contains("biookf_version"), "{yaml}");
        assert_eq!(load(dir.path()).unwrap().profile(), None);
    }

    /// An unknown profile word must not be able to make a base vanish.
    ///
    /// `list_bases` still *drops* a base whose manifest will not parse (it logs
    /// the path rather than dropping it silently, which is all S-b promised), so
    /// a strict enum here would put the DR-12 cascade one typo away: the id
    /// leaves the installed universe and the next selection edit persists a
    /// cleared `.active-kb`. Reading the word as plain OKF loses constraints,
    /// never content.
    #[test]
    fn an_unknown_format_word_reads_as_plain_okf_rather_than_losing_the_base() {
        let dir = tempfile::tempdir().unwrap();
        // Both shapes: a typo, and a profile a *later* build invented that this
        // one has never heard of.
        for word in ["biokf", "okf-lite-2027"] {
            let bad = format!("{V1_ON_DISK}format: {word}\n");
            std::fs::write(manifest_path(dir.path()), bad).unwrap();
            let m = load(dir.path()).unwrap_or_else(|e| panic!("`{word}` cost us the base: {e:#}"));
            assert_eq!(m.format, KbFormat::Okf);
            assert_eq!(m.id, "ms", "the rest of the manifest still read");
        }
    }

    #[test]
    fn save_uses_atomic_rename() {
        let dir = tempfile::tempdir().unwrap();
        save(dir.path(), &sample()).unwrap();
        let listing: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert!(listing.iter().any(|n| n == "manifest.yaml"));
        assert!(!listing.iter().any(|n| n == "manifest.yaml.tmp"));
    }
}
