//! The cross-institution **mixing policy** (issue #56, DR-27) — three named
//! modes, and the ratchet that makes leaving the strict one cost something.
//!
//! [DR-26] shipped exactly one answer to a cross-institutional mismatch: refuse,
//! and let the user clear it with one in-app confirmation. That is the right
//! *default* and the wrong *only option*. A lab doing sanctioned multi-site work
//! under a DUA meets a prompt it has already papered — and a control people
//! click through is not a control (DR-19). A clinical deployment wants the
//! barrier to cost something real. DR-27 names the three:
//!
//! | mode | public ↔ private | institution mixing | proof to mix |
//! | --- | --- | --- | --- |
//! | [`MixingPolicy::Open`] | still enforced | allowed **silently** | none |
//! | [`MixingPolicy::Standard`] ← default | still enforced | allowed after a prompt | in-app `X-User-Action` |
//! | [`MixingPolicy::Strict`] | still enforced | allowed after a prompt | **system password** |
//!
//! ⚠ **All three keep the public/private barrier.** This setting governs the
//! **affiliation** axis and nothing else. Turning the tier system off is what the
//! master switch does, it already exists, and this module must not duplicate or
//! weaken it — pinned by `capability.rs`'s `open_is_not_the_master_switch`, which
//! drives a PUBLIC model at a private extension in `open` and requires the
//! refusal to stand.
//!
//! # Why the value does not live in `config.yaml`
//!
//! It goes where [DR-22] put the master switch, for the reason DR-22 gives:
//! DR-17 left `config.yaml` agent-writable, so a setting stored there is one a
//! public model can set to `open`, erasing every affiliation check on the next
//! launch. This is a security control wearing a preference's clothes, and it is
//! stored like one — its own record beside `config.yaml`, written through one
//! door that demands proof of a human ([`UserMixingPolicyChange`]).
//!
//! The bound is [`super::master_switch`]'s, unchanged and not restated as
//! anything better: this closes the *config key*, not the *file*. An agent
//! holding `developer__shell` can still write the record directly, exactly as it
//! can write the master switch's. Closing that needs the filesystem barrier
//! DR-17 deferred or an OS-authenticated store, and neither is in v1.
//!
//! # The asymmetry: loosening costs, tightening is free
//!
//! [`loosens`] compares the **ordering of the modes**, never "is this a write".
//! A guard that prompted for every change would make tightening expensive, which
//! is the direction that matters: requiring the password to *leave* `strict` is
//! the whole point of `strict`, and requiring it to *enter* `strict` would be a
//! control that punishes the careful user. It is the same ratchet classification
//! and the knowledge-base tier already have — raising is free, lowering is gated.
//!
//! # What `open` does, and what it deliberately does not
//!
//! `open` is **not** implemented by granting everything. The compatibility check
//! still runs, still resolves, and the result is still available for display:
//! [`super::affiliation::gate_cross_affiliation`] and
//! [`super::CallCapability::cross_affiliation`] answer identically in all three
//! modes. Only the spellings a **refusal** is stated in —
//! [`super::CallCapability::cross_affiliation_warning`], its free twin, and the
//! agent's bind-time statement — go quiet, through the single GATE read in
//! [`super::affiliation::refusing_mismatch`]. A mode that short-circuited the
//! resolver would blind the badges and the audit trail, and would leave
//! `open → standard` with nothing to re-tighten.
//!
//! ⚠ **"One read" means one read ON THE GATE'S PATH**, and the distinction is
//! worth stating because two other things do ask [`policy`]: the grant route's
//! `strict` layer (through [`grant_needs_system_authentication`]) and the two
//! `/config` read arms that report the setting back to the panel. Neither decides
//! whether a mismatch refuses, which is the question that must have exactly one
//! answer — a second gate reading a mode is a tool marked at discovery and then
//! dispatched.
//!
//! ⚠ **The residual, recorded here rather than left to be discovered.** The one
//! read is on the *extension* axis — dispatch (Gate C), the eight non-tool-call
//! entry points, the agent's own enable path, and the subagent spawn's filter.
//! The **union-of-owners** surfaces built on
//! [`super::affiliation::owners_compatible`] — chat recall, the knowledge base's
//! conversation ingest, and the SQL half of chat-history search — do NOT consult
//! this setting, and neither does `biorouter-mcp`'s knowledge-base barrier, which
//! cannot see this crate. In `open` those still refuse. That is the safe
//! direction (a user in `open` meets a refusal they did not expect, rather than a
//! disclosure they did not accept), and it is narrower than DR-27 asks for.
//! Closing it means threading the policy across the crate boundary the way the
//! master toggle is, plus expressing it in the search's SQL; it is a task, not a
//! line to add here.
//!
//! [DR-17]: super::disclosure
//! [DR-22]: super::master_switch
//! [DR-26]: super::affiliation

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};

use serde::{Deserialize, Serialize};

use super::system_auth::{self, AuthRequest, SystemAuthRefusal, SystemAuthenticator};
use crate::config::Config;

/// The record's filename, beside `config.yaml` and beside the master switch's.
pub const POLICY_FILE_NAME: &str = "privacy-mixing-policy.json";

/// The key the mixing policy is addressed by over `/config/upsert`.
///
/// ⚠ **It is a name on the wire and never a place the value is STORED** — the
/// same distinction [`super::PRIVACY_TIERS_CONFIG_KEY`] carries after DR-22.
/// Nothing in this tree reads it out of `config.yaml`, and nothing may start to:
/// a reader there would hand an agent-writable file authority over the setting,
/// which is the whole reason DR-27 put it here.
pub const MIXING_POLICY_CONFIG_KEY: &str = "BIOROUTER_PRIVACY_MIXING_POLICY";

/// What the user is told `open` actually does.
///
/// ⚠ **DR-27 requires this sentence to be modest.** `open` is not the master
/// switch and must not read like it: it makes cross-institution reach silent and
/// changes nothing else. Copy that implied more would recruit users into turning
/// off a control they meant to keep.
pub const OPEN_DISCLOSURE: &str =
    "Open: Biorouter stops asking before data crosses between institutions — those flows happen \
     silently. Nothing else changes: private chats, private models and private extensions are \
     still separated from public ones exactly as before.";

/// The three modes, ordered from most permissive to most restrictive.
///
/// ⚠ **`Ord` is derived on purpose, and the variant order is the semantics.**
/// [`super::ProviderTier`] refuses `Ord` because `max` over it is always a bug;
/// here the ordering *is* the ruling — DR-27's guard is a comparison of modes,
/// not a test for "is this a write" — so reordering these variants silently
/// inverts which direction costs a password. Pinned by
/// `the_three_modes_are_ordered_from_open_to_strict`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MixingPolicy {
    /// A cross-institution mismatch resolves, is reported for display, and does
    /// not refuse.
    Open,
    /// Today's behaviour: it refuses, and one in-app `X-User-Action` grant
    /// clears it.
    Standard,
    /// It refuses, and clearing it takes the operating system's own
    /// authentication as well as the in-app grant.
    Strict,
}

impl Default for MixingPolicy {
    /// `standard`, so no existing install changes behaviour on upgrade.
    fn default() -> Self {
        Self::Standard
    }
}

impl MixingPolicy {
    /// The stored and wire spelling. A stable string rather than a serde
    /// encoding of the enum, for the reason `grant::model_key` gives: this value
    /// is written to disk by one build and read by the next.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Standard => "standard",
            Self::Strict => "strict",
        }
    }

    /// The inverse of [`Self::as_str`]. `None` for anything else — **never a
    /// default**, because a typo that resolved to a mode would either loosen a
    /// machine nobody asked to loosen or tighten one and be blamed on a bug.
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "open" => Some(Self::Open),
            "standard" => Some(Self::Standard),
            "strict" => Some(Self::Strict),
            _ => None,
        }
    }
}

impl std::fmt::Display for MixingPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Is this key the mixing policy? **One predicate, every verb** — the argument
/// [`super::is_privacy_tiers_key`] makes, for the same reason: a rule that holds
/// for `POST /config/upsert` and not for `POST /config/remove` is a door with one
/// lock.
pub fn is_mixing_policy_key(key: &str) -> bool {
    key == MIXING_POLICY_CONFIG_KEY
}

/// The mode a `/config/upsert` body is asking for, or `None` if it is not asking
/// for one of the three.
///
/// Deliberately stricter than [`super::privacy_tiers_value_is_on`], which
/// resolves an unrecognised shape to the safe direction. There is no safe
/// direction here — the two wrong answers fail opposite ways — so an
/// unrecognised value is refused rather than guessed at.
pub fn mixing_policy_value(value: &serde_json::Value) -> Option<MixingPolicy> {
    match value {
        serde_json::Value::String(text) => MixingPolicy::parse(text),
        _ => None,
    }
}

/// Is moving `from` → `to` a **loosening**? DR-27's direction-aware guard.
///
/// ⚠ **The comparison is on the ordering of the modes, not on "is this a
/// write".** A guard that prompted for every change makes tightening expensive
/// and is wrong in the direction that matters: `strict → standard → open` is
/// what costs the system password, `open → standard → strict` costs nothing, and
/// standing still costs nothing either.
pub fn loosens(from: MixingPolicy, to: MixingPolicy) -> bool {
    to < from
}

// ---------------------------------------------------------------------------
// The store — DR-22's directory, DR-22's write discipline.
// ---------------------------------------------------------------------------

/// What is written when the policy moves.
///
/// The timestamp is beside the mode for the reason
/// [`super::master_switch::MasterSwitchRecord`] carries one: "when did this
/// machine stop asking" is the first question an incident review asks, and a bare
/// mode cannot answer it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MixingPolicyRecord {
    pub policy: MixingPolicy,
    /// RFC 3339, in UTC. `default` so a hand-written `{"policy": "strict"}` still
    /// reads — this field is an audit aid, not a checksum.
    #[serde(default)]
    pub changed_at: String,
}

/// The directory the record lives in: the one holding `config.yaml`.
///
/// Derived from [`Config::path`] rather than from
/// [`crate::config::paths::Paths::config_dir`], for the reason
/// [`super::master_switch`] documents at length — the two disagree in any
/// process where `BIOROUTER_PATH_ROOT` moves after the config was first touched,
/// which is every integration-test binary in this tree.
fn dir_of(config: &Config) -> PathBuf {
    Path::new(&config.path())
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Where the record is, for a given configuration directory.
pub fn path_in(config_dir: &Path) -> PathBuf {
    config_dir.join(POLICY_FILE_NAME)
}

/// The recorded mode, or `None` if this install has not recorded one.
///
/// ⚠ **Absent, unreadable and malformed all read `None`, and every caller
/// resolves that to [`MixingPolicy::Standard`].** The fail-safe direction here is
/// the *default*, not the strictest mode: resolving a corrupt byte to `strict`
/// would strand a machine behind a password prompt nobody chose, and resolving it
/// to `open` would make corrupting one file a way to silence the control. The
/// middle is the only answer that is neither.
///
/// A malformed record is deliberately not repaired, for
/// [`super::master_switch::read_in`]'s reason: rewriting a file this function is
/// only supposed to read would make a reader into a second writer.
pub fn read_in(config_dir: &Path) -> Option<MixingPolicy> {
    let raw = std::fs::read_to_string(path_in(config_dir)).ok()?;
    serde_json::from_str::<MixingPolicyRecord>(&raw)
        .ok()
        .map(|record| record.policy)
}

/// [`read_in`], for the directory a given [`Config`] lives in.
pub fn read_for(config: &Config) -> Option<MixingPolicy> {
    read_in(&dir_of(config))
}

/// Record the mode.
///
/// ⚠ **Private, and that is the control.** [`super::master_switch`] exposes its
/// writer because its one caller lives in another crate and reaches it after its
/// own guard; this one does not, so the only way to move the policy from outside
/// this module is [`set_policy`], which takes [`UserMixingPolicyChange`] and
/// raises DR-24's prompt when the move loosens. A `pub` writer here would be a
/// second door with neither.
///
/// ⚠ **Staged and renamed, never written in place**, for the hazard
/// [`super::master_switch::write_in`] documents: `fs::write` truncates first, and
/// a process that died inside that window would leave a record that reads as
/// *nothing recorded*.
fn write_in(config_dir: &Path, policy: MixingPolicy) -> std::io::Result<()> {
    std::fs::create_dir_all(config_dir)?;
    let staging = staging_path(config_dir);
    std::fs::write(&staging, body(policy)?)?;
    match std::fs::rename(&staging, path_in(config_dir)) {
        Ok(()) => Ok(()),
        Err(e) => {
            // Do not leave the staging file in the user's config directory.
            let _ = std::fs::remove_file(&staging);
            Err(e)
        }
    }
}

/// The record's serialised body, timestamped now.
fn body(policy: MixingPolicy) -> std::io::Result<String> {
    let record = MixingPolicyRecord {
        policy,
        changed_at: chrono::Utc::now().to_rfc3339(),
    };
    serde_json::to_string_pretty(&record)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

/// Where a write stages before it is published. **A fresh path per call**, for
/// [`super::master_switch`]'s reason: the pid alone is not enough, because axum
/// runs `/config/upsert` handlers concurrently and two writes can be in flight
/// inside one process.
fn staging_path(config_dir: &Path) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let seq = NEXT.fetch_add(1, Ordering::Relaxed);
    config_dir.join(format!(
        "{POLICY_FILE_NAME}.{}.{seq}.tmp",
        std::process::id()
    ))
}

// ---------------------------------------------------------------------------
// The live value.
// ---------------------------------------------------------------------------

/// The authoritative value for this process — installed at start-up by
/// [`load_from_record`] and moved thereafter only by [`set_policy`].
///
/// ⚠ **In memory because the read is on a dispatch path.** Every tool call asks
/// [`mismatch_refuses`], and a file read per call to answer a question whose
/// answer changes at most once a session would be a cost paid for nothing. This
/// is the same shape, and for the same reason, as the master switch's
/// `TIERS_ENABLED`.
///
/// ⚠ **It rests at the DEFAULT rather than at an "unresolved" sentinel, and that
/// is not a simplification — it is what keeps the developer's home directory out
/// of the test suite.** An earlier version resolved lazily on first read, which
/// meant any unit or integration test that reached a cross-affiliation gate read
/// `~/.config/biorouter/privacy-mixing-policy.json`: on a machine whose owner had
/// chosen `open`, every unpinned Gate C test in this crate would change outcome,
/// and whichever test got there first would freeze that value for the whole
/// binary. Loading at start-up, as [`super::load_privacy_tiers_from_config`]
/// already does, means a test binary never touches the file at all.
static CACHED: AtomicU8 = AtomicU8::new(encode(MixingPolicy::Standard));

const fn encode(policy: MixingPolicy) -> u8 {
    match policy {
        MixingPolicy::Open => 0,
        MixingPolicy::Standard => 1,
        MixingPolicy::Strict => 2,
    }
}

const fn decode(value: u8) -> Option<MixingPolicy> {
    match value {
        0 => Some(MixingPolicy::Open),
        1 => Some(MixingPolicy::Standard),
        2 => Some(MixingPolicy::Strict),
        _ => None,
    }
}

/// The mixing policy this process is enforcing.
///
/// A relaxed atomic load and nothing else, so it is free on a dispatch path.
/// `unwrap_or_default` is unreachable — `encode` is the only thing that ever
/// stores here — and is the safe direction anyway.
pub fn policy() -> MixingPolicy {
    if let Some(pinned) = pinned_policy() {
        return pinned;
    }
    decode(CACHED.load(Ordering::Relaxed)).unwrap_or_default()
}

/// Move the live value. **Private**: the two callers are the start-up load and
/// [`set_policy`], and a third would be a way past the proof-of-user.
fn install(policy: MixingPolicy) {
    CACHED.store(encode(policy), Ordering::Relaxed);
}

/// Install the recorded mode. Called ONCE per process, at start-up, by both
/// hosts of this library — `biorouterd` (`commands/agent.rs`) and the `biorouter`
/// CLI (its `main`) — through [`super::load_mixing_policy_from_record`], beside
/// the master switch's load and pinned to the same call sites by
/// `every_host_that_loads_the_master_switch_also_loads_the_mixing_policy`.
///
/// ⚠ **Not a second door onto the setting.** It cannot express a value the
/// record does not already hold, and the record has exactly one writer
/// ([`set_policy`], which takes [`UserMixingPolicyChange`] and raises DR-24's
/// prompt to loosen). What it can do is make this process agree with the disk,
/// which is the whole of its job.
///
/// A host that skips it enforces `standard`: safe, unchanged from before DR-27,
/// and the same fail-safe direction the master switch's omission had — but a user
/// who chose `strict` would silently be served `standard`, which is why the call
/// sites are pinned rather than remembered.
pub fn load_from_record(config: &Config) {
    install(resolve_for(config));
}

/// What [`load_from_record`] would install, for an explicit [`Config`].
///
/// The seam exists so a test can exercise the real resolution against a scratch
/// configuration directory rather than the developer's own, and — unlike the
/// loader — without moving a process-global that the tests running beside it can
/// see. It is the same split [`super::resolve_privacy_tiers`] has, for the same
/// two reasons.
///
/// ⚠ **Not a second reader of the setting.** This is the DISK resolution; the
/// authoritative value is the atomic, and gates read [`policy`]. Calling this
/// from a gate would re-introduce exactly the per-gate file read that hardening
/// measure (3) forbids.
pub fn resolve_for(config: &Config) -> MixingPolicy {
    read_for(config).unwrap_or_default()
}

/// Always `None` outside a `cfg(test)` build of this crate — see
/// [`pin_for_test`]. `const fn` so the branch above compiles out entirely.
#[cfg(not(test))]
const fn pinned_policy() -> Option<MixingPolicy> {
    None
}

/// **The** question every cross-institution refusal asks — DR-27 Step 3, and the
/// one read of this setting on a gate's path.
///
/// Its single caller is [`super::affiliation::refusing_mismatch`], which is what
/// every spelling of "the warning a refusal is stated in" runs through. Three
/// call sites reading a mode is three places to disagree, and the disagreement
/// would be silent: a tool marked and then dispatched, or listed and then
/// refused.
pub fn mismatch_refuses() -> bool {
    policy() != MixingPolicy::Open
}

/// Does clearing a mismatch take the operating system's authentication as well
/// as the in-app grant? `strict`, and only `strict`.
///
/// Read by `POST /agent/cross_affiliation_grant`, which is the one door a grant
/// comes through — see `routes::agent`.
pub fn grant_needs_system_authentication() -> bool {
    policy() == MixingPolicy::Strict
}

// ---------------------------------------------------------------------------
// The write, and the one door it comes through.
// ---------------------------------------------------------------------------

/// Proof that a human asked for this change of policy. A ZST with a **private
/// field**, so the tuple literal is unavailable outside this module and the named
/// constructor below is the only door.
///
/// It is the fourth proof-of-user in this feature and it is a **separate type on
/// purpose**, for the reason [`super::grant::UserCrossAffiliationGrant`] is
/// separate from [`super::declassify`]'s: one proof must not be spendable on
/// another subject.
///
/// ⚠ **DR-19 applies unchanged, in every mode.** The setting is user-only whether
/// the machine is currently `open`, `standard` or `strict`, and the enforcement
/// is the existing single-writer discipline rather than a new mechanism: the
/// constructor is `pub` only because its one caller lives in `biorouter-server`,
/// and what caps it at one call site is
/// [`tests::only_the_user_may_move_the_mixing_policy`], a repo walk asserting the
/// set of files naming this type is exactly the one HTTP handler behind the
/// `X-User-Action` header.
pub struct UserMixingPolicyChange(());

impl UserMixingPolicyChange {
    /// Mint the proof. Call this **only** after `auth::is_user_action` has
    /// answered yes; see the handler, which is the sole call site.
    pub fn from_user_action() -> Self {
        Self(())
    }
}

/// Why a policy change did not happen.
///
/// Two variants because the surfaces answer them differently: a refusal is the
/// user's own answer (or their machine's incapacity, which
/// [`SystemAuthRefusal::outcome`] distinguishes), and a failed write is an
/// error. Collapsing them would report "you cancelled" for a full disk.
#[derive(Debug)]
pub enum SetPolicyError {
    /// The system authentication did not approve. Nothing was written.
    Refused(SystemAuthRefusal),
    /// The authentication was not needed or approved, and the record still could
    /// not be written. Nothing was changed.
    NotRecorded(std::io::Error),
}

impl std::fmt::Display for SetPolicyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Refused(refusal) => write!(f, "{refusal}"),
            Self::NotRecorded(e) => write!(
                f,
                "Failed to record the cross-institution mixing policy: {e}. The setting was not \
                 changed."
            ),
        }
    }
}

/// What the prompt names where a declassification would name its chats.
///
/// ⚠ **Not a session id, and never compared with one.** The caller branches on
/// the outcome inside the function that raises the prompt, so no proof is minted
/// from this request — the same shape the master switch's prompt has.
const MIXING_POLICY_AUTH_SUBJECT: &str = "Biorouter's cross-institution mixing policy";

/// The sentence the operating system shows above the password field.
///
/// DR-20 point 4 wants the dialog to state the operation rather than *"BioRouter
/// wants to make changes"*, and it states the **consequence** rather than the
/// setting's name: a user authorising a system-level change is owed the sentence
/// that tells them what stops happening.
fn loosening_reason(from: MixingPolicy, to: MixingPolicy) -> String {
    match to {
        MixingPolicy::Open => format!(
            "Change Biorouter from {from} to open, so data crosses between institutions with no \
             confirmation at all."
        ),
        // ⚠ Not "your system password". F-13: on macOS the prompt behind this
        // can be satisfied by a recent authentication with no dialog and no
        // password typed, so describing the cost as a password overstates what
        // the user is actually giving up by loosening the policy.
        _ => format!(
            "Change Biorouter from {from} to {to}, so crossing between institutions no longer \
             needs your operating system to confirm it is you."
        ),
    }
}

/// Move the mixing policy. **The only writer of this setting outside this
/// module.**
///
/// The proof of a human is required in every mode (DR-19); the operating
/// system's authentication is required only when the move **loosens**
/// ([`loosens`]).
///
/// ⚠ **The prompt is raised LAST, immediately before the write** — the discipline
/// [`super::system_auth`]'s header states, so a user is never asked for a
/// password to be told afterwards that the request was malformed. The value the
/// direction is measured from is the **record**, not the process cache: a daemon
/// whose cache was warmed before the user edited the record would otherwise
/// measure the direction against a value the store no longer holds.
pub async fn set_policy(
    config: &Config,
    to: MixingPolicy,
    _ok: &UserMixingPolicyChange,
) -> Result<(), SetPolicyError> {
    let _serialised = WRITE_LOCK.lock().await;
    set_policy_with(system_auth::prompter(), &dir_of(config), to).await?;
    // Second, and only on success: the live value is what every gate reads, and
    // moving it for a write that failed is the divergence Task 30's measure (3)
    // exists to prevent.
    install(to);
    Ok(())
}

/// Serialises the whole measure-decide-write-install sequence above.
///
/// ⚠ **Held across the system prompt's `await`, deliberately.** axum runs
/// `/config/upsert` handlers concurrently, so two policy writes really can be in
/// flight at once; without this they would both measure the direction from the
/// same starting mode and then race on two independent stores — the record's and
/// the process cache's — and the two can land in either order. A record saying
/// `strict` while the daemon enforces `open` is Task 30's measure (3) seen from
/// the writing end, and it is the one outcome of that race that fails unsafe.
///
/// Holding it across a human-scale prompt cannot strand anyone: the only caller
/// that waits is a second write of the same setting, and the thing it is waiting
/// on is a dialog the user is looking at.
static WRITE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// [`set_policy`] with the prompter and the directory given rather than resolved.
///
/// **Private**, and that is the security boundary — the same one
/// [`super::declassify`]'s `authorize_with` draws: a caller that could choose the
/// prompter could choose one that approves. What the split buys is that "leaving
/// strict costs a password", "entering strict costs nothing" and "a denied prompt
/// changes nothing" are assertions about the real decision path, testable on any
/// host with no password typed.
///
/// It deliberately does **not** touch [`CACHED`]: the tests drive it against
/// scratch directories, and a seam that moved the process-wide value would make
/// every one of them leak into its neighbours.
async fn set_policy_with(
    prompter: &dyn SystemAuthenticator,
    config_dir: &Path,
    to: MixingPolicy,
) -> Result<(), SetPolicyError> {
    let from = read_in(config_dir).unwrap_or_default();
    if loosens(from, to) {
        let request = AuthRequest::about(loosening_reason(from, to), MIXING_POLICY_AUTH_SUBJECT);
        if let Some(refusal) = system_auth::authenticate_or_refuse(prompter, &request).await {
            // Nothing has been written at this point, so the machine is left in
            // the mode it was already in — which is the stricter of the two.
            return Err(SetPolicyError::Refused(refusal));
        }
    }
    write_in(config_dir, to).map_err(SetPolicyError::NotRecorded)
}

// The test pin, PER THREAD. (`//` rather than `///`: rustdoc does not document a
// macro invocation, and a doc comment here is an `unused_doc_comments` warning.)
//
// ⚠ **Thread-local rather than process-global, and that is not a stylistic
// choice.** `cargo test` runs a binary's tests in parallel threads, and every
// Gate C dispatch test in this crate reaches
// `CallCapability::cross_affiliation_warning` and expects today's refusal. A
// process-wide override — even one held for microseconds — is a window in which
// one of them reads `open` and fails for a reason that has nothing to do with
// it, occasionally, in CI. A thread-local one cannot be seen by any test but the
// one that set it, and the gate is reached synchronously on that same thread.
//
// It shadows the cached value rather than replacing it, so nothing has to be
// restored on the process global at all.
//
// ⚠ **A pin cannot cross a thread, and the consequence of that is now bounded
// rather than arbitrary.** A future `#[tokio::test]` that pins and then awaits
// work on a multi-thread runtime would read the unpinned value on the worker —
// so the pin would silently not apply. Since the live value now rests at the
// DEFAULT until a host loads the record (see `CACHED`), what such a test reads is
// `standard`, which is a wrong answer a test can fail on rather than the
// developer's own machine setting, which is a wrong answer that varies by
// machine. `binding_a_foreign_institutions_model_warns_and_still_binds` is the
// one place a pin spans an `await`; it runs on the default current-thread
// runtime, where there is no other thread to lose it to.
#[cfg(test)]
thread_local! {
    static PINNED: std::cell::Cell<Option<MixingPolicy>> =
        const { std::cell::Cell::new(None) };
}

#[cfg(test)]
fn pinned_policy() -> Option<MixingPolicy> {
    PINNED.with(std::cell::Cell::get)
}

/// Pin the live policy for the duration of a test on **this thread**, restoring
/// whatever was pinned before when the guard drops.
///
/// `#[cfg(test)]`, so it is absent from every shipped binary and from the
/// integration-test binaries, which link the non-test library.
#[cfg(test)]
pub(crate) struct PolicyPin(Option<MixingPolicy>);

#[cfg(test)]
impl Drop for PolicyPin {
    fn drop(&mut self) {
        PINNED.with(|pinned| pinned.set(self.0));
    }
}

#[cfg(test)]
pub(crate) fn pin_for_test(policy: MixingPolicy) -> PolicyPin {
    PolicyPin(PINNED.with(|pinned| pinned.replace(Some(policy))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::sync::Mutex;

    const ALL: [MixingPolicy; 3] = [
        MixingPolicy::Open,
        MixingPolicy::Standard,
        MixingPolicy::Strict,
    ];

    /// A prompter that answers as told and **counts**, so "tightening asks
    /// nothing" is an assertion rather than a hope.
    struct Prompter {
        answer: system_auth::AuthOutcome,
        prompts: AtomicUsize,
        reason: Mutex<Option<String>>,
    }

    impl Prompter {
        fn answering(answer: system_auth::AuthOutcome) -> Self {
            Self {
                answer,
                prompts: AtomicUsize::new(0),
                reason: Mutex::new(None),
            }
        }

        fn prompts(&self) -> usize {
            self.prompts.load(Ordering::Relaxed)
        }
    }

    #[async_trait::async_trait]
    impl SystemAuthenticator for Prompter {
        async fn authenticate(&self, req: &AuthRequest) -> system_auth::AuthOutcome {
            self.prompts.fetch_add(1, Ordering::Relaxed);
            *self.reason.lock().unwrap() = Some(req.reason.clone());
            self.answer
        }

        fn platform(&self) -> &'static str {
            "counting test prompter"
        }
    }

    #[test]
    fn the_three_modes_are_ordered_from_open_to_strict() {
        assert!(MixingPolicy::Open < MixingPolicy::Standard);
        assert!(MixingPolicy::Standard < MixingPolicy::Strict);
        assert_eq!(
            MixingPolicy::default(),
            MixingPolicy::Standard,
            "the default must be `standard`, so no existing install changes behaviour \
             on upgrade"
        );
        for mode in ALL {
            assert_eq!(MixingPolicy::parse(mode.as_str()), Some(mode));
        }
        // Never a default: the two wrong answers fail opposite ways.
        assert_eq!(MixingPolicy::parse("permissive"), None);
        assert_eq!(MixingPolicy::parse(""), None);
        // …but the spelling is forgiving about case and whitespace, the way the
        // master switch's is.
        assert_eq!(MixingPolicy::parse(" OPEN "), Some(MixingPolicy::Open));
    }

    /// Task 52 Step 2, the pure half. ⚠ The comparison is the ORDERING of the
    /// modes, not "is this a write" — a guard that prompted for every change
    /// makes tightening expensive, which is wrong in the direction that matters.
    #[test]
    fn the_direction_table_is_the_ordering_and_not_is_this_a_write() {
        assert!(loosens(MixingPolicy::Strict, MixingPolicy::Open));
        assert!(loosens(MixingPolicy::Strict, MixingPolicy::Standard));
        assert!(loosens(MixingPolicy::Standard, MixingPolicy::Open));
        assert!(!loosens(MixingPolicy::Open, MixingPolicy::Strict));
        assert!(!loosens(MixingPolicy::Open, MixingPolicy::Standard));
        assert!(!loosens(MixingPolicy::Standard, MixingPolicy::Strict));
        for mode in ALL {
            assert!(!loosens(mode, mode), "standing still is not a loosening");
        }
    }

    #[test]
    fn nothing_recorded_reads_as_the_default_and_never_as_open() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_in(dir.path()), None);
        assert_eq!(
            read_in(dir.path()).unwrap_or_default(),
            MixingPolicy::Standard
        );

        // Truncated, scribbled on, and a mode that does not exist. None of the
        // three may resolve to `open`, or corrupting one file is a way to
        // silence the control; none may resolve to `strict` either, or one bad
        // byte strands a machine behind a password nobody chose.
        for corrupt in ["{", r#"{"policy": "permissive"}"#, ""] {
            std::fs::write(path_in(dir.path()), corrupt).unwrap();
            assert_eq!(read_in(dir.path()), None, "corrupt record: {corrupt}");
            assert_eq!(
                read_in(dir.path()).unwrap_or_default(),
                MixingPolicy::Standard
            );
        }
    }

    #[test]
    fn the_record_round_trips_in_all_three_positions() {
        let dir = tempfile::tempdir().unwrap();
        for mode in ALL {
            write_in(dir.path(), mode).unwrap();
            assert_eq!(read_in(dir.path()), Some(mode));
        }
        // A record written by hand with only the mode must read: the timestamp
        // is an audit aid, not a checksum.
        std::fs::write(path_in(dir.path()), r#"{"policy": "strict"}"#).unwrap();
        assert_eq!(read_in(dir.path()), Some(MixingPolicy::Strict));
        // And no staging file is left in the user's configuration directory.
        let leftovers: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".tmp"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "staging files left behind: {leftovers:?}"
        );
    }

    /// Task 52 gate (2) — **the direction asymmetry, both ways, in every cell**.
    ///
    /// A test for only the loosening leg passes an implementation that prompts on
    /// every change, which is the wrong answer in the direction that matters: it
    /// makes tightening expensive. So this drives all nine (from, to) pairs and
    /// asserts the prompt COUNT as well as the outcome.
    #[tokio::test]
    async fn loosening_asks_the_operating_system_and_tightening_asks_nothing() {
        for from in ALL {
            for to in ALL {
                let dir = tempfile::tempdir().unwrap();
                write_in(dir.path(), from).unwrap();
                let prompter = Prompter::answering(system_auth::AuthOutcome::Denied);

                let outcome = set_policy_with(&prompter, dir.path(), to).await;

                if loosens(from, to) {
                    assert!(
                        matches!(outcome, Err(SetPolicyError::Refused(_))),
                        "{from} -> {to} loosened without the operating system's approval"
                    );
                    assert_eq!(prompter.prompts(), 1, "{from} -> {to}");
                    assert_eq!(
                        read_in(dir.path()),
                        Some(from),
                        "{from} -> {to}: a refused change was written anyway"
                    );
                    let reason = prompter.reason.lock().unwrap().clone().unwrap();
                    assert!(
                        reason.contains(to.as_str()) && reason.contains(from.as_str()),
                        "the prompt did not say which change it authorises: {reason}"
                    );
                } else {
                    outcome.unwrap_or_else(|e| {
                        panic!("{from} -> {to} is a tightening and must cost nothing: {e}")
                    });
                    assert_eq!(
                        prompter.prompts(),
                        0,
                        "{from} -> {to} raised a password prompt to TIGHTEN the setting. \
                         Requiring the password to leave `strict` is the whole point of \
                         `strict`; requiring it to enter `strict` punishes the careful user"
                    );
                    assert_eq!(read_in(dir.path()), Some(to), "{from} -> {to}");
                }
            }
        }
    }

    /// …and the named pair DR-27 states outright, with the loosening leg then
    /// succeeding once the operating system approves.
    #[tokio::test]
    async fn strict_to_open_needs_the_password_and_open_to_strict_needs_nothing() {
        let dir = tempfile::tempdir().unwrap();
        write_in(dir.path(), MixingPolicy::Strict).unwrap();

        let denied = Prompter::answering(system_auth::AuthOutcome::Denied);
        assert!(set_policy_with(&denied, dir.path(), MixingPolicy::Open)
            .await
            .is_err());
        assert_eq!(read_in(dir.path()), Some(MixingPolicy::Strict));

        let approved = Prompter::answering(system_auth::AuthOutcome::Approved);
        set_policy_with(&approved, dir.path(), MixingPolicy::Open)
            .await
            .expect("an approved system authentication is what leaving strict costs");
        assert_eq!(read_in(dir.path()), Some(MixingPolicy::Open));

        let never = Prompter::answering(system_auth::AuthOutcome::Denied);
        set_policy_with(&never, dir.path(), MixingPolicy::Strict)
            .await
            .expect("tightening must succeed with no prompt at all");
        assert_eq!(never.prompts(), 0);
        assert_eq!(read_in(dir.path()), Some(MixingPolicy::Strict));
    }

    /// A machine that cannot raise a prompt at all may not loosen either — the
    /// same fail-closed direction DR-24 states: a build whose control refuses is
    /// a missing feature, one that silently approves is the worst outcome.
    #[tokio::test]
    async fn a_machine_that_cannot_prompt_cannot_loosen() {
        let dir = tempfile::tempdir().unwrap();
        write_in(dir.path(), MixingPolicy::Strict).unwrap();
        let unavailable = Prompter::answering(system_auth::AuthOutcome::Unavailable);
        let error = set_policy_with(&unavailable, dir.path(), MixingPolicy::Standard)
            .await
            .expect_err("an unavailable prompter must not loosen the policy");
        assert!(
            error.to_string().contains("counting test prompter"),
            "an unavailable prompt must name the platform, so it reads as a missing \
             capability rather than as a bug: {error}"
        );
        assert_eq!(read_in(dir.path()), Some(MixingPolicy::Strict));
    }

    /// **The renderer and the daemon spell this setting the same way** — open
    /// question 31's detector, which came due when Task 52 landed the constant.
    ///
    /// ⚠ **The failure it exists to catch is silent in both directions.** The
    /// renderer reads the mode with a live `/config/read` of a key it spells
    /// itself; if that string and [`MIXING_POLICY_CONFIG_KEY`] ever differ, the
    /// read answers "absent", the parser resolves that to `standard`, and the
    /// panel renders `standard` on a machine enforcing `strict`. Nothing throws,
    /// nothing logs, and the user is told something false about the control they
    /// just used. The mode SPELLINGS have the same property: a renderer that
    /// wrote `"permissive"` would meet a 400 it has no arm for.
    ///
    /// It also closes the older half of the same gap — `PRIVACY_TIERS_KEY` — for
    /// the reason open question 31 gives: the master switch's shared spelling has
    /// never had a detector either, and it fails the same way.
    ///
    /// Same shape as
    /// `privacy::grant::tests::the_scope_copy_the_user_reads_is_the_one_the_daemon_records`,
    /// which is the precedent in this tree for a Rust test asserting a fact about
    /// a TypeScript file.
    #[test]
    fn the_renderer_and_the_daemon_spell_this_setting_the_same_way() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();

        let mirror = root.join("ui/desktop/src/utils/crossAffiliation.ts");
        let src = std::fs::read_to_string(&mirror).unwrap_or_else(|e| {
            panic!(
                "the renderer's cross-affiliation module is missing at {} ({e}). Without it \
                 nothing in the app can read or write DR-27's setting.",
                mirror.display()
            )
        });
        assert!(
            src.contains(MIXING_POLICY_CONFIG_KEY),
            "the renderer addresses the mixing policy by a different key from the one the \
             daemon answers for. That read resolves to `standard` for ever and NOTHING \
             fails — re-mirror {MIXING_POLICY_CONFIG_KEY} into {}.",
            mirror.display()
        );
        for mode in ALL {
            assert!(
                src.contains(&format!("'{}'", mode.as_str())),
                "the renderer no longer names the `{mode}` mode, so it can neither render \
                 nor write it: {}",
                mirror.display()
            );
        }

        // …and the older half of the same gap. The master switch's key is shared
        // between the two sides in exactly the same way and has never had a
        // detector; open question 31 asked for both.
        let switch_mirror = root.join("ui/desktop/src/components/settings/privacy/privacyTiers.ts");
        let switch_src = std::fs::read_to_string(&switch_mirror).unwrap_or_else(|e| {
            panic!(
                "Settings > Privacy's module is missing at {} ({e})",
                switch_mirror.display()
            )
        });
        assert!(
            switch_src.contains(super::super::PRIVACY_TIERS_CONFIG_KEY),
            "Settings > Privacy addresses the master switch by a different key from the one \
             the daemon answers for — re-mirror it into {}.",
            switch_mirror.display()
        );
    }

    /// `open` says what it does, and no more. DR-27: it is not the master switch
    /// and must not read like one.
    #[test]
    fn the_open_disclosure_does_not_claim_to_turn_anything_off() {
        assert!(
            OPEN_DISCLOSURE.contains("institutions"),
            "{OPEN_DISCLOSURE}"
        );
        assert!(
            OPEN_DISCLOSURE.contains("Nothing else changes"),
            "{OPEN_DISCLOSURE}"
        );
        for overclaim in ["turns off", "disable", "no longer protected"] {
            assert!(
                !OPEN_DISCLOSURE.to_ascii_lowercase().contains(overclaim),
                "the `open` disclosure reads like the master switch: {OPEN_DISCLOSURE}"
            );
        }
    }

    /// The start-up load installs exactly what the record holds — and the
    /// **resting** value, which every test binary in this tree runs against, is
    /// the default.
    ///
    /// ⚠ **The second assertion is the one that matters, and it is about the test
    /// suite rather than about a user.** An earlier version of [`policy`]
    /// resolved from `Config::global()` on first read, so any test that reached a
    /// cross-affiliation gate read the DEVELOPER's own
    /// `~/.config/biorouter/privacy-mixing-policy.json`. On a machine whose owner
    /// had chosen `open`, unpinned Gate C tests across this crate would have
    /// changed outcome — and passed or failed for a reason nowhere in their
    /// source. Driven through [`resolve_for`] rather than [`load_from_record`] so
    /// this test does not move a process-global its neighbours can see.
    #[test]
    fn the_start_up_load_reads_the_record_and_an_unloaded_process_rests_at_the_default() {
        let dir = tempfile::tempdir().unwrap();
        // Nothing recorded is the default, not the strictest and not the most
        // permissive.
        assert_eq!(
            read_in(dir.path()).unwrap_or_default(),
            MixingPolicy::Standard
        );
        for mode in ALL {
            write_in(dir.path(), mode).unwrap();
            assert_eq!(
                read_in(dir.path()).unwrap_or_default(),
                mode,
                "the start-up load must install what the record holds"
            );
        }
        assert_eq!(
            decode(CACHED.load(Ordering::Relaxed)),
            Some(MixingPolicy::Standard),
            "a process that never called the start-up load is enforcing something \
             other than the default, so this test binary is reading a value from \
             outside its own source"
        );
    }

    /// Both start-up loads, from the same places.
    ///
    /// ⚠ **A host list would be the wrong test, because the next host will not be
    /// on it.** What is asserted instead is a relation: the files that load the
    /// master switch are exactly the files that load the mixing policy. The
    /// master switch's set is already right — and its own doc records the CLI
    /// omitting it for a whole round, in a direction safe enough to be invisible.
    /// A `strict` machine quietly served `standard` is that same silence, so the
    /// pair is pinned rather than remembered.
    #[test]
    fn every_host_that_loads_the_master_switch_also_loads_the_mixing_policy() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let crates = root.join("crates");
        assert!(crates.is_dir(), "the audit walks {}", crates.display());

        // Composed, so this file is not itself a call site of either.
        let switch = concat!("load_privacy_tiers", "_from_config()");
        let mixing = concat!("load_mixing_policy", "_from_record()");

        let mut loads_switch: Vec<String> = vec![];
        let mut loads_mixing: Vec<String> = vec![];
        let mut scanned = 0usize;
        for entry in walkdir::WalkDir::new(&crates) {
            let entry = entry.expect("the audit must not silently skip an unreadable directory");
            let p = entry.path();
            if p.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let rel = p
                .strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            // Production only: a test that loads one of them is not a host.
            if !rel.contains("/src/") {
                continue;
            }
            scanned += 1;
            let src = std::fs::read_to_string(p)
                .unwrap_or_else(|e| panic!("the audit could not read {rel}: {e}"));
            let mut s = false;
            let mut m = false;
            for line in src.lines() {
                let code = line.trim_start();
                if code.starts_with("//") {
                    continue;
                }
                s |= code.contains(switch);
                m |= code.contains(mixing);
            }
            if s {
                loads_switch.push(rel.clone());
            }
            if m {
                loads_mixing.push(rel);
            }
        }
        assert!(
            scanned >= 400,
            "only {scanned} production .rs files were scanned. A broken walk reports \
             the same empty set as a clean tree."
        );
        loads_switch.sort();
        loads_mixing.sort();
        assert!(
            loads_switch.len() >= 3,
            "the master switch's start-up load was found in {loads_switch:?}, which is \
             fewer files than its own definition plus two hosts — the scan is broken, \
             not the tree"
        );
        assert_eq!(
            loads_switch, loads_mixing,
            "a host loads one privacy setting at start-up and not the other. The one \
             that is missed enforces its default silently: `standard` for the mixing \
             policy, which is not what a user who chose `strict` asked for."
        );
    }

    /// Task 52 gate (5). **An agent cannot change the mode, in any of the three
    /// modes**, and the mechanism is the existing single-writer discipline rather
    /// than a new one: [`UserMixingPolicyChange`] is unforgeable outside this
    /// module, and this pins the set of files that so much as NAME it.
    ///
    /// The policy is not an input to the rule anywhere — [`loosens`] decides only
    /// whether the OPERATING SYSTEM is asked, never whether the user is — so
    /// there is no mode in which this set is different.
    ///
    /// It pins two things and takes both: the set of FILES outside this one that
    /// name the type, and the number of times the constructor is CALLED. A count
    /// alone says nothing about which function holds it, and a file set alone
    /// says nothing about a second construction inside a permitted file.
    #[test]
    fn only_the_user_may_move_the_mixing_policy() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let crates = root.join("crates");
        assert!(
            crates.is_dir(),
            "the audit walks {} — if that path is wrong every assertion below passes \
             for the wrong reason",
            crates.display()
        );

        // Composed rather than written out, so this file does not match its own
        // audit and a reader cannot mistake the needle for a call site.
        let named = concat!("User", "MixingPolicyChange");
        let minted = concat!("User", "MixingPolicyChange::from_user_action(");

        let mut naming: Vec<String> = vec![];
        let mut constructions: std::collections::BTreeMap<String, usize> = Default::default();
        let mut scanned = 0usize;
        for entry in walkdir::WalkDir::new(&crates) {
            let entry = entry.expect("the audit must not silently skip an unreadable directory");
            let p = entry.path();
            if p.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let rel = p
                .strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            scanned += 1;
            // This file declares the type and names it in its own test door.
            if rel == "crates/biorouter/src/privacy/mixing.rs" {
                continue;
            }
            let src = std::fs::read_to_string(p)
                .unwrap_or_else(|e| panic!("the audit could not read {rel}: {e}"));
            let mut names_it = false;
            for line in src.lines() {
                let code = line.trim_start();
                // ⚠ The comment skip covers the NAMING set too, for the reason
                // `grant.rs`'s twin audit records: prose cannot hold a
                // proof-of-user, and an audit that counted a `///` line goes red
                // across a task boundary with nobody's code at fault.
                if code.starts_with("//") {
                    continue;
                }
                names_it |= code.contains(named);
                let hits = code.matches(minted).count();
                if hits > 0 {
                    *constructions.entry(rel.clone()).or_default() += hits;
                }
            }
            if names_it {
                naming.push(rel.clone());
            }
        }
        assert!(
            scanned >= 400,
            "only {scanned} .rs files were scanned. A broken walk reports the same \
             empty set as a clean tree."
        );
        naming.sort();
        assert_eq!(
            naming,
            vec!["crates/biorouter-server/src/routes/config_management.rs".to_string()],
            "a new file names the mixing-policy proof-of-user. The whole claim that an \
             agent cannot loosen this setting rests on this set being exactly the one \
             HTTP handler behind the X-User-Action header."
        );
        let called: Vec<(String, usize)> = constructions.into_iter().collect();
        assert_eq!(
            called,
            vec![(
                "crates/biorouter-server/src/routes/config_management.rs".to_string(),
                1
            )],
            "the proof-of-user is minted more than once. A second construction site is a \
             second way to move the mixing policy."
        );
    }

    /// …and the other half of gate (5): there is no public door into the store
    /// that skips the proof.
    ///
    /// [`super::master_switch`] exposes `write_in`/`write_for` because its one
    /// caller is in another crate and reaches them after its own guard. This
    /// module must not, or the proof above becomes advisory.
    #[test]
    fn the_store_has_one_public_writer_and_its_signature_demands_the_proof() {
        const SOURCE: &str = include_str!("mixing.rs");
        let proof = concat!("User", "MixingPolicyChange");
        let entry = concat!("pub async fn ", "set_policy(");

        let mut public_writers: Vec<String> = vec![];
        for line in SOURCE.lines() {
            let code = line.trim_start();
            if code.starts_with("//") {
                continue;
            }
            if !code.starts_with("pub fn ") && !code.starts_with("pub async fn ") {
                continue;
            }
            if code.contains("write") || code.contains("set_policy") {
                public_writers.push(code.to_string());
            }
        }
        assert_eq!(
            public_writers.len(),
            1,
            "a second public writer reaches the mixing-policy store without the \
             proof-of-user: {public_writers:#?}"
        );

        // Line-wise rather than by byte slice: `clippy::string_slice` is
        // warn-by-default in this workspace and `-D warnings` makes it an error
        // — the same correction `918459cd` applied to the declassification
        // audit. The signature runs from the entry line to the first line
        // closing the parameter list.
        let mut signature: Vec<&str> = vec![];
        let mut inside = false;
        for line in SOURCE.lines() {
            if !inside && line.trim_start().starts_with(entry) {
                inside = true;
            }
            if inside {
                signature.push(line);
                if line.contains(") -> ") {
                    break;
                }
            }
        }
        assert!(
            !signature.is_empty(),
            "the one public writer is no longer spelled `set_policy`"
        );
        assert!(
            signature.last().is_some_and(|l| l.contains(") -> ")),
            "the writer's signature no longer ends where this scan expects: {signature:#?}"
        );
        assert!(
            signature.iter().any(|line| line.contains(proof)),
            "the mixing policy's one writer no longer takes the proof-of-user, so an \
             agent-reachable caller could move it: {signature:#?}"
        );

        // …and the LIVE value has one writer too, which the scan above cannot
        // see: it reads signatures, and a function that moved [`CACHED`] without
        // "write" or "set_policy" in its name would pass it. `install` is
        // private, it is the only thing that stores, and its two callers are the
        // start-up load and `set_policy`. A third would move what every gate
        // reads without touching the record the repo walk above pins.
        let code: Vec<&str> = SOURCE
            .lines()
            .map(str::trim_start)
            .filter(|line| !line.starts_with("//"))
            .collect();
        assert_eq!(
            code.iter()
                .filter(|l| l.contains(concat!("CACHED", ".store(")))
                .count(),
            1,
            "the live mixing policy is stored from more than one place"
        );
        assert_eq!(
            code.iter()
                .filter(|l| l.contains(concat!("install", "(")))
                .count(),
            3,
            "the private installer's definition plus its two sanctioned callers — the \
             start-up load and `set_policy` — is three lines. A fourth is a third way \
             to move what every gate reads."
        );
    }
}
