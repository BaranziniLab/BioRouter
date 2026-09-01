// Tests for `render_dashboard` — combining several figures into one artifact.

// `STANDARD` and the `Engine` trait already reach here through `use super::*`.
use base64::engine::general_purpose::STANDARD as B64;

/// Decode the `ui://` HTML blob out of a tool result.
fn dashboard_html(result: &CallToolResult) -> String {
    if let RawContent::Resource(resource) = &*result.content[0] {
        if let ResourceContents::BlobResourceContents { blob, .. } = &resource.resource {
            return String::from_utf8(B64.decode(blob).unwrap()).unwrap();
        }
    }
    panic!("expected a blob resource as the first content item");
}

/// Decode every embedded panel document out of an assembled report.
fn panel_documents(html: &str) -> Vec<String> {
    html.split("<script type=\"text/plain\" id=\"autovis-panel-")
        .skip(1)
        .filter_map(|chunk| {
            // chunk == `0">BASE64</script>…`; take the text between the tags.
            let (_attrs, rest) = chunk.split_once('>')?;
            let (b64, _tail) = rest.split_once("</script>")?;
            Some(String::from_utf8(B64.decode(b64.trim()).unwrap()).unwrap())
        })
        .collect()
}

fn count_occurrences(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

/// The `var DATA = {…};` payload the report's runtime reads.
fn dashboard_data(html: &str) -> Value {
    let (_, after) = html.split_once("var DATA = ").expect("report must embed DATA");
    let line = after.lines().next().unwrap();
    serde_json::from_str(line.trim_end().trim_end_matches(';')).expect("DATA must be valid JSON")
}

fn params_from(value: serde_json::Value) -> Parameters<RenderDashboardParams> {
    Parameters(serde_json::from_value(value).unwrap())
}

fn volcano_figure() -> serde_json::Value {
    json!({
        "tool": "render_volcano",
        "params": {
            "data": {
                "points": [
                    {"label": "MYC", "log2fc": 2.4, "negLog10P": 4.0},
                    {"label": "TP53", "log2fc": -1.8, "negLog10P": 2.7},
                ]
            }
        }
    })
}

fn bar_chart_figure() -> serde_json::Value {
    json!({
        "tool": "show_chart",
        "params": {
            "data": {
                "type": "bar",
                "title": "Counts",
                "labels": ["A", "B"],
                "datasets": [{"label": "Sample", "data": [3.0, 7.0]}]
            }
        }
    })
}

fn mermaid_figure() -> serde_json::Value {
    json!({
        "tool": "render_mermaid",
        "params": { "mermaid_code": "graph TD; A-->B;" }
    })
}

// ---------------------------------------------------------------------------
// Registration — the tool is useless if the model never sees it
// ---------------------------------------------------------------------------

#[test]
fn test_dashboard_tool_is_registered_and_advertised() {
    let router = AutoVisualiserRouter::new();
    assert!(router.tool_router.has_route("render_dashboard"));

    // Every figure the dashboard can dispatch to must still BE a tool — it is
    // just no longer an ADVERTISED one. The invariant did not weaken when the
    // 32 moved behind `render_figure`; it moved to `figure_router`, which is
    // also what `describe_figure` reads their schemas out of.
    for name in [
        "show_chart",
        "render_volcano",
        "render_heatmap",
        "render_mermaid",
        "render_choropleth",
    ] {
        assert!(
            router.figure_router.has_route(name),
            "{name} is not registered"
        );
        assert!(
            !router.tool_router.has_route(name),
            "{name} is advertised again; the 128-tool ceiling is why it must not be"
        );
    }

    // And the instructions must steer the model to combine figures.
    let instructions = router.get_info().instructions.unwrap();
    assert!(instructions.contains("render_dashboard"));
    assert!(instructions.contains("call `render_dashboard` once"));

    // The tool DESCRIPTION (not just the server instructions) must carry the
    // call-once guidance, so the model reads it before the FIRST call — this is
    // the only lever that can curb the ~20% same-turn duplicate-call rate before
    // the model emits the second call.
    let tools = router.tool_router.list_all();
    let dash = tools
        .iter()
        .find(|t| t.name.as_ref() == "render_dashboard")
        .expect("render_dashboard must be listed");
    let desc = dash.description.as_deref().unwrap_or_default();
    assert!(desc.to_lowercase().contains("once"), "description must say call it once");
    assert!(desc.contains("finalise or confirm"));
}

// ---------------------------------------------------------------------------
// Tool-name normalization + slugs
// ---------------------------------------------------------------------------

#[test]
fn test_normalize_tool_name_is_lenient() {
    assert_eq!(normalize_tool_name("render_volcano"), "render_volcano");
    assert_eq!(normalize_tool_name("volcano"), "render_volcano");
    assert_eq!(normalize_tool_name("Volcano"), "render_volcano");
    assert_eq!(normalize_tool_name(" render-volcano "), "render_volcano");
    assert_eq!(normalize_tool_name("kaplan_meier"), "render_kaplan_meier");
    assert_eq!(normalize_tool_name("show_chart"), "show_chart");
    assert_eq!(normalize_tool_name("chart"), "show_chart");
    assert_eq!(normalize_tool_name("render_chart"), "show_chart");
}

#[test]
fn test_slugify() {
    assert_eq!(slugify("Tumour vs Normal"), "tumour-vs-normal");
    assert_eq!(slugify("  A/B  test!  "), "a-b-test");
    assert_eq!(slugify("!!!"), "report");
    assert_eq!(slugify(""), "report");
}

// ---------------------------------------------------------------------------
// Figure spec deserialization (models emit several shapes)
// ---------------------------------------------------------------------------

#[test]
fn test_figure_accepts_tool_and_params() {
    let f: DashboardFigure =
        serde_json::from_value(json!({"tool": "render_volcano", "params": {"data": {}}})).unwrap();
    assert_eq!(f.tool, "render_volcano");
    assert!(f.params["data"].is_object());
}

#[test]
fn test_figure_accepts_type_alias_and_bare_args() {
    // `type` instead of `tool`, and the tool's args inline rather than nested.
    let f: DashboardFigure =
        serde_json::from_value(json!({"type": "volcano", "data": {"points": []}})).unwrap();
    assert_eq!(f.tool, "volcano");
    assert!(f.params["data"]["points"].is_array());
}

#[test]
fn test_figure_accepts_stringified_object() {
    let f: DashboardFigure = serde_json::from_value(json!(
        "{\"tool\": \"show_chart\", \"params\": {\"data\": {}}}"
    ))
    .unwrap();
    assert_eq!(f.tool, "show_chart");
}

#[test]
fn test_figure_requires_a_tool() {
    let err = serde_json::from_value::<DashboardFigure>(json!({"params": {}})).unwrap_err();
    assert!(err.to_string().contains("needs a `tool`"));
}

// ---------------------------------------------------------------------------
// Argument shapes real models send
//
// Every other Auto Visualiser tool takes a single `data` argument, so models
// generalise. GPT-5.5 wrapped the whole report in one and, when rejected,
// retried with the same shape — two wasted turns and no figure.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_accepts_the_report_wrapped_in_a_data_envelope() {
    let router = AutoVisualiserRouter::new();
    let result = router
        .render_dashboard(params_from(json!({
            "data": {
                "title": "RNA-seq quality and signal",
                "summary": "Even depth; MYC up, TP53 down.",
                "sections": [{
                    "title": "Overview",
                    "panels": [
                        {"title": "Depth", "figure": bar_chart_figure()},
                        {"title": "Volcano", "figure": volcano_figure()},
                    ]
                }]
            }
        })))
        .await
        .unwrap();

    let html = dashboard_html(&result);
    assert_eq!(panel_documents(&html).len(), 2);
    assert_eq!(dashboard_data(&html)["title"], "RNA-seq quality and signal");
}

#[tokio::test]
async fn test_accepts_a_stringified_data_envelope() {
    let router = AutoVisualiserRouter::new();
    let payload = json!({
        "title": "Stringified",
        "panels": [{"figure": bar_chart_figure()}]
    })
    .to_string();

    let html = dashboard_html(
        &router
            .render_dashboard(params_from(json!({ "data": payload })))
            .await
            .unwrap(),
    );
    assert_eq!(panel_documents(&html).len(), 1);
    assert_eq!(dashboard_data(&html)["title"], "Stringified");
}

#[tokio::test]
async fn test_accepts_stringified_sections() {
    let router = AutoVisualiserRouter::new();
    let sections = json!([{"panels": [{"figure": bar_chart_figure()}]}]).to_string();

    let html = dashboard_html(
        &router
            .render_dashboard(params_from(json!({
                "title": "Stringified sections",
                "sections": sections
            })))
            .await
            .unwrap(),
    );
    assert_eq!(panel_documents(&html).len(), 1);
}

#[test]
fn test_plain_shape_still_wins_over_a_data_field() {
    // A report that legitimately has a `title` is never unwrapped, even if some
    // stray `data` key rides along.
    let params: RenderDashboardParams = serde_json::from_value(json!({
        "title": "Real title",
        "data": {"title": "Decoy"},
        "panels": []
    }))
    .unwrap();
    assert_eq!(params.title, "Real title");
}

#[test]
fn test_missing_title_error_names_the_data_envelope() {
    let err = serde_json::from_value::<RenderDashboardParams>(json!({"summary": "no title"}))
        .unwrap_err();
    assert!(err.to_string().contains("not wrapped in a `data` argument"));
}

// ---------------------------------------------------------------------------
// Happy path: several figures become one artifact
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_dashboard_combines_multiple_figures() {
    let router = AutoVisualiserRouter::new();
    let result = router
        .render_dashboard(params_from(json!({
            "title": "Tumour vs normal",
            "subtitle": "RNA-seq, n=48",
            "summary": "412 genes pass FDR < 0.05.",
            "sections": [{
                "title": "Genome-wide signal",
                "description": "Effect size against significance.",
                "panels": [
                    {"title": "Volcano plot", "caption": "Above the line: FDR < 0.05.",
                     "notes": "Wald test.", "figure": volcano_figure()},
                    {"title": "Top genes", "width": "half", "figure": bar_chart_figure()},
                ]
            }],
            "footer": "GENCODE v44."
        })))
        .await
        .unwrap();

    let html = dashboard_html(&result);

    // One artifact, two embedded panel documents.
    assert_eq!(panel_documents(&html).len(), 2);

    // The prose all made it into the page data.
    assert!(html.contains("Tumour vs normal"));
    assert!(html.contains("RNA-seq, n=48"));
    assert!(html.contains("412 genes pass FDR"));
    assert!(html.contains("Volcano plot"));
    assert!(html.contains("Above the line"));
    assert!(html.contains("Wald test."));
    assert!(html.contains("GENCODE v44."));

    // The assistant is told what happened, and told not to re-render on success.
    if let RawContent::Text(text) = &*result.content[1] {
        assert!(text.text.contains("2 figures"));
        assert!(text.text.contains("The report is complete"));
        assert!(text.text.contains("ready for the artifact panel"));
        assert!(text.text.contains("Inspect the existing artifact"));
        assert!(!text.text.contains("already displayed"));
        assert!(text.text.contains("finalise or confirm"));
    } else {
        panic!("expected an assistant-audience text note");
    }
}

#[tokio::test]
async fn test_dashboard_uri_and_frame_size() {
    let router = AutoVisualiserRouter::new();
    let result = router
        .render_dashboard(params_from(json!({
            "title": "Cohort Overview",
            "panels": [{"figure": bar_chart_figure()}]
        })))
        .await
        .unwrap();

    if let RawContent::Resource(resource) = &*result.content[0] {
        if let ResourceContents::BlobResourceContents { uri, meta, .. } = &resource.resource {
            assert_eq!(uri, "ui://dashboard/cohort-overview");
            let meta = meta.as_ref().expect("frame size meta");
            assert!(meta.0.contains_key("mcpui.dev/ui-preferred-frame-size"));
            return;
        }
    }
    panic!("expected a blob resource");
}

// ---------------------------------------------------------------------------
// Asset de-duplication — the reason panels are rendered in fragment mode
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_panels_carry_placeholders_not_libraries() {
    let router = AutoVisualiserRouter::new();
    let result = router
        .render_dashboard(params_from(json!({
            "title": "Two charts",
            "panels": [{"figure": bar_chart_figure()}, {"figure": bar_chart_figure()}]
        })))
        .await
        .unwrap();
    let html = dashboard_html(&result);

    // Each panel document has exactly one asset placeholder and no library:
    // Chart.js alone is ~200 KB, so a panel carrying it could not be this small.
    let panels = panel_documents(&html);
    assert_eq!(panels.len(), 2);
    for panel in &panels {
        assert_eq!(count_occurrences(panel, common::ASSET_PLACEHOLDER), 1);
        assert!(
            panel.len() < 80_000,
            "panel must not inline Chart.js ({} bytes); the report stores it once",
            panel.len()
        );
    }

    // Chart.js appears exactly once in the whole report, in the shared store.
    assert_eq!(
        count_occurrences(&html, "data-autovis-asset=\"chartjs\""),
        1,
        "Chart.js must be stored once, not once per panel"
    );
}

#[tokio::test]
async fn test_shared_store_holds_each_library_once_across_types() {
    let router = AutoVisualiserRouter::new();
    let result = router
        .render_dashboard(params_from(json!({
            "title": "Mixed",
            "panels": [
                {"figure": bar_chart_figure()},   // Chart.js
                {"figure": volcano_figure()},     // Chart.js again
                {"figure": mermaid_figure()},     // Mermaid
            ]
        })))
        .await
        .unwrap();
    let html = dashboard_html(&result);

    assert_eq!(count_occurrences(&html, "data-autovis-asset=\"chartjs\""), 1);
    assert_eq!(count_occurrences(&html, "data-autovis-asset=\"mermaid\""), 1);
    assert_eq!(panel_documents(&html).len(), 3);
}

#[tokio::test]
async fn test_dedup_keeps_the_report_far_smaller_than_naive_composition() {
    let router = AutoVisualiserRouter::new();

    // A standalone Mermaid figure inlines the whole ~3.3 MB library.
    let standalone = dashboard_html(
        &router
            .render_mermaid(Parameters(RenderMermaidParams {
                mermaid_code: "graph TD; A-->B;".to_string(),
            }))
            .await
            .unwrap(),
    );

    // Three Mermaid panels in one report must not cost three copies of it.
    let report = dashboard_html(
        &router
            .render_dashboard(params_from(json!({
                "title": "Three diagrams",
                "panels": [
                    {"figure": mermaid_figure()},
                    {"figure": mermaid_figure()},
                    {"figure": mermaid_figure()},
                ]
            })))
            .await
            .unwrap(),
    );

    assert!(
        report.len() < standalone.len() * 2,
        "3-panel report ({} bytes) should stay near one library copy, not three \
         (a standalone figure is {} bytes)",
        report.len(),
        standalone.len()
    );
}

#[tokio::test]
async fn test_panel_assets_are_declared_for_the_runtime() {
    let router = AutoVisualiserRouter::new();
    let html = dashboard_html(
        &router
            .render_dashboard(params_from(json!({
                "title": "Mixed",
                "panels": [{"figure": bar_chart_figure()}, {"figure": mermaid_figure()}]
            })))
            .await
            .unwrap(),
    );

    // The page data tells the runtime which stored libraries each panel needs.
    let panels = &dashboard_data(&html)["sections"][0]["panels"];
    assert_eq!(panels[0]["assets"], json!(["chartjs"]));
    assert_eq!(panels[1]["assets"], json!(["mermaid"]));
    assert_eq!(panels[0]["index"], json!(0));
    assert_eq!(panels[1]["index"], json!(1));
}

#[tokio::test]
async fn test_fragment_mode_does_not_leak_into_standalone_figures() {
    // Rendering a dashboard must not leave later standalone figures assetless.
    let router = AutoVisualiserRouter::new();
    router
        .render_dashboard(params_from(json!({
            "title": "First",
            "panels": [{"figure": bar_chart_figure()}]
        })))
        .await
        .unwrap();

    let standalone = dashboard_html(
        &router
            .show_chart(Parameters(
                serde_json::from_value(bar_chart_figure()["params"].clone()).unwrap(),
            ))
            .await
            .unwrap(),
    );
    assert!(!standalone.contains(common::ASSET_PLACEHOLDER));
    assert!(
        standalone.len() > 150_000,
        "a standalone chart must still inline Chart.js, got {} bytes",
        standalone.len()
    );
}

// ---------------------------------------------------------------------------
// Structure, layout and documentation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_flat_panels_shorthand_becomes_one_section() {
    let router = AutoVisualiserRouter::new();
    let html = dashboard_html(
        &router
            .render_dashboard(params_from(json!({
                "title": "Quick look",
                "panels": [{"title": "Counts", "figure": bar_chart_figure()}]
            })))
            .await
            .unwrap(),
    );
    // One implicit, untitled section wrapping the single panel.
    let data = dashboard_data(&html);
    assert_eq!(data["sections"].as_array().unwrap().len(), 1);
    assert!(data["sections"][0]["title"].is_null());
    assert_eq!(data["sections"][0]["panels"].as_array().unwrap().len(), 1);
    assert_eq!(panel_documents(&html).len(), 1);
}

#[tokio::test]
async fn test_panel_width_defaults_to_full_and_accepts_half() {
    let router = AutoVisualiserRouter::new();
    let html = dashboard_html(
        &router
            .render_dashboard(params_from(json!({
                "title": "Widths",
                "panels": [
                    {"figure": bar_chart_figure()},
                    {"width": "Half", "figure": bar_chart_figure()},
                ]
            })))
            .await
            .unwrap(),
    );
    let panels = &dashboard_data(&html)["sections"][0]["panels"];
    assert_eq!(panels[0]["width"], "full");
    assert_eq!(panels[1]["width"], "half");
}

#[tokio::test]
async fn test_rejects_unknown_width() {
    let router = AutoVisualiserRouter::new();
    let err = router
        .render_dashboard(params_from(json!({
            "title": "Widths",
            "panels": [{"width": "third", "figure": bar_chart_figure()}]
        })))
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
    assert!(err.message.contains("must be 'full' or 'half'"));
}

// ---------------------------------------------------------------------------
// Validation + partial failure
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_rejects_empty_title() {
    let router = AutoVisualiserRouter::new();
    let err = router
        .render_dashboard(params_from(json!({
            "title": "   ",
            "panels": [{"figure": bar_chart_figure()}]
        })))
        .await
        .unwrap_err();
    assert!(err.message.contains("non-empty `title`"));
}

#[tokio::test]
async fn test_rejects_no_panels() {
    let router = AutoVisualiserRouter::new();
    let err = router
        .render_dashboard(params_from(json!({ "title": "Empty" })))
        .await
        .unwrap_err();
    assert!(err.message.contains("at least one figure"));
}

#[tokio::test]
async fn test_rejects_too_many_panels() {
    let router = AutoVisualiserRouter::new();
    let panels: Vec<_> = (0..MAX_PANELS + 1)
        .map(|_| json!({"figure": bar_chart_figure()}))
        .collect();
    let err = router
        .render_dashboard(params_from(json!({"title": "Too many", "panels": panels})))
        .await
        .unwrap_err();
    assert!(err.message.contains("dashboard panels"));
}

#[tokio::test]
async fn partial_dashboard_receipt_preserves_failure_guidance_after_a_long_title() {
    let router = AutoVisualiserRouter::new();
    let result = router
        .render_dashboard(params_from(json!({
            "title": "Δ".repeat(2_000),
            "panels": [
                {"title": "Good", "figure": bar_chart_figure()},
                {"title": "Long panel title ".repeat(100), "figure": {
                    "tool": "show_chart", "params": {"data": {"type":"bar", "datasets":[]}}
                }}
            ]
        })))
        .await
        .unwrap();
    let receipt = result.structured_content.as_ref().unwrap();
    assert_eq!(receipt["status"], "created_with_errors");
    assert_eq!(receipt["figuresCreated"], 1);
    assert_eq!(receipt["figuresFailed"], 1);
    assert_eq!(receipt["failures"][0]["figure"], 2);
    assert_eq!(receipt["failures"][0]["tool"], "show_chart");
    assert!(receipt["failures"][0]["error"].as_str().unwrap().contains("at least one dataset"));
    assert!(receipt["recovery"].as_str().unwrap().contains("whole report"));
    assert!(serde_json::to_string(receipt).unwrap().len() < 3_000);
    assert_eq!(panel_documents(&dashboard_html(&result)).len(), 1);
}

#[tokio::test]
async fn dashboard_receipt_keeps_guidance_for_a_successful_long_title() {
    let result = AutoVisualiserRouter::new()
        .render_dashboard(params_from(json!({
            "title": "Δ".repeat(2_000),
            "panels": [{"figure": bar_chart_figure()}]
        })))
        .await
        .unwrap();
    let receipt = result.structured_content.as_ref().unwrap();
    assert_eq!(receipt["status"], "created");
    assert_eq!(receipt["figuresCreated"], 1);
    assert_eq!(receipt["figuresFailed"], 0);
    assert_eq!(receipt["failuresOmitted"], 0);
    assert_eq!(receipt["failures"], json!([]));
    assert!(receipt["recovery"].as_str().unwrap().contains("Inspect the existing artifact"));
    assert!(serde_json::to_string(receipt).unwrap().len() < 1_000);
}

#[tokio::test]
async fn partial_dashboard_receipt_bounds_errors_and_numbers_them_across_sections() {
    let bad_panels: Vec<_> = (1..MAX_PANELS).map(|_| json!({
        "figure": {"tool": "unknown_🧬".repeat(200), "params": {}}
    })).collect();
    let result = AutoVisualiserRouter::new()
        .render_dashboard(params_from(json!({
            "title": "Partial",
            "sections": [
                {"title": "Good", "panels": [{"figure": bar_chart_figure()}]},
                {"title": "Failures", "panels": bad_panels}
            ]
        })))
        .await
        .unwrap();
    let receipt = result.structured_content.as_ref().unwrap();
    assert_eq!(receipt["status"], "created_with_errors");
    assert_eq!(receipt["figuresCreated"], 1);
    assert_eq!(receipt["figuresFailed"], 23);
    assert_eq!(receipt["failuresOmitted"], 15);
    let errors = receipt["failures"].as_array().unwrap();
    assert_eq!(errors.len(), 8);
    for (index, error) in errors.iter().enumerate() {
        assert_eq!(error["figure"], index + 2);
        assert!(error["tool"].as_str().unwrap().chars().count() <= 64);
        assert!(error["error"].as_str().unwrap().chars().count() <= 128);
        assert_eq!(error["detailsTruncated"], true);
    }
    assert!(serde_json::to_string(receipt).unwrap().len() < 12_000);
    assert_eq!(panel_documents(&dashboard_html(&result)).len(), 1);
}

#[tokio::test]
async fn test_unknown_tool_fails_only_its_own_panel() {
    let router = AutoVisualiserRouter::new();
    let result = router
        .render_dashboard(params_from(json!({
            "title": "Partial",
            "panels": [
                {"title": "Good", "figure": bar_chart_figure()},
                {"title": "Bad", "figure": {"tool": "render_sparkline", "params": {}}},
            ]
        })))
        .await
        .unwrap();

    let html = dashboard_html(&result);
    // The good panel still renders; the bad one becomes an error card in-page.
    assert_eq!(panel_documents(&html).len(), 1);
    assert!(html.contains("Unknown visualization 'render_sparkline'"));

    // ...and the model is told precisely which figure to fix.
    if let RawContent::Text(text) = &*result.content[1] {
        assert!(text.text.contains("1 figure(s) could not be rendered"));
        assert!(text.text.contains("Figure 2 (Bad)"));
    } else {
        panic!("expected assistant text");
    }
}

#[tokio::test]
async fn test_bad_figure_arguments_fail_only_their_panel() {
    let router = AutoVisualiserRouter::new();
    let result = router
        .render_dashboard(params_from(json!({
            "title": "Partial",
            "panels": [
                {"title": "Good", "figure": bar_chart_figure()},
                // show_chart requires at least one dataset.
                {"title": "Empty chart", "figure": {"tool": "show_chart",
                    "params": {"data": {"type": "bar", "datasets": []}}}},
            ]
        })))
        .await
        .unwrap();

    let html = dashboard_html(&result);
    assert_eq!(panel_documents(&html).len(), 1);
    assert!(html.contains("at least one dataset"));
}

#[tokio::test]
async fn test_all_panels_failing_is_a_tool_error() {
    let router = AutoVisualiserRouter::new();
    let err = router
        .render_dashboard(params_from(json!({
            "title": "All bad",
            "panels": [{"figure": {"tool": "render_nonsense", "params": {}}}]
        })))
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
    assert!(err.message.contains("Every figure in the dashboard failed"));
}

#[tokio::test]
async fn test_dashboard_theme_param_bakes_a_locked_theme() {
    let router = AutoVisualiserRouter::new();

    // Explicit light/dark lock `window.__BR_VIZ_THEME__` into the report so the
    // preview and the expanded view render identically regardless of the host.
    for theme in ["light", "dark"] {
        let result = router
            .render_dashboard(params_from(json!({
                "title": "Themed report",
                "theme": theme,
                "panels": [{"title": "A", "figure": bar_chart_figure()}],
            })))
            .await
            .unwrap();
        let html = dashboard_html(&result);
        assert!(
            html.contains(&format!("window.__BR_VIZ_THEME__=\"{theme}\"")),
            "theme={theme} must bake a locked __BR_VIZ_THEME__"
        );
    }

    // Default (auto) bakes NO locked theme — the report follows the app's theme.
    let auto = router
        .render_dashboard(params_from(json!({
            "title": "Themed report",
            "panels": [{"title": "A", "figure": bar_chart_figure()}],
        })))
        .await
        .unwrap();
    // The runtime's panel-propagation helper contains `__BR_VIZ_THEME__=' +` (JS
    // concatenation); a *baked* literal is the double-quoted form, which must be absent.
    assert!(!dashboard_html(&auto).contains("__BR_VIZ_THEME__=\""));
}

#[tokio::test]
async fn test_rejects_both_sections_and_panels() {
    let router = AutoVisualiserRouter::new();
    let err = router
        .render_dashboard(params_from(json!({
            "title": "Conflict",
            "sections": [{"panels": [{"figure": bar_chart_figure()}]}],
            "panels": [{"figure": bar_chart_figure()}]
        })))
        .await
        .unwrap_err();
    assert!(err.message.contains("not both"));
}

#[tokio::test]
async fn test_rejects_overlong_prose() {
    let router = AutoVisualiserRouter::new();
    let err = router
        .render_dashboard(params_from(json!({
            "title": "Long",
            "summary": "x".repeat(MAX_PROSE_LEN + 1),
            "panels": [{"figure": bar_chart_figure()}]
        })))
        .await
        .unwrap_err();
    assert!(err.message.contains("too long"));
}

// ---------------------------------------------------------------------------
// Escaping — user prose and figure data are both model-influenced
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_prose_cannot_break_out_of_the_data_script() {
    let router = AutoVisualiserRouter::new();
    let html = dashboard_html(
        &router
            .render_dashboard(params_from(json!({
                "title": "Escaping",
                "summary": "</script><img src=x onerror=alert(1)>",
                "panels": [{"title": "</script>", "figure": bar_chart_figure()}]
            })))
            .await
            .unwrap(),
    );

    // The payload survives only as an escaped JS string literal.
    let (_, after) = html.split_once("var DATA = ").unwrap();
    let data_line = after.lines().next().unwrap();
    assert!(!data_line.contains("</script>"));
    assert!(!data_line.contains("<img"));
    assert!(data_line.contains("\\u003c"));
}

#[tokio::test]
async fn test_title_cannot_inject_a_template_placeholder() {
    let router = AutoVisualiserRouter::new();
    let html = dashboard_html(
        &router
            .render_dashboard(params_from(json!({
                "title": "{{PANEL_STORE}}",
                "panels": [{"figure": bar_chart_figure()}]
            })))
            .await
            .unwrap(),
    );
    // Exactly one panel store, and the title is inert text in <title>.
    assert_eq!(panel_documents(&html).len(), 1);
    assert!(html.contains("&#123;&#123;PANEL_STORE}}"));
}

#[tokio::test]
async fn test_asset_placeholder_literal_is_substituted_in_the_runtime() {
    let router = AutoVisualiserRouter::new();
    let html = dashboard_html(
        &router
            .render_dashboard(params_from(json!({
                "title": "Runtime",
                "panels": [{"figure": bar_chart_figure()}]
            })))
            .await
            .unwrap(),
    );
    // The template's `html.replace('{{ASSET_PLACEHOLDER}}', …)` must have been
    // rewritten to the real sentinel — otherwise no panel ever loads. It arrives
    // `<`-escaped so the literal never breaks out of the surrounding script.
    assert!(!html.contains("{{ASSET_PLACEHOLDER}}"));
    // `<` is escaped so the sentinel literal can never close the script it sits
    // in; JS unescapes it back to `<!--AUTOVIS_ASSETS-->` before matching.
    assert!(html.contains("replace(\"\\u003c!--AUTOVIS_ASSETS-->\""));

    // And the panel it will splice into really does contain that sentinel.
    assert!(panel_documents(&html)[0].contains(common::ASSET_PLACEHOLDER));
}

// ---------------------------------------------------------------------------
// Fixture generator: writes a realistic multi-figure report so the rendered page
// can be opened in a real browser. Not part of the normal run.
//
//   AUTOVIS_DUMP=/tmp/report.html cargo test -p biorouter-mcp --lib \
//     autovisualiser::tests::dump_sample_dashboard -- --ignored --nocapture
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "fixture generator; writes HTML to $AUTOVIS_DUMP"]
async fn dump_sample_dashboard() {
    let out = std::env::var("AUTOVIS_DUMP").expect("set AUTOVIS_DUMP to an output path");
    let router = AutoVisualiserRouter::new();

    let result = router
        .render_dashboard(params_from(json!({
            "title": "Tumour vs normal: differential expression",
            "subtitle": "Bulk RNA-seq · 48 paired samples · GENCODE v44",
            "summary": "Across 18,204 tested genes, **412 pass FDR < 0.05**. The signal is \
                        dominated by proliferation programmes: *MYC*, *CDK4* and *CCND1* are all \
                        strongly up-regulated in tumour tissue.\n\n\
                        - Library sizes are even across the two arms.\n\
                        - No chromosome shows a positional artefact.\n\
                        - Survival separates cleanly on the `MYC`-high stratum.",
            "sections": [
                {
                    "title": "Quality control",
                    "description": "Before interpreting effect sizes, confirm the libraries are \
                                    comparable and the count distribution is well behaved.",
                    "panels": [
                        {
                            "title": "Library size per sample",
                            "caption": "Total mapped reads. Even coverage across arms.",
                            "width": "half",
                            "figure": {"tool": "show_chart", "params": {"data": {
                                "type": "bar",
                                "labels": ["S1", "S2", "S3", "S4", "S5", "S6"],
                                "datasets": [{"label": "Million reads",
                                              "data": [31.2, 29.8, 33.1, 30.4, 32.7, 29.1]}]
                            }}}
                        },
                        {
                            "title": "Count distribution",
                            "caption": "log10 counts per gene, pooled across samples.",
                            "width": "half",
                            "figure": {"tool": "render_histogram", "params": {"data": {
                                "values": [1.2, 2.4, 2.9, 3.1, 3.3, 3.4, 3.6, 3.8, 4.0, 4.1,
                                           4.2, 4.4, 4.5, 4.7, 5.0, 5.2, 5.6, 6.1, 2.8, 3.9],
                                "title": "log10(count)"
                            }}}
                        }
                    ]
                },
                {
                    "title": "Genome-wide signal",
                    "description": "Effect size against statistical significance, then position \
                                    along the genome.",
                    "panels": [
                        {
                            "title": "Volcano plot",
                            "caption": "Points beyond the dashed thresholds pass **FDR < 0.05** \
                                        with |log2FC| > 1.",
                            "notes": "Wald test on DESeq2 shrunken effect sizes, \
                                      Benjamini-Hochberg correction across 18,204 genes.",
                            "figure": volcano_figure()
                        },
                        {
                            "title": "Sample correlation",
                            "caption": "Spearman rho between the six representative samples.",
                            "width": "half",
                            "figure": {"tool": "render_heatmap", "params": {"data": {
                                "xLabels": ["S1", "S2", "S3", "S4"],
                                "yLabels": ["S1", "S2", "S3", "S4"],
                                "values": [[1.0, 0.91, 0.62, 0.60],
                                           [0.91, 1.0, 0.64, 0.61],
                                           [0.62, 0.64, 1.0, 0.93],
                                           [0.60, 0.61, 0.93, 1.0]]
                            }}}
                        },
                        {
                            "title": "Analysis pipeline",
                            "caption": "How the counts reached this report.",
                            "width": "half",
                            "figure": {"tool": "render_mermaid", "params": {
                                "mermaid_code": "graph TD; FASTQ-->STAR; STAR-->featureCounts; \
                                                 featureCounts-->DESeq2; DESeq2-->Report;"
                            }}
                        }
                    ]
                }
            ],
            "footer": "Counts from GENCODE v44. Thresholds are the DESeq2 defaults. \
                       Raw data under restricted access."
        })))
        .await
        .unwrap();

    let html = dashboard_html(&result);
    std::fs::write(&out, &html).unwrap();
    println!("wrote {} bytes to {out}", html.len());
}

// ---------------------------------------------------------------------------
// The consolidation guards: 33 declared tools became 3, and nothing was lost
// ---------------------------------------------------------------------------

/// The ceiling this consolidation exists for. Azure and OpenAI reject a
/// `tools` array longer than 128 with a non-retryable 400, and Auto Visualiser
/// alone was 33 of the 130 built-in tools. A future contributor re-adding a
/// `#[tool]` attribute to a figure method fails here rather than silently
/// re-inflating the surface and killing every turn on those providers.
#[test]
fn exactly_three_tools_are_advertised() {
    let router = AutoVisualiserRouter::new();
    let mut advertised: Vec<String> = router
        .tool_router
        .list_all()
        .into_iter()
        .map(|tool| tool.name.to_string())
        .collect();
    advertised.sort();
    assert_eq!(
        advertised,
        ["describe_figure", "render_dashboard", "render_figure"],
        "the advertised Auto Visualiser surface changed"
    );
}

/// The three tables agree by construction — `figure_kinds!` generates the enum,
/// the slug and the tool name together — so what this actually checks is the
/// join to the OTHER table, `call_figure_tool`'s dispatch arm, which is written
/// by hand. A kind that names a tool the dispatcher does not know would fail
/// only at call time, on the model's first attempt.
#[test]
fn every_kind_resolves_to_a_real_figure_tool() {
    let router = AutoVisualiserRouter::new();
    assert_eq!(FigureKind::ALL.len(), 32);
    for kind in FigureKind::ALL {
        assert!(
            router.figure_router.has_route(kind.tool_name()),
            "kind `{}` names `{}`, which is not a registered tool",
            kind.slug(),
            kind.tool_name()
        );
    }
    // And no figure tool is orphaned — one that no kind names would be
    // unreachable through the only door the model now has.
    let named: std::collections::HashSet<&str> =
        FigureKind::ALL.iter().map(|k| k.tool_name()).collect();
    for tool in router.figure_router.list_all() {
        assert!(
            named.contains(tool.name.as_ref()),
            "`{}` is registered but no kind reaches it",
            tool.name
        );
    }
}

/// The no-regression proof, aimed at the only place behaviour could differ.
///
/// For 31 of the 32 kinds `render_figure` builds `{"data": …}` and calls
/// `call_figure_tool` — byte for byte the call the dashboard already makes, so
/// equivalence there is structural, and `every_kind_resolves_to_a_real_figure_tool`
/// covers the part that could actually be wrong (the kind → tool join). What
/// needs real evidence is `mermaid`, the one kind whose payload is reshaped,
/// and a spread of kinds across the families to catch a dispatch mix-up.
#[tokio::test]
async fn the_entry_point_renders_what_the_legacy_call_rendered() {
    let router = AutoVisualiserRouter::new();

    let cases: Vec<(FigureKind, serde_json::Value)> = vec![
        (
            FigureKind::Chart,
            serde_json::json!({"type":"bar","title":"T","labels":["a","b"],
                "datasets":[{"label":"s","data":[1.0,2.0]}]}),
        ),
        (
            FigureKind::Donut,
            serde_json::json!({"title":"T","data":[{"label":"a","value":10.0}]}),
        ),
        (
            FigureKind::Sankey,
            serde_json::json!({"nodes":[{"name":"a"},{"name":"b"}],
                "links":[{"source":"a","target":"b","value":10.0}]}),
        ),
        (
            FigureKind::Chord,
            serde_json::json!({"labels":["a","b"],"matrix":[[0.0,10.0],[5.0,0.0]]}),
        ),
        (
            FigureKind::Treemap,
            serde_json::json!({"name":"root","children":[{"name":"a","value":10.0}]}),
        ),
        (
            FigureKind::Map,
            serde_json::json!({"title":"T","markers":[
                {"lat":37.7,"lng":-122.4,"name":"a","color":"#556677","value":0.0}]}),
        ),
    ];

    for (kind, payload) in cases {
        let legacy = router
            .call_figure_tool(kind.tool_name(), serde_json::json!({ "data": payload }))
            .await
            .unwrap_or_else(|e| panic!("legacy call for `{}` failed: {e:?}", kind.slug()));
        let via_entry = router
            .render_figure(Parameters(RenderFigureParams {
                kind,
                data: payload,
            }))
            .await
            .unwrap_or_else(|e| panic!("render_figure for `{}` failed: {e:?}", kind.slug()));
        assert_eq!(
            decode_html(&legacy),
            decode_html(&via_entry),
            "`{}` renders differently through render_figure",
            kind.slug()
        );
    }
}

/// `mermaid` is the one kind whose wire shape is genuinely different — it takes
/// `mermaid_code`, not `data`. The entry point normalizes that away, and the
/// normalization has to accept what a model told "pass it as `data`" will send.
#[tokio::test]
async fn mermaid_accepts_the_shapes_a_model_will_send_and_refuses_the_rest() {
    let router = AutoVisualiserRouter::new();
    let source = "graph TD; A-->B;";

    let legacy = router
        .call_figure_tool(
            "render_mermaid",
            serde_json::json!({ "mermaid_code": source }),
        )
        .await
        .expect("legacy mermaid");

    for shape in [
        serde_json::json!(source),
        serde_json::json!({ "mermaid_code": source }),
        serde_json::json!({ "code": source }),
        serde_json::json!({ "source": source }),
        serde_json::json!({ "data": source }),
    ] {
        let rendered = router
            .render_figure(Parameters(RenderFigureParams {
                kind: FigureKind::Mermaid,
                data: shape.clone(),
            }))
            .await
            .unwrap_or_else(|e| panic!("mermaid rejected {shape}: {e:?}"));
        assert_eq!(decode_html(&legacy), decode_html(&rendered), "{shape}");
    }

    // A payload with no source anywhere must say what to do, not render an
    // empty diagram.
    let err = router
        .render_figure(Parameters(RenderFigureParams {
            kind: FigureKind::Mermaid,
            data: serde_json::json!({ "nodes": [] }),
        }))
        .await
        .unwrap_err();
    assert!(err.message.contains("describe_figure"), "{}", err.message);
}

/// A describe tool that advertised shapes the renderer rejects would be worse
/// than none: it is the model's only route to a kind's arguments now.
#[tokio::test]
async fn describe_figure_answers_for_every_kind_and_lists_them_all() {
    let router = AutoVisualiserRouter::new();

    let catalog = router
        .describe_figure(Parameters(DescribeFigureParams { kind: None }))
        .await
        .expect("catalog");
    let catalog = result_text(&catalog);
    for kind in FigureKind::ALL {
        assert!(
            catalog.contains(kind.slug()),
            "the catalog omits `{}`",
            kind.slug()
        );
    }

    for kind in FigureKind::ALL {
        let described = router
            .describe_figure(Parameters(DescribeFigureParams { kind: Some(*kind) }))
            .await
            .unwrap_or_else(|e| panic!("describe_figure({}) failed: {e:?}", kind.slug()));
        let body: serde_json::Value =
            serde_json::from_str(&result_text(&described)).expect("describe returns JSON");
        assert_eq!(body["kind"], kind.slug());
        assert!(
            body["schema"].is_object() && !body["schema"].as_object().unwrap().is_empty(),
            "`{}` has no schema",
            kind.slug()
        );
        assert!(
            body["guidance"].as_str().is_some_and(|g| !g.trim().is_empty()),
            "`{}` lost its worked example — that text is the model's only \
             per-kind documentation now",
            kind.slug()
        );
    }
}

/// The 17 KB of per-kind prose must stay in `describe_figure`'s RESULT, not
/// creep back into the declarations.
///
/// ⚠ **This is NOT a provider limit, and an earlier draft of this test wrongly
/// said it was.** A design review claimed Azure/OpenAI cap
/// `tools[n].function.description` at 1024 characters and reject the request.
/// Measured against the live Versa Azure gpt-5.5 endpoint on 2026-08-31: a turn
/// carrying `render_dashboard`'s 2,626-character description succeeded. The
/// array-length cap of 128 is real and enforced; a description cap is not, at
/// least not at that size.
///
/// What this guards is the thing that made the surface expensive in the first
/// place: 33 declarations carrying 17,509 bytes of worked examples, sent on
/// every single turn. The budget is generous enough that `render_dashboard`'s
/// deliberately long call-once guidance fits, and tight enough that folding a
/// kind's examples back into a declared tool fails here.
#[test]
fn per_kind_documentation_stays_out_of_the_declarations() {
    let router = AutoVisualiserRouter::new();
    let total: usize = router
        .tool_router
        .list_all()
        .iter()
        .map(|tool| tool.description.as_deref().unwrap_or_default().len())
        .sum();
    assert!(
        total <= 4_096,
        "the advertised Auto Visualiser descriptions total {total} bytes; per-kind \
         schemas and examples belong in describe_figure's result, which is not sent \
         on every turn"
    );
}

/// The plain text of a tool result, for the tools that answer with text rather
/// than a `ui://` resource.
fn result_text(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|c| c.as_text().map(|t| t.text.clone()))
        .collect::<Vec<_>>()
        .join("")
}
