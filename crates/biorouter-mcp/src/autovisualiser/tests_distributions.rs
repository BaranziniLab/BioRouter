#[tokio::test]
async fn distribution_figures_keep_academic_shells_and_exact_data_alternatives() {
    let label = "Δοκιμή 東京 <img src=x onerror=alert(1)> </script>";
    let results = [
        ok_render!(render_gauge, RenderGaugeParams, "ui://gauge/chart", {
            "data":{"title":label,"label":label,"value":150,"min":0,"max":100}
        }),
        ok_render!(render_histogram, RenderHistogramParams, "ui://histogram/chart", {
            "data":{"title":label,"xAxisLabel":label,"values":[1.00001,1.00002],"bins":2,"color":"#a05a32"}
        }),
    ];
    for result in &results {
        let html = decode_html(result);
        assert!(html.contains("BioRouterViz.applyScientificStyles()"));
        assert!(html.contains("BioRouterViz.renderFigureData"));
        assert!(html.contains("role=\"img\""));
        assert!(html.contains("tabindex=\"0\" role=\"region\""));
        assert!(html.contains("Δοκιμή 東京 \\u003cimg"));
        assert!(!html.contains(label));
    }
    assert!(decode_html(&results[1]).contains("#a05a32"));
}
