/// Tree-sitter query for extracting MATLAB code elements
///
/// Targets the `tree-sitter-matlab` grammar. The function name is a direct
/// `name:` child of `function_definition`, so the default direct-child name
/// extraction works and no custom handler is required.
pub const ELEMENT_QUERY: &str = r#"
    ; Functions: function y = f(x) ... end
    (function_definition name: (identifier) @func)

    ; Classes: classdef Cls ... end
    (class_definition name: (identifier) @class)
"#;

/// Tree-sitter query for extracting MATLAB function calls
///
/// Note: MATLAB syntax does not distinguish a function call from array
/// indexing, so `a(i)` is also reported as a call by the grammar.
pub const CALL_QUERY: &str = r#"
    (function_call
      name: (identifier) @function.call)
"#;
