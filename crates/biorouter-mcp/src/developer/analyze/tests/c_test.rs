use crate::developer::analyze::graph::CallGraph;
use crate::developer::analyze::parser::{ElementExtractor, ParserManager};
use crate::developer::analyze::types::AnalysisResult;
use std::collections::HashSet;
use std::path::PathBuf;

fn parse_and_extract(code: &str) -> AnalysisResult {
    let manager = ParserManager::new();
    let tree = manager.parse(code, "c").unwrap();
    ElementExtractor::extract_with_depth(&tree, code, "c", "semantic", None).unwrap()
}

const SAMPLE: &str = r#"
#include <stdio.h>

struct Point { int x; int y; };

static int add(int a, int b) {
    return a + b;
}

int main(void) {
    int s = add(1, 2);
    printf("%d", s);
    return 0;
}
"#;

#[test]
fn test_c_functions_structs_imports() {
    let result = parse_and_extract(SAMPLE);

    let funcs: HashSet<_> = result.functions.iter().map(|f| f.name.as_str()).collect();
    assert!(funcs.contains("add"), "expected add, got {funcs:?}");
    assert!(funcs.contains("main"), "expected main, got {funcs:?}");

    let structs: HashSet<_> = result.classes.iter().map(|c| c.name.as_str()).collect();
    assert!(structs.contains("Point"), "expected Point, got {structs:?}");

    assert!(result.imports.iter().any(|i| i.contains("stdio")));
}

#[test]
fn test_c_calls_and_graph() {
    let result = parse_and_extract(SAMPLE);
    let callees: HashSet<_> = result
        .calls
        .iter()
        .map(|c| c.callee_name.as_str())
        .collect();
    assert!(
        callees.contains("add"),
        "expected add call, got {callees:?}"
    );
    assert!(
        callees.contains("printf"),
        "expected printf call, got {callees:?}"
    );

    let graph = CallGraph::build_from_results(&[(PathBuf::from("t.c"), result)]);
    let incoming = graph.find_incoming_chains("add", 2);
    assert!(!incoming.is_empty(), "expected main to call add");
}
