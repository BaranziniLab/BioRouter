#[tokio::test]
async fn mermaid_preserves_safe_source_and_readable_viewport() {
    let source = "flowchart LR\n A[\"Δοκιμή 東京 <img src=x onerror=alert(1)> </script>\"] --> B";
    let result = ok_render!(render_mermaid, RenderMermaidParams, "ui://mermaid/diagram", {
        "mermaid_code": source
    });
    let html = decode_html(&result);
    assert!(html.contains("securityLevel: 'strict'"));
    assert!(html.contains("document.getElementById('diagramSource').textContent = mermaidCode"));
    assert!(html.contains("tabindex=\"0\" role=\"region\""));
    assert!(html.contains("svg.style.width = bounds.width + 'px'"));
    assert!(html.contains("fontSize: '16px'"));
    assert!(!html.contains(source));
    assert!(html.contains("\\u003cimg"));
}

#[tokio::test]
async fn dashboard_documented_example_renders_all_panels() {
    let source = include_str!("tools_dashboard.rs");
    let example = source
        .split("Example:\n")
        .nth(1)
        .unwrap()
        .split("\n\nPanel width")
        .next()
        .unwrap();
    let params: RenderDashboardParams = serde_json::from_str(example).unwrap();
    let result = AutoVisualiserRouter::new()
        .render_dashboard(Parameters(params))
        .await
        .unwrap();
    let receipt = result.structured_content.as_ref().unwrap();
    assert_eq!(receipt["figuresCreated"], 2);
    assert_eq!(receipt["figuresFailed"], 0);
    let html = decode_html(&result);
    assert!(html.contains("updateExpandControl(fig.classList.contains('span-full'))"));
    assert!(html.contains("expandBtn.setAttribute('aria-expanded', String(full))"));
    assert!(html.contains("minmax(min(100%, 260px), 1fr)"));
    assert!(html.contains("f.el.contentWindow === event.source"));
}

fn diagram_source(result: &CallToolResult) -> String {
    let html = decode_html(result);
    let json = html
        .split("const mermaidCode = ")
        .nth(1)
        .unwrap()
        .split(";\n")
        .next()
        .unwrap();
    serde_json::from_str(json).unwrap()
}

#[test]
fn typed_mermaid_ids_are_injective_and_ascii_safe() {
    let raw = [
        "A-B",
        "A B",
        "A_B",
        "br_412d42",
        "",
        " ",
        "Δ東京",
        "[*]",
        "a",
        " a",
    ];
    let ids: std::collections::HashSet<_> = raw.iter().map(|value| mermaid_id(value)).collect();
    assert_eq!(ids.len(), raw.len());
    assert_eq!(mermaid_id("A-B"), "br_412d42");
    assert_eq!(mermaid_id(""), "br_");
    assert!(ids.iter().all(|id| id
        .strip_prefix("br_")
        .is_some_and(|suffix| suffix.chars().all(|c| c.is_ascii_hexdigit()))));
    assert_eq!(state_token("[*]"), "[*]");
    assert_ne!(mermaid_id("[*]"), "[*]");
}

#[test]
fn typed_mermaid_gantt_exact_ids_precede_unambiguous_word_lists() {
    let ids = std::collections::HashSet::from(["A", "B", "C", "A B", "", " spaced "]);
    assert_eq!(
        mermaid_gantt_start("after A B", &ids).unwrap(),
        "after br_412042"
    );
    assert!(mermaid_gantt_start("after A B C", &ids).is_err());
    assert_eq!(
        mermaid_gantt_start("after A C", &ids).unwrap(),
        "after br_41 br_43"
    );
    assert_eq!(mermaid_gantt_start("after ", &ids).unwrap(), "after br_");
    assert_eq!(
        mermaid_gantt_start("after  spaced ", &ids).unwrap(),
        format!("after {}", mermaid_id(" spaced "))
    );
    assert!(mermaid_gantt_start("after unknown", &ids).is_err());
    assert_eq!(
        mermaid_gantt_start(" 2026-01-01 ", &ids).unwrap(),
        "2026-01-01"
    );
    let unicode_ids =
        std::collections::HashSet::from(["Δ", "東京", "終", "Δ\u{3000}東京", " \u{3000}Δ"]);
    assert_eq!(
        mermaid_gantt_start("after\u{3000}Δ", &unicode_ids).unwrap(),
        format!("after {}", mermaid_id("Δ"))
    );
    assert_eq!(
        mermaid_gantt_start("after\u{3000} \u{3000}Δ", &unicode_ids).unwrap(),
        format!("after {}", mermaid_id(" \u{3000}Δ"))
    );
    assert_eq!(
        mermaid_gantt_start("after Δ\u{3000}東京", &unicode_ids).unwrap(),
        format!("after {}", mermaid_id("Δ\u{3000}東京"))
    );
    assert!(mermaid_gantt_start("after 終 Δ\u{3000}東京", &unicode_ids).is_err());
    assert!(mermaid_gantt_start("after\u{2003}未定", &unicode_ids).is_err());
}

async fn typed_mermaid_fixture_results() -> Vec<(&'static str, CallToolResult)> {
    vec![
        (
            "raw",
            ok_render!(render_mermaid, RenderMermaidParams, "ui://mermaid/diagram", {
                "mermaid_code":"flowchart LR\n A[\"Raw source unchanged: Δ東京 👩🏽‍🔬 — measured observations with a deliberately long informative label, preserved without shrinking text\"] --> B[\"Independent review\"]\n style A fill:#e8eee8,stroke:#527967,color:#2a2520"
            }),
        ),
        (
            "flowchart",
            ok_render!(render_flowchart, RenderFlowchartParams, "ui://mermaid/diagram", {
                "data":{"title":"Synthetic typed identities","nodes":[{"id":"A-B","label":"A-B Δ東京"}],"edges":[{"from":"A-B","to":"A B"},{"from":"A B","to":"A_B"}]}
            }),
        ),
        (
            "gantt",
            ok_render!(render_gantt, RenderGanttParams, "ui://mermaid/diagram", {
                "data":{"title":"Synthetic dependencies","sections":[{"name":"Analysis","tasks":[
                    {"id":"A-B","name":"A-B","start":"2026-01-01","duration":"2d"},
                    {"id":"A B","name":"A B","start":"after A-B","duration":"2d"},
                    {"id":"A_B","name":"A_B","start":"after A B","duration":"2d"},
                    {"name":"Generated task","start":"after A-B A_B","duration":"1d"}
                ]}]}
            }),
        ),
        (
            "sequence",
            ok_render!(render_sequence, RenderSequenceParams, "ui://mermaid/diagram", {
                "data":{"participants":["A-B","A B","A_B"],"messages":[{"from":"A-B","to":"A B","text":"First"},{"from":"A B","to":"A_B","text":"Second"}]}
            }),
        ),
        (
            "mindmap",
            ok_render!(render_mindmap, RenderMindmapParams, "ui://mermaid/diagram", {
                "data":{"root":{"text":"Synthetic observations","children":[{"text":"Δ東京"},{"text":"Review"}]}}
            }),
        ),
        (
            "timeline",
            ok_render!(render_timeline, RenderTimelineParams, "ui://mermaid/diagram", {
                "data":{"periods":[{"period":"2025","events":["Observations"]},{"period":"2026","events":["Review"]}]}
            }),
        ),
        (
            "er",
            ok_render!(render_er_diagram, RenderErParams, "ui://mermaid/diagram", {
                "data":{"entities":[{"name":"A-B","attributes":[{"name":"sample_id","type":"string","key":"PK"}]},{"name":"A B"},{"name":"A_B"}],"relationships":[{"from":"A-B","to":"A B"},{"from":"A B","to":"A_B"}]}
            }),
        ),
        (
            "state",
            ok_render!(render_state_diagram, RenderStateParams, "ui://mermaid/diagram", {
                "data":{"transitions":[{"from":"[*]","to":"A-B"},{"from":"A-B","to":"A B"},{"from":"A B","to":"A_B"},{"from":"A_B","to":"[*]"}]}
            }),
        ),
        (
            "class",
            ok_render!(render_class_diagram, RenderClassParams, "ui://mermaid/diagram", {
                "data":{"classes":[{"name":"A-B","attributes":["+String sample_id"]},{"name":"A B"},{"name":"A_B"}],"relationships":[{"from":"A-B","to":"A B"},{"from":"A B","to":"A_B"}]}
            }),
        ),
    ]
}

#[tokio::test]
async fn typed_mermaid_wrappers_preserve_distinct_definitions_references_and_labels() {
    for (name, result) in typed_mermaid_fixture_results().await {
        let source = diagram_source(&result);
        if ["raw", "mindmap", "timeline"].contains(&name) {
            continue;
        }
        for (raw, expected) in [
            ("A-B", "br_412d42"),
            ("A B", "br_412042"),
            ("A_B", "br_415f42"),
        ] {
            assert!(
                source.contains(expected),
                "{name}: missing identity {expected}: {source}"
            );
            assert!(source.contains(raw), "{name}: lost visible label {raw}");
        }
        match name {
            "gantt" => {
                assert!(source.contains("after br_412042"));
                assert!(source.contains("after br_412d42 br_415f42"));
                assert!(source.contains("br_auto_3"));
            }
            "er" => assert!(source.contains("string sample_id PK")),
            "state" => assert!(source.contains("[*] --> br_412d42")),
            _ => {}
        }
    }
}

#[tokio::test]
async fn typed_mermaid_gantt_rejects_unknown_or_ambiguous_dependencies() {
    for tasks in [
        serde_json::json!([{"name":"A","id":"known","start":"after missing","duration":"1d"}]),
        serde_json::json!([{"name":"A","id":"same","start":"2026-01-01","duration":"1d"},{"name":"B","id":"same","start":"after same","duration":"1d"}]),
    ] {
        let params: RenderGanttParams = serde_json::from_value(
            serde_json::json!({"data":{"sections":[{"name":"Synthetic","tasks":tasks}]}}),
        )
        .unwrap();
        assert!(AutoVisualiserRouter::new()
            .render_gantt(Parameters(params))
            .await
            .is_err());
    }
}

#[tokio::test]
async fn typed_mermaid_exports_actual_wrapper_artifacts_when_requested() {
    let Some(directory) = std::env::var_os("BIOROUTER_MERMAID_FIXTURE_DIR") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    assert!(
        directory.is_absolute() && directory.is_dir(),
        "Create an explicit temporary fixture directory first"
    );
    for (name, result) in typed_mermaid_fixture_results().await {
        std::fs::write(directory.join(format!("{name}.html")), decode_html(&result)).unwrap();
    }
}
