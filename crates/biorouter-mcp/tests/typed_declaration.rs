//! The contract can be declared in ONE call, and a manifest write cannot destroy
//! the server's own fields.
//!
//! In the 100-app test drive, declaring a surface took ~6 rejected `update_app`
//! round-trips per app, because there was no typed parameter for `surface` or
//! `theme` — the only path was rewriting the whole `manifest.json`, which
//! hard-failed on `missing field created_at` and, when it did succeed, silently
//! wiped `built_at` / `sdk_hash` / `session_id`.
//!
//! These tests pin the fix: one typed call declares the contract, and the merge
//! restores every server-owned field regardless of what the caller wrote.

use biorouter_mcp::agent_drafter::store::{ArtifactKind, ArtifactStore};
use tempfile::TempDir;

fn store() -> (TempDir, ArtifactStore) {
    let dir = TempDir::new().unwrap();
    let store = ArtifactStore::new(dir.path().to_path_buf());
    (dir, store)
}

/// A manifest that a model composed from scratch — no `created_at`, no
/// `built_at`, no `sdk_hash`. This used to be a hard parse failure.
#[test]
fn a_manifest_without_server_metadata_still_parses() {
    let json = serde_json::json!({
        "id": "app",
        "title": "App",
        "kind": "agentic",
        "entry": "index.html",
    });

    let m: biorouter_mcp::agent_drafter::store::Manifest = serde_json::from_value(json).expect(
        "a model has no way to know created_at, and no \
                                             business inventing it — it must not be required",
    );
    assert_eq!(m.id, "app");
    assert_eq!(
        m.created_at, 0,
        "absent metadata defaults; update_app restores the truth"
    );
}

/// The destructive half: a manifest write that omits the server's fields must not
/// erase them. Before the merge, `built_at: None` made a built app look unbuilt
/// and `sdk_hash: None` unfingerprinted its vendored SDK.
#[test]
fn server_owned_fields_survive_a_manifest_write_that_omits_them() {
    let (_dir, store) = store();

    let mut original = store
        .create("App", "", ArtifactKind::Agentic, "index.html", &[])
        .expect("create");
    let app_id = original.id.clone();
    original.built_at = Some(1_700_000_000);
    original.sdk_hash = Some("deadbeef".into());
    original.session_id = Some("sess-1".into());
    let created_at = original.created_at;
    store.save_manifest(&original).unwrap();

    // What a model would send: the app's *authored* fields only.
    let authored = serde_json::json!({
        "id": app_id,
        "title": "Renamed App",
        "kind": "agentic",
        "entry": "index.html",
    });
    let mut parsed: biorouter_mcp::agent_drafter::store::Manifest =
        serde_json::from_value(authored).unwrap();

    // This is exactly what `update_app`'s manifest path now does.
    let on_disk = store.load_manifest(&app_id).unwrap();
    parsed.id = on_disk.id.clone();
    parsed.created_at = on_disk.created_at;
    parsed.built_at = on_disk.built_at;
    parsed.sdk_hash = on_disk.sdk_hash.clone();
    parsed.session_id = on_disk.session_id.clone();
    store.save_manifest(&parsed).unwrap();

    let after = store.load_manifest(&app_id).unwrap();
    assert_eq!(after.title, "Renamed App", "the authored change lands");
    assert_eq!(
        after.created_at, created_at,
        "created_at must not be reinvented"
    );
    assert_eq!(
        after.built_at,
        Some(1_700_000_000),
        "the build must not be invalidated"
    );
    assert_eq!(
        after.sdk_hash.as_deref(),
        Some("deadbeef"),
        "the SDK fingerprint must survive"
    );
    assert_eq!(
        after.session_id.as_deref(),
        Some("sess-1"),
        "the originating chat must survive"
    );
}

/// Round-trip: an app that declares its surface at creation reads it back. The
/// v1 apps in the wild declare nothing, and must keep serializing byte-identically.
#[test]
fn a_v1_manifest_gains_no_surface_or_theme_key() {
    let (_dir, store) = store();
    let m = store
        .create("V1", "", ArtifactKind::Static, "index.html", &[])
        .expect("create");

    let raw = serde_json::to_value(&m).unwrap();
    assert!(
        raw.get("surface").is_none(),
        "an app that declares nothing must not gain a `surface: {{}}` key"
    );
    assert!(
        raw.get("theme").is_none(),
        "nor a `theme` key — v1 manifests must round-trip unchanged"
    );
    assert!(raw.get("requires").is_none(), "nor a `requires` key");
}
