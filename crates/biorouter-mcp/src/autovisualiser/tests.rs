use super::common::validate_data_param;
use super::*;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{ErrorCode, RawContent, ResourceContents, Role};
use serde_json::json;

#[tokio::test]
async fn figure_receipt_is_small_while_the_user_keeps_the_complete_artifact() {
    let html = format!(
        "<html><body>{}</body></html>",
        "synthetic-figure".repeat(20_000)
    );
    let (result, _) = common::render_fragment(async {
        common::finish(
            "ui://chart/test",
            "receipt-test",
            &"Δ".repeat(2_000),
            html.clone(),
        )
    })
    .await;
    let wire = serde_json::to_value(&result).unwrap();
    let receipt = wire.get("structuredContent").expect("model-facing receipt");
    assert_eq!(receipt["status"], "created");
    assert_eq!(receipt["uri"], "ui://chart/test");
    assert_eq!(receipt["mimeType"], "text/html");
    assert_eq!(receipt["summary"].as_str().unwrap().chars().count(), 512);
    assert!(serde_json::to_string(receipt).unwrap().len() < 2_000);
    assert_eq!(common::html_from_result(&result).unwrap(), html);
    assert_eq!(result.content.len(), 2);
    assert_eq!(result.content[0].audience().unwrap(), &vec![Role::User]);
    assert_eq!(
        result.content[1].audience().unwrap(),
        &vec![Role::Assistant]
    );
}

#[test]
fn forced_theme_injection_preserves_unicode_document_content() {
    let html = "<!doctype html><html><head><title>Résumé</title></head><body>Δ</body></html>";
    let result = inject_forced_theme(html.to_string(), "dark");

    assert!(result
        .contains("<head><script>window.__BR_VIZ_THEME__=\"dark\";</script><title>Résumé</title>"));
    assert!(result.ends_with("<body>Δ</body></html>"));
}

// ---------------------------------------------------------------------------
// validate_data_param (loosely-typed data guard)
// ---------------------------------------------------------------------------

#[test]
fn test_validate_data_param_rejects_string() {
    let params = json!({
        "data": "{\"labels\": [\"A\", \"B\"], \"matrix\": [[0, 1], [1, 0]]}"
    });
    let err = validate_data_param(&params, false).unwrap_err();
    assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
    assert!(err
        .message
        .contains("must be a JSON object, not a JSON string"));
    assert!(err.message.contains("without comments"));
}

#[test]
fn test_validate_data_param_accepts_object() {
    let params = json!({ "data": { "labels": ["A", "B"], "matrix": [[0, 1], [1, 0]] } });
    let data = validate_data_param(&params, false).unwrap();
    assert!(data.is_object());
    assert_eq!(data["labels"][0], "A");
}

#[test]
fn test_validate_data_param_rejects_array_when_not_allowed() {
    let params = json!({ "data": [{"label": "A", "value": 10}] });
    let err = validate_data_param(&params, false).unwrap_err();
    assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
    assert!(err.message.contains("must be a JSON object"));
}

#[test]
fn test_validate_data_param_accepts_array_when_allowed() {
    let params = json!({ "data": [{"label": "A", "value": 10}] });
    let data = validate_data_param(&params, true).unwrap();
    assert!(data.is_array());
    assert_eq!(data[0]["label"], "A");
}

#[test]
fn test_validate_data_param_missing_data() {
    let params = json!({ "other": "value" });
    let err = validate_data_param(&params, false).unwrap_err();
    assert!(err.message.contains("Missing 'data' parameter"));
}

#[test]
fn test_validate_data_param_rejects_primitive_values() {
    assert!(validate_data_param(&json!({ "data": 42 }), false).is_err());
    assert!(validate_data_param(&json!({ "data": true }), false).is_err());
    assert!(validate_data_param(&json!({ "data": null }), false).is_err());
}

// ---------------------------------------------------------------------------
// Shared infrastructure (escaping, assets, lenient enums)
// ---------------------------------------------------------------------------

#[test]
fn test_js_data_neutralizes_script_breakout() {
    // A literal </script> in data must not be able to break out of the script tag.
    let v = json!({ "name": "</script><script>alert(1)</script>" });
    let s = common::js_data(&v).unwrap();
    assert!(!s.contains("</script>"));
    assert!(s.contains("\\u003c"));
}

#[test]
fn test_js_data_escapes_line_separators() {
    let v = Value::String("line\u{2028}sep\u{2029}end".to_string());
    let s = common::js_data(&v).unwrap();
    assert!(!s.contains('\u{2028}'));
    assert!(!s.contains('\u{2029}'));
    assert!(s.contains("\\u2028"));
}

#[test]
fn test_html_escape() {
    assert_eq!(
        common::html_escape("<b>\"x\" & 'y'</b>"),
        "&lt;b&gt;&quot;x&quot; &amp; &#39;y&#39;&lt;/b&gt;"
    );
}

#[test]
fn test_asset_html_inline_default() {
    // Default (no env) inlines the library.
    let html = common::asset_html(&[Asset::ChartJs]);
    assert!(html.contains("<script>"));
    assert!(!html.contains("cdn.jsdelivr.net"));
}

#[test]
fn test_lenient_chart_type_parsing() {
    // Capitalized / uppercase / padded all parse.
    for raw in ["\"Line\"", "\"LINE\"", "\" line \"", "\"line\""] {
        let parsed: ChartType = serde_json::from_str(raw).unwrap();
        assert!(matches!(parsed, ChartType::Line));
    }
    assert!(serde_json::from_str::<ChartType>("\"pie\"").is_err());
}

#[test]
fn test_lenient_donut_type_parsing() {
    for raw in ["\"Doughnut\"", "\"DONUT\"", "\"doughnut\""] {
        let parsed: DonutChartType = serde_json::from_str(raw).unwrap();
        assert!(matches!(parsed, DonutChartType::Doughnut));
    }
    assert!(matches!(
        serde_json::from_str::<DonutChartType>("\"Pie\"").unwrap(),
        DonutChartType::Pie
    ));
}

// ---------------------------------------------------------------------------
// Result-shape helper used by every render-tool test below.
// ---------------------------------------------------------------------------

fn assert_resource_result(result: &CallToolResult, expected_uri: &str) {
    // Two items: user-audience resource + assistant-audience text confirmation.
    assert_eq!(result.content.len(), 2);
    assert_eq!(result.content[0].audience().unwrap(), &vec![Role::User]);
    assert_eq!(
        result.content[1].audience().unwrap(),
        &vec![Role::Assistant]
    );
    assert!(matches!(&*result.content[1], RawContent::Text(_)));
    if let RawContent::Text(text) = &*result.content[1] {
        assert!(!text.text.contains("rendered inline"));
        assert!(!text.text.contains("already displayed"));
    }
    if let RawContent::Resource(resource) = &*result.content[0] {
        if let ResourceContents::BlobResourceContents {
            uri,
            mime_type,
            blob,
            ..
        } = &resource.resource
        {
            assert_eq!(uri, expected_uri);
            assert_eq!(mime_type.as_ref().unwrap(), "text/html");
            assert!(!blob.is_empty(), "HTML content should not be empty");
        } else {
            panic!("Expected BlobResourceContents");
        }
    } else {
        panic!("Expected Resource content");
    }
}

// ---------------------------------------------------------------------------
// Existing tools (happy path)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_render_sankey() {
    let router = AutoVisualiserRouter::new();
    let params = Parameters(RenderSankeyParams {
        data: SankeyData {
            nodes: vec![
                SankeyNode {
                    name: "A".to_string(),
                    category: None,
                },
                SankeyNode {
                    name: "B".to_string(),
                    category: None,
                },
            ],
            links: vec![SankeyLink {
                source: "A".to_string(),
                target: "B".to_string(),
                value: 10.0,
            }],
        },
    });
    let result = router.render_sankey(params).await.unwrap();
    assert_resource_result(&result, "ui://sankey/diagram");
}

#[tokio::test]
async fn test_render_sankey_rejects_unknown_node() {
    let router = AutoVisualiserRouter::new();
    let params = Parameters(RenderSankeyParams {
        data: SankeyData {
            nodes: vec![SankeyNode {
                name: "A".to_string(),
                category: None,
            }],
            links: vec![SankeyLink {
                source: "A".to_string(),
                target: "GHOST".to_string(),
                value: 1.0,
            }],
        },
    });
    let err = router.render_sankey(params).await.unwrap_err();
    assert!(err.message.contains("GHOST"));
}

#[tokio::test]
async fn test_render_sankey_rejects_empty() {
    let router = AutoVisualiserRouter::new();
    let params = Parameters(RenderSankeyParams {
        data: SankeyData {
            nodes: vec![],
            links: vec![],
        },
    });
    assert!(router.render_sankey(params).await.is_err());
}

#[tokio::test]
async fn test_render_radar() {
    let router = AutoVisualiserRouter::new();
    let params = Parameters(RenderRadarParams {
        data: RadarData {
            labels: vec![
                "Speed".to_string(),
                "Power".to_string(),
                "Agility".to_string(),
            ],
            datasets: vec![RadarDataset {
                label: "Player 1".to_string(),
                data: vec![80.0, 90.0, 85.0],
            }],
        },
    });
    let result = router.render_radar(params).await.unwrap();
    assert_resource_result(&result, "ui://radar/chart");
}

#[tokio::test]
async fn test_render_radar_rejects_length_mismatch() {
    let router = AutoVisualiserRouter::new();
    let params = Parameters(RenderRadarParams {
        data: RadarData {
            labels: vec!["A".to_string(), "B".to_string()],
            datasets: vec![RadarDataset {
                label: "x".to_string(),
                data: vec![1.0],
            }],
        },
    });
    assert!(router.render_radar(params).await.is_err());
}

#[tokio::test]
async fn test_render_donut() {
    let router = AutoVisualiserRouter::new();
    let params = Parameters(RenderDonutParams {
        data: DonutData {
            data: DonutChartData::Single(SingleDonutChart {
                data: vec![
                    DonutDataItem::Number(30.0),
                    DonutDataItem::Number(40.0),
                    DonutDataItem::Number(30.0),
                ],
                labels: Some(vec!["A".to_string(), "B".to_string(), "C".to_string()]),
                title: None,
                chart_type: None,
            }),
        },
    });
    let result = router.render_donut(params).await.unwrap();
    assert_resource_result(&result, "ui://donut/chart");
}

#[tokio::test]
async fn test_render_treemap() {
    let router = AutoVisualiserRouter::new();
    let params = Parameters(RenderTreemapParams {
        data: TreemapNode {
            name: "root".to_string(),
            value: None,
            category: None,
            children: Some(vec![
                TreemapNode {
                    name: "A".to_string(),
                    value: Some(100.0),
                    category: Some("Type1".to_string()),
                    children: None,
                },
                TreemapNode {
                    name: "B".to_string(),
                    value: Some(200.0),
                    category: Some("Type2".to_string()),
                    children: None,
                },
            ]),
        },
    });
    let result = router.render_treemap(params).await.unwrap();
    assert_resource_result(&result, "ui://treemap/visualization");
}

#[tokio::test]
async fn test_render_chord() {
    let router = AutoVisualiserRouter::new();
    let params = Parameters(RenderChordParams {
        data: ChordData {
            labels: vec!["A".to_string(), "B".to_string(), "C".to_string()],
            matrix: vec![
                vec![0.0, 10.0, 5.0],
                vec![10.0, 0.0, 15.0],
                vec![5.0, 15.0, 0.0],
            ],
        },
    });
    let result = router.render_chord(params).await.unwrap();
    assert_resource_result(&result, "ui://chord/diagram");
}

#[tokio::test]
async fn test_render_chord_rejects_non_square() {
    let router = AutoVisualiserRouter::new();
    let params = Parameters(RenderChordParams {
        data: ChordData {
            labels: vec!["A".to_string(), "B".to_string()],
            matrix: vec![vec![0.0, 1.0]],
        },
    });
    assert!(router.render_chord(params).await.is_err());
}

#[tokio::test]
async fn test_render_map() {
    let router = AutoVisualiserRouter::new();
    let params = Parameters(RenderMapParams {
        data: MapData {
            markers: vec![MapMarker {
                lat: 0.0,
                lng: 0.0,
                name: Some("Origin".to_string()),
                value: None,
                description: None,
                popup: None,
                color: None,
                label: None,
                use_default_icon: None,
            }],
            title: None,
            subtitle: None,
            center: None,
            zoom: None,
            clustering: None,
            cluster_radius: None,
            auto_fit: None,
        },
    });
    let result = router.render_map(params).await.unwrap();
    assert_resource_result(&result, "ui://map/visualization");
}

#[tokio::test]
async fn test_render_map_rejects_bad_coords() {
    let router = AutoVisualiserRouter::new();
    let params = Parameters(RenderMapParams {
        data: MapData {
            markers: vec![MapMarker {
                lat: 999.0,
                lng: 0.0,
                name: None,
                value: None,
                description: None,
                popup: None,
                color: None,
                label: None,
                use_default_icon: None,
            }],
            title: None,
            subtitle: None,
            center: None,
            zoom: None,
            clustering: None,
            cluster_radius: None,
            auto_fit: None,
        },
    });
    assert!(router.render_map(params).await.is_err());
}

#[tokio::test]
async fn test_show_chart() {
    let router = AutoVisualiserRouter::new();
    let params = Parameters(ShowChartParams {
        data: ChartData {
            chart_type: ChartType::Scatter,
            datasets: vec![ChartDataset {
                label: "Test Data".to_string(),
                data: ChartDataValues::Points(vec![
                    ChartPoint { x: 1.0, y: 2.0 },
                    ChartPoint { x: 2.0, y: 4.0 },
                ]),
                background_color: None,
                border_color: None,
                border_width: None,
                tension: None,
                fill: None,
            }],
            labels: None,
            title: None,
            subtitle: None,
            x_axis_label: None,
            y_axis_label: None,
        },
    });
    let result = router.show_chart(params).await.unwrap();
    assert_resource_result(&result, "ui://scatter/chart");
}

#[tokio::test]
async fn academic_chart_defaults_preserve_labels_styles_and_script_escaping() {
    let html = render_standalone_figure(
        "show_chart",
        json!({"data": {
            "type": "line",
            "title": "Δοκιμή 東京 — study outcome",
            "subtitle": "Synthetic </script><script>bad()</script> evidence",
            "xAxisLabel": "Follow-up (days)",
            "yAxisLabel": "Outcome (mg/L)",
            "labels": ["Long Unicode category — 東京 🧬", "Comparison"],
            "datasets": [{"label": "Cohort A", "data": [1.0, 2.0],
                "backgroundColor": "#123456", "borderColor": "#654321",
                "borderWidth": 3.0, "tension": 0.2, "fill": true}]
        }}),
    )
    .await
    .expect("academic chart renders");

    assert!(!html.contains("linear-gradient"));
    assert!(!html.contains("Interactive data visualization"));
    assert!(html.contains("BioRouterViz.applyChartDefaults()"));
    assert!(html.contains("BioRouterViz.wrapLabel"));
    assert!(html.contains("role=\"img\""));
    assert!(html.contains("<table"));
    assert!(html.contains("Δοκιμή 東京 — study outcome"));
    assert!(html.contains("Outcome (mg/L)"));
    assert!(html.contains("#123456"));
    assert!(html.contains("#654321"));
    assert!(html.contains("\"tension\":0.2"));
    assert!(html.contains("\"fill\":true"));
    assert!(!html.contains("</script><script>bad()"));
    assert!(html.contains("\\u003c/script>\\u003cscript>bad()"));
}

#[test]
fn academic_figure_guidance_is_part_of_the_capability_prompt() {
    let router = AutoVisualiserRouter::new();
    for guidance in [
        "clean, minimal academic figure",
        "label axes with quantities and units",
        "largest legible text that fits without overlap",
        "long Unicode labels",
        "do not smooth measured line data",
        "Do not claim visual verification",
    ] {
        assert!(router.instructions.contains(guidance), "{guidance}");
    }
}

#[tokio::test]
async fn test_render_mermaid() {
    let router = AutoVisualiserRouter::new();
    let params = Parameters(RenderMermaidParams {
        mermaid_code: "graph TD;\n    A-->B;\n    A-->C;".to_string(),
    });
    let result = router.render_mermaid(params).await.unwrap();
    assert_resource_result(&result, "ui://mermaid/diagram");
}

#[tokio::test]
async fn test_render_mermaid_rejects_empty() {
    let router = AutoVisualiserRouter::new();
    let params = Parameters(RenderMermaidParams {
        mermaid_code: "   ".to_string(),
    });
    assert!(router.render_mermaid(params).await.is_err());
}

#[tokio::test]
async fn test_mermaid_blob_has_escaped_code() {
    // The mermaid source must be injected without a raw </script> breakout.
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    let router = AutoVisualiserRouter::new();
    let params = Parameters(RenderMermaidParams {
        mermaid_code: "graph TD; A[\"</script>\"]-->B;".to_string(),
    });
    let result = router.render_mermaid(params).await.unwrap();
    if let RawContent::Resource(resource) = &*result.content[0] {
        if let ResourceContents::BlobResourceContents { blob, .. } = &resource.resource {
            let html = String::from_utf8(STANDARD.decode(blob).unwrap()).unwrap();
            // The injected JS string literal must not contain a literal </script>.
            let marker = "const mermaidCode =";
            let start = html.find(marker).unwrap();
            let snippet: String = html
                .get(start..)
                .unwrap_or_default()
                .chars()
                .take(200)
                .collect();
            assert!(!snippet.contains("</script>"));
        }
    }
}

// ---------------------------------------------------------------------------
// render_standalone_figure — the embedding API (figures inside apps).
// ---------------------------------------------------------------------------

#[tokio::test]
async fn standalone_figure_returns_self_contained_html() {
    let html = render_standalone_figure(
        "show_chart",
        json!({"data": {
            "type": "bar",
            "labels": ["A", "B"],
            "datasets": [{"label": "S", "data": [1.0, 2.0]}]
        }}),
    )
    .await
    .expect("show_chart should render");

    // A complete standalone document...
    assert!(html.contains("<!DOCTYPE") || html.contains("<html"));
    // ...with the chart library inlined (not a CDN reference).
    assert!(html.contains("Chart.js v"), "Chart.js should be inlined");
    assert!(html.contains("<script>"));
    assert!(!html.contains("cdn.jsdelivr.net"));
}

#[tokio::test]
async fn standalone_figure_accepts_prefixless_name() {
    // "volcano" must resolve to render_volcano just like the dashboard panels.
    let html = render_standalone_figure(
        "volcano",
        json!({"data": {"points": [{"label": "MYC", "log2fc": 2.4, "negLog10P": 4.0}]}}),
    )
    .await
    .expect("prefixless 'volcano' should render");
    assert!(html.contains("<!DOCTYPE") || html.contains("<html"));
}

#[tokio::test]
async fn standalone_figure_dashboard_works() {
    // A report embedded in an app is legitimate: render_dashboard must dispatch.
    let html = render_standalone_figure(
        "render_dashboard",
        json!({
            "title": "Embedded report",
            "panels": [
                {"title": "Counts", "figure": {"tool": "show_chart", "params": {"data": {
                    "type": "bar", "labels": ["A"], "datasets": [{"label": "S", "data": [1.0]}]
                }}}}
            ]
        }),
    )
    .await
    .expect("render_dashboard should render");
    assert!(html.contains("Embedded report"));
    // A report inlines its libraries too — never a CDN reference.
    assert!(!html.contains("cdn.jsdelivr.net"));
}

/// ⚠ This asserts the per-kind TOOL NAMES on purpose, and it is the one place
/// that still should.
///
/// The dashboard's copy of this message was rewritten to name `render_figure`
/// and the `kind` slugs, because a chat agent sees only three tools and cannot
/// call `render_volcano` (#142). This door is Agent Drafter's `ui_figure`, whose
/// own description hands the app agent exactly these names and which has neither
/// `render_figure` nor `describe_figure` — `configure_agent` never injects
/// autovisualiser into an app agent. Sharing one phrasing between the two doors
/// is what created a NEW dead end here while fixing the one over there; the
/// vocabulary is therefore chosen per call site.
#[tokio::test]
async fn standalone_figure_unknown_tool_errs_with_suggestions() {
    let err = render_standalone_figure("totally_made_up", json!({"data": {}}))
        .await
        .unwrap_err();
    assert!(err.contains("Unknown visualization"), "got: {err}");
    // Names the caller can reach for instead.
    assert!(
        err.contains("render_volcano") || err.contains("show_chart"),
        "got: {err}"
    );
    assert!(
        !err.contains("describe_figure"),
        "an app agent has no describe_figure to call: {err}"
    );
}

#[tokio::test]
async fn standalone_figure_invalid_args_err_is_friendly() {
    // show_chart validates that at least one dataset is present.
    let err = render_standalone_figure(
        "show_chart",
        json!({"data": {"type": "bar", "datasets": []}}),
    )
    .await
    .unwrap_err();
    assert!(err.contains("at least one dataset"), "got: {err}");
}

/// The same rule for a REJECTED PAYLOAD, which is the half the shared
/// `figure_argument_error` choke point actually broke.
///
/// Measured before this fix: `ui_figure("render_volcano", …)` with a missing
/// field came back as "`render_figure` arguments are invalid for kind
/// \"volcano\": missing field `log2fc`. Call describe_figure with kind
/// \"volcano\"…" — an app agent being told to fix a call it never made, with two
/// tools it does not have. It must name the tool the caller named.
#[tokio::test]
async fn standalone_figure_invalid_args_name_the_tool_the_caller_named() {
    let err = render_standalone_figure(
        "render_volcano",
        json!({"data": {"points": [{"label": "MYC", "negLog10P": 4.0}]}}),
    )
    .await
    .unwrap_err();

    assert!(
        err.contains("log2fc"),
        "must still say what is wrong: {err}"
    );
    assert!(
        err.contains("render_volcano"),
        "must name the tool `ui_figure` takes: {err}"
    );
    assert!(
        !err.contains("render_figure"),
        "an app agent cannot call render_figure: {err}"
    );
    assert!(
        !err.contains("describe_figure"),
        "an app agent cannot call describe_figure: {err}"
    );
}

#[tokio::test]
async fn standalone_figure_ignores_cdn_env_flag() {
    // The standalone path forces inlining via a task-local checked *before* the
    // BIOROUTER_AUTOVIS_CDN env read, so a figure is self-contained even when the
    // desktop app has CDN mode on. We assert the override mechanism directly here
    // rather than mutating the process-wide env var, which would race the other
    // figure unit tests that assert no CDN (see autovis_dashboard_cdn.rs, which is
    // a separate binary for exactly that reason).
    let cdn_inside = common::with_inline_assets(async { common::use_cdn() }).await;
    assert!(!cdn_inside, "with_inline_assets must force use_cdn() off");

    // And end-to-end: the emitted document inlines the library, never a CDN tag.
    let html = render_standalone_figure(
        "show_chart",
        json!({"data": {
            "type": "line",
            "labels": ["A"],
            "datasets": [{"label": "S", "data": [1.0]}]
        }}),
    )
    .await
    .unwrap();
    assert!(html.contains("Chart.js v"));
    assert!(!html.contains("cdn.jsdelivr.net"));
    assert!(!html.contains("<script src="));
}

include!("tests_extra.rs");
include!("tests_dashboard.rs");
include!("tests_distributions.rs");
include!("tests_cartesian.rs");
