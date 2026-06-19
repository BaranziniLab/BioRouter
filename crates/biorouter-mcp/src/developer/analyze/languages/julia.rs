/// Tree-sitter query for extracting Julia code elements
///
/// Targets the `tree-sitter-julia` grammar. Long-form function names are nested
/// in `function_definition -> signature -> call_expression -> identifier`;
/// short-form definitions (`h(x) = ...`) are assignments whose first child is a
/// call. Caller attribution uses [`extract_function_name_for_kind`].
pub const ELEMENT_QUERY: &str = r#"
    ; Long-form: function f(x) ... end
    (function_definition
      (signature
        (call_expression
          (identifier) @func)))

    ; Short-form: h(x) = x + 1  (call must be the FIRST child = the lhs)
    (assignment
      .
      (call_expression
        (identifier) @func))

    ; Macros: macro m(x) ... end
    (macro_definition
      (signature
        (call_expression
          (identifier) @func)))

    ; Types
    (struct_definition (type_head (identifier) @struct))
    (abstract_definition (type_head (identifier) @struct))
    (module_definition name: (identifier) @class)

    ; Imports
    (using_statement (identifier) @import)
    (import_statement (identifier) @import)
"#;

/// Tree-sitter query for extracting Julia function calls
pub const CALL_QUERY: &str = r#"
    (call_expression
      (identifier) @function.call)
"#;

/// Find the first descendant-or-self child of the given kind.
fn child_of_kind<'a>(node: &tree_sitter::Node<'a>, kind: &str) -> Option<tree_sitter::Node<'a>> {
    (0..node.child_count())
        .filter_map(|i| node.child(i as u32))
        .find(|c| c.kind() == kind)
}

/// Extract the function name from a Julia `function_definition` node.
pub fn extract_function_name_for_kind(
    node: &tree_sitter::Node,
    source: &str,
    _kind: &str,
) -> Option<String> {
    let signature = child_of_kind(node, "signature")?;
    let call = child_of_kind(&signature, "call_expression")?;
    let ident = child_of_kind(&call, "identifier")?;
    source.get(ident.byte_range()).map(|s| s.to_string())
}
