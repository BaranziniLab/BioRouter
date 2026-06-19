use crate::developer::analyze::graph::CallGraph;
use crate::developer::analyze::parser::{ElementExtractor, ParserManager};
use crate::developer::analyze::types::AnalysisResult;
use std::collections::HashSet;
use std::path::PathBuf;

fn parse_and_extract(code: &str) -> AnalysisResult {
    let manager = ParserManager::new();
    let tree = manager.parse(code, "julia").unwrap();
    ElementExtractor::extract_with_depth(&tree, code, "julia", "semantic", None).unwrap()
}

const SAMPLE: &str = r#"
using LinearAlgebra

function process(data)
    clean(data)
    transform(data)
end

clean(x) = identity(x)

struct Config
    size::Int
end

process(input)
"#;

#[test]
fn test_julia_long_and_short_function_forms() {
    let result = parse_and_extract(SAMPLE);
    let funcs: HashSet<_> = result.functions.iter().map(|f| f.name.as_str()).collect();
    assert!(funcs.contains("process"), "expected process, got {funcs:?}");
    assert!(
        funcs.contains("clean"),
        "expected short-form clean, got {funcs:?}"
    );
}

#[test]
fn test_julia_structs_and_imports() {
    let result = parse_and_extract(SAMPLE);
    let classes: HashSet<_> = result.classes.iter().map(|c| c.name.as_str()).collect();
    assert!(
        classes.contains("Config"),
        "expected Config, got {classes:?}"
    );
    assert!(
        result.imports.iter().any(|i| i.contains("LinearAlgebra")),
        "expected using import, got {:?}",
        result.imports
    );
}

#[test]
fn test_julia_calls_and_graph() {
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
        callees.contains("transform"),
        "expected transform call, got {callees:?}"
    );

    let graph = CallGraph::build_from_results(&[(PathBuf::from("t.jl"), result)]);
    let incoming = graph.find_incoming_chains("clean", 2);
    assert!(!incoming.is_empty(), "expected process to call clean");
}
