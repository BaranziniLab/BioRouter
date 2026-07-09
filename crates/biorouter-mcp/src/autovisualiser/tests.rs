use super::common::validate_data_param;
use super::*;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{ErrorCode, RawContent, ResourceContents, Role};
use serde_json::json;

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

include!("tests_extra.rs");
include!("tests_dashboard.rs");
