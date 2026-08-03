//! What a non-private model can reach, said out loud (issue #56, DR-17
//! requirement 3).
//!
//! The operator descoped the general filesystem barrier *"for now"* and, in the
//! same breath, required that users *"understand the risks of using non-private
//! models"* — that a model which is not HIPAA-compliant, not hosted on-premise
//! and not local *"can potentially gather information from them"*. An accepted
//! risk the user is told about is a tradeoff the user makes; the same risk
//! undisclosed is a misrepresentation. This module is the term on which DR-17's
//! accepted risks are acceptable.
//!
//! Three properties of this module are load-bearing, and each has a Step 5 gate:
//!
//! 1. **It does not read the master switch.** DR-15 turns off gates, the ratchet
//!    and refusals; it does not turn off the truth, and with enforcement off the
//!    exposure is *larger*, not smaller. Wiring the disclosure behind the master
//!    switch is the plausible wrong implementation — every other privacy surface
//!    reads it — and it would silence this one in exactly the configuration
//!    where the risk is highest. Nothing below consults it, and a gate counts
//!    the token in this file and expects none.
//!
//! 2. **One copy, one definition, served to every surface.** The sentence exists
//!    in the GUI dialog, the settings panel, the provider grid, the model chip,
//!    the CLI, `docs/` and the landing site. Four hand-written copies drift
//!    within one release and the drifted one is always the one a user reads. The
//!    text lives here; the renderer fetches it over `GET /privacy/disclosure`
//!    and the CLI prints these constants directly.
//!
//! 3. **The predicate is the tier, not a third list.** [`required_for`] is
//!    `metadata.tier != Private`. Task 5 owns the membership and DR-1 owns the
//!    rule, so a fourth private provider must switch this off with no edit here.
//!    A hand-written provider list in this file is the wrong implementation a
//!    gate greps for, which is also why this module's tests live beside it in
//!    `disclosure_tests.rs` rather than inside it.
//!
//! **Why once per install rather than once per session.** A confirmation a user
//! sees daily is a confirmation they stop reading, and this one has no *action*
//! to gate — there is no safe alternative being offered at that moment, only a
//! fact to convey. So it is shown once, forcefully, and is then carried
//! permanently by a badge, a section header and a settings panel anyone can go
//! back to. That is the same reasoning DR-8 uses for grading a session's
//! declassification rather than confirming every read.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::ProviderTier;
use crate::config::paths::Paths;
use crate::providers::base::ProviderMetadata;

/// The dialog's heading, with the provider's display name interpolated by
/// [`title_for`]. A template rather than a formatted constant so the *renderer*
/// never has to know the English around the name.
pub const COPY_TITLE_TEMPLATE: &str = "{provider} is not hosted by your institution.";

/// The long form — the blocking dialog, the settings panel, `docs/`, the landing
/// site.
///
/// ⚠ **It says what is *not* protected, and it says it first.** The temptation
/// is to lead with the guarantee — locked transcripts, no private extensions —
/// because that is the flattering half. A user who reads only the guarantee
/// concludes the machine is opaque to a public model, which is precisely what
/// DR-17 says it must not imply. The order is: what this model can reach → what
/// Biorouter does stop → what to use instead.
///
/// Paragraphs are separated by a blank line; every surface that renders this
/// splits on `\n\n` rather than shipping its own prose.
pub const COPY_LONG: &str = "\
It is not HIPAA-compliant, is not hosted on-premise, and does not run on this machine. \
It can read files on this computer — anything a chat on this model can reach, it can send \
there: the contents of your working directory, and whatever a command you approve prints.

Biorouter does stop three things: this model cannot read another chat's transcript, cannot \
read a knowledge base marked private, and cannot use the private data extensions (UCSF OMOP, \
CDW) or switch this chat to a private model to reach them.

It does not stop it reading ordinary files on this computer through the shell — including \
files an earlier private chat wrote outside Biorouter's own storage. If the work involves \
patient data, use a local model or an institutional one.";

/// The one-line form — the model chip's Public badge tooltip, the provider
/// grid's Commercial section, the CLI's print on selecting a public provider.
///
/// It carries all three of the ruling's conditions, because a short form that
/// dropped one would be the drifted copy a user actually reads.
pub const COPY_SHORT: &str = "Not HIPAA-compliant, not on-premise, not local — this model can \
read files on this computer. Private chats and private knowledge bases stay out of its reach.";

/// The dialog heading for `provider_display_name`.
pub fn title_for(provider_display_name: &str) -> String {
    COPY_TITLE_TEMPLATE.replace("{provider}", provider_display_name)
}

/// Must this provider's user be told what it can reach?
///
/// The tier, and only the tier. `Private` covers both of the reasons a model is
/// safe to point at patient data — it runs on this machine, or it is a
/// recognised institutional endpoint — so `Public` is exactly "none of the
/// three", which is the operator's own condition.
pub fn required_for(metadata: &ProviderMetadata) -> bool {
    required_for_tier(metadata.tier)
}

/// The same predicate for a caller that already holds a tier — a live
/// [`crate::providers::base::Provider`] instance, or a tier read off the wire.
pub const fn required_for_tier(tier: ProviderTier) -> bool {
    !tier.is_private()
}

/// The acknowledgement record's filename, inside the config directory.
pub const ACK_FILE_NAME: &str = "privacy-disclosure-ack.json";

/// What is written when the user acknowledges. A timestamp rather than a bare
/// flag: "when were they told" is the question anyone auditing this will ask,
/// and an empty file cannot answer it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Acknowledgement {
    /// RFC 3339, in UTC.
    pub acknowledged_at: String,
}

fn ack_path_in(config_dir: &Path) -> PathBuf {
    config_dir.join(ACK_FILE_NAME)
}

/// Has the user acknowledged the disclosure on this install?
///
/// ⚠ **Fail-safe means fail towards disclosing.** An absent, unreadable or
/// malformed record reads `false`. The cost of showing the dialog a second time
/// is an annoyance; the cost of skipping it is the misrepresentation DR-17
/// forbids.
pub fn is_acknowledged_in(config_dir: &Path) -> bool {
    let Ok(raw) = std::fs::read_to_string(ack_path_in(config_dir)) else {
        return false;
    };
    serde_json::from_str::<Acknowledgement>(&raw).is_ok()
}

/// Record the acknowledgement. Idempotent — re-acknowledging rewrites the
/// timestamp and is not an error.
///
/// ⚠ **Staged and renamed, never written in place.** `fs::write` opens with
/// `truncate`, so between the truncate and the write the record on disk is
/// empty — and [`is_acknowledged_in`] reads an empty record as *not
/// acknowledged*, which is the fail-safe polarity turning a written record into
/// a false negative. Two panes acknowledging at once reach that window, and a
/// process that dies inside it leaves a malformed record behind for good. A
/// rename within one directory is atomic, so the record path is only ever absent
/// or complete.
pub fn record_acknowledgement_in(config_dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(config_dir)?;
    let record = Acknowledgement {
        acknowledged_at: chrono::Utc::now().to_rfc3339(),
    };
    let body = serde_json::to_string_pretty(&record)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    // Named per process, so two Biorouter processes acknowledging at the same
    // moment stage into different files and each rename is a complete record.
    let staging = config_dir.join(format!("{ACK_FILE_NAME}.{}.tmp", std::process::id()));
    std::fs::write(&staging, body)?;
    match std::fs::rename(&staging, ack_path_in(config_dir)) {
        Ok(()) => Ok(()),
        Err(e) => {
            // Do not leave the staging file in the user's config directory.
            let _ = std::fs::remove_file(&staging);
            Err(e)
        }
    }
}

/// [`is_acknowledged_in`] against this install's real config directory.
pub fn is_acknowledged() -> bool {
    is_acknowledged_in(&Paths::config_dir())
}

/// [`record_acknowledgement_in`] against this install's real config directory.
///
/// ⚠ **This is a USER act, and the only caller that may reach it over HTTP is
/// behind DR-16's proof-of-user.** A model that could acknowledge on the user's
/// behalf would silently remove the only thing making DR-17's accepted risks
/// acceptable. The residual is written down rather than hidden: DR-17 descoped
/// the general filesystem barrier, so an agent holding `developer__shell` can
/// still write this file directly. That is the same residual every other
/// config-dir file carries, and it is the reason the disclosure is *also*
/// carried permanently by surfaces that read no record at all — the badge
/// tooltip, the provider-grid section, the settings panel.
pub fn record_acknowledgement() -> std::io::Result<()> {
    record_acknowledgement_in(&Paths::config_dir())
}
