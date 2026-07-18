# Extension trait design

> **What this is.** The original design sketch for a BioRouter extension framework built
> around a hand-written `Extension` trait, a `ToolRegistry`, and a `#[tool]` proc macro.
> **Status:** Superseded — this architecture was never shipped. Extensions are now MCP
> (Model Context Protocol) servers built on the `rmcp` Rust SDK with `#[tool_router]`;
> the current truth lives in [the extensions and skills guide](../../extensions/extensions-and-skills-guide.md)
> and [the extension manager reference](../../extensions/built-in/extension-manager.md).
> **Audience:** developers working on extensions, and anyone tracing why the extension
> API looks the way it does.

BioRouter extensions let an AI agent operate an external component through a
tool-based interface. This document captures how that was originally meant to work:
each extension would implement a Rust `Extension` trait directly, declare its tools with
a proc macro from a `biorouter_macros` crate, and return `AgentResult<Value>` from every
tool. None of that exists in the codebase today — there is no `pub trait Extension`, no
`biorouter_macros` crate in the nine-crate workspace, and no `AgentResult` or
`ToolResult<Value>` type. The extension surface was rebuilt on MCP instead, so tools are
declared with `rmcp`'s `#[tool]` / `#[tool_router]` attributes and errors travel as
`rmcp` `ErrorData`. See `crates/biorouter-mcp/src/autovisualiser/mod.rs` for a
representative built-in server.

Read this only as a record of the intended shape and of the conventions — naming, error
propagation, testing levels — that partly survived the rewrite. Do not treat any type
name, import, or signature below as an API you can call.

> **Warning.** Every code sample in this document is preserved exactly as originally
> written and is illustrative only. The `read_file` example does not compile: the
> `map_err` closure has unbalanced parentheses and braces. It has been left unfixed so
> the historical record stays faithful.

## Core concepts

### Extension

An Extension represents any component that can be operated by an AI agent. Extensions
expose their capabilities through Tools and maintain their own state. The core interface
was to be defined by the `Extension` trait:

```rust
#[async_trait]
pub trait Extension: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn instructions(&self) -> &str;
    fn tools(&self) -> &[Tool];
    async fn status(&self) -> AnyhowResult<HashMap<String, Value>>;
    async fn call_tool(&self, tool_name: &str, parameters: HashMap<String, Value>) -> ToolResult<Value>;
}
```

### Tools

Tools are the primary way Extensions expose functionality to agents. Each tool has:

- A name
- A description
- A set of parameters
- An implementation that executes the tool's functionality

A tool must take a Value and return an `AgentResult<Value>` (it must also be async). This
is what makes it compatible with the tool calling framework from the agent.

```rust
async fn echo(&self, params: Value) -> AgentResult<Value>
```

## Architecture

### Component overview

- **Extension trait** — the core interface that all extensions must implement.
- **Error handling** — specialized error types for tool execution.
- **Proc macros** — simplify tool definition and registration. Marked *not yet
  implemented* in the original document, and never implemented: no macros crate was
  ever added to the workspace, and `rmcp`'s own macros took over this job.

### Error handling

The system uses two main error types:

- `ErrorData`: Specific errors related to tool execution
- `anyhow::Error`: General purpose errors for extension status and other operations

This split allows precise error handling for tool execution while maintaining flexibility
for general extension operations.

> **Note.** This document mixes two error vocabularies. `ErrorData` is real — it is
> `rmcp`'s error type and is what extensions return today. `AgentResult<Value>` and
> `ToolResult<Value>`, used in the trait and tool signatures above, were never defined
> anywhere in the workspace. For the error model actually in force, read
> [the agent error model](../../architecture/agent-error-model.md).

## Best practices

### Tool design

- **Clear names**: Use clear, action-oriented names for tools (e.g., "create_user" not "user")
- **Descriptive parameters**: Each parameter should have a clear description
- **Error handling**: Return specific errors when possible, the errors become "prompts"
- **State management**: Be explicit about state modifications

### Extension implementation

- **State encapsulation**: Keep extension state private and controlled
- **Error propagation**: Use `?` operator with `ErrorData` for tool execution
- **Status clarity**: Provide clear, structured status information
- **Documentation**: Document all tools and their effects

### Example implementation

A complete example of a simple extension, as originally drafted. The
`biorouter_macros` crate it imports does not exist:

```rust
use biorouter_macros::tool;

struct FileSystem {
    registry: ToolRegistry,
    root_path: PathBuf,
}

impl FileSystem {
    #[tool(
        name = "read_file",
        description = "Read contents of a file"
    )]
    async fn read_file(&self, path: String) -> ToolResult<Value> {
        let full_path = self.root_path.join(path);
        let content = tokio::fs::read_to_string(full_path)
            .await
            .map_err(|e| ErrorData {
                code: ErrorCode::INTERNAL_ERROR,
                message: Cow::from(e.to_string(),
                data: None,
            }))?;
            
        Ok(json!({ "content": content }))
    }
}

#[async_trait]
impl Extension for FileSystem {
    // ... implement trait methods ...
}
```

## Testing conventions

Extensions should be tested at multiple levels:

- Unit tests for individual tools
- Integration tests for extension behavior
- Property tests for tool invariants

Example test:

```rust
#[tokio::test]
async fn test_echo_tool() {
    let extension = TestExtension::new();
    let result = extension.call_tool(
        "echo",
        hashmap!{ "message" => json!("hello") }
    ).await;
    
    assert_eq!(result.unwrap(), json!({ "response": "hello" }));
}
```

## Related documentation

- [Extensions and skills guide](../../extensions/extensions-and-skills-guide.md) — how extensions are actually authored, installed, and configured today.
- [Extension manager](../../extensions/built-in/extension-manager.md) — the component that owns MCP extension lifecycle and tool registration, the role this design assigned to the `Extension` trait.
- [Agent error model](../../architecture/agent-error-model.md) — the error types that replaced the `AgentResult` / `ToolResult` vocabulary sketched here.
- [System overview](../../architecture/system-overview.md) — where extensions sit in the Interface → Agent → Extensions architecture.
- [Auto Visualiser extension](../../extensions/built-in/auto-visualiser.md) — a large real built-in MCP server, useful as the concrete counterexample to the design above.
