//! Live end-to-end tests for the `code_execution` platform extension.
//!
//! Unlike the unit tests in `code_execution_extension.rs` (which exercise the JS
//! engine in isolation), these wire up a *real* `ExtensionManager` with the
//! bundled `developer` extension and drive `execute_code` end-to-end through
//! `dispatch_tool_call`, the same path the agent loop uses. Each test models a
//! realistic use case, from trivial arithmetic up to multi-tool data-flow chains.

use std::sync::Arc;

use rmcp::model::{CallToolRequestParams, RawContent};
use rmcp::object;
use serde_json::json;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};

use biorouter::agents::extension::ExtensionConfig;
use biorouter::agents::extension_manager::ExtensionManager;

const SESSION: &str = "code-exec-it";

/// Build an ExtensionManager with `developer` + `code_execution` enabled.
async fn manager() -> Arc<ExtensionManager> {
    let temp_dir = tempfile::tempdir().unwrap();
    // Keep the tempdir alive for the process lifetime; tests are short-lived.
    let temp_path = temp_dir.keep();
    let session_manager = Arc::new(biorouter::session::SessionManager::new(temp_path));
    let manager = Arc::new(ExtensionManager::new(
        Arc::new(Mutex::new(None)),
        session_manager,
    ));

    manager
        .add_extension(ExtensionConfig::Builtin {
            name: "developer".to_string(),
            description: "developer".to_string(),
            display_name: Some("Developer".to_string()),
            timeout: Some(300),
            bundled: Some(true),
            available_tools: vec![],
        })
        .await
        .expect("add developer");

    manager
        .add_extension(ExtensionConfig::Platform {
            name: "code_execution".to_string(),
            description: "Execute JavaScript code in a sandboxed environment".to_string(),
            bundled: Some(true),
            available_tools: vec![],
        })
        .await
        .expect("add code_execution");

    manager
}

async fn manager_with_computercontroller() -> Arc<ExtensionManager> {
    let manager = manager().await;
    manager
        .add_extension(ExtensionConfig::Builtin {
            name: "computercontroller".to_string(),
            description: "Computer and web tools".to_string(),
            display_name: Some("Computer Controller".to_string()),
            timeout: Some(300),
            bundled: Some(true),
            available_tools: vec![],
        })
        .await
        .expect("add computercontroller");
    manager
}

/// Run `execute_code` with the given JS source and return the textual result
/// (the `Result: ...` string the agent would see). Panics on dispatch errors.
async fn exec(manager: &Arc<ExtensionManager>, code: &str) -> String {
    let call = CallToolRequestParams {
        task: None,
        meta: None,
        name: "code_execution__execute_code".into(),
        arguments: Some(object!({ "code": code })),
    };
    let dispatched = manager
        .dispatch_tool_call(SESSION, call, CancellationToken::new())
        .await
        .expect("dispatch");
    let result = dispatched.result.await.expect("tool result");
    assert!(
        !result.is_error.unwrap_or(false),
        "execute_code reported is_error=true: {:?}",
        result.content
    );
    // Concatenate all text content blocks.
    result
        .content
        .iter()
        .filter_map(|c| match &c.raw {
            RawContent::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Run `execute_code` and return `(is_error, joined_text)` without asserting.
/// Used for cases where an error result is the expected outcome.
async fn exec_raw(manager: &Arc<ExtensionManager>, code: &str) -> (bool, String) {
    let call = CallToolRequestParams {
        task: None,
        meta: None,
        name: "code_execution__execute_code".into(),
        arguments: Some(object!({ "code": code })),
    };
    let dispatched = manager
        .dispatch_tool_call(SESSION, call, CancellationToken::new())
        .await
        .expect("dispatch");
    let result = dispatched.result.await.expect("tool result");
    let text = result
        .content
        .iter()
        .filter_map(|c| match &c.raw {
            RawContent::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    (result.is_error.unwrap_or(false), text)
}

/// Create a temp dir *inside* the current working directory (the developer
/// extension refuses paths outside the working dir) and return its canonical
/// absolute path as a String.
fn workdir_tempdir() -> (tempfile::TempDir, String) {
    let dir = tempfile::Builder::new()
        .prefix("codeexec_it")
        .tempdir_in(".")
        .unwrap();
    let abs = dir
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .to_string();
    (dir, abs)
}

/// Same as `exec` but for read_module / search_modules etc.
async fn call_tool(manager: &Arc<ExtensionManager>, tool: &str, args: serde_json::Value) -> String {
    let call = CallToolRequestParams {
        task: None,
        meta: None,
        name: format!("code_execution__{tool}").into(),
        arguments: Some(args.as_object().unwrap().clone()),
    };
    let dispatched = manager
        .dispatch_tool_call(SESSION, call, CancellationToken::new())
        .await
        .expect("dispatch");
    let result = dispatched.result.await.expect("tool result");
    result
        .content
        .iter()
        .filter_map(|c| match &c.raw {
            RawContent::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// ---------------------------------------------------------------------------
// 1-4: Pure JS (no tool calls) — exercises the engine + record_result paths.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn case01_arithmetic() {
    let m = manager().await;
    assert_eq!(exec(&m, "record_result(2 + 2 * 10)").await, "Result: 22");
}

#[tokio::test]
async fn case02_string_methods() {
    let m = manager().await;
    assert_eq!(
        exec(&m, r#"record_result("biorouter".toUpperCase())"#).await,
        "Result: \"BIOROUTER\""
    );
}

#[tokio::test]
async fn case03_array_pipeline() {
    let m = manager().await;
    // map/filter/reduce chain
    let out = exec(
        &m,
        r#"
        const xs = [1,2,3,4,5,6,7,8,9,10];
        const result = xs.filter(x => x % 2 === 0).map(x => x * x).reduce((a,b) => a+b, 0);
        record_result(result);
        "#,
    )
    .await;
    assert_eq!(out, "Result: 220"); // 4+16+36+64+100
}

#[tokio::test]
async fn case04_object_result_is_valid_json() {
    let m = manager().await;
    let out = exec(
        &m,
        r#"record_result({ name: "x", values: [1,2,3], nested: { ok: true } })"#,
    )
    .await;
    let json_str = out.strip_prefix("Result: ").unwrap();
    let parsed: serde_json::Value = serde_json::from_str(json_str).expect("valid JSON");
    assert_eq!(parsed["values"].as_array().unwrap().len(), 3);
    assert_eq!(parsed["nested"]["ok"], true);
}

// ---------------------------------------------------------------------------
// 5-7: Single tool calls into the developer extension.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn case05_shell_echo() {
    let m = manager().await;
    let out = exec(
        &m,
        r#"
        import { shell } from "developer";
        const r = shell({ command: "echo hello-from-shell" });
        record_result(r);
        "#,
    )
    .await;
    assert!(out.contains("hello-from-shell"), "got: {out}");
}

#[tokio::test]
async fn case06_shell_numeric_output_processed_in_js() {
    let m = manager().await;
    // Shell prints a number; JS adds to it. Verifies result parsing + arithmetic.
    let out = exec(
        &m,
        r#"
        import { shell } from "developer";
        const r = shell({ command: "echo 40" });
        // r may be parsed as a number or come back as text; coerce.
        record_result(Number(String(r).trim()) + 2);
        "#,
    )
    .await;
    let n: f64 = out.strip_prefix("Result: ").unwrap().parse().unwrap();
    assert_eq!(n, 42.0, "got: {out}");
}

#[tokio::test]
async fn case07_shell_multiline_split_and_count() {
    let m = manager().await;
    let out = exec(
        &m,
        r#"
        import { shell } from "developer";
        const r = String(shell({ command: "echo a; echo b; echo c" }));
        const lines = r.split("\n").filter(l => l.length > 0);
        record_result({ count: lines.length, first: lines[0] });
        "#,
    )
    .await;
    let json_str = out.strip_prefix("Result: ").unwrap();
    let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();
    assert_eq!(parsed["count"], 3);
    assert_eq!(parsed["first"], "a");
}

// ---------------------------------------------------------------------------
// 8-11: text_editor + multi-tool chains (the headline "batch in one call" use).
// ---------------------------------------------------------------------------

#[tokio::test]
async fn case08_write_then_view_file() {
    let m = manager().await;
    let (_dir, dir_s) = workdir_tempdir();
    let path_s = format!("{dir_s}/note.txt");
    let out = exec(
        &m,
        &format!(
            r#"
            import {{ text_editor }} from "developer";
            text_editor({{ command: "write", path: "{path_s}", file_text: "line one\nline two\n" }});
            const content = text_editor({{ command: "view", path: "{path_s}" }});
            record_result(content);
            "#
        ),
    )
    .await;
    assert!(out.contains("line one"), "got: {out}");
    assert!(out.contains("line two"), "got: {out}");
    // File really exists on disk with expected content.
    let on_disk = std::fs::read_to_string(&path_s).unwrap();
    assert_eq!(on_disk, "line one\nline two\n");
}

#[tokio::test]
async fn case09_shell_write_texteditor_read_dataflow() {
    let m = manager().await;
    let (_dir, dir_s) = workdir_tempdir();
    let path_s = format!("{dir_s}/data.txt");
    // shell writes the file; text_editor reads it back; JS asserts on content.
    let out = exec(
        &m,
        &format!(
            r#"
            import {{ shell, text_editor }} from "developer";
            shell({{ command: "echo 'generated content' > {path_s}" }});
            const view = String(text_editor({{ command: "view", path: "{path_s}" }}));
            record_result({{ hasContent: view.includes("generated content") }});
            "#
        ),
    )
    .await;
    let json_str = out.strip_prefix("Result: ").unwrap();
    let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();
    assert_eq!(parsed["hasContent"], true);
}

#[tokio::test]
async fn case10_loop_multiple_tool_calls() {
    let m = manager().await;
    // A JS loop issuing several synchronous tool calls and aggregating.
    let out = exec(
        &m,
        r#"
        import { shell } from "developer";
        let total = 0;
        for (let i = 1; i <= 5; i++) {
            const r = String(shell({ command: "echo " + i })).trim();
            total += Number(r);
        }
        record_result(total);
        "#,
    )
    .await;
    let n: f64 = out.strip_prefix("Result: ").unwrap().parse().unwrap();
    assert_eq!(n, 15.0, "got: {out}");
}

#[tokio::test]
async fn case11_write_edit_view_chain() {
    let m = manager().await;
    let (_dir, dir_s) = workdir_tempdir();
    let path_s = format!("{dir_s}/doc.md");
    let out = exec(
        &m,
        &format!(
            r##"
            import {{ text_editor }} from "developer";
            text_editor({{ command: "write", path: "{path_s}", file_text: "# Title\nbody\n" }});
            text_editor({{ command: "str_replace", path: "{path_s}", old_str: "# Title", new_str: "# Changed" }});
            const out = String(text_editor({{ command: "view", path: "{path_s}" }}));
            record_result({{ changed: out.includes("# Changed"), gone: !out.includes("# Title") }});
            "##
        ),
    )
    .await;
    let json_str = out.strip_prefix("Result: ").unwrap();
    let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();
    assert_eq!(parsed["changed"], true);
    assert_eq!(parsed["gone"], true);
}

// ---------------------------------------------------------------------------
// 12-14: JSON handling, namespace imports, JS built-ins.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn case12_json_parse_stringify() {
    let m = manager().await;
    let out = exec(
        &m,
        r#"
        const obj = JSON.parse('{"a": 1, "b": [2,3]}');
        record_result({ sum: obj.a + obj.b[0] + obj.b[1] });
        "#,
    )
    .await;
    let json_str = out.strip_prefix("Result: ").unwrap();
    let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();
    assert_eq!(parsed["sum"], 6);
}

#[tokio::test]
async fn case13_namespace_import() {
    let m = manager().await;
    let out = exec(
        &m,
        r#"
        import * as developer from "developer";
        const r = developer.shell({ command: "echo namespaced" });
        record_result(String(r).includes("namespaced"));
        "#,
    )
    .await;
    assert_eq!(out, "Result: true");
}

#[tokio::test]
async fn case14_js_builtins_math_json() {
    let m = manager().await;
    let out = exec(
        &m,
        r#"
        const data = [3.2, 1.5, 9.8, 4.1];
        const max = Math.max(...data);
        const rounded = data.map(x => Math.round(x));
        record_result({ max, rounded, sorted: [...data].sort((a,b)=>a-b) });
        "#,
    )
    .await;
    let json_str = out.strip_prefix("Result: ").unwrap();
    let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();
    assert_eq!(parsed["max"], 9.8);
    assert_eq!(parsed["rounded"][2], 10);
}

// ---------------------------------------------------------------------------
// 15-17: Error handling & edge cases.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn case15_tool_error_propagates_as_js_exception() {
    let m = manager().await;
    // cat a nonexistent file: developer shell should fail; the JS native fn
    // throws, the module rejects, and execute_code returns a "Module error".
    //
    // `exec_raw`, not `exec`: an error result IS the expected outcome here. Since
    // PAR-02 the shell reports a non-zero exit as `is_error`, so the failure now
    // propagates all the way out as `execute_code`'s own error flag — which is
    // the point of the case. (`case16` covers the other side: an error the JS
    // catches leaves `execute_code` successful.)
    let (is_error, out) = exec_raw(
        &m,
        r#"
        import { shell } from "developer";
        const r = shell({ command: "cat /nonexistent/path/xyz123" });
        record_result(r);
        "#,
    )
    .await;
    // What we must NOT see is a silent success that pretends the file was read.
    assert!(
        is_error,
        "an uncaught tool failure must surface as execute_code's error flag, \
         not as a successful result: {out}"
    );
    assert!(
        out.contains("Module error")
            || out.to_lowercase().contains("no such file")
            || out.to_lowercase().contains("error"),
        "expected an error surfaced, got: {out}"
    );
    // The shell's exit status reaches the JS layer, so the failure is legible
    // rather than an empty rejection.
    assert!(
        out.contains("exited with status 1") || out.to_lowercase().contains("no such file"),
        "the propagated error must name what actually failed, got: {out}"
    );
}

#[tokio::test]
async fn case16_try_catch_recovers_from_tool_error() {
    let m = manager().await;
    let out = exec(
        &m,
        r#"
        import { shell } from "developer";
        let captured = "none";
        try {
            shell({ command: "cat /nonexistent/path/xyz123" });
        } catch (e) {
            captured = "caught";
        }
        record_result(captured);
        "#,
    )
    .await;
    // If shell errors raise a JS exception, try/catch should recover.
    // If shell returns an error string (no throw), captured stays "none".
    assert!(
        out == "Result: \"caught\"" || out == "Result: \"none\"",
        "got: {out}"
    );
}

#[tokio::test]
async fn case17_no_record_result_returns_undefined() {
    let m = manager().await;
    let out = exec(&m, r#"const x = 1 + 1;"#).await;
    assert_eq!(out, "Result: undefined");
}

// ---------------------------------------------------------------------------
// 18-20: Discovery tools (read_module / search_modules) + syntax errors.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn case18_read_module_lists_developer_tools() {
    let m = manager().await;
    let out = call_tool(&m, "read_module", json!({ "module_path": "developer" })).await;
    assert!(out.contains("shell"), "got: {out}");
    assert!(out.contains("text_editor"), "got: {out}");
    assert!(
        out.contains("import * as developer"),
        "should include import hint, got: {out}"
    );
}

#[tokio::test]
async fn case19_search_modules_finds_shell() {
    let m = manager().await;
    let out = call_tool(&m, "search_modules", json!({ "terms": "shell" })).await;
    assert!(out.contains("developer/shell"), "got: {out}");
    assert!(out.contains("import * as module_developer"), "got: {out}");
    assert!(out.contains("command: string"), "got: {out}");
    assert!(!out.contains("Use the read_module"), "got: {out}");
}

#[tokio::test]
async fn simple_news_search_discovery_and_fetch_needs_no_read_module_call() {
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            "<rss><channel><item><title>Apple Watch update</title><link>https://example.test/watch</link></item></channel></rss>",
        ))
        .mount(&mock_server)
        .await;
    let manager = manager_with_computercontroller().await;

    let discovery = call_tool(
        &manager,
        "search_modules",
        json!({ "terms": ["web", "search", "browser", "news"] }),
    )
    .await;
    assert!(discovery.contains("computercontroller/web_scrape"));
    assert!(discovery.contains("module_computercontroller[\"web_scrape\"]"));
    assert!(discovery.contains("do not call read_module"));

    let result = exec(
        &manager,
        &format!(
            r#"
            import * as module_computercontroller from "computercontroller";
            const feed = module_computercontroller["web_scrape"]({{ url: "{}" }});
            record_result(feed);
            "#,
            mock_server.uri()
        ),
    )
    .await;
    assert!(result.contains("Apple Watch update"), "got: {result}");
}

#[cfg(not(target_os = "windows"))]
#[tokio::test]
async fn failed_nested_search_script_is_an_execute_code_error() {
    let manager = manager_with_computercontroller().await;
    let (is_error, result) = exec_raw(
        &manager,
        r#"
        import * as module_computercontroller from "computercontroller";
        const script = String.raw`printf 'search failed\n' >&2
exit 7`;
        const output = module_computercontroller["automation_script"]({
            language: "shell",
            script,
            save_output: false
        });
        record_result(output);
        "#,
    )
    .await;

    assert!(
        is_error,
        "failed inner script must fail execute_code: {result}"
    );
    assert!(result.contains("Script failed"), "got: {result}");
    assert!(result.contains("search failed"), "got: {result}");
}

#[tokio::test]
async fn case20_syntax_error_is_reported() {
    let m = manager().await;
    let (is_error, out) = exec_raw(&m, r#"record_result( this is not valid js )"#).await;
    // A malformed script must surface as an error result, not a silent success.
    assert!(
        is_error,
        "syntax error should set is_error=true, got: {out}"
    );
    assert!(
        out.to_lowercase().contains("parse error") || out.to_lowercase().contains("syntaxerror"),
        "expected a parse error, got: {out}"
    );
}

#[tokio::test]
async fn case22_autovisualiser_blob_resource_is_collected_and_rendered_inline() {
    // A tool (autovisualiser) that returns a ui:// blob resource for the User
    // plus an Assistant-audience text label. The blob must be collected and
    // appended to the execute_code result as a Resource (for inline UI render),
    // while the JS script sees the Assistant text label as its result.
    let temp_dir = tempfile::tempdir().unwrap();
    let session_manager = Arc::new(biorouter::session::SessionManager::new(temp_dir.keep()));
    let m = Arc::new(ExtensionManager::new(
        Arc::new(Mutex::new(None)),
        session_manager,
    ));
    m.add_extension(ExtensionConfig::Builtin {
        name: "autovisualiser".to_string(),
        description: "autovis".to_string(),
        display_name: Some("Auto Visualiser".to_string()),
        timeout: Some(300),
        bundled: Some(true),
        available_tools: vec![],
    })
    .await
    .expect("add autovisualiser");
    m.add_extension(ExtensionConfig::Platform {
        name: "code_execution".to_string(),
        description: "Execute JavaScript code in a sandboxed environment".to_string(),
        bundled: Some(true),
        available_tools: vec![],
    })
    .await
    .expect("add code_execution");

    let call = CallToolRequestParams {
        task: None,
        meta: None,
        name: "code_execution__execute_code".into(),
        arguments: Some(object!({ "code": r#"
            import { show_chart } from "autovisualiser";
            const r = show_chart({ data: { type: "bar", title: "T", labels: ["a","b"], datasets: [{ label: "x", data: [1,2] }] } });
            record_result(r);
        "# })),
    };
    let result = m
        .dispatch_tool_call(SESSION, call, CancellationToken::new())
        .await
        .expect("dispatch")
        .result
        .await
        .expect("tool result");

    assert!(!result.is_error.unwrap_or(false), "should succeed");
    // The blob resource must be appended for inline rendering.
    let has_resource = result
        .content
        .iter()
        .any(|c| matches!(&c.raw, RawContent::Resource(_)));
    assert!(has_resource, "ui:// chart resource should be appended");
    // And the script's textual result references the chart (the Assistant label),
    // not a doubled/empty value.
    let text = result
        .content
        .iter()
        .filter_map(|c| match &c.raw {
            RawContent::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        text.to_lowercase().contains("chart") || text.to_lowercase().contains("inline"),
        "result should reflect the chart label, got: {text}"
    );
}

#[tokio::test]
async fn case23_agent_drafter_preview_and_launch_metadata_survive_execute_code() {
    let temp_dir = tempfile::tempdir().unwrap();
    let session_manager = Arc::new(biorouter::session::SessionManager::new(
        temp_dir.path().join("sessions"),
    ));
    let m = Arc::new(ExtensionManager::new(
        Arc::new(Mutex::new(None)),
        session_manager,
    ));
    m.add_inprocess_server(
        "agent_drafter",
        biorouter_mcp::AgentDrafterServer::with_root(temp_dir.path().join("apps")),
    )
    .await
    .expect("add agent drafter");
    m.add_extension(ExtensionConfig::Platform {
        name: "code_execution".to_string(),
        description: "Execute JavaScript code in a sandboxed environment".to_string(),
        bundled: Some(true),
        available_tools: vec![],
    })
    .await
    .expect("add code_execution");

    let call = CallToolRequestParams {
        task: None,
        meta: None,
        name: "code_execution__execute_code".into(),
        arguments: Some(object!({ "code": r#"
            import { create_app, launch_app } from "agent_drafter";
            create_app({ title: "Nested Preview", id: "nested-preview", kind: "static" });
            const launched = launch_app({ id: "nested-preview" });
            record_result(launched);
        "# })),
    };
    let result = m
        .dispatch_tool_call(SESSION, call, CancellationToken::new())
        .await
        .expect("dispatch")
        .result
        .await
        .expect("tool result");

    assert!(
        !result.is_error.unwrap_or(false),
        "should succeed, got: {:?}",
        result.content
    );
    assert!(
        result
            .content
            .iter()
            .any(|content| matches!(&content.raw, RawContent::Resource(_))),
        "Agent Drafter preview resources should be appended"
    );
    assert!(
        result
            .content
            .iter()
            .any(|content| matches!(&content.raw, RawContent::ResourceLink(_))),
        "Agent Drafter launch link should be appended"
    );
    let meta = result.meta.expect("launch metadata");
    assert_eq!(
        meta.0
            .get("biorouter/app-path")
            .and_then(|value| value.as_str()),
        Some("/apps/nested-preview/")
    );
    assert_eq!(
        meta.0
            .get("biorouter/app-paths")
            .and_then(|value| value.as_array())
            .map(Vec::len),
        Some(1)
    );
}

#[tokio::test]
async fn case21_chained_data_real_dataflow() {
    let m = manager().await;
    let (_dir, dir_s) = workdir_tempdir();
    let src_s = format!("{dir_s}/src.txt");
    let dst_s = format!("{dir_s}/dst.txt");
    let dst = std::path::PathBuf::from(&dst_s);
    // The README example use case: read one file, transform, write another.
    let out = exec(
        &m,
        &format!(
            r#"
            import {{ text_editor }} from "developer";
            text_editor({{ command: "write", path: "{src_s}", file_text: "hello world\n" }});
            const content = String(text_editor({{ command: "view", path: "{src_s}" }}));
            // Extract the actual line (view output may include line numbers/framing).
            const upper = content.toUpperCase();
            text_editor({{ command: "write", path: "{dst_s}", file_text: upper }});
            record_result({{ done: true }});
            "#
        ),
    )
    .await;
    assert!(out.contains("done"), "got: {out}");
    let dst_content = std::fs::read_to_string(&dst).unwrap();
    assert!(
        dst_content.contains("HELLO WORLD"),
        "dst should contain uppercased content, got: {dst_content}"
    );
}

// ---------------------------------------------------------------------------
// Sandbox module/call errors must be self-correcting on the real dispatch path.
//
// The unit tests exercise `run_js_module` directly; these drive the same
// failures through `dispatch_tool_call`, so they assert the text the agent
// actually receives as a tool result.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn case24_unknown_module_error_names_the_real_importable_modules() {
    let m = manager().await;
    let (is_error, out) = exec_raw(
        &m,
        r#"
        import fs from "fs";
        record_result(fs.readFileSync("/etc/hosts", "utf8"));
        "#,
    )
    .await;

    assert!(is_error, "importing a non-existent module must be an error");
    assert!(
        out.contains(r#"Module "fs" could not be found"#),
        "must name the failing module, got: {out}"
    );
    // The live inventory, not a hardcoded list: this manager has developer.
    assert!(
        out.contains("Importable modules are exactly:") && out.contains("developer"),
        "must list the modules this session really has, got: {out}"
    );
    assert!(
        out.contains(r#"import { shell, text_editor } from "developer""#),
        "a Node builtin guess must be redirected to developer, got: {out}"
    );
}

#[tokio::test]
async fn case25_calling_a_non_function_explains_itself() {
    let m = manager().await;
    // Boa answers any call on a non-function with a bare "not a callable
    // function" — no value, no call site. That is the message that sent the
    // model round in circles before falling back to a Node builtin, so the
    // dispatch path must carry the explanation with it.
    let (is_error, out) = exec_raw(
        &m,
        r#"
        import { shell } from "developer";
        const result = shell({ command: "echo hi" });
        record_result(result.no_such_method());
        "#,
    )
    .await;

    assert!(
        is_error,
        "calling a non-function must be an error, got: {out}"
    );
    assert!(
        out.contains("not a callable function"),
        "the engine's own message must survive, got: {out}"
    );
    assert!(
        out.contains("parsed object when the tool's result is JSON"),
        "the opaque engine error must carry its explanation, got: {out}"
    );
    assert!(
        out.contains("JSON.stringify"),
        "must name the recovery, got: {out}"
    );
}
