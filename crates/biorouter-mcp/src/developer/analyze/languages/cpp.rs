/// Tree-sitter query for extracting C++ code elements
///
/// Targets the `tree-sitter-cpp` grammar. Function names live nested inside a
/// `function_declarator` rather than as a direct child of `function_definition`,
/// so caller attribution uses [`extract_function_name_for_kind`].
pub const ELEMENT_QUERY: &str = r#"
    ; Free functions and out-of-line method definitions: void Foo::m() {}
    (function_definition
      declarator: (function_declarator
        declarator: (identifier) @func))
    (function_definition
      declarator: (function_declarator
        declarator: (field_identifier) @func))
    (function_definition
      declarator: (function_declarator
        declarator: (qualified_identifier
          name: (identifier) @func)))

    ; Method declarations inside a class/struct body
    (field_declaration
      declarator: (function_declarator
        declarator: (field_identifier) @func))

    ; Classes and structs
    (class_specifier name: (type_identifier) @class)
    (struct_specifier name: (type_identifier) @struct)

    ; Includes
    (preproc_include) @import
"#;

/// Tree-sitter query for extracting C++ function calls
pub const CALL_QUERY: &str = r#"
    ; Free function calls: g()
    (call_expression
      function: (identifier) @function.call)

    ; Method calls: obj.h()
    (call_expression
      function: (field_expression
        field: (field_identifier) @method.call))

    ; Scoped calls: ns::k()
    (call_expression
      function: (qualified_identifier
        name: (identifier) @scoped.call))
"#;

/// Recursively descend through declarator wrappers (pointer/reference/function)
/// to find the actual function name identifier.
fn declarator_name(node: &tree_sitter::Node, source: &str) -> Option<String> {
    match node.kind() {
        "identifier" | "field_identifier" | "type_identifier" | "destructor_name"
        | "operator_name" => source.get(node.byte_range()).map(|s| s.to_string()),
        "qualified_identifier" => node
            .child_by_field_name("name")
            .and_then(|n| declarator_name(&n, source)),
        _ => {
            if let Some(inner) = node.child_by_field_name("declarator") {
                return declarator_name(&inner, source);
            }
            (0..node.child_count())
                .filter_map(|i| node.child(i as u32))
                .find_map(|c| declarator_name(&c, source))
        }
    }
}

/// Extract the function name from a `function_definition` node.
pub fn extract_function_name_for_kind(
    node: &tree_sitter::Node,
    source: &str,
    _kind: &str,
) -> Option<String> {
    let declarator = node.child_by_field_name("declarator")?;
    declarator_name(&declarator, source)
}
