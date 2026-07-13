//! Post-edit syntax diagnostics (BR-47).
//!
//! Tree-sitter parses every file the analyzer touches, but the analyzer only
//! ever reads the *shape* it recovers — it never looks at the ERROR / MISSING
//! nodes the parse leaves behind when the source does not conform to the grammar.
//! Those nodes are exactly a cheap, offline, zero-dependency syntax check: the
//! "diagnostics" capability BR-47 wires into the edit path. This module turns a
//! parsed tree into a list of [`Diagnostic`]s.
//!
//! Scope is deliberately narrow. Only languages whose tree-sitter grammar matches
//! the file's real language closely enough to *trust* its error nodes are
//! diagnosed ([`is_diagnosable_language`]); `typescript`, which the analyzer
//! parses with the *javascript* grammar (so every type annotation becomes a
//! spurious ERROR), is excluded. This is a syntax check, not a type check or a
//! linter — a clean result means "it parses", not "it is correct".

use tree_sitter::{Node, Tree};

/// A single tree-sitter syntax diagnostic (an ERROR or MISSING node).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// 1-based line of the node's start.
    pub line: usize,
    /// 1-based column of the node's start.
    pub column: usize,
    /// A short human-readable description.
    pub message: String,
}

impl Diagnostic {
    /// `line L:C: message`, the form fed back to the model.
    pub fn render(&self) -> String {
        format!("line {}:{}: {}", self.line, self.column, self.message)
    }
}

/// Languages whose tree-sitter grammar is a faithful match for the file's actual
/// language, so ERROR / MISSING nodes are trustworthy syntax errors rather than
/// grammar-mismatch artifacts.
///
/// Notably excludes `typescript` (parsed with the JavaScript grammar by the
/// analyzer's [`super::parser::ParserManager`], which flags valid type syntax as
/// an error). The set is the parser's supported languages minus that one.
const DIAGNOSABLE_LANGUAGES: &[&str] = &[
    "python",
    "rust",
    "javascript",
    "go",
    "java",
    "kotlin",
    "swift",
    "ruby",
    "cpp",
    "c",
    "r",
    "julia",
    "matlab",
];

/// Cap on how many diagnostics one file reports — one broken edit can produce a
/// cascade of ERROR nodes, and the first few are enough to act on.
pub const MAX_DIAGNOSTICS_PER_FILE: usize = 10;

/// Files larger than this are skipped: a generated/vendored blob is not worth a
/// full parse on the edit hot path, and a human is unlikely to be hand-editing it.
pub const MAX_DIAGNOSE_BYTES: usize = 512 * 1024;

/// Whether a syntax check on `language` is trustworthy enough to surface.
pub fn is_diagnosable_language(language: &str) -> bool {
    DIAGNOSABLE_LANGUAGES.contains(&language)
}

/// Collect syntax diagnostics from a parsed tree, up to `limit`.
///
/// Walks down through clean nodes and stops at the first ERROR or MISSING node on
/// each path, recording it and *not* descending further (an error node's children
/// are the fragments of the same mistake, not separate errors). Pure over
/// `(tree, source)`, so it is unit-testable with a real parser and no filesystem.
pub fn collect_syntax_errors(tree: &Tree, source: &str, limit: usize) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    collect(tree.root_node(), source, limit, &mut out);
    out
}

fn collect(node: Node, source: &str, limit: usize, out: &mut Vec<Diagnostic>) {
    if out.len() >= limit {
        return;
    }
    // A MISSING node is zero-width; tree-sitter inserted it during error recovery,
    // so it is the single most precise "you forgot an X here" signal.
    if node.is_missing() {
        let start = node.start_position();
        out.push(Diagnostic {
            line: start.row + 1,
            column: start.column + 1,
            message: format!("missing `{}`", node.kind()),
        });
        return;
    }
    if node.is_error() {
        let start = node.start_position();
        let snippet = source
            .get(node.byte_range())
            .map(snippet_of)
            .unwrap_or_default();
        let message = if snippet.is_empty() {
            "syntax error".to_string()
        } else {
            format!("syntax error near `{snippet}`")
        };
        out.push(Diagnostic {
            line: start.row + 1,
            column: start.column + 1,
            message,
        });
        return;
    }
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i as u32) {
            collect(child, source, limit, out);
        }
        if out.len() >= limit {
            break;
        }
    }
}

/// First line of `text`, trimmed and truncated to a readable length.
fn snippet_of(text: &str) -> String {
    let first_line = text.lines().next().unwrap_or("").trim();
    const MAX: usize = 48;
    if first_line.chars().count() > MAX {
        let truncated: String = first_line.chars().take(MAX).collect();
        format!("{truncated}…")
    } else {
        first_line.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::developer::analyze::parser::ParserManager;

    fn diagnose(source: &str, language: &str) -> Vec<Diagnostic> {
        let pm = ParserManager::new();
        let tree = pm.parse(source, language).expect("parse");
        collect_syntax_errors(&tree, source, MAX_DIAGNOSTICS_PER_FILE)
    }

    #[test]
    fn clean_rust_has_no_diagnostics() {
        let src = "fn main() {\n    let x = 1;\n    println!(\"{}\", x);\n}\n";
        assert!(diagnose(src, "rust").is_empty());
    }

    #[test]
    fn unbalanced_brace_is_flagged() {
        // Missing closing brace on `main`.
        let src = "fn main() {\n    let x = 1;\n";
        let diags = diagnose(src, "rust");
        assert!(
            !diags.is_empty(),
            "expected a diagnostic for the unbalanced brace"
        );
    }

    #[test]
    fn clean_python_has_no_diagnostics() {
        let src = "def add(a, b):\n    return a + b\n";
        assert!(diagnose(src, "python").is_empty());
    }

    #[test]
    fn broken_python_is_flagged() {
        // `def` with no body / colon is a parse error.
        let src = "def add(a, b\n    return a + b\n";
        let diags = diagnose(src, "python");
        assert!(
            !diags.is_empty(),
            "expected a diagnostic for the broken def"
        );
        // Positions are 1-based.
        assert!(diags.iter().all(|d| d.line >= 1 && d.column >= 1));
    }

    #[test]
    fn diagnostics_are_capped() {
        // Many stray closing braces produce many error nodes; the cap holds.
        let src = format!("fn main() {{}}\n{}", "}\n".repeat(40));
        let diags = diagnose(&src, "rust");
        assert!(diags.len() <= MAX_DIAGNOSTICS_PER_FILE);
    }

    #[test]
    fn render_is_line_col_prefixed() {
        let d = Diagnostic {
            line: 3,
            column: 5,
            message: "missing `}`".to_string(),
        };
        assert_eq!(d.render(), "line 3:5: missing `}`");
    }

    #[test]
    fn typescript_is_not_diagnosable() {
        // Guards the JS-grammar mismatch: TS must never be syntax-checked here.
        assert!(!is_diagnosable_language("typescript"));
        assert!(is_diagnosable_language("rust"));
        assert!(is_diagnosable_language("python"));
        assert!(!is_diagnosable_language(""));
        assert!(!is_diagnosable_language("markdown"));
    }
}
