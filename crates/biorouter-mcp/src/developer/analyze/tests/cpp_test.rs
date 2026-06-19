use crate::developer::analyze::graph::CallGraph;
use crate::developer::analyze::parser::{ElementExtractor, ParserManager};
use crate::developer::analyze::types::AnalysisResult;
use std::collections::HashSet;
use std::path::PathBuf;

fn parse_and_extract(code: &str) -> AnalysisResult {
    let manager = ParserManager::new();
    let tree = manager.parse(code, "cpp").unwrap();
    ElementExtractor::extract_with_depth(&tree, code, "cpp", "semantic", None).unwrap()
}

fn build_graph(code: &str) -> CallGraph {
    let manager = ParserManager::new();
    let tree = manager.parse(code, "cpp").unwrap();
    let result =
        ElementExtractor::extract_with_depth(&tree, code, "cpp", "semantic", None).unwrap();
    CallGraph::build_from_results(&[(PathBuf::from("test.cpp"), result)])
}

const SAMPLE: &str = r#"
#include <vector>

namespace ns {
class Widget {
public:
    void start();
    void stop();
};
}

void ns::Widget::start() {
    helper();
    obj.run();
}

int main() {
    ns::Widget w;
    w.start();
    return 0;
}
"#;

#[test]
fn test_cpp_functions_and_classes() {
    let result = parse_and_extract(SAMPLE);

    let funcs: HashSet<_> = result.functions.iter().map(|f| f.name.as_str()).collect();
    assert!(funcs.contains("start"), "expected start, got {funcs:?}");
    assert!(funcs.contains("stop"), "expected stop, got {funcs:?}");
    assert!(funcs.contains("main"), "expected main, got {funcs:?}");

    let classes: HashSet<_> = result.classes.iter().map(|c| c.name.as_str()).collect();
    assert!(
        classes.contains("Widget"),
        "expected Widget, got {classes:?}"
    );

    assert!(result.imports.iter().any(|i| i.contains("vector")));
    assert_eq!(result.main_line.is_some(), true);
}

#[test]
fn test_cpp_calls() {
    let result = parse_and_extract(SAMPLE);
    let callees: HashSet<_> = result
        .calls
        .iter()
        .map(|c| c.callee_name.as_str())
        .collect();
    assert!(
        callees.contains("helper"),
        "expected helper call, got {callees:?}"
    );
    assert!(
        callees.contains("run"),
        "expected run method call, got {callees:?}"
    );
    assert!(
        callees.contains("start"),
        "expected start call, got {callees:?}"
    );
}

#[test]
fn test_cpp_call_graph() {
    let graph = build_graph(SAMPLE);
    // main() -> w.start()
    let incoming = graph.find_incoming_chains("start", 2);
    assert!(!incoming.is_empty(), "expected callers of start");
    // ns::Widget::start() -> helper(), obj.run()
    let outgoing = graph.find_outgoing_chains("start", 2);
    assert!(!outgoing.is_empty(), "expected callees of start");
}

#[test]
fn test_cpp_structure_mode_counts_without_bodies() {
    let manager = ParserManager::new();
    let tree = manager.parse(SAMPLE, "cpp").unwrap();
    let result =
        ElementExtractor::extract_with_depth(&tree, SAMPLE, "cpp", "structure", None).unwrap();
    // structure mode clears element details
    assert!(result.functions.is_empty());
    assert!(result.classes.is_empty());
}
