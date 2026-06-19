use crate::developer::analyze::graph::CallGraph;
use crate::developer::analyze::parser::{ElementExtractor, ParserManager};
use crate::developer::analyze::types::AnalysisResult;
use std::collections::HashSet;
use std::path::PathBuf;

fn parse_and_extract(code: &str) -> AnalysisResult {
    let manager = ParserManager::new();
    let tree = manager.parse(code, "r").unwrap();
    ElementExtractor::extract_with_depth(&tree, code, "r", "semantic", None).unwrap()
}

const SAMPLE: &str = r#"
library(dplyr)

process <- function(data) {
    clean(data)
    data$summarize()
}

clean = function(x) {
    x
}

process(df)
"#;

#[test]
fn test_r_function_definitions_both_assignment_styles() {
    let result = parse_and_extract(SAMPLE);
    let funcs: HashSet<_> = result.functions.iter().map(|f| f.name.as_str()).collect();
    // `<-` and `=` assignment forms both recognized
    assert!(funcs.contains("process"), "expected process, got {funcs:?}");
    assert!(funcs.contains("clean"), "expected clean, got {funcs:?}");
}

#[test]
fn test_r_calls_including_dollar_member() {
    let result = parse_and_extract(SAMPLE);
    let callees: HashSet<_> = result
        .calls
        .iter()
        .map(|c| c.callee_name.as_str())
        .collect();
    assert!(
        callees.contains("clean"),
        "expected clean call, got {callees:?}"
    );
    assert!(
        callees.contains("library"),
        "expected library call, got {callees:?}"
    );
    assert!(
        callees.contains("summarize"),
        "expected $ member call, got {callees:?}"
    );
}

#[test]
fn test_r_call_graph_attribution() {
    let result = parse_and_extract(SAMPLE);
    let graph = CallGraph::build_from_results(&[(PathBuf::from("t.R"), result)]);
    // process() -> clean()
    let incoming = graph.find_incoming_chains("clean", 2);
    assert!(!incoming.is_empty(), "expected process to call clean");
}
