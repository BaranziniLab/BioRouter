#[tokio::test]
async fn cartesian_figures_preserve_literal_labels_and_readable_data() {
    let label = "Δοκιμή 東京 <img src=x onerror=alert(1)> </script>";
    let results = [
        ok_render!(render_area, RenderAreaParams, "ui://area/chart", {
            "data":{"labels":[label],"datasets":[{"label":label,"data":[-5],"color":"#abc"}],"stacked":true}
        }),
        ok_render!(render_bubble, RenderBubbleParams, "ui://bubble/chart", {
            "data":{"datasets":[{"label":label,"color":"rgb(1,2,3)","data":[{"x":1,"y":2,"r":0,"label":label}]}]}
        }),
        ok_render!(render_volcano, RenderVolcanoParams, "ui://volcano/chart", {
            "data":{"points":[{"label":label,"log2fc":-2,"negLog10P":3}],"fcThreshold":2,"pThreshold":3}
        }),
        ok_render!(render_manhattan, RenderManhattanParams, "ui://manhattan/chart", {
            "data":{"points":[{"label":label,"chrom":"__proto__","pos":1,"negLog10P":2}]}
        }),
    ];
    for result in &results {
        let html = decode_html(result);
        assert!(html.contains("BioRouterViz.applyScientificStyles()"));
        assert!(html.contains("BioRouterViz.renderFigureData"));
        assert!(html.contains("role=\"img\""));
        assert!(html.contains("Δοκιμή 東京 \\u003cimg"));
        assert!(!html.contains(label));
    }
}

#[tokio::test]
async fn cartesian_figures_reject_invalid_numbers_without_rejecting_observed_zero() {
    let router = AutoVisualiserRouter::new();
    for invalid in [-1.0, f64::NAN, f64::INFINITY] {
        let mut bubble: RenderBubbleParams = serde_json::from_value(serde_json::json!({
            "data":{"datasets":[{"label":"Synthetic","data":[{"x":0,"y":0,"r":0}]}]}
        }))
        .unwrap();
        bubble.data.datasets[0].data[0].r = invalid;
        assert!(router.render_bubble(Parameters(bubble)).await.is_err());
        let mut volcano: RenderVolcanoParams = serde_json::from_value(serde_json::json!({
            "data":{"points":[{"log2fc":-2,"negLog10P":0}]}
        }))
        .unwrap();
        volcano.data.p_threshold = Some(invalid);
        assert!(router.render_volcano(Parameters(volcano)).await.is_err());
        let mut manhattan: RenderManhattanParams = serde_json::from_value(serde_json::json!({
            "data":{"points":[{"chrom":"1","pos":0,"negLog10P":0}]}
        }))
        .unwrap();
        manhattan.data.points[0].pos = invalid;
        assert!(router
            .render_manhattan(Parameters(manhattan))
            .await
            .is_err());
    }
    for invalid in [f64::NAN, f64::INFINITY] {
        let mut area: RenderAreaParams = serde_json::from_value(serde_json::json!({
            "data":{"labels":["Synthetic"],"datasets":[{"label":"Signed","data":[-1]}]}
        }))
        .unwrap();
        area.data.datasets[0].data[0] = invalid;
        assert!(router.render_area(Parameters(area)).await.is_err());
    }
}
