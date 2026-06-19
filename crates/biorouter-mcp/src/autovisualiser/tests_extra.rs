// Edge-case tests for the expansion tools. Params are built with `from_value`
// so these also exercise real deserialization (defaults, renames, lenient input).

use base64::{engine::general_purpose::STANDARD, Engine as _};

/// Decode the HTML blob from a successful render result.
fn decode_html(result: &CallToolResult) -> String {
    if let RawContent::Resource(resource) = &*result.content[0] {
        if let ResourceContents::BlobResourceContents { blob, .. } = &resource.resource {
            return String::from_utf8(STANDARD.decode(blob).unwrap()).unwrap();
        }
    }
    panic!("no resource blob");
}

macro_rules! ok_render {
    ($method:ident, $ty:ty, $uri:expr, $json:tt) => {{
        let router = AutoVisualiserRouter::new();
        let params: $ty = serde_json::from_value(serde_json::json!($json)).unwrap();
        let result = router.$method(Parameters(params)).await.unwrap();
        assert_resource_result(&result, $uri);
        result
    }};
}

macro_rules! err_render {
    ($method:ident, $ty:ty, $json:tt) => {{
        let router = AutoVisualiserRouter::new();
        let params: $ty = serde_json::from_value(serde_json::json!($json)).unwrap();
        assert!(router.$method(Parameters(params)).await.is_err());
    }};
}

// ===========================================================================
// Diagrams (Mermaid wrappers)
// ===========================================================================

#[tokio::test]
async fn test_flowchart_ok_and_compiles() {
    let r = ok_render!(render_flowchart, RenderFlowchartParams, "ui://mermaid/diagram", {
        "data": {"direction":"LR","nodes":[{"id":"a","label":"Start","shape":"circle"},{"id":"b","shape":"diamond"}],"edges":[{"from":"a","to":"b","label":"go","style":"dotted"}]}
    });
    let html = decode_html(&r);
    assert!(html.contains("flowchart LR"));
    assert!(html.contains("-.->"));
}

#[tokio::test]
async fn test_flowchart_empty_errors() {
    err_render!(render_flowchart, RenderFlowchartParams, {"data": {"edges": []}});
}

#[tokio::test]
async fn test_flowchart_sanitizes_ids_and_escapes() {
    // A malicious id/label must not break out of the script context.
    let r = ok_render!(render_flowchart, RenderFlowchartParams, "ui://mermaid/diagram", {
        "data": {"edges":[{"from":"a b","to":"</script>","label":"\"x\""}]}
    });
    let html = decode_html(&r);
    let start = html.find("const mermaidCode =").unwrap();
    assert!(!html[start..start + 200].contains("</script>"));
}

#[tokio::test]
async fn test_gantt_ok() {
    let r = ok_render!(render_gantt, RenderGanttParams, "ui://mermaid/diagram", {
        "data": {"title":"S","sections":[{"name":"P1","tasks":[{"name":"Recruit","id":"t1","start":"2024-01-01","duration":"30d","status":"active"}]}]}
    });
    assert!(decode_html(&r).contains("gantt"));
}

#[tokio::test]
async fn test_gantt_empty_errors() {
    err_render!(render_gantt, RenderGanttParams, {"data": {"sections": []}});
}

#[tokio::test]
async fn test_sequence_ok() {
    let r = ok_render!(render_sequence, RenderSequenceParams, "ui://mermaid/diagram", {
        "data": {"messages":[{"from":"Client","to":"Server","text":"Req"},{"from":"Server","to":"Client","text":"Res","arrow":"dashed"}]}
    });
    let html = decode_html(&r);
    assert!(html.contains("sequenceDiagram"));
    assert!(html.contains("-->>"));
}

#[tokio::test]
async fn test_sequence_empty_errors() {
    err_render!(render_sequence, RenderSequenceParams, {"data": {"messages": []}});
}

#[tokio::test]
async fn test_mindmap_ok() {
    ok_render!(render_mindmap, RenderMindmapParams, "ui://mermaid/diagram", {
        "data": {"root":{"text":"Root","children":[{"text":"A","children":[{"text":"A1"}]},{"text":"B"}]}}
    });
}

#[tokio::test]
async fn test_timeline_ok_and_empty() {
    ok_render!(render_timeline, RenderTimelineParams, "ui://mermaid/diagram", {
        "data": {"periods":[{"period":"2019","events":["Founded"]},{"period":"2021","events":["A","B"]}]}
    });
    err_render!(render_timeline, RenderTimelineParams, {"data": {"periods": []}});
}

#[tokio::test]
async fn test_er_ok_and_empty() {
    let r = ok_render!(render_er_diagram, RenderErParams, "ui://mermaid/diagram", {
        "data": {"entities":[{"name":"CUSTOMER","attributes":[{"name":"id","type":"int","key":"PK"}]},{"name":"ORDER"}],"relationships":[{"from":"CUSTOMER","to":"ORDER","cardinality":"one-to-many"}]}
    });
    assert!(decode_html(&r).contains("erDiagram"));
    err_render!(render_er_diagram, RenderErParams, {"data": {"entities": []}});
}

#[tokio::test]
async fn test_state_ok_and_empty() {
    let r = ok_render!(render_state_diagram, RenderStateParams, "ui://mermaid/diagram", {
        "data": {"transitions":[{"from":"[*]","to":"Idle"},{"from":"Idle","to":"Run","label":"go"}]}
    });
    let html = decode_html(&r);
    assert!(html.contains("stateDiagram-v2"));
    assert!(html.contains("[*]"));
    err_render!(render_state_diagram, RenderStateParams, {"data": {"transitions": []}});
}

#[tokio::test]
async fn test_class_ok_and_empty() {
    let r = ok_render!(render_class_diagram, RenderClassParams, "ui://mermaid/diagram", {
        "data": {"classes":[{"name":"Animal","attributes":["+String name"],"methods":["+eat()"]},{"name":"Dog"}],"relationships":[{"from":"Dog","to":"Animal","type":"inheritance"}]}
    });
    let html = decode_html(&r);
    assert!(html.contains("classDiagram"));
    // The inheritance arrow `<|--` is present but `<` is escaped to < in the blob.
    assert!(html.contains("\\u003c|--"));
    err_render!(render_class_diagram, RenderClassParams, {"data": {"classes": []}});
}

// ===========================================================================
// Chart.js tools
// ===========================================================================

#[tokio::test]
async fn test_histogram_ok_and_edges() {
    ok_render!(render_histogram, RenderHistogramParams, "ui://histogram/chart", {
        "data": {"title":"Ages","values":[1,2,3,4,5,6,7,8,9,10],"bins":4}
    });
    err_render!(render_histogram, RenderHistogramParams, {"data": {"values": []}});
}

#[tokio::test]
async fn test_bubble_ok_and_empty() {
    ok_render!(render_bubble, RenderBubbleParams, "ui://bubble/chart", {
        "data": {"datasets":[{"label":"A","data":[{"x":1,"y":2,"r":5,"label":"p"}]}]}
    });
    err_render!(render_bubble, RenderBubbleParams, {"data": {"datasets":[{"label":"A","data":[]}]}});
}

#[tokio::test]
async fn test_area_ok_and_mismatch() {
    ok_render!(render_area, RenderAreaParams, "ui://area/chart", {
        "data": {"labels":["Jan","Feb"],"stacked":true,"datasets":[{"label":"Web","data":[1,2]}]}
    });
    err_render!(render_area, RenderAreaParams, {"data": {"labels":["Jan","Feb"],"datasets":[{"label":"x","data":[1]}]}});
}

#[tokio::test]
async fn test_gauge_ok_and_bad_range() {
    ok_render!(render_gauge, RenderGaugeParams, "ui://gauge/chart", {
        "data": {"value":72,"min":0,"max":100,"label":"%","thresholds":[{"value":50,"color":"#2ecc71"},{"value":100,"color":"#e74c3c"}]}
    });
    err_render!(render_gauge, RenderGaugeParams, {"data": {"value":5,"min":10,"max":10}});
}

#[tokio::test]
async fn test_volcano_ok_and_empty() {
    ok_render!(render_volcano, RenderVolcanoParams, "ui://volcano/chart", {
        "data": {"points":[{"label":"TP53","log2fc":2.4,"negLog10P":6.1},{"log2fc":0.1,"negLog10P":0.2}]}
    });
    err_render!(render_volcano, RenderVolcanoParams, {"data": {"points": []}});
}

#[tokio::test]
async fn test_manhattan_ok_and_empty() {
    ok_render!(render_manhattan, RenderManhattanParams, "ui://manhattan/chart", {
        "data": {"points":[{"chrom":"1","pos":100,"negLog10P":3.0},{"chrom":"X","pos":50,"negLog10P":8.0,"label":"rs1"}]}
    });
    err_render!(render_manhattan, RenderManhattanParams, {"data": {"points": []}});
}

// ===========================================================================
// D3 tools
// ===========================================================================

#[tokio::test]
async fn test_network_ok_and_unknown_node() {
    ok_render!(render_network, RenderNetworkParams, "ui://network/graph", {
        "data": {"nodes":[{"id":"A","group":"g1"},{"id":"B"}],"links":[{"source":"A","target":"B","value":2}],"directed":true}
    });
    err_render!(render_network, RenderNetworkParams, {"data": {"nodes":[{"id":"A"}],"links":[{"source":"A","target":"Z"}]}});
}

#[tokio::test]
async fn test_heatmap_ok_and_dim_mismatch() {
    ok_render!(render_heatmap, RenderHeatmapParams, "ui://heatmap/chart", {
        "data": {"xLabels":["S1","S2"],"yLabels":["G1","G2"],"values":[[1.0,2.0],[3.0,4.0]]}
    });
    // Row count must match yLabels count.
    err_render!(render_heatmap, RenderHeatmapParams, {"data": {"xLabels":["S1","S2"],"yLabels":["G1","G2"],"values":[[1.0,2.0]]}});
    // Column count must match xLabels count.
    err_render!(render_heatmap, RenderHeatmapParams, {"data": {"xLabels":["S1","S2"],"yLabels":["G1"],"values":[[1.0]]}});
}

#[tokio::test]
async fn test_sunburst_and_dendrogram_ok() {
    ok_render!(render_sunburst, RenderSunburstParams, "ui://sunburst/chart", {
        "data": {"name":"Body","children":[{"name":"Brain","children":[{"name":"Cortex","value":40}]},{"name":"Heart","value":20}]}
    });
    ok_render!(render_dendrogram, RenderDendrogramParams, "ui://dendrogram/chart", {
        "data": {"name":"root","children":[{"name":"A","children":[{"name":"x"}]},{"name":"B"}]}
    });
}

#[tokio::test]
async fn test_calendar_ok_and_empty() {
    ok_render!(render_calendar_heatmap, RenderCalendarParams, "ui://calendar/heatmap", {
        "data": {"title":"Act","values":[{"date":"2024-01-01","value":3},{"date":"2024-01-05","value":7}]}
    });
    err_render!(render_calendar_heatmap, RenderCalendarParams, {"data": {"values": []}});
}

#[tokio::test]
async fn test_boxplot_ok_and_empty() {
    ok_render!(render_boxplot, RenderBoxplotParams, "ui://boxplot/chart", {
        "data": {"groups":[{"label":"Control","values":[5,6,7,6,8,5,20]},{"label":"Treated","values":[10,12,11,13]}]}
    });
    err_render!(render_boxplot, RenderBoxplotParams, {"data": {"groups":[{"label":"x","values":[]}]}});
}

#[tokio::test]
async fn test_wordcloud_ok_and_empty() {
    ok_render!(render_wordcloud, RenderWordcloudParams, "ui://wordcloud/chart", {
        "data": {"words":[{"text":"genomics","weight":40},{"text":"AI","weight":30}]}
    });
    err_render!(render_wordcloud, RenderWordcloudParams, {"data": {"words": []}});
}

#[tokio::test]
async fn test_kaplan_meier_ok_and_empty() {
    ok_render!(render_kaplan_meier, RenderKaplanMeierParams, "ui://kaplanmeier/chart", {
        "data": {"groups":[{"label":"A","points":[{"time":0,"survival":1.0},{"time":5,"survival":0.8},{"time":10,"survival":0.6,"censored":true}]}]}
    });
    err_render!(render_kaplan_meier, RenderKaplanMeierParams, {"data": {"groups":[{"label":"A","points":[]}]}});
}

#[tokio::test]
async fn test_forest_ok_and_invalid_ci() {
    ok_render!(render_forest, RenderForestParams, "ui://forest/chart", {
        "data": {"title":"OR","logScale":true,"rows":[{"label":"S1","estimate":1.4,"lower":1.1,"upper":1.8,"weight":3},{"label":"S2","estimate":0.9,"lower":0.6,"upper":1.3}]}
    });
    // lower > upper
    err_render!(render_forest, RenderForestParams, {"data": {"rows":[{"label":"x","estimate":1.0,"lower":2.0,"upper":1.0}]}});
    // non-positive on log scale
    err_render!(render_forest, RenderForestParams, {"data": {"logScale":true,"rows":[{"label":"x","estimate":0.0,"lower":-1.0,"upper":1.0}]}});
}

// ===========================================================================
// Geo
// ===========================================================================

#[tokio::test]
async fn test_choropleth_ok() {
    ok_render!(render_choropleth, RenderChoroplethParams, "ui://choropleth/map", {
        "data": {"valueProperty":"cases","nameProperty":"name","geojson":{"type":"FeatureCollection","features":[{"type":"Feature","properties":{"name":"A","cases":120},"geometry":{"type":"Polygon","coordinates":[[[0,0],[0,1],[1,1],[1,0],[0,0]]]}}]}}
    });
}

#[tokio::test]
async fn test_choropleth_errors() {
    // No valueProperty and no values.
    err_render!(render_choropleth, RenderChoroplethParams, {"data": {"geojson":{"type":"FeatureCollection","features":[{"type":"Feature","properties":{},"geometry":{}}]}}});
    // Empty features.
    err_render!(render_choropleth, RenderChoroplethParams, {"data": {"valueProperty":"v","geojson":{"type":"FeatureCollection","features":[]}}});
    // Not a geojson object.
    err_render!(render_choropleth, RenderChoroplethParams, {"data": {"valueProperty":"v","geojson":"nope"}});
}

// ===========================================================================
// Cross-cutting hardening
// ===========================================================================

#[tokio::test]
async fn test_chart_blob_escapes_malicious_title() {
    let router = AutoVisualiserRouter::new();
    let params: ShowChartParams = serde_json::from_value(serde_json::json!({
        "data": {"type":"bar","title":"</script><script>alert(1)</script>","datasets":[{"label":"x","data":[1,2,3]}]}
    }))
    .unwrap();
    let result = router.show_chart(Parameters(params)).await.unwrap();
    let html = decode_html(&result);
    let start = html.find("const chartData =").unwrap();
    assert!(!html[start..start + 300].contains("</script>"));
}

#[tokio::test]
async fn test_show_chart_lenient_uppercase_type() {
    // "Bar" (capitalized) must parse via the lenient enum.
    let router = AutoVisualiserRouter::new();
    let params: ShowChartParams = serde_json::from_value(serde_json::json!({
        "data": {"type":"Bar","datasets":[{"label":"x","data":[1,2,3]}]}
    }))
    .unwrap();
    assert!(router.show_chart(Parameters(params)).await.is_ok());
}

/// Generate a rich-data HTML gallery for every tool into /tmp/av_gallery for
/// headless browser render-verification. Run with:
///   cargo test -p biorouter-mcp --lib autovisualiser::tests::generate_gallery -- --ignored
#[tokio::test]
#[ignore]
async fn generate_gallery() {
    let dir = std::path::Path::new("/tmp/av_gallery");
    std::fs::create_dir_all(dir).unwrap();
    let router = AutoVisualiserRouter::new();
    macro_rules! gen {
        ($name:expr, $method:ident, $ty:ty, $json:tt) => {{
            let params: $ty = serde_json::from_value(serde_json::json!($json)).unwrap();
            let r = router.$method(Parameters(params)).await.unwrap();
            std::fs::write(dir.join(concat!($name, ".html")), decode_html(&r)).unwrap();
        }};
    }

    // Original tools
    gen!("show_chart", show_chart, ShowChartParams, {"data":{"type":"line","title":"Sales","labels":["Jan","Feb","Mar","Apr"],"datasets":[{"label":"A","data":[5,9,7,12]},{"label":"B","data":[3,4,8,6]}]}});
    gen!("donut", render_donut, RenderDonutParams, {"data":{"title":"Budget","data":[{"label":"R&D","value":40},{"label":"Sales","value":25},{"label":"Ops","value":35}]}});
    gen!("radar", render_radar, RenderRadarParams, {"data":{"labels":["Speed","Power","Range","Agility","IQ"],"datasets":[{"label":"P1","data":[80,70,90,60,85]},{"label":"P2","data":[60,90,70,80,75]}]}});
    gen!("sankey", render_sankey, RenderSankeyParams, {"data":{"nodes":[{"name":"A","category":"source"},{"name":"B","category":"process"},{"name":"C","category":"end"},{"name":"D","category":"end"}],"links":[{"source":"A","target":"B","value":10},{"source":"B","target":"C","value":6},{"source":"B","target":"D","value":4}]}});
    gen!("treemap", render_treemap, RenderTreemapParams, {"data":{"name":"root","children":[{"name":"G1","children":[{"name":"a","value":10,"category":"x"},{"name":"b","value":20,"category":"y"}]},{"name":"c","value":15,"category":"x"}]}});
    gen!("chord", render_chord, RenderChordParams, {"data":{"labels":["NA","EU","AS","AF"],"matrix":[[0,15,25,8],[18,0,20,12],[22,18,0,15],[5,10,18,0]]}});
    gen!("map", render_map, RenderMapParams, {"data":{"title":"Sites","markers":[{"lat":37.77,"lng":-122.42,"name":"SF","value":150},{"lat":40.71,"lng":-74.0,"name":"NYC","value":200}]}});
    gen!("mermaid", render_mermaid, RenderMermaidParams, {"mermaid_code":"graph TD; A-->B; A-->C; B-->D; C-->D;"});

    // Diagrams
    gen!("flowchart", render_flowchart, RenderFlowchartParams, {"data":{"direction":"LR","nodes":[{"id":"a","label":"Start","shape":"circle"},{"id":"b","label":"Choose","shape":"diamond"},{"id":"c","label":"Done","shape":"stadium"}],"edges":[{"from":"a","to":"b"},{"from":"b","to":"c","label":"yes"}]}});
    gen!("gantt", render_gantt, RenderGanttParams, {"data":{"title":"Plan","sections":[{"name":"Phase 1","tasks":[{"name":"Design","id":"t1","start":"2024-01-01","duration":"20d","status":"active"},{"name":"Build","start":"after t1","duration":"30d"}]}]}});
    gen!("sequence", render_sequence, RenderSequenceParams, {"data":{"title":"Auth","messages":[{"from":"Client","to":"Server","text":"Login"},{"from":"Server","to":"DB","text":"Verify"},{"from":"Server","to":"Client","text":"Token","arrow":"dashed"}]}});
    gen!("mindmap", render_mindmap, RenderMindmapParams, {"data":{"root":{"text":"Research","children":[{"text":"Data","children":[{"text":"Clean"},{"text":"Label"}]},{"text":"Model"}]}}});
    gen!("timeline", render_timeline, RenderTimelineParams, {"data":{"title":"History","periods":[{"period":"2019","events":["Founded"]},{"period":"2021","events":["Series A","Launch"]}]}});
    gen!("er_diagram", render_er_diagram, RenderErParams, {"data":{"entities":[{"name":"CUSTOMER","attributes":[{"name":"id","type":"int","key":"PK"},{"name":"name","type":"string"}]},{"name":"ORDER","attributes":[{"name":"id","type":"int","key":"PK"}]}],"relationships":[{"from":"CUSTOMER","to":"ORDER","label":"places","cardinality":"one-to-many"}]}});
    gen!("state_diagram", render_state_diagram, RenderStateParams, {"data":{"transitions":[{"from":"[*]","to":"Idle"},{"from":"Idle","to":"Running","label":"start"},{"from":"Running","to":"[*]","label":"stop"}]}});
    gen!("class_diagram", render_class_diagram, RenderClassParams, {"data":{"classes":[{"name":"Animal","attributes":["+String name"],"methods":["+eat()"]},{"name":"Dog","methods":["+bark()"]}],"relationships":[{"from":"Dog","to":"Animal","type":"inheritance"}]}});

    // Chart.js
    gen!("histogram", render_histogram, RenderHistogramParams, {"data":{"title":"Ages","values":[21,23,25,28,31,33,34,34,35,37,40,41,42,45,52,55,61],"bins":7}});
    gen!("bubble", render_bubble, RenderBubbleParams, {"data":{"title":"Markets","datasets":[{"label":"2024","data":[{"x":10,"y":20,"r":15,"label":"A"},{"x":30,"y":12,"r":8,"label":"B"},{"x":22,"y":28,"r":22,"label":"C"}]}]}});
    gen!("area", render_area, RenderAreaParams, {"data":{"title":"Traffic","labels":["Jan","Feb","Mar","Apr"],"stacked":true,"datasets":[{"label":"Web","data":[10,15,12,18]},{"label":"Mobile","data":[5,9,14,11]}]}});
    gen!("gauge", render_gauge, RenderGaugeParams, {"data":{"title":"CPU","value":72,"min":0,"max":100,"label":"%","thresholds":[{"value":50,"color":"#2ecc71"},{"value":80,"color":"#f6c945"},{"value":100,"color":"#e74c3c"}]}});
    gen!("volcano", render_volcano, RenderVolcanoParams, {"data":{"title":"DE","points":[{"label":"TP53","log2fc":2.4,"negLog10P":6.1},{"label":"MYC","log2fc":-2.1,"negLog10P":5.2},{"label":"GAPDH","log2fc":0.1,"negLog10P":0.3},{"label":"EGFR","log2fc":1.5,"negLog10P":3.0}]}});
    gen!("manhattan", render_manhattan, RenderManhattanParams, {"data":{"title":"GWAS","points":[{"chrom":"1","pos":100,"negLog10P":3.0},{"chrom":"1","pos":5000,"negLog10P":5.5},{"chrom":"2","pos":200,"negLog10P":8.2,"label":"rs1"},{"chrom":"X","pos":300,"negLog10P":2.0}]}});

    // D3
    gen!("network", render_network, RenderNetworkParams, {"data":{"title":"PPI","nodes":[{"id":"TP53","group":"tumor","value":5},{"id":"MDM2","group":"reg"},{"id":"CDKN2A","group":"tumor"},{"id":"ATM","group":"reg"}],"links":[{"source":"MDM2","target":"TP53","value":3},{"source":"ATM","target":"TP53","value":2},{"source":"CDKN2A","target":"MDM2","value":1}],"directed":true}});
    gen!("heatmap", render_heatmap, RenderHeatmapParams, {"data":{"title":"Expr","xLabels":["S1","S2","S3"],"yLabels":["GeneA","GeneB","GeneC"],"values":[[1.2,-0.4,0.8],[0.0,2.1,-1.1],[0.5,0.3,1.5]]}});
    gen!("sunburst", render_sunburst, RenderSunburstParams, {"data":{"name":"Body","children":[{"name":"Brain","children":[{"name":"Cortex","value":40},{"name":"Cerebellum","value":10}]},{"name":"Heart","value":20},{"name":"Liver","value":15}]}});
    gen!("dendrogram", render_dendrogram, RenderDendrogramParams, {"data":{"name":"root","children":[{"name":"Cluster A","children":[{"name":"x"},{"name":"y"}]},{"name":"Cluster B","children":[{"name":"z"},{"name":"w"}]}]}});
    gen!("calendar", render_calendar_heatmap, RenderCalendarParams, {"data":{"title":"Activity","values":[{"date":"2024-01-01","value":3},{"date":"2024-01-02","value":7},{"date":"2024-01-08","value":2},{"date":"2024-02-01","value":9},{"date":"2024-02-15","value":5}]}});
    gen!("boxplot", render_boxplot, RenderBoxplotParams, {"data":{"title":"Expr","yAxisLabel":"TPM","groups":[{"label":"Control","values":[5,6,7,6,8,5,20]},{"label":"Treated","values":[10,12,11,13,12,11,9]}]}});
    gen!("wordcloud", render_wordcloud, RenderWordcloudParams, {"data":{"title":"Topics","words":[{"text":"genomics","weight":40},{"text":"AI","weight":33},{"text":"clinical","weight":25},{"text":"protein","weight":20},{"text":"variant","weight":15},{"text":"cohort","weight":12}]}});
    gen!("kaplan_meier", render_kaplan_meier, RenderKaplanMeierParams, {"data":{"title":"Survival","groups":[{"label":"Arm A","points":[{"time":0,"survival":1.0},{"time":5,"survival":0.85},{"time":10,"survival":0.6,"censored":true},{"time":15,"survival":0.4}]},{"label":"Arm B","points":[{"time":0,"survival":1.0},{"time":5,"survival":0.7},{"time":10,"survival":0.45},{"time":15,"survival":0.25}]}]}});
    gen!("forest", render_forest, RenderForestParams, {"data":{"title":"OR","logScale":true,"rows":[{"label":"Study 1","estimate":1.4,"lower":1.1,"upper":1.8,"weight":3},{"label":"Study 2","estimate":0.9,"lower":0.6,"upper":1.3,"weight":2},{"label":"Study 3","estimate":1.1,"lower":0.8,"upper":1.5,"weight":4}]}});

    // Geo
    gen!("choropleth", render_choropleth, RenderChoroplethParams, {"data":{"title":"Cases","valueProperty":"cases","nameProperty":"name","geojson":{"type":"FeatureCollection","features":[{"type":"Feature","properties":{"name":"West","cases":120},"geometry":{"type":"Polygon","coordinates":[[[0,0],[0,2],[2,2],[2,0],[0,0]]]}},{"type":"Feature","properties":{"name":"East","cases":60},"geometry":{"type":"Polygon","coordinates":[[[2,0],[2,2],[4,2],[4,0],[2,0]]]}}]}}});

    eprintln!("Gallery written to {}", dir.display());
}

#[tokio::test]
async fn test_every_render_returns_two_audience_tagged_items() {
    // Spot-check that a representative tool keeps the user-resource +
    // assistant-text contract that prevents retry loops.
    let r = ok_render!(render_network, RenderNetworkParams, "ui://network/graph", {
        "data": {"nodes":[{"id":"A"}],"links":[]}
    });
    assert_eq!(r.content.len(), 2);
    assert_eq!(r.content[0].audience().unwrap(), &vec![Role::User]);
    assert_eq!(r.content[1].audience().unwrap(), &vec![Role::Assistant]);
}
