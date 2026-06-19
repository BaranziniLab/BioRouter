/// Tree-sitter query for extracting Kotlin code elements
///
/// Targets the `tree-sitter-kotlin-ng` grammar, whose node kinds differ from the
/// older `tree-sitter-kotlin`: identifiers are `identifier` (not `simple_identifier`
/// / `type_identifier`) and imports are `import` (not `import_header`).
pub const ELEMENT_QUERY: &str = r#"
    ; Functions
    (function_declaration name: (identifier) @func)

    ; Classes / interfaces
    (class_declaration name: (identifier) @class)

    ; Objects (singleton classes)
    (object_declaration name: (identifier) @class)

    ; Imports
    (import) @import
"#;

/// Tree-sitter query for extracting Kotlin function calls
pub const CALL_QUERY: &str = r#"
    ; Simple function calls: g()
    (call_expression
      (identifier) @function.call)

    ; Method calls with navigation: obj.method()
    (call_expression
      (navigation_expression
        (identifier)
        (identifier) @method.call))
"#;
