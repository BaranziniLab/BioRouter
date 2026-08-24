//! Issue #117: **the credential must not be anywhere the model can reach.**
//!
//! Every test here asserts an absence. That is deliberate: an implementation
//! that renders the dialog perfectly, stores the value correctly and *also*
//! writes it into `config.yaml` would pass every functional test ever written
//! for this feature. So these look at the four places a value could end up —
//! the plaintext config, a published card, a parked call's outcome, and a
//! diagnostics bundle — and require it in none of them.
//!
//! Runs in its own binary with `BIOROUTER_PATH_ROOT` pointed at a temp tree, so
//! it reads and writes a sandbox rather than the developer's real config.

use std::collections::HashMap;
use std::sync::OnceLock;

use biorouter::conversation::message::{SecretDestination, SecretKeyRequest};
use biorouter::extension_install::{
    classify_supplied, compose_config, submit_credentials, BrxtEnvVar, BrxtManifest,
    CredentialSpec, SubmitOutcome,
};
use biorouter::pending_user_action::{
    PendingUserActions, ResolveOutcome, SecretsRequest, UserActionOutcome, UserActionRequest,
};

/// The value that must never turn up anywhere below.
const SECRET: &str = "sk-live-please-do-not-leak-this-anywhere";

/// One sandbox for the whole binary. `Config::global()` resolves its paths once,
/// on first use, so this has to be set before any test touches it — and every
/// test calls it first.
fn sandbox() -> &'static std::path::Path {
    static ROOT: OnceLock<tempfile::TempDir> = OnceLock::new();
    let dir = ROOT.get_or_init(|| {
        let dir = tempfile::tempdir().expect("a temp dir");
        std::env::set_var("BIOROUTER_PATH_ROOT", dir.path());
        // Plaintext `secrets.yaml` instead of the OS store: the assertion is
        // about what reaches `config.yaml`, and a test must never write into the
        // developer's real keychain to make it.
        std::env::set_var("BIOROUTER_DISABLE_KEYRING", "true");
        for sub in ["config", "data", "state"] {
            std::fs::create_dir_all(dir.path().join(sub)).expect("a sandbox tree");
        }
        // `Config` opens these on first read; an absent file is an error, not an
        // empty config.
        std::fs::write(dir.path().join("config/config.yaml"), "{}\n").expect("a config file");
        std::fs::write(dir.path().join("config/secrets.yaml"), "{}\n").expect("a secrets file");
        dir
    });
    dir.path()
}

fn var(key: &str, required: bool, secret: bool) -> BrxtEnvVar {
    BrxtEnvVar {
        key: key.to_string(),
        required,
        auto_propagate: false,
        default: None,
        description: String::new(),
        secret,
    }
}

fn manifest(env_vars: Vec<BrxtEnvVar>) -> BrxtManifest {
    BrxtManifest {
        name: "spokeagent".to_string(),
        display_name: "SPOKE Agent".to_string(),
        description: "SPOKE knowledge graph".to_string(),
        version: "0.4.1".to_string(),
        entry_point: "main.py".to_string(),
        repository: "https://github.com/BaranziniLab/SPOKEAgent".to_string(),
        tools_count: None,
        env_vars,
    }
}

/// `config.yaml` is a plaintext file. A credential in its `envs` map is a
/// credential in the user's home directory in the clear, readable by every tool
/// on the machine — and by the agent's own shell.
#[test]
fn a_declared_secret_never_reaches_the_plaintext_config_entry() {
    sandbox();
    let m = manifest(vec![
        var("SPOKEAGENT_PASSCODE", true, true),
        var("SPOKE_HOST", false, false),
    ]);
    let supplied = HashMap::from([
        ("SPOKEAGENT_PASSCODE".to_string(), SECRET.to_string()),
        ("SPOKE_HOST".to_string(), "https://spoke.ucsf".to_string()),
    ]);

    let (secret_keys, settings) = classify_supplied(&m, &supplied);
    assert_eq!(secret_keys, vec!["SPOKEAGENT_PASSCODE".to_string()]);
    assert_eq!(
        settings,
        HashMap::from([("SPOKE_HOST".to_string(), "https://spoke.ucsf".to_string())]),
        "an ordinary setting belongs in the config; a credential does not"
    );

    let config = compose_config(
        &m,
        std::path::Path::new("/ext/spokeagent"),
        settings,
        secret_keys,
    );
    let serialised = serde_yaml::to_string(&config).expect("a serialisable config entry");
    assert!(
        !serialised.contains(SECRET),
        "the credential reached config.yaml:\n{serialised}"
    );
    // The NAME is there — that is how the daemon knows to fetch it at launch.
    assert!(serialised.contains("SPOKEAGENT_PASSCODE"));
    assert!(serialised.contains("https://spoke.ucsf"));
}

/// A key the manifest never declared is a setting, not a credential. Guessing
/// the other way would silently hide a value the user expected to see.
#[test]
fn an_undeclared_key_is_treated_as_a_setting() {
    sandbox();
    let m = manifest(vec![var("SPOKEAGENT_PASSCODE", true, true)]);
    let supplied = HashMap::from([("SPOKE_TIMEOUT".to_string(), "30".to_string())]);
    let (secret_keys, settings) = classify_supplied(&m, &supplied);
    assert!(secret_keys.is_empty());
    assert_eq!(
        settings.get("SPOKE_TIMEOUT").map(String::as_str),
        Some("30")
    );
}

/// The card is published into the session's action-required queue and rendered
/// in the chat. It asks for key names; it must have no shape a value fits in.
#[test]
fn the_published_card_carries_no_value() {
    sandbox();
    let content = biorouter::conversation::message::MessageContent::action_required_secrets(
        "card-1",
        "SPOKE Agent needs 1 value before it can run.".to_string(),
        vec![SecretKeyRequest {
            key: "SPOKEAGENT_PASSCODE".to_string(),
            label: "SPOKEAGENT_PASSCODE".to_string(),
            description: Some("From the UCSF wiki credentials page".to_string()),
            required: true,
        }],
        SecretDestination::ExtensionEnv {
            extension_name: "spokeagent".to_string(),
        },
    );
    let json = serde_json::to_string(&content).expect("a serialisable card");
    assert!(json.contains("SPOKEAGENT_PASSCODE"));
    assert!(!json.contains(SECRET));
    // Not merely absent for this input — there is no field for one.
    assert!(!json.contains("\"value\""));
}

/// The registry refuses a value-bearing answer to a credential card, and the
/// caller stays parked rather than receiving one. This is the guarantee that
/// holds for surfaces `pending_user_action` has never heard of.
#[tokio::test]
async fn a_value_bearing_answer_to_a_credential_card_is_refused() {
    sandbox();
    let registry = PendingUserActions::global();
    let parked = registry.park(
        Some("s-refuse"),
        None,
        UserActionRequest::Secrets(SecretsRequest {
            prompt: "Configure".to_string(),
            keys: vec![SecretKeyRequest {
                key: "SPOKEAGENT_PASSCODE".to_string(),
                label: "SPOKEAGENT_PASSCODE".to_string(),
                description: None,
                required: true,
            }],
            destination: SecretDestination::Keyring,
        }),
    );
    let id = parked.id().to_string();

    let outcome = registry.resolve(
        &id,
        UserActionOutcome::Provided {
            data: serde_json::json!({ "SPOKEAGENT_PASSCODE": SECRET }),
        },
    );
    assert_eq!(
        outcome,
        ResolveOutcome::Rejected,
        "a credential card must not accept a data-bearing outcome"
    );
    assert!(
        registry.is_pending(&id),
        "a refused answer must leave the caller parked, not release it with the value"
    );

    registry.resolve(&id, UserActionOutcome::Cancelled);
    drop(parked);
}

/// What the parked install — and therefore the model — is told.
#[tokio::test]
async fn the_install_learns_key_names_and_nothing_else() {
    sandbox();
    let registry = PendingUserActions::global();
    let parked = biorouter::extension_install::park_credentials(
        Some("s-names"),
        None,
        "Configure".to_string(),
        CredentialSpec {
            destination: SecretDestination::ExtensionEnv {
                extension_name: "spokeagent".to_string(),
            },
            vars: vec![var("SPOKEAGENT_PASSCODE", true, true)],
        },
    );
    let id = parked.id().to_string();

    let outcome = submit_credentials(
        &id,
        HashMap::from([("SPOKEAGENT_PASSCODE".to_string(), SECRET.to_string())]),
    );
    assert_eq!(
        outcome,
        SubmitOutcome::Configured {
            configured_keys: vec!["SPOKEAGENT_PASSCODE".to_string()]
        }
    );

    let (released, settings) = parked.wait(std::time::Duration::from_secs(5), None).await;
    match &released {
        UserActionOutcome::SecretsConfigured { configured_keys } => {
            assert_eq!(configured_keys, &vec!["SPOKEAGENT_PASSCODE".to_string()]);
        }
        other => panic!("expected SecretsConfigured, got {other:?}"),
    }
    // The credential half of the answer never comes back through here.
    assert!(
        !settings.contains_key("SPOKEAGENT_PASSCODE"),
        "a declared secret must not arrive as a plain setting"
    );
    let rendered = format!("{released:?}");
    assert!(
        !rendered.contains(SECRET),
        "the outcome's Debug rendering leaked the value: {rendered}"
    );

    // ...and it did land where it was supposed to.
    let stored: String = biorouter::config::Config::global()
        .get_secret("SPOKEAGENT_PASSCODE")
        .expect("the credential store holds it");
    assert_eq!(stored, SECRET);

    let _ = registry.is_pending(&id);
}

/// Diagnostics bundles are attached to bug reports. `secrets.yaml` (and its
/// keyring equivalent) is where the credential lives, so nothing that collects
/// diagnostics may read it.
///
/// A structural assertion rather than a behavioural one, because the failure it
/// guards against is somebody adding one `read_to_string` to a collector — which
/// no behavioural test of today's bundle would ever fail on.
#[test]
fn nothing_in_the_diagnostics_collector_reads_the_secret_store() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/")
        .join("biorouter-server/src/routes/status.rs");
    let source = std::fs::read_to_string(&root).unwrap_or_default();
    assert!(
        !source.contains("secrets.yaml"),
        "the diagnostics route reads the secret store: {}",
        root.display()
    );
    assert!(
        !source.contains("get_secrets("),
        "the diagnostics route enumerates stored secrets: {}",
        root.display()
    );
}

/// A credential card is a decision prompt, not a record.
///
/// It must not be persisted into the session row: reopening the chat would then
/// show a live-looking dialog for an install that finished long ago, and — worse
/// for this feature — every surface that replays a session (export, share,
/// `chatrecall`, a subagent's flattened transcript) would carry the card
/// forward. It carries no value, but a card that cannot be answered is still a
/// prompt inviting one.
#[test]
fn a_credential_card_is_never_written_into_the_session() {
    sandbox();
    let card = biorouter::conversation::message::Message::assistant().with_content(
        biorouter::conversation::message::MessageContent::action_required_secrets(
            "card-1",
            "SPOKE Agent needs 1 value.".to_string(),
            vec![SecretKeyRequest {
                key: "SPOKEAGENT_PASSCODE".to_string(),
                label: "SPOKEAGENT_PASSCODE".to_string(),
                description: None,
                required: true,
            }],
            SecretDestination::Keyring,
        ),
    );
    assert!(
        biorouter::pending_user_action::is_ephemeral_card(&card),
        "a credential card must be ephemeral, or the drain persists it"
    );
}

/// What a provider actually receives.
///
/// The coding-agent providers flatten the whole conversation into one prompt —
/// the widest possible outbound payload, and the one place a stray credential
/// would be hardest to notice. The card is in the conversation here on purpose:
/// even if a future change stopped it being ephemeral, flattening it must still
/// produce no value, because there is none in it to produce.
#[test]
fn flattening_a_conversation_that_contains_the_card_yields_no_value() {
    sandbox();
    use biorouter::conversation::message::{Message, MessageContent};

    let messages = vec![
        Message::user().with_text("Install the SPOKE extension for me"),
        Message::assistant().with_content(MessageContent::action_required_secrets(
            "card-1",
            "SPOKE Agent needs 1 value before it can run.".to_string(),
            vec![SecretKeyRequest {
                key: "SPOKEAGENT_PASSCODE".to_string(),
                label: "SPOKEAGENT_PASSCODE".to_string(),
                description: Some("From the UCSF wiki".to_string()),
                required: true,
            }],
            SecretDestination::ExtensionEnv {
                extension_name: "spokeagent".to_string(),
            },
        )),
        // The redacted audit line the chat is allowed to keep.
        Message::assistant()
            .with_text("Credentials configured for SPOKE Agent: SPOKEAGENT_PASSCODE"),
    ];

    let flattened =
        biorouter::providers::coding_agent::transcript::flatten(&messages).unwrap_or_default();
    assert!(
        !flattened.contains(SECRET),
        "the flattened provider prompt carried the credential:\n{flattened}"
    );
    // The audit line survives — it is the useful, safe half.
    assert!(flattened.contains("SPOKEAGENT_PASSCODE"));
}
