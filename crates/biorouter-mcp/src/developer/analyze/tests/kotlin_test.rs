// Verifies the analyzer still works after swapping tree-sitter-kotlin ->
// tree-sitter-kotlin-ng, whose node kinds differ (identifier vs simple_identifier,
// import vs import_header).
use crate::developer::analyze::graph::CallGraph;
use crate::developer::analyze::parser::{ElementExtractor, ParserManager};
use crate::developer::analyze::types::AnalysisResult;
use std::collections::HashSet;
use std::path::PathBuf;

fn parse_and_extract(code: &str) -> AnalysisResult {
    let manager = ParserManager::new();
    let tree = manager.parse(code, "kotlin").unwrap();
    ElementExtractor::extract_with_depth(&tree, code, "kotlin", "semantic", None).unwrap()
}

const SAMPLE: &str = r#"
import kotlin.math.sqrt

class Calculator {
    fun compute(x: Int): Int {
        return helper(x)
    }
    fun helper(y: Int): Int {
        return y * 2
    }
}

object Singleton {
    fun run() {}
}

fun main() {
    val c = Calculator()
    c.compute(5)
}
"#;

#[test]
fn test_kotlin_functions_classes_objects() {
    let result = parse_and_extract(SAMPLE);
    let funcs: HashSet<_> = result.functions.iter().map(|f| f.name.as_str()).collect();
    assert!(funcs.contains("compute"), "got {funcs:?}");
    assert!(funcs.contains("helper"), "got {funcs:?}");
    assert!(funcs.contains("run"), "got {funcs:?}");
    assert!(funcs.contains("main"), "got {funcs:?}");

    let classes: HashSet<_> = result.classes.iter().map(|c| c.name.as_str()).collect();
    assert!(classes.contains("Calculator"), "got {classes:?}");
    assert!(classes.contains("Singleton"), "got {classes:?}");

    assert!(!result.imports.is_empty(), "expected import captured");
}

#[test]
fn test_kotlin_calls_and_graph() {
    let result = parse_and_extract(SAMPLE);
    let callees: HashSet<_> = result
        .calls
        .iter()
        .map(|c| c.callee_name.as_str())
        .collect();
    assert!(callees.contains("helper"), "got {callees:?}");
    assert!(
        callees.contains("compute"),
        "expected navigation call, got {callees:?}"
    );

    let graph = CallGraph::build_from_results(&[(PathBuf::from("t.kt"), result)]);
    let incoming = graph.find_incoming_chains("helper", 2);
    assert!(!incoming.is_empty(), "expected compute to call helper");
}
