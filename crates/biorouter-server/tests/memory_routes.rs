//! Integration tests for the `/memory` HTTP routes — the ones behind the
//! Settings surface that lets a user see and delete what has been remembered
//! (issue #63).
//!
//! The routes are driven through a router bound to a throwaway global store, so
//! nothing here reads or deletes the machine's real memories.

// Redirects this binary's Biorouter data/config/state dirs at a throwaway root
// before `main`, so nothing here can open the developer's real `sessions.db`.
#[path = "../src/test_sandbox.rs"]
mod test_sandbox;

use axum::{body::Body, http::Request};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use tempfile::TempDir;
use tower::ServiceExt;

/// A router over `<temp>/global`, plus the temp dir the caller uses as the
/// project root for the local store.
fn app(temp: &TempDir) -> axum::Router {
    biorouter_server::routes::memory::router_with_global_store(temp.path().join("global"))
}

fn write_store(dir: &Path, category: &str, body: &str) {
    fs::create_dir_all(dir).unwrap();
    fs::write(dir.join(format!("{category}.txt")), body).unwrap();
}

fn global_store(temp: &TempDir) -> std::path::PathBuf {
    temp.path().join("global")
}

/// The project directory a window is open in; its store is
/// `<project>/.biorouter/memory`.
fn project(temp: &TempDir) -> std::path::PathBuf {
    temp.path().join("project")
}

fn local_store(temp: &TempDir) -> std::path::PathBuf {
    project(temp).join(".biorouter").join("memory")
}

async fn body_json(res: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

async fn get(temp: &TempDir, uri: &str) -> (axum::http::StatusCode, Value) {
    let res = app(temp)
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    (res.status(), body_json(res).await)
}

async fn post(temp: &TempDir, uri: &str, payload: Value) -> (axum::http::StatusCode, Value) {
    let res = app(temp)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    (res.status(), body_json(res).await)
}

/// The response body as text, for asserting on an error message.
async fn post_text(temp: &TempDir, uri: &str, payload: Value) -> (axum::http::StatusCode, String) {
    let res = app(temp)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

fn inventory_uri(temp: &TempDir) -> String {
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("working_dir", &project(temp).display().to_string())
        .finish();
    format!("/memory/inventory?{query}")
}

/// One category exactly as the Settings surface would render it: the rows and
/// the revision a delete then has to carry back. Fetched through the inventory
/// route, so every delete below is guarded by what a client actually saw.
async fn listed(temp: &TempDir, scope: &str, category: &str) -> Value {
    let (_, body) = get(temp, &inventory_uri(temp)).await;
    body[scope]["categories"]
        .as_array()
        .unwrap_or(&Vec::new())
        .iter()
        .find(|c| c["name"] == category)
        .unwrap_or_else(|| panic!("category {category:?} is not in the {scope} inventory"))
        .clone()
}

/// The whole point of the view: a user can see what is in the machine-wide
/// store, with the category, the body, the tags, and where the store lives.
#[tokio::test]
async fn the_inventory_shows_both_stores_with_their_contents() {
    let temp = TempDir::new().unwrap();
    write_store(
        &global_store(&temp),
        "clinical",
        "# phi cohort\nthe cohort has 812 patients\n\n",
    );
    write_store(
        &local_store(&temp),
        "development",
        "we format with black\n\n",
    );

    let (status, body) = get(&temp, &inventory_uri(&temp)).await;
    assert_eq!(status, 200);

    let global = &body["global"];
    assert_eq!(global["scope"], "global");
    assert_eq!(global["exists"], true);
    assert_eq!(global["path"], global_store(&temp).display().to_string());
    assert_eq!(global["categories"][0]["name"], "clinical");
    assert_eq!(
        global["categories"][0]["entries"][0]["content"],
        "the cohort has 812 patients"
    );
    assert_eq!(
        global["categories"][0]["entries"][0]["tags"],
        json!(["phi", "cohort"]),
        "tags are the only per-entry provenance the store has"
    );
    assert!(
        global["categories"][0]["modified"].is_i64(),
        "the category file's mtime is reported"
    );

    let local = &body["local"];
    assert_eq!(local["scope"], "local");
    assert_eq!(local["path"], local_store(&temp).display().to_string());
    assert_eq!(local["categories"][0]["name"], "development");
}

/// Settings can be opened from a window with no project. That must still show
/// the machine-wide store — it is the one the consent gate is about — and say
/// plainly that there is no local store to show, rather than inventing one out
/// of the daemon's own working directory.
#[tokio::test]
async fn without_a_project_the_global_store_is_still_listed_and_local_is_absent() {
    let temp = TempDir::new().unwrap();
    write_store(&global_store(&temp), "clinical", "a note\n\n");

    let (status, body) = get(&temp, "/memory/inventory").await;
    assert_eq!(status, 200);
    assert_eq!(body["global"]["categories"][0]["name"], "clinical");
    assert!(
        body["local"].is_null(),
        "no project means no local store, not the daemon's cwd"
    );
}

/// A store that has never been written to is empty, not an error.
#[tokio::test]
async fn empty_stores_list_as_empty_rather_than_failing() {
    let temp = TempDir::new().unwrap();

    let (status, body) = get(&temp, &inventory_uri(&temp)).await;
    assert_eq!(status, 200);
    assert_eq!(body["global"]["exists"], false);
    assert_eq!(body["global"]["categories"], json!([]));
    assert_eq!(body["local"]["exists"], false);
}

/// Deleting one memory deletes that memory.
#[tokio::test]
async fn deleting_an_entry_removes_exactly_that_entry() {
    let temp = TempDir::new().unwrap();
    write_store(
        &global_store(&temp),
        "clinical",
        "# phi\npatient A\n\n# phi\npatient B\n\n",
    );

    let listing = listed(&temp, "global", "clinical").await;
    let (status, body) = post(
        &temp,
        "/memory/delete_entry",
        json!({
            "scope": "global",
            "category": "clinical",
            "index": 0,
            "digest": listing["entries"][0]["digest"],
            "revision": listing["revision"],
        }),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["remaining"], 1);
    assert_eq!(body["category_removed"], false);

    let (_, after) = get(&temp, &inventory_uri(&temp)).await;
    assert_eq!(
        after["global"]["categories"][0]["entries"][0]["content"],
        "patient B"
    );
    assert_eq!(
        after["global"]["categories"][0]["entries"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

/// Removing the last memory in a category removes the category, and the
/// response says so — the Settings list has to stop showing it, and the global
/// category index in every future system prompt has to stop carrying its name.
#[tokio::test]
async fn deleting_the_last_entry_reports_that_the_category_went_with_it() {
    let temp = TempDir::new().unwrap();
    write_store(&global_store(&temp), "clinical", "the only note\n\n");

    let listing = listed(&temp, "global", "clinical").await;
    let (status, body) = post(
        &temp,
        "/memory/delete_entry",
        json!({
            "scope": "global",
            "category": "clinical",
            "index": 0,
            "digest": listing["entries"][0]["digest"],
            "revision": listing["revision"],
        }),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["category_removed"], true);
    assert_eq!(body["remaining"], 0);

    let (_, after) = get(&temp, &inventory_uri(&temp)).await;
    assert_eq!(after["global"]["categories"], json!([]));
}

/// An agent may be appending to the store while the list sits open. If the row
/// the user clicked is no longer the row at that index, the delete refuses
/// instead of taking out whatever moved into the slot.
#[tokio::test]
async fn a_stale_row_is_refused_with_conflict_and_deletes_nothing() {
    let temp = TempDir::new().unwrap();
    write_store(&global_store(&temp), "clinical", "first\n\nsecond\n\n");

    let listing = listed(&temp, "global", "clinical").await;
    let (status, text) = post_text(
        &temp,
        "/memory/delete_entry",
        json!({
            "scope": "global",
            "category": "clinical",
            "index": 0,
            // The row the user clicked is no longer the row at that index.
            "digest": "a digest of some other row",
            "revision": listing["revision"],
        }),
    )
    .await;
    assert_eq!(status, 409);
    assert!(
        text.to_lowercase().contains("changed"),
        "the message must tell the user to refresh, got: {text}"
    );

    let (_, after) = get(&temp, &inventory_uri(&temp)).await;
    assert_eq!(
        after["global"]["categories"][0]["entries"]
            .as_array()
            .unwrap()
            .len(),
        2,
        "a refused delete removes nothing"
    );
}

/// The row the user clicked can still be that row while the category around it
/// has changed — a conversation appended to it while the confirmation dialog
/// was open. Deleting then acts on a list the user never saw, so the route
/// refuses with a conflict and the client reloads (#63 review, finding 6).
#[tokio::test]
async fn an_append_since_the_listing_is_refused_with_conflict() {
    let temp = TempDir::new().unwrap();
    write_store(&global_store(&temp), "clinical", "first\n\n");
    let listing = listed(&temp, "global", "clinical").await;

    // A conversation saves a memory while the confirmation is open.
    fs::write(
        global_store(&temp).join("clinical.txt"),
        "first\n\narrived afterwards\n\n",
    )
    .unwrap();

    let (status, text) = post_text(
        &temp,
        "/memory/delete_entry",
        json!({
            "scope": "global",
            "category": "clinical",
            "index": 0,
            "digest": listing["entries"][0]["digest"],
            "revision": listing["revision"],
        }),
    )
    .await;
    assert_eq!(status, 409, "got: {text}");
    assert!(
        text.to_lowercase().contains("reload"),
        "the message must tell the user how to recover, got: {text}"
    );

    let (status, text) = post_text(
        &temp,
        "/memory/delete_category",
        json!({
            "scope": "global",
            "category": "clinical",
            "revision": listing["revision"],
        }),
    )
    .await;
    assert_eq!(status, 409, "got: {text}");

    let (_, after) = get(&temp, &inventory_uri(&temp)).await;
    assert_eq!(
        after["global"]["categories"][0]["entries"]
            .as_array()
            .unwrap()
            .len(),
        2,
        "a refused delete removes nothing"
    );

    // Reloading gives the current revision, and the delete goes through — a
    // conflict the client cannot recover from is a button that never works.
    let reloaded = listed(&temp, "global", "clinical").await;
    let (status, _) = post(
        &temp,
        "/memory/delete_entry",
        json!({
            "scope": "global",
            "category": "clinical",
            "index": 0,
            "digest": reloaded["entries"][0]["digest"],
            "revision": reloaded["revision"],
        }),
    )
    .await;
    assert_eq!(status, 200);
}

#[tokio::test]
async fn deleting_a_row_that_is_no_longer_there_is_a_not_found() {
    let temp = TempDir::new().unwrap();
    write_store(&global_store(&temp), "clinical", "only one\n\n");

    let listing = listed(&temp, "global", "clinical").await;
    let (status, _) = post_text(
        &temp,
        "/memory/delete_entry",
        json!({
            "scope": "global",
            "category": "clinical",
            "index": 9,
            "digest": listing["entries"][0]["digest"],
            "revision": listing["revision"],
        }),
    )
    .await;
    assert_eq!(status, 404);
}

/// The category delete reports what it cost, so the UI can confirm afterwards
/// what was actually lost.
#[tokio::test]
async fn deleting_a_category_reports_how_many_memories_went_with_it() {
    let temp = TempDir::new().unwrap();
    write_store(&global_store(&temp), "clinical", "one\n\ntwo\n\nthree\n\n");

    let listing = listed(&temp, "global", "clinical").await;
    let (status, body) = post(
        &temp,
        "/memory/delete_category",
        json!({
            "scope": "global",
            "category": "clinical",
            "revision": listing["revision"],
        }),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["removed_entries"], 3);

    let (status, _) = post_text(
        &temp,
        "/memory/delete_category",
        json!({
            "scope": "global",
            "category": "clinical",
            "revision": listing["revision"],
        }),
    )
    .await;
    assert_eq!(status, 404, "the category is already gone");
}

/// Local deletes need to know which project, and the two scopes must not be
/// confusable — deleting "development" locally must not touch a global
/// category of the same name.
#[tokio::test]
async fn a_local_delete_leaves_the_global_category_of_the_same_name_alone() {
    let temp = TempDir::new().unwrap();
    write_store(&global_store(&temp), "development", "global note\n\n");
    write_store(&local_store(&temp), "development", "local note\n\n");

    let listing = listed(&temp, "local", "development").await;
    let (status, _) = post(
        &temp,
        "/memory/delete_category",
        json!({
            "scope": "local",
            "category": "development",
            "revision": listing["revision"],
            "working_dir": project(&temp).display().to_string(),
        }),
    )
    .await;
    assert_eq!(status, 200);

    let (_, after) = get(&temp, &inventory_uri(&temp)).await;
    assert_eq!(after["local"]["categories"], json!([]));
    assert_eq!(
        after["global"]["categories"][0]["entries"][0]["content"], "global note",
        "the machine-wide category of the same name must be untouched"
    );
}

/// A local operation with no project named is a client bug, and has to be told
/// so — silently falling back to the daemon's working directory would delete
/// memories belonging to some other project.
#[tokio::test]
async fn a_local_delete_without_a_project_is_rejected() {
    let temp = TempDir::new().unwrap();
    write_store(&local_store(&temp), "development", "local note\n\n");

    let (status, text) = post_text(
        &temp,
        "/memory/delete_category",
        json!({"scope": "local", "category": "development", "revision": "any"}),
    )
    .await;
    assert_eq!(status, 400);
    assert!(
        text.contains("working_dir"),
        "the error must name what is missing, got: {text}"
    );
}

/// #73 again: a category is a name. The management routes are a second door
/// into the same store and must not be the way around its lock — and a refused
/// category is the caller's mistake, so 400, not 500.
#[tokio::test]
async fn a_traversing_category_is_a_bad_request_and_touches_nothing() {
    let temp = TempDir::new().unwrap();
    let outside = temp.path().join("outside");
    fs::create_dir_all(&outside).unwrap();
    let victim = outside.join("victim.txt");
    fs::write(&victim, "ORIGINAL\n").unwrap();

    for escaping in ["../../outside/victim", "/etc/hosts", "..", "a/b"] {
        let (status, _) = post_text(
            &temp,
            "/memory/delete_category",
            json!({"scope": "global", "category": escaping, "revision": "any"}),
        )
        .await;
        assert_eq!(status, 400, "delete_category accepted {escaping:?}");

        let (status, _) = post_text(
            &temp,
            "/memory/delete_entry",
            json!({
                "scope": "global",
                "category": escaping,
                "index": 0,
                "digest": "any",
                "revision": "any",
            }),
        )
        .await;
        assert_eq!(status, 400, "delete_entry accepted {escaping:?}");
    }

    assert_eq!(
        fs::read_to_string(&victim).unwrap(),
        "ORIGINAL\n",
        "a file outside the store was touched"
    );
}

/// The Settings surface has to manage the *same* global store the memory tools
/// write to. Every past bug in this area was a second, hand-rolled resolver
/// that ignored `BIOROUTER_PATH_ROOT` and quietly pointed somewhere else.
#[test]
fn the_default_router_manages_the_real_global_store() {
    assert_eq!(
        biorouter_server::routes::memory::default_global_store(),
        biorouter_mcp::global_memory_dir(),
        "the routes must resolve the global store through the one resolver that \
         honours BIOROUTER_PATH_ROOT, not a hand-rolled copy"
    );
}

/// #63 review, finding 7. These routes list and **irreversibly delete** the
/// user's memories, and the daemon's global middleware protects them — but the
/// annotations did not say so, so the published contract and the generated
/// TypeScript client both described them as open. A client generated from that
/// spec is entitled to omit the key, and an operator reading it is entitled to
/// believe the memory store is world-readable on the loopback port.
///
/// The declaration is asserted against the *generated* document, because that is
/// the artefact the client is built from.
#[test]
fn the_memory_routes_declare_the_authentication_they_actually_require() {
    let spec: Value = serde_json::from_str(&biorouter_server::openapi::generate_schema()).unwrap();

    let scheme = &spec["components"]["securitySchemes"]["api_key"];
    assert!(
        scheme.is_object(),
        "the routes name an `api_key` scheme; a spec that does not define it is \
         a dangling reference no generator can honour: {scheme:?}"
    );

    for (path, method) in [
        ("/memory/inventory", "get"),
        ("/memory/delete_entry", "post"),
        ("/memory/delete_category", "post"),
    ] {
        let operation = &spec["paths"][path][method];
        assert!(
            operation.is_object(),
            "{method} {path} is not in the generated spec at all"
        );
        assert_eq!(
            operation["security"],
            json!([{"api_key": []}]),
            "{method} {path} does not declare the secret key it requires"
        );
        assert!(
            operation["responses"]["401"].is_object(),
            "{method} {path} does not document the 401 the middleware returns, so \
             a generated client has no branch for it: {:?}",
            operation["responses"]
        );
    }
}
