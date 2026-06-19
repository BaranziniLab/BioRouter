// Cross-cutting guarantees for the multi-language analyzer after the
// tree-sitter 0.26 bump: every supported language must (a) be registered with a
// non-empty element query, (b) construct a real tree-sitter parser, and (c) be
// reachable from a file extension.
use crate::developer::analyze::languages::get_language_info;
use crate::developer::analyze::parser::ParserManager;
use crate::developer::lang::get_language_identifier;
use std::path::Path;

const ALL_LANGUAGES: &[&str] = &[
    "python",
    "rust",
    "javascript",
    "typescript",
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

#[test]
fn test_every_language_has_queries_and_a_working_parser() {
    let manager = ParserManager::new();
    for lang in ALL_LANGUAGES {
        let info = get_language_info(lang)
            .unwrap_or_else(|| panic!("no LanguageInfo registered for {lang}"));
        assert!(
            !info.element_query.is_empty(),
            "{lang} has an empty element query"
        );
        assert!(
            !info.call_query.is_empty(),
            "{lang} has an empty call query"
        );
        // Parser construction must succeed (validates the grammar links + ABI).
        manager
            .get_or_create_parser(lang)
            .unwrap_or_else(|e| panic!("failed to build parser for {lang}: {e:?}"));
    }
}

#[test]
fn test_new_file_extensions_map_to_languages() {
    let cases = [
        ("foo.cpp", "cpp"),
        ("foo.cc", "cpp"),
        ("foo.hpp", "cpp"),
        ("foo.hh", "cpp"),
        ("foo.c", "c"),
        ("foo.r", "r"),
        ("foo.R", "r"),
        ("foo.jl", "julia"),
        ("foo.m", "matlab"),
        ("foo.kt", "kotlin"),
        ("foo.swift", "swift"),
    ];
    for (file, expected) in cases {
        assert_eq!(
            get_language_identifier(Path::new(file)),
            expected,
            "extension mapping wrong for {file}"
        );
    }
}

#[test]
fn test_unsupported_language_is_rejected() {
    let manager = ParserManager::new();
    assert!(manager.get_or_create_parser("cobol").is_err());
    assert!(get_language_info("cobol").is_none());
}
