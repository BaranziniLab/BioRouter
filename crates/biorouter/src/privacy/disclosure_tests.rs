//! Task 30A (issue #56, DR-17 requirement 3): what a non-private model can
//! reach, said out loud.
//!
//! ⚠ **Its own file, and that is a gate requirement, not tidiness.** Step 5's
//! gate (4) greps `disclosure.rs` for provider NAMES and fails if it finds any —
//! the predicate must be the tier, never a second hand-written list — and these
//! tests necessarily name six providers. Step 5's gate (3) counts
//! `privacy_tiers_enabled` in the same file and expects zero. Both are satisfied
//! by construction if the production module holds no tests and no test lives
//! inside it.
//!
//! ⚠ **The toggle-independence row is NOT here.** It mutates the process-global
//! master switch, and `cargo test` runs a crate's unit tests in parallel threads
//! of ONE process — the same hazard `crates/biorouter/tests/privacy_toggle.rs`
//! documents at length. It lives in `crates/biorouter/tests/
//! privacy_disclosure_toggle.rs`, its own process, where its writes can reach
//! nothing else.

use super::disclosure;
use crate::privacy::ProviderTier;
use crate::providers::base::ProviderMetadata;

/// The metadata a freshly-installed Biorouter publishes for `name` — the real
/// registry entry, which is exactly what every UI surface reads. A name with no
/// entry gets the fail-safe default (Public), which is the whole point of the
/// default being Public.
async fn meta(name: &str) -> ProviderMetadata {
    crate::providers::providers()
        .await
        .into_iter()
        .find(|(m, _)| m.name == name)
        .map(|(m, _)| m)
        .unwrap_or_else(|| panic!("no registry entry for `{name}`"))
}

#[test]
fn the_disclosure_names_all_three_properties_the_ruling_names() {
    // Not a spelling test. The operator's sentence has three conditions —
    // "not HIPAA compliant", "not hosted on-premise", "not local" — and a copy
    // edit that drops one turns a specific warning into a vague one. Each is
    // asserted separately so the failure names which was lost.
    let c = disclosure::COPY_LONG;
    assert!(c.contains("HIPAA"), "{c}");
    assert!(c.contains("on-premise") || c.contains("on premises"), "{c}");
    assert!(c.to_lowercase().contains("local"), "{c}");
    // …and the thing at risk is named concretely, not as "your data".
    assert!(c.contains("files on this computer"), "{c}");
}

#[test]
fn the_disclosure_states_the_limit_of_the_protection_not_only_the_protection() {
    // DR-17's honest consequence, in the copy: the barrier stops the
    // agent-mediated path, the transcript path and tier escalation. It does NOT
    // make the machine opaque. A copy that omits this is the failure mode this
    // whole task exists to prevent.
    let c = disclosure::COPY_LONG;
    assert!(c.contains("does not"), "{c}");
    assert!(c.contains("shell") || c.contains("read files"), "{c}");
}

#[tokio::test]
async fn only_a_model_that_is_none_of_the_three_triggers_it() {
    assert!(disclosure::required_for(&meta("openai").await)); // public
    assert!(disclosure::required_for(&meta("anthropic").await)); // public
    assert!(!disclosure::required_for(&meta("llamacpp").await)); // local
    assert!(!disclosure::required_for(&meta("ollama").await)); // local
    assert!(!disclosure::required_for(&meta("versa_azure").await)); // institutional
    #[cfg(feature = "aws-providers")]
    assert!(!disclosure::required_for(&meta("versa_bedrock").await)); // institutional

    // The predicate is the tier, not a second list. A provider added to the
    // private set in Task 5 must stop triggering this with no edit in
    // `disclosure.rs`.
    //
    // ⚠ Written as an equivalence over the WHOLE registry rather than as the
    // plan's one-provider `matches!` line, which does not compile (`t` is bound
    // and then compared to itself) and which, written correctly, would only
    // restate the `versa_azure` assertion above. This is the assertion that
    // actually forbids a second list: every registered provider's answer has to
    // be its tier's answer, so a hardcoded name would have to be right about
    // every one of them and would still fail the moment Task 5's set moved.
    for (m, _) in crate::providers::providers().await {
        assert_eq!(
            disclosure::required_for(&m),
            m.tier != ProviderTier::Private,
            "`{}` — the disclosure predicate disagreed with the tier",
            m.name
        );
    }
}

#[test]
fn the_short_form_says_the_same_three_things_in_one_line() {
    // The chip tooltip and the CLI print the short form, and a short form that
    // dropped a condition would be the drifted copy a user actually reads —
    // which is the whole reason there is one definition rather than four.
    let c = disclosure::COPY_SHORT;
    assert!(c.contains("HIPAA"), "{c}");
    assert!(c.contains("on-premise"), "{c}");
    assert!(c.to_lowercase().contains("local"), "{c}");
    assert!(c.contains("read files on this computer"), "{c}");
}

#[test]
fn the_title_names_the_provider_and_does_not_leave_the_placeholder_behind() {
    let title = disclosure::title_for("OpenAI");
    assert!(title.starts_with("OpenAI"), "{title}");
    assert!(title.contains("not hosted by your institution"), "{title}");
    assert!(!title.contains('{'), "{title}");
}

#[test]
fn the_acknowledgement_is_recorded_once_and_survives_a_restart() {
    // Once per install, not once per session: a dialog on every chat is clicked
    // through, which is exactly the outcome this task exists to avoid. The
    // record is a file, so "a restart" is a second read of the same root.
    let dir = tempfile::TempDir::new().unwrap();
    let config_dir = dir.path();
    assert!(!disclosure::is_acknowledged_in(config_dir));
    disclosure::record_acknowledgement_in(config_dir).unwrap();
    assert!(disclosure::is_acknowledged_in(config_dir));
    // Idempotent: a second acknowledgement is not an error and does not undo the
    // first.
    disclosure::record_acknowledgement_in(config_dir).unwrap();
    assert!(disclosure::is_acknowledged_in(config_dir));
}

/// The record is swapped into place, never truncated in place.
///
/// `fs::write` opens with `truncate`, so between the truncate and the write the
/// file on disk is empty — and `is_acknowledged_in` reads an empty file as *not
/// acknowledged*, i.e. a record that WAS written momentarily reports the
/// opposite. Two panes acknowledging at once (the split view mounts one gate
/// each) is enough to reach it, and the same window is what leaves a malformed
/// record behind if the process dies mid-write.
///
/// Staging in a sibling file and renaming closes it: within a directory a rename
/// is atomic, so the record path is only ever *absent* or *complete*. The
/// mechanism is asserted through the inode, because that is what distinguishes
/// the two implementations without racing them — a truncate-in-place keeps the
/// inode, a swap replaces it.
#[cfg(unix)]
#[test]
fn the_record_is_swapped_into_place_not_truncated_in_place() {
    use std::os::unix::fs::MetadataExt;

    let dir = tempfile::TempDir::new().unwrap();
    let config_dir = dir.path();
    let record = config_dir.join(disclosure::ACK_FILE_NAME);

    disclosure::record_acknowledgement_in(config_dir).unwrap();
    let first = std::fs::metadata(&record).unwrap().ino();

    disclosure::record_acknowledgement_in(config_dir).unwrap();
    let second = std::fs::metadata(&record).unwrap().ino();

    assert_ne!(
        first, second,
        "the acknowledgement was rewritten in place, so a reader can observe it \
         empty between the truncate and the write"
    );
    assert!(disclosure::is_acknowledged_in(config_dir));

    // …and the staging file does not outlive the write. It sits in the user's
    // config directory, so one left behind per acknowledgement is litter in a
    // directory the user reads.
    let leftovers: Vec<String> = std::fs::read_dir(config_dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name != disclosure::ACK_FILE_NAME)
        .collect();
    assert!(leftovers.is_empty(), "left behind: {leftovers:?}");
}

#[test]
fn an_unreadable_record_reads_as_not_yet_acknowledged() {
    // Fail-safe here means fail TOWARDS disclosing. The cost of showing the
    // dialog twice is an annoyance; the cost of skipping it is the
    // misrepresentation DR-17 forbids.
    let dir = tempfile::TempDir::new().unwrap();
    let config_dir = dir.path();
    std::fs::write(
        config_dir.join(disclosure::ACK_FILE_NAME),
        b"\x00not json at all",
    )
    .unwrap();
    assert!(!disclosure::is_acknowledged_in(config_dir));
}
