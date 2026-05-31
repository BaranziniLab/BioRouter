#[test]
fn knowledge_is_in_builtin_registry() {
    assert!(biorouter_mcp::BUILTIN_EXTENSIONS.contains_key("knowledge"));
}
