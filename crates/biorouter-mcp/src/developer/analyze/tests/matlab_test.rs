use crate::developer::analyze::graph::CallGraph;
use crate::developer::analyze::parser::{ElementExtractor, ParserManager};
use crate::developer::analyze::types::AnalysisResult;
use std::collections::HashSet;
use std::path::PathBuf;

fn parse_and_extract(code: &str) -> AnalysisResult {
    let manager = ParserManager::new();
    let tree = manager.parse(code, "matlab").unwrap();
    ElementExtractor::extract_with_depth(&tree, code, "matlab", "semantic", None).unwrap()
}

const SAMPLE: &str = r#"
function result = process(data)
    result = clean(data);
end

function out = clean(x)
    out = x * 2;
end

process(input);
"#;

const CLASSDEF: &str = r#"
classdef Calculator
    properties
        value
    end
    methods
        function obj = compute(self)
            obj = 1;
        end
    end
end
"#;

#[test]
fn test_matlab_functions() {
    let result = parse_and_extract(SAMPLE);
    let funcs: HashSet<_> = result.functions.iter().map(|f| f.name.as_str()).collect();
    assert!(funcs.contains("process"), "expected process, got {funcs:?}");
    assert!(funcs.contains("clean"), "expected clean, got {funcs:?}");
}

#[test]
fn test_matlab_calls_and_graph() {
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

    let graph = CallGraph::build_from_results(&[(PathBuf::from("t.m"), result)]);
    let incoming = graph.find_incoming_chains("clean", 2);
    assert!(!incoming.is_empty(), "expected process to call clean");
}

#[test]
fn test_matlab_classdef() {
    let result = parse_and_extract(CLASSDEF);
    let classes: HashSet<_> = result.classes.iter().map(|c| c.name.as_str()).collect();
    assert!(
        classes.contains("Calculator"),
        "expected Calculator class, got {classes:?}"
    );
    let funcs: HashSet<_> = result.functions.iter().map(|f| f.name.as_str()).collect();
    assert!(
        funcs.contains("compute"),
        "expected method compute, got {funcs:?}"
    );
}
