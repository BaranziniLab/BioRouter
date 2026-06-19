// Verifies the analyzer still works after swapping devgen-tree-sitter-swift ->
// tree-sitter-swift (alex-pinkus grammar) under the tree-sitter 0.26 bump.
use crate::developer::analyze::graph::CallGraph;
use crate::developer::analyze::parser::{ElementExtractor, ParserManager};
use crate::developer::analyze::types::AnalysisResult;
use std::collections::HashSet;
use std::path::PathBuf;

fn parse_and_extract(code: &str) -> AnalysisResult {
    let manager = ParserManager::new();
    let tree = manager.parse(code, "swift").unwrap();
    ElementExtractor::extract_with_depth(&tree, code, "swift", "semantic", None).unwrap()
}

const SAMPLE: &str = r#"
import Foundation

class Service {
    func start() {
        helper()
        client.fetch()
    }
    func helper() {}
}

protocol Runnable {}

func main() {
    let s = Service()
    s.start()
}
"#;

#[test]
fn test_swift_functions_and_classes() {
    let result = parse_and_extract(SAMPLE);
    let funcs: HashSet<_> = result.functions.iter().map(|f| f.name.as_str()).collect();
    assert!(funcs.contains("start"), "got {funcs:?}");
    assert!(funcs.contains("helper"), "got {funcs:?}");
    assert!(funcs.contains("main"), "got {funcs:?}");

    let classes: HashSet<_> = result.classes.iter().map(|c| c.name.as_str()).collect();
    assert!(classes.contains("Service"), "got {classes:?}");
    assert!(
        classes.contains("Runnable"),
        "expected protocol, got {classes:?}"
    );
}

#[test]
fn test_swift_calls_and_graph() {
    let result = parse_and_extract(SAMPLE);
    let callees: HashSet<_> = result
        .calls
        .iter()
        .map(|c| c.callee_name.as_str())
        .collect();
    assert!(callees.contains("helper"), "got {callees:?}");
    assert!(
        callees.contains("fetch"),
        "expected method call, got {callees:?}"
    );

    let graph = CallGraph::build_from_results(&[(PathBuf::from("t.swift"), result)]);
    let incoming = graph.find_incoming_chains("start", 2);
    assert!(!incoming.is_empty(), "expected main to call start");
}
