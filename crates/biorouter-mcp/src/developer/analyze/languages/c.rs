/// Tree-sitter query for extracting C code elements
///
/// Targets the `tree-sitter-c` grammar. Like C++, the function name is nested
/// inside a `function_declarator`, so caller attribution uses
/// [`extract_function_name_for_kind`].
pub const ELEMENT_QUERY: &str = r#"
    ; Function definitions: int add(int a, int b) {}
    (function_definition
      declarator: (function_declarator
        declarator: (identifier) @func))

    ; Structs / unions / enums
    (struct_specifier name: (type_identifier) @struct)
    (union_specifier name: (type_identifier) @struct)
    (enum_specifier name: (type_identifier) @struct)

    ; Includes
    (preproc_include) @import
"#;

/// Tree-sitter query for extracting C function calls
pub const CALL_QUERY: &str = r#"
    ; Function calls: add(1, 2)
    (call_expression
      function: (identifier) @function.call)

    ; Member calls via function pointer fields: obj.fn()
    (call_expression
      function: (field_expression
        field: (field_identifier) @method.call))
"#;

/// Recursively descend through declarator wrappers (pointer/function) to find
/// the function name identifier.
fn declarator_name(node: &tree_sitter::Node, source: &str) -> Option<String> {
    match node.kind() {
        "identifier" | "field_identifier" | "type_identifier" => {
            source.get(node.byte_range()).map(|s| s.to_string())
        }
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
