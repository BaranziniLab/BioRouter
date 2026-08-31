#[tokio::test]
async fn hierarchy_figures_preserve_literal_labels_and_readable_alternatives() {
    let label = "Δοκιμή 東京🧬 <img src=x onerror=alert(1)> </script>";
    let results = [
        ok_render!(render_boxplot, RenderBoxplotParams, "ui://boxplot/chart", {
            "data":{"groups":[{"label":label,"values":[]},{"label":label,"values":[1,2,3]}]}
        }),
        ok_render!(render_sunburst, RenderSunburstParams, "ui://sunburst/chart", {
            "data":{"name":label,"value":2,"children":[{"name":label,"value":3}]}
        }),
        ok_render!(render_wordcloud, RenderWordcloudParams, "ui://wordcloud/chart", {
            "data":{"words":[{"text":label,"weight":0},{"text":"Observed","weight":9}]}
        }),
    ];
    for result in &results {
        let html = decode_html(result);
        assert!(html.contains("BioRouterViz.applyScientificStyles()"));
        assert!(html.contains("BioRouterViz.renderFigureData"));
        assert!(html.contains("tabindex=\"0\" role=\"region\""));
        assert!(html.contains("Δοκιμή 東京🧬 \\u003cimg"));
        assert!(!html.contains(label));
        assert!(!html.contains(".html("));
    }
}

#[tokio::test]
async fn sunburst_rejects_invalid_own_values_including_internal_nodes() {
    let router = AutoVisualiserRouter::new();
    for value in [-1.0, f64::NAN, f64::INFINITY] {
        for internal in [true, false] {
            let mut params: RenderSunburstParams = serde_json::from_value(serde_json::json!({
                "data":{"name":"Root","value":2,"children":[{"name":"Leaf","value":3}]}
            }))
            .unwrap();
            if internal {
                params.data.value = Some(value);
            } else {
                params.data.children.as_mut().unwrap()[0].value = Some(value);
            }
            let error = router
                .render_sunburst(Parameters(params))
                .await
                .unwrap_err();
            assert!(error.message.contains("finite and non-negative"));
        }
    }
    ok_render!(render_sunburst, RenderSunburstParams, "ui://sunburst/chart", {
        "data":{"name":"All zero","children":[{"name":"Observed zero","value":0}]}
    });
}

#[tokio::test]
async fn wordcloud_rejects_invalid_terms_and_weights_but_preserves_zero() {
    let router = AutoVisualiserRouter::new();
    for weight in [-1.0, f64::NAN, f64::INFINITY] {
        let mut params: RenderWordcloudParams = serde_json::from_value(serde_json::json!({
            "data":{"words":[{"text":"Synthetic","weight":1}]}
        }))
        .unwrap();
        params.data.words[0].weight = weight;
        let error = router
            .render_wordcloud(Parameters(params))
            .await
            .unwrap_err();
        assert!(error.message.contains("finite and non-negative"));
    }
    for text in ["", " \t\n"] {
        let params: RenderWordcloudParams = serde_json::from_value(serde_json::json!({
            "data":{"words":[{"text":text,"weight":1}]}
        }))
        .unwrap();
        assert!(router.render_wordcloud(Parameters(params)).await.is_err());
    }
    ok_render!(render_wordcloud, RenderWordcloudParams, "ui://wordcloud/chart", {
        "data":{"words":[{"text":"Observed zero","weight":0}]}
    });
}

#[tokio::test]
async fn boxplot_validates_finite_observations_and_existing_resource_limits() {
    let router = AutoVisualiserRouter::new();
    for value in [f64::NAN, f64::INFINITY] {
        let mut params: RenderBoxplotParams = serde_json::from_value(serde_json::json!({
            "data":{"groups":[{"label":"Synthetic","values":[1]}]}
        }))
        .unwrap();
        params.data.groups[0].values[0] = value;
        assert!(router.render_boxplot(Parameters(params)).await.is_err());
    }
    let mut params: RenderBoxplotParams = serde_json::from_value(serde_json::json!({
        "data":{"groups":[{"label":"Synthetic","values":[1]}]}
    }))
    .unwrap();
    params.data.groups[0].values = vec![1.0; MAX_VALUES + 1];
    assert!(router.render_boxplot(Parameters(params)).await.is_err());
}
