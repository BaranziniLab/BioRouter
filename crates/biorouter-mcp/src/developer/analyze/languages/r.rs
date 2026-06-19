/// Tree-sitter query for extracting R code elements
///
/// Targets the `tree-sitter-r` grammar. R functions are anonymous
/// `function_definition` values bound by assignment, so the name is the left
/// side of the enclosing `binary_operator` (`f <- function(...)` or
/// `g = function(...)`). Caller attribution recovers it via
/// [`extract_function_name_for_kind`].
pub const ELEMENT_QUERY: &str = r#"
    ; Named function definitions: f <- function(...) / g = function(...)
    (binary_operator
      lhs: (identifier) @func
      rhs: (function_definition))
"#;

/// Tree-sitter query for extracting R function calls
pub const CALL_QUERY: &str = r#"
    ; Plain calls: g(x), library(dplyr)
    (call
      function: (identifier) @function.call)

    ; Member calls via $ : obj$method()
    (call
      function: (extract_operator
        rhs: (identifier) @method.call))
"#;

/// Extract the function name from an R `function_definition` node by reading the
/// identifier on the left of the enclosing assignment.
pub fn extract_function_name_for_kind(
    node: &tree_sitter::Node,
    source: &str,
    _kind: &str,
) -> Option<String> {
    let parent = node.parent()?;
    if parent.kind() == "binary_operator" {
        if let Some(lhs) = parent.child_by_field_name("lhs") {
            if lhs.kind() == "identifier" {
                return source.get(lhs.byte_range()).map(|s| s.to_string());
            }
        }
    }
    None
}
