//! The refusals the privacy gates return (issue #56).
//!
//! One module rather than one per gate, because a refusal is the *same object*
//! at three different altitudes: a typed Rust error the caller can branch on, a
//! typed HTTP body the GUI renders a repair card from, and — for Gate C — a
//! string the model reads. Keeping them together is what lets §14.4's rule
//! ("never leak content in a refusal": name the tool and the tier, never a
//! session title or a working directory) be checked by reading one file.
//!
//! Later tasks add one item each and say which:
//!
//! | Task | Adds |
//! |---|---|
//! | 12 (this one) | [`PrivacyRefusal`] and its two accessors |
//! | 13 | `turn_refusal(&Session) -> String`, and Task 10's `CHATRECALL_LOAD_REFUSAL` moves here |
//! | 14 | `privacy_refusal(extension, extension_tier, caller_tier) -> Option<ErrorData>` |
//! | 18A | [`ASK_THE_USER_TO_SWITCH`], [`raise_needs_user_action`] and the three HTTP-channel variants |
//! | 23 | the two spawn variants and `PrivacyRefusal::spawn_upgrade` / `spawn_downgrade` |
//! | 41 | [`PrivacyRefusal::AppSessionTierFixed`], DR-21's app-runtime refusal |

use super::{ProviderTier, SessionClassification};
use crate::session::session_manager::Session;
use rmcp::model::{ErrorCode, ErrorData};

/// The one sentence every refusal in this feature ends on. DR-16 turns step 1 of
/// the two-ways-out message from something the model DOES into something it
/// SAYS, so the wording is shared rather than re-typed per call site —
/// including by Task 18's `check_enable_allowed` arm, which Task 18A rewrote to
/// use it. Two audiences, one vocabulary.
pub const ASK_THE_USER_TO_SWITCH: &str =
    "ask the user to switch this chat to a private model first — in the desktop app under \
     Settings > Models, or with the model chip in the composer.";

/// The substring that marks a refusal as *"this request carried no proof it came
/// from the user"*, as opposed to any other failure of the same route.
///
/// It exists for the same reason [`TURN_REFUSAL_MARKER`] does: two independent
/// readers key on this text. The model reads the whole sentence; the desktop
/// renderer (`ModelAndProviderContext.changeModel`) has to tell this refusal
/// apart from a 500 so it can explain the *cause* — a backend the app did not
/// start, and therefore one that was handed no user-action key (open question
/// 23) — instead of reporting a policy refusal as `${error}` under a
/// "provider/model failed" title.
///
/// A substring is all the renderer has: under `throwOnError` the generated
/// client throws the parsed BODY, not the response, so the 409 status never
/// reaches the catch arm. Gate A's refusal is distinguishable because its body
/// is a typed JSON object; these two are plain text, and plain text is also what
/// a 500 from the same route carries.
///
/// ⚠ Mirrored verbatim in `ui/desktop/src/utils/userAction.ts`. A reword that
/// dropped it here would put every refused switch back on the generic error
/// toast, with nothing failing.
pub const USER_ACTION_REFUSAL_MARKER: &str = "is the user's decision, not yours";

/// DR-16's rule, as a predicate: raising a session's capability to Private is
/// the user's act alone, and only an **upward** bind is a raise.
///
/// Sideways (`Public → Public`, `Private → Private`) and downward
/// (`Private → Public`) binds are untouched **for every caller**, which is what
/// keeps Gate A's own path, the CLI, `restore_provider_from_session` and every
/// apps-runtime bind working exactly as before. Downward is Gate A's job, not
/// this one.
pub const fn raise_needs_user_action(current: ProviderTier, incoming: ProviderTier) -> bool {
    !current.is_private() && incoming.is_private()
}

/// A privacy boundary refused an operation.
///
/// It is an ordinary `std::error::Error` carried inside `anyhow::Error`, so
/// every existing caller keeps its `Result<()>` signature and the one place
/// that needs to tell a refusal from a database failure — the
/// `/agent/update_provider` handler, which owes the GUI a 409 rather than a
/// 500 — asks with `downcast_ref`.
///
/// ⚠ The payload is deliberately thin: a session **id** and a provider **name**.
/// Refusals reach the model's context (Gate C) and the user's screen, and §14.4
/// forbids either carrying conversation content.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PrivacyRefusal {
    /// Gate A: a public model was offered to a session already classified
    /// private. The classification is a permanent ratchet, so this is not
    /// retryable — the way forward is a private model, or declassifying the
    /// session.
    #[error(
        "this chat is private, so it cannot be switched to `{provider}`, which is a public model"
    )]
    PublicModelOnPrivateSession {
        session_id: String,
        provider: String,
    },

    /// DR-16 / Task 18A: `POST /agent/update_provider` was asked to raise this
    /// session's capability to Private, and the request carried no proof it came
    /// from the person at the keyboard.
    ///
    /// Model-facing: it is rendered as the 409 body and reaches the model as a
    /// tool result. It names only the provider the caller itself named — which
    /// is what `{requested}` is, and why it is rendered rather than carried: an
    /// un-rendered field made `a_refusal_names_nothing_the_caller_did_not_ask_for`
    /// pass against any implementation that merely avoided typing one of three
    /// provider names into a constant.
    #[error(
        "Switching this chat to a private model {}. The request to switch it to '{requested}' \
         did not come from the model picker, so the chat is unchanged and still on its current \
         model. Do not retry — the same call will be refused again. If this task genuinely needs \
         a private model, stop and {}",
        USER_ACTION_REFUSAL_MARKER,
        ASK_THE_USER_TO_SWITCH
    )]
    TierRaiseNeedsUser { requested: String },

    /// DR-16 / Task 18A: `POST /agent/add_extension` was asked to attach a
    /// private extension to a session running on a public model.
    ///
    /// Refused **outright**, with no user-proof branch: attaching a private
    /// extension to a public session is not a raise the user can authorize
    /// either. The way forward is to switch the model first, then attach.
    ///
    /// Same shape as Gate F1's `extensionmanager__manage_extensions` refusal, so
    /// the two channels speak with one voice.
    #[error(
        "Extension '{name}' is a private extension: the Biorouter marketplace marks it as \
         reaching data held inside the institution, so only a private model may enable or call \
         it. This session is running on a public model, so the extension was not attached. Do not \
         retry. If it is needed for this task, {}",
        ASK_THE_USER_TO_SWITCH
    )]
    PrivateExtensionOverHttp { name: String },

    /// DR-16 / Task 18A, open question 24: an HTTP config write named a key that
    /// decides what privacy capability new chats start at
    /// (`privacy::config_keys::CAPABILITY_CONFIG_KEYS`), and carried no
    /// user-proof.
    #[error(
        "'{key}' decides what privacy level new chats start at, so changing it {}. The setting \
         is unchanged. Do not retry. If *this* task needs a private model, that is a per-chat \
         change and not a default: {}",
        USER_ACTION_REFUSAL_MARKER,
        ASK_THE_USER_TO_SWITCH
    )]
    CapabilityConfigNeedsUser { key: String },

    /// R4 / §8.2: a public-capability session may never gain private reach, not
    /// even through a child it spawns. A subagent is an extension of the chat
    /// that started it, so letting one reach further would make the boundary a
    /// formality — spawn a child on a private model, have it read what the
    /// parent may not, and hand the answer back as a summary.
    ///
    /// `requested` is the CHILD's tier, never the parent's, so the message can
    /// say what was asked for without naming the parent's provider (R10's
    /// disclosure bound: a refusal must not become a classification oracle).
    #[error(
        "This chat is running on a public model, so it cannot delegate to a private one: a \
         subagent may never reach further than the chat that started it. No subagent was \
         started and this chat is unchanged. Do not retry — the same call will be refused \
         again. If this task genuinely needs a private model, stop and {}",
        ASK_THE_USER_TO_SWITCH
    )]
    PrivateChildOfPublicParent { requested: ProviderTier },

    /// DR-19: a private-capability session may not hand its task prompt to a
    /// public model the **model** chose.
    ///
    /// R4 permits a private session to have public children; it says nothing
    /// about who may ask for one. DR-19 supplies the initiator R4 never named,
    /// and here there is only ever one: a subagent spawn is a tool call, and no
    /// shipped surface lets a human spawn a subagent and pick its provider. So
    /// this is an agent-initiated send of private-origin text to a public model,
    /// which DR-19's agent half refuses outright — there being no in-process
    /// channel to escalate on (Task 18A's `X-User-Action` is an HTTP header, and
    /// the approval machinery is unlockable by hooks the same agent can author).
    ///
    /// It ends on the user's route rather than on [`ASK_THE_USER_TO_SWITCH`],
    /// and that difference is deliberate: this chat is *already* private, so
    /// "switch to a private model" would be advice it has already taken. The
    /// route out is a second chat, which costs the user two steps and not a
    /// wall. The message names neither the parent's provider (R10: a refusal
    /// must not become a classification oracle) nor the prompt — the prompt is
    /// the thing being withheld.
    #[error(
        "This chat is private, so it cannot start a subagent on a public model: the task \
         prompt is private-origin text, sending it would put that text on a model outside \
         the institution, and this request came from the assistant rather than from the \
         person at the keyboard. No subagent was started and this chat is unchanged. Do not \
         retry — the same call will be refused again, and no setting, hook or permission mode \
         changes it. If the task really belongs on that model, tell the user so: they can \
         start a new chat on it and give it the task directly."
    )]
    PublicChildOfPrivateParent { requested: ProviderTier },

    /// DR-21 / Task 41: a Biorouter **app** session was asked to move to a more
    /// private model than the one it was created on.
    ///
    /// An app session's capability tier is decided at session creation and
    /// cannot be changed afterwards — not by a manifest edit, not by a
    /// reconnect, not by a client frame. All three channels are agent-authored:
    /// `agent_drafter__declare_profiles` lets a Public model write the
    /// manifest's `model`, and `ClientFrame::ModelSelect` arrives on
    /// `GET /apps/{id}/agent`, a route exempt from secret-key auth entirely. So
    /// there is no user-proof branch here and there deliberately is not one: a
    /// per-manifest grant would be exactly the scoped permission DR-21 refused
    /// to invent.
    ///
    /// The way out is **not** [`ASK_THE_USER_TO_SWITCH`] — an app has no model
    /// picker, and switching this session is the very thing that is refused.
    /// It is a fresh app session, which is the same shape as
    /// [`Self::PublicChildOfPrivateParent`]'s "start a new chat".
    ///
    /// §14.4: it names the provider the caller itself asked for and nothing
    /// else — not the model the app is on (which would tell a public model
    /// which providers are private), not the app, not the session.
    #[error(
        "An app's model is fixed when its session is created, so this app cannot be switched to \
         '{requested}', which is a more private model than the one it started on. Nothing was \
         changed and the app is still on its current model. Do not retry — the same call will be \
         refused again, and no manifest field, frame or setting changes it. If this app really \
         needs that model, it has to be *created* on it: tell the user, so they can set the app's \
         model and start a fresh session for it."
    )]
    AppSessionTierFixed { requested: String },
}

impl PrivacyRefusal {
    /// R4: the child a public parent asked for was private. Named here, and
    /// nowhere before, because Task 23's spawn gate is its only caller.
    pub fn spawn_upgrade(requested: ProviderTier) -> Self {
        Self::PrivateChildOfPublicParent { requested }
    }

    /// DR-19: the child a private parent asked for was public, and only a model
    /// can have asked. Same call site, opposite direction.
    pub fn spawn_downgrade(requested: ProviderTier) -> Self {
        Self::PublicChildOfPrivateParent { requested }
    }

    /// The classification of the session that refused. Half of the pair the
    /// GUI's card names ("this chat is **private**, that model is **public**").
    ///
    /// `None` for the three DR-16 variants, and that is the point: they are
    /// refusals about a *channel*, not about a session whose contents collided
    /// with a model, and inventing a classification for them would put a
    /// fabricated pair on the GUI's repair card. Task 18A's handlers render
    /// those three straight from [`std::fmt::Display`] and never ask. DR-21's
    /// app refusal joins them for the same reason.
    pub fn session_classification(&self) -> Option<SessionClassification> {
        match self {
            Self::PublicModelOnPrivateSession { .. } => Some(SessionClassification::Private),
            Self::TierRaiseNeedsUser { .. }
            | Self::PrivateExtensionOverHttp { .. }
            | Self::CapabilityConfigNeedsUser { .. }
            // A spawn refusal is about the CAPABILITY the parent has, not about
            // a session whose stored contents collided with a model. Inventing a
            // classification for it would put a fabricated pair on the GUI card.
            | Self::PrivateChildOfPublicParent { .. }
            | Self::PublicChildOfPrivateParent { .. }
            // DR-21 is about WHEN an app session's tier may be set, not about a
            // stored transcript, so it has no classification either.
            | Self::AppSessionTierFixed { .. } => None,
        }
    }

    /// The tier of the thing that was refused.
    pub fn provider_tier(&self) -> Option<ProviderTier> {
        match self {
            Self::PublicModelOnPrivateSession { .. } => Some(ProviderTier::Public),
            // The child's tier — what was asked for, which is what was refused.
            Self::PrivateChildOfPublicParent { requested }
            | Self::PublicChildOfPrivateParent { requested } => Some(*requested),
            Self::TierRaiseNeedsUser { .. }
            | Self::PrivateExtensionOverHttp { .. }
            | Self::CapabilityConfigNeedsUser { .. }
            // The refused bind was necessarily private (a raise is the only
            // thing DR-21's guard fires on), but this is a refusal about a
            // channel rather than about a session/model pair, so it reports the
            // same `None` its DR-16 siblings do rather than seeding a repair
            // card the app has no way to act on.
            | Self::AppSessionTierFixed { .. } => None,
        }
    }

    /// The session the refusal is about. Kept as an accessor rather than
    /// destructured at every call site so a future variant without one is a
    /// compile error here instead of at six handlers.
    pub fn session_id(&self) -> Option<&str> {
        match self {
            Self::PublicModelOnPrivateSession { session_id, .. } => Some(session_id),
            Self::TierRaiseNeedsUser { .. }
            | Self::PrivateExtensionOverHttp { .. }
            | Self::CapabilityConfigNeedsUser { .. }
            | Self::PrivateChildOfPublicParent { .. }
            | Self::PublicChildOfPrivateParent { .. }
            // §14.4: an app refusal reaches the app's own page and the model
            // reading it. It carries no session id for the same reason the
            // spawn refusals do not.
            | Self::AppSessionTierFixed { .. } => None,
        }
    }
}

/// The one sentence every turn refusal contains, and the reason it is a
/// constant rather than a phrase inlined at the format site: two independent
/// readers key on it — the desktop client, which must be able to tell a refusal
/// from an ordinary assistant reply, and `agents::agent::gate_b_turn_tests`,
/// whose whole discrimination between "refused" and "ran" is this substring. A
/// reworded refusal that dropped it would leave both of them silently reading
/// every refusal as a completed turn.
pub const TURN_REFUSAL_MARKER: &str = "this turn was not sent";

/// Gate B: the turn was refused because the session is classified private and
/// the model it is bound to is public, with no private model recorded on the
/// row to repair to.
///
/// §14.4: this string is rendered straight into the transcript, so it may name
/// the tier and the provider and nothing else. It takes the whole [`Session`]
/// rather than the two strings it uses so that a future variant needing the
/// classification does not have to change every call site — but note that the
/// row carries the session's *name* and *working directory*, which are CONTENT
/// and must never reach the sentence. The unit test below is what holds that.
pub fn turn_refusal(session: &Session) -> String {
    format!(
        "This chat is private, so only a private model — one hosted inside the institution — \
         may run in it. The model this chat is set to (`{provider}`) is public, so \
         {TURN_REFUSAL_MARKER}. Switch this chat to a private model (Settings → Models, or the \
         model chip in the composer) and send it again. Nothing about the chat has been changed.",
        provider = session.provider_name.as_deref().unwrap_or("no model"),
    )
}

/// Gate D: the model asked to load a chat history more private than the model
/// itself.
///
/// Moved here from `agents::chatrecall_extension`, which is where Task 10 put
/// it before this module existed. Constant on purpose: a model that sees a
/// different string on retry concludes the refusal is transient and loops. It
/// names no target — not the session, not its working directory — because
/// §11.4 classifies both as CONTENT.
pub const fn chatrecall_load_refusal() -> &'static str {
    "This chat history is private: it was created under a model hosted inside the institution, \
     so only a private model may read it. This session is running on a public model. Ask the user \
     to switch this chat to a private model — Settings → Models, or the model chip in the \
     composer — and try again. Do not retry with a different session id or through another tool; \
     the boundary is the same everywhere."
}

/// Gate C's refusal. Returns `None` when the call is permitted, so the caller
/// reads as `if let Some(err) = privacy_refusal(..) { return Err(err.into()); }`.
///
/// Pure: no config, no session, no provider, no I/O. That is what lets the
/// dispatch choke point ask it while holding nothing, and what lets its whole
/// behaviour be pinned by a table-driven unit test.
///
/// `ErrorData` directly, **not** a `ToolInspector`. `Agent::handle_denied_tools`
/// passes a real reason through for exactly three inspector names — the hook
/// inspector, `"security"` and the repetition inspector — and everything else
/// falls to `DECLINED_RESPONSE`, which the code itself calls "actively
/// misleading": it tells the model *the user* refused. An inspector-shaped Gate
/// C would also be invisible to `POST /agent/call_tool`, to the `execute_code`
/// bridge and to `Agent::call_prefetch_tool`, none of which run inspectors.
///
/// §14.4: the string reaches the model's context, so it names the extension and
/// the two tiers and nothing else — no session id, no title, no working
/// directory.
pub fn privacy_refusal(
    extension: &str,
    extension_tier: ProviderTier,
    caller_tier: ProviderTier,
) -> Option<ErrorData> {
    if extension_tier != ProviderTier::Private || caller_tier == ProviderTier::Private {
        return None;
    }
    Some(ErrorData::new(
        ErrorCode::INVALID_REQUEST,
        format!(
            "`{extension}` is a private extension: it reaches data held inside the institution, \
             so only a private model may call it. This session is running on a public model. \
             Ask the user to switch this chat to a private model — Settings > Models, or the \
             model chip in the composer — and then try again. This is a data-protection \
             boundary set by the Biorouter marketplace, not something to work around: do not \
             retry with a different tool name, through code execution, or through a resource \
             read."
        ),
        None,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Gate C's refusal, exercised the way `check_enable_allowed`'s four tests
    /// exercise it (`extension_manager_extension.rs`): no global config, no
    /// session, no provider — because the function is pure. Same register, too:
    /// name the state, name the reason, foreclose the workaround, name the
    /// human action.
    #[test]
    fn the_refusal_is_pure_deterministic_and_forecloses_the_workaround() {
        use ProviderTier::{Private, Public};

        // The three permitted combinations. Only "private extension, public
        // caller" is a boundary crossing.
        assert!(privacy_refusal("ucsfomopagent", Private, Private).is_none());
        assert!(privacy_refusal("developer", Public, Public).is_none());
        assert!(privacy_refusal("developer", Public, Private).is_none());

        let e = privacy_refusal("ucsfomopagent", Private, Public).unwrap();
        let m = e.message.to_string();
        assert!(m.contains("ucsfomopagent"), "{m}");
        assert!(m.contains("private"), "{m}");
        assert!(m.contains("marketplace"), "{m}"); // names the grantor (R11)
        assert!(m.contains("Settings"), "{m}"); // names the human action
        assert!(m.contains("do not"), "{m}"); // forecloses the workaround

        // Deterministic: a model that sees a different string on retry
        // concludes the refusal is transient and loops.
        assert_eq!(
            m,
            privacy_refusal("ucsfomopagent", Private, Public)
                .unwrap()
                .message
                .to_string()
        );
    }

    /// §14.4 again, for the one refusal that reaches the MODEL's context: it may
    /// name the extension and the tier and nothing else. There is no session in
    /// the signature, so this is a statement about what the function *cannot*
    /// say rather than about what it happens not to.
    #[test]
    fn gate_c_names_the_extension_and_carries_no_session_content() {
        let m = privacy_refusal("ucsfomopagent", ProviderTier::Private, ProviderTier::Public)
            .unwrap()
            .message
            .to_string();
        for content in ["20260801_7", "Patient MRN 4471 workup", "phi/cohort-3"] {
            assert!(!m.contains(content), "{m}");
        }
    }

    #[test]
    fn a_refusal_survives_a_round_trip_through_anyhow() {
        // The whole reason this is a typed error: the route handler has an
        // `anyhow::Error` and owes a 409 rather than a 500.
        let err: anyhow::Error = PrivacyRefusal::PublicModelOnPrivateSession {
            session_id: "s1".into(),
            provider: "anthropic".into(),
        }
        .into();
        let refusal = err
            .downcast_ref::<PrivacyRefusal>()
            .expect("a privacy refusal must survive anyhow");
        assert_eq!(
            refusal.session_classification(),
            Some(SessionClassification::Private)
        );
        assert_eq!(refusal.provider_tier(), Some(ProviderTier::Public));
        assert_eq!(refusal.session_id(), Some("s1"));
    }

    /// §14.4: refusals are rendered to the user and (for Gate C) to the model,
    /// so the text may name the tool and the tier and nothing else. The id is
    /// carried as data for the handler; it must not be in the sentence.
    #[test]
    fn the_message_names_the_provider_and_the_tier_and_no_session_content() {
        let refusal = PrivacyRefusal::PublicModelOnPrivateSession {
            session_id: "20260801_7".into(),
            provider: "anthropic".into(),
        };
        let message = refusal.to_string();
        assert!(message.contains("anthropic"), "{message}");
        assert!(message.contains("private"), "{message}");
        assert!(
            !message.contains("20260801_7"),
            "a refusal must not carry the session id into text the user or the model reads: \
             {message}"
        );
    }

    /// The same §14.4 rule for Gate B's turn refusal, which is the one refusal
    /// built from a whole `Session` row — so it is the one with the session's
    /// name and working directory within arm's reach of the format string.
    #[test]
    fn a_turn_refusal_names_the_model_and_no_session_content() {
        let session = Session {
            id: "20260801_7".into(),
            name: "Patient MRN 4471 workup".into(),
            working_dir: std::path::PathBuf::from("/Users/someone/phi/cohort-3"),
            provider_name: Some("anthropic".into()),
            privacy_tier: SessionClassification::Private,
            ..Default::default()
        };
        let text = turn_refusal(&session);
        assert!(text.contains("anthropic"), "{text}");
        assert!(text.contains("private"), "{text}");
        for content in ["20260801_7", "Patient MRN 4471 workup", "phi/cohort-3"] {
            assert!(
                !text.contains(content),
                "a refusal carried session content ({content}) into text the user reads: {text}"
            );
        }
    }

    /// The marker is what the desktop client and `gate_b_turn_tests` key on to
    /// tell a refusal from a completed turn. A reword that dropped it would
    /// make both of them read every refusal as a turn that ran.
    #[test]
    fn every_turn_refusal_carries_the_marker() {
        for provider in [Some("anthropic".to_string()), None] {
            let session = Session {
                provider_name: provider,
                privacy_tier: SessionClassification::Private,
                ..Default::default()
            };
            let text = turn_refusal(&session);
            assert!(text.contains(TURN_REFUSAL_MARKER), "{text}");
        }
    }

    #[test]
    fn only_an_upward_bind_needs_the_user() {
        use ProviderTier::{Private, Public};
        assert!(raise_needs_user_action(Public, Private)); // the one raise
        assert!(!raise_needs_user_action(Public, Public)); // sideways
        assert!(!raise_needs_user_action(Private, Private)); // sideways
        assert!(!raise_needs_user_action(Private, Public)); // downward — Gate A's job, not this one
    }

    #[test]
    fn a_refusal_names_nothing_the_caller_did_not_ask_for() {
        // R10's disclosure bound. A refusal that says "pick versa_azure instead"
        // tells a public model which providers are private, and a refusal that
        // says "ucsfomopagent is private, cdwagent is not" is a classification
        // oracle the model can drive one name at a time.
        let msg = PrivacyRefusal::TierRaiseNeedsUser {
            requested: "llamacpp".into(),
        }
        .to_string();
        for other in ["versa_azure", "versa_bedrock", "ollama"] {
            assert!(
                !msg.contains(other),
                "refusal leaked the classification of {other}"
            );
        }
        // …and the loop above only means something if the name IS rendered.
        // While `requested` was carried but never interpolated, every assertion
        // in that loop held against a refusal that named nothing at all.
        assert!(
            msg.contains("llamacpp"),
            "the caller's own name may be named — and must be, or the loop above is vacuous"
        );

        // The private extension set comes from the generator, not from a
        // hand-written list here: a hand-written one stops tracking it and the
        // assertion goes quietly vacuous.
        let msg = PrivacyRefusal::PrivateExtensionOverHttp {
            name: "ucsfomopagent".into(),
        }
        .to_string();
        for other in crate::privacy::private_extension_ids().filter(|id| *id != "ucsfomopagent") {
            assert!(
                !msg.contains(other),
                "refusal leaked the classification of {other}"
            );
        }
        assert!(
            msg.contains("ucsfomopagent"),
            "the caller's own name may be named"
        );
    }

    #[test]
    fn every_refusal_ends_in_the_same_two_ways_out_sentence() {
        // DR-16's knock-on: "switch this chat to a private model" is step 1 of
        // the two-ways-out message in EVERY refusal this feature ships, and here
        // it becomes something the model hands to the USER instead of following.
        // One constant, so the two audiences cannot drift into two vocabularies.
        for msg in [
            PrivacyRefusal::TierRaiseNeedsUser {
                requested: "llamacpp".into(),
            }
            .to_string(),
            PrivacyRefusal::PrivateExtensionOverHttp {
                name: "ucsfomopagent".into(),
            }
            .to_string(),
            PrivacyRefusal::CapabilityConfigNeedsUser {
                key: "BIOROUTER_PROVIDER".into(),
            }
            .to_string(),
            PrivacyRefusal::spawn_upgrade(ProviderTier::Private).to_string(),
        ] {
            assert!(msg.contains(ASK_THE_USER_TO_SWITCH), "{msg}");
            assert!(
                msg.contains("Do not retry"),
                "a refusal the model will retry is a loop: {msg}"
            );
        }
    }

    /// R4's refusal, which Task 23's spawn gate is the only caller of. §14.4:
    /// it reaches the model's context, so it may name the boundary and nothing
    /// else — not the parent's provider (which would tell a public model which
    /// providers are private), not a session id.
    #[test]
    fn the_spawn_refusal_names_the_boundary_and_no_provider() {
        let refusal = PrivacyRefusal::spawn_upgrade(ProviderTier::Private);
        let msg = refusal.to_string();
        assert!(msg.contains("public model"), "{msg}");
        assert!(msg.contains("subagent"), "{msg}");
        // It is a refusal, not a warning: nothing was started.
        assert!(msg.contains("No subagent was started"), "{msg}");
        for leak in [
            "versa_azure",
            "versa_bedrock",
            "ollama",
            "llamacpp",
            "anthropic",
            "20260801_7",
        ] {
            assert!(!msg.contains(leak), "spawn refusal leaked {leak}: {msg}");
        }
        // The child's tier is carried for the handler, and the refusal is about
        // a capability rather than about a stored session.
        assert_eq!(refusal.provider_tier(), Some(ProviderTier::Private));
        assert_eq!(refusal.session_classification(), None);
        assert_eq!(refusal.session_id(), None);
    }

    /// DR-19's refusal, the other direction. Same §14.4 bound, and one extra
    /// obligation the R4 refusal does not carry: it must name the way out.
    ///
    /// The way out is a NEW CHAT, not [`ASK_THE_USER_TO_SWITCH`] — this chat is
    /// already private, so "switch to a private model" is advice it has already
    /// taken. That is why this variant is deliberately absent from
    /// `every_refusal_ends_in_the_same_two_ways_out_sentence`, and the exclusion
    /// is asserted here rather than left as a silent omission from a list.
    #[test]
    fn the_downgrade_refusal_names_the_boundary_and_the_way_out_and_no_provider() {
        let refusal = PrivacyRefusal::spawn_downgrade(ProviderTier::Public);
        let msg = refusal.to_string();
        assert!(msg.contains("public model"), "{msg}");
        assert!(msg.contains("No subagent was started"), "{msg}");
        assert!(
            msg.contains("Do not retry"),
            "a refusal the model will retry is a loop: {msg}"
        );
        // DR-19's second half, said out loud to the model: the wall is not
        // something it can unlock by writing a hook or flipping a mode.
        assert!(
            msg.contains("no setting, hook or permission mode changes it"),
            "{msg}"
        );
        // The way out, and it is a different one.
        assert!(msg.contains("start a new chat"), "{msg}");
        assert!(
            !msg.contains(ASK_THE_USER_TO_SWITCH),
            "this chat is already private; telling the user to switch to a private model is \
             advice it has already taken: {msg}"
        );

        // R10 / §14.4: it names neither a provider nor any session content.
        // These are provider names rather than extension ids, so there is no
        // generator to draw them from — the list is the same one the sibling
        // spawn-refusal test uses, and the `start a new chat` assertion above is
        // what keeps this loop from passing against a message that says nothing.
        for leak in [
            "versa_azure",
            "versa_bedrock",
            "ollama",
            "llamacpp",
            "anthropic",
        ] {
            assert!(
                !msg.contains(leak),
                "downgrade refusal leaked {leak}: {msg}"
            );
        }
        for content in ["20260801_7", "Patient MRN 4471 workup", "phi/cohort-3"] {
            assert!(!msg.contains(content), "{msg}");
        }

        assert_eq!(refusal.provider_tier(), Some(ProviderTier::Public));
        assert_eq!(refusal.session_classification(), None);
        assert_eq!(refusal.session_id(), None);
    }

    /// DR-21's refusal. Same §14.4 bound as the spawn pair, and the same extra
    /// obligation: it must name a way out, and its way out is a *fresh app
    /// session* rather than [`ASK_THE_USER_TO_SWITCH`] — an app has no model
    /// picker, and switching this session is the very thing being refused. That
    /// exclusion is asserted here rather than left as a silent omission from
    /// `every_refusal_ends_in_the_same_two_ways_out_sentence`'s list.
    #[test]
    fn the_app_tier_refusal_names_the_boundary_and_the_way_out_and_no_other_provider() {
        let refusal = PrivacyRefusal::AppSessionTierFixed {
            requested: "llamacpp".into(),
        };
        let msg = refusal.to_string();

        // It says what the boundary IS: the tier is fixed at creation.
        assert!(msg.contains("created"), "{msg}");
        // It is a refusal, not a warning: nothing moved.
        assert!(msg.contains("Nothing was changed"), "{msg}");
        assert!(
            msg.contains("Do not retry"),
            "a refusal the model will retry is a loop: {msg}"
        );
        // The wall is not something the app can unlock by rewriting its own
        // manifest — which is the channel this refusal exists to close.
        assert!(msg.contains("no manifest field, frame or setting"), "{msg}");
        // The way out, and it is a different one.
        assert!(msg.contains("start a fresh session"), "{msg}");
        assert!(
            !msg.contains(ASK_THE_USER_TO_SWITCH),
            "an app has no model picker, and switching this session is what was refused: {msg}"
        );

        // R10's disclosure bound: the caller's own name may be named — and must
        // be, or the loop below is vacuous — but no other provider's tier may be
        // inferable from it.
        assert!(msg.contains("llamacpp"), "{msg}");
        for leak in ["versa_azure", "versa_bedrock", "ollama", "anthropic"] {
            assert!(!msg.contains(leak), "app refusal leaked {leak}: {msg}");
        }
        for content in ["20260801_7", "Patient MRN 4471 workup", "phi/cohort-3"] {
            assert!(!msg.contains(content), "{msg}");
        }

        // A channel refusal, like its DR-16 siblings: no fabricated session/model
        // pair for a repair card the app cannot act on.
        assert_eq!(refusal.provider_tier(), None);
        assert_eq!(refusal.session_classification(), None);
        assert_eq!(refusal.session_id(), None);
    }

    /// The two spawn refusals are opposite crossings and must not collapse into
    /// one sentence: a model that gets the R4 wording for a DR-19 crossing is
    /// told to ask for a private model, which is the reverse of what it should
    /// do — and the whole point of the second variant is that its way out is
    /// different.
    #[test]
    fn the_two_spawn_refusals_do_not_say_the_same_thing() {
        let upgrade = PrivacyRefusal::spawn_upgrade(ProviderTier::Private).to_string();
        let downgrade = PrivacyRefusal::spawn_downgrade(ProviderTier::Public).to_string();
        assert_ne!(upgrade, downgrade);
        assert!(upgrade.contains(ASK_THE_USER_TO_SWITCH), "{upgrade}");
        assert!(!downgrade.contains(ASK_THE_USER_TO_SWITCH), "{downgrade}");
        assert!(!upgrade.contains("start a new chat"), "{upgrade}");
        assert!(downgrade.contains("start a new chat"), "{downgrade}");
    }

    /// The two refusals the desktop renderer can receive from the model picker
    /// both carry the marker it keys on, and the one it cannot receive is not
    /// required to.
    ///
    /// `changeModel` calls `/agent/update_provider` and then
    /// `/config/set_provider`; a DR-16 refusal from either is the same fact —
    /// this backend was handed no user-action key — and gets the same user-facing
    /// explanation. `/agent/add_extension` is not on that path, and its refusal
    /// is about the extension rather than about the proof, so it is deliberately
    /// outside the marker.
    #[test]
    fn the_two_refusals_the_picker_can_receive_carry_the_marker_the_renderer_keys_on() {
        for msg in [
            PrivacyRefusal::TierRaiseNeedsUser {
                requested: "llamacpp".into(),
            }
            .to_string(),
            PrivacyRefusal::CapabilityConfigNeedsUser {
                key: "BIOROUTER_PROVIDER".into(),
            }
            .to_string(),
        ] {
            assert!(
                msg.contains(USER_ACTION_REFUSAL_MARKER),
                "the renderer cannot tell this refusal from a 500 and will report it as a \
                 provider failure: {msg}"
            );
        }
        assert!(
            !PrivacyRefusal::PrivateExtensionOverHttp {
                name: "ucsfomopagent".into(),
            }
            .to_string()
            .contains(USER_ACTION_REFUSAL_MARKER),
            "the extension refusal is not a missing-user-proof refusal and must not claim to be"
        );
    }

    /// Gate D's refusal is a constant for a reason (a model that sees a
    /// different string on retry concludes the refusal is transient and loops),
    /// and it is subject to the same content rule.
    #[test]
    fn the_chatrecall_refusal_is_stable_and_names_no_target() {
        assert_eq!(chatrecall_load_refusal(), chatrecall_load_refusal());
        assert!(chatrecall_load_refusal().contains("private"));
        assert!(!chatrecall_load_refusal().contains("session id\": "));
    }
}
