# Code Execution capability

> **What this is.** User guide to the built-in Code Execution capability, which backs Code Mode: instead of calling MCP tools one at a time, the model writes a short JavaScript program that batches many tool calls into a single execution.
> **Status:** Current. The capability is enabled by default, so no manual setup is normally needed.
> **Audience:** end users.

In Code Mode the model discovers which tools the enabled capabilities and loaded extensions expose, then writes JavaScript that BioRouter runs in one execution. Because intermediate results stay inside that script rather than round-tripping through the conversation, Code Mode is more context-efficient for a genuine multi-tool workflow.

> **Note.** This capability is **enabled by default**. Its internal registration still uses the legacy `PlatformExtensionDef` type; that storage name does not make it an installed extension. The configuration walkthrough below is only needed if you previously disabled it, or want to confirm its state.

## Configuration

1. Run the `configure` command:

   ```bash
   biorouter configure
   ```

2. Choose `Toggle Extensions`, then confirm `code_execution` is enabled:

   ```text
   ┌   biorouter-configure
   │
   ◇  What would you like to configure?
   │  Toggle Extensions
   │
   ◆  Enable capabilities and extensions: (use "space" to toggle and "enter" to submit)
   │  ● code_execution
   └  Extension settings updated successfully
   ```

## Available tools

| Tool | Description |
|------|-------------|
| `execute_code` | Run one JavaScript program that batches multiple MCP tool calls into a single execution. This is the capability's primary tool. |
| `search_modules` | Find a tool when the model does not know which module provides it. The result contains ready-to-use imports and signatures. |
| `read_module` | Read the tool definitions for a module the model already knows — `"serverName"` lists every tool with its signature, `"serverName/toolName"` gives full detail for one tool. |

## What Code Mode code looks like

Tools become importable functions grouped by capability or extension module. All calls are synchronous and return strings, and `record_result(value)` is how a script returns a value to the conversation.

```javascript
import { text_editor } from "developer";
const content = text_editor({ path: "/path/to/source.md", command: "view" });
text_editor({ path: "/path/to/dest.md", command: "write", file_text: content });
record_result({ copied: true });
```

Several operations chain in one call rather than becoming three separate tool round-trips:

```javascript
import { shell, text_editor } from "developer";
const files = shell({ command: "ls -la" });
const readme = text_editor({ path: "./README.md", command: "view" });
const status = shell({ command: "git status" });
record_result({ files, readme, status });
```

The syntax rules are:

- Import with `import { tool1, tool2 } from "serverName";`
- Call with `toolName({ param1: value, param2: value })`
- Return with `record_result(value)`
- Use `` String.raw`...` `` when passing multiline shell, Ruby or PowerShell source as an argument, so backslash sequences such as `\n` survive intact rather than becoming real newlines in the JavaScript source.

`execute_code` also takes a `tool_graph` argument describing the execution flow for the UI. Each node names a `tool` (as `server/name`), a `description`, and the indices of the nodes it `depends_on`:

```json
[
  {"tool": "developer/shell", "description": "list files", "depends_on": []},
  {"tool": "developer/text_editor", "description": "read README.md", "depends_on": []},
  {"tool": "developer/text_editor", "description": "write output.txt", "depends_on": [0, 1]}
]
```

> **Warning.** `execute_code` is annotated as destructive and non-idempotent, and it can reach every effective tool exposed by enabled capabilities and loaded extensions — including `developer`'s `shell` and `text_editor`. It inherits the same blast radius as those tools, so the permission controls in the [Developer capability guide](developer.md) and [permission modes](../../security/permission-modes.md) apply to it too.

## Example usage

In this example, BioRouter compiles a report that would otherwise take several separate tool calls.

### BioRouter prompt

```text
Create a LOG.md file with the current git branch, last 3 commits, and the version from package.json
```

### BioRouter output

> **Note.** In the transcript below the executed script is elided as `...`; see [What Code Mode code looks like](#what-code-mode-code-looks-like) above for the real syntax.

I'll help you create a LOG.md file with the git branch, last 3 commits, and version from package.json. Let me gather all this information in one operation.

`Execute Code code: import { shell, text_editor } from "developer" ...`

Let me check the package.json path first:

`Execute Code code: import { shell, text_editor } from "developer" ...`

Perfect! I've created the **LOG.md** file with:

- ✅ **Current git branch**
- ✅ **Last 3 commits** (with hash, message, author, and relative time)
- ✅ **Version** from ui/desktop/package.json

The file has been saved to the root directory as `LOG.md`.

## Related documentation

- [Developer capability](developer.md) — the `shell` and `text_editor` tools most Code Mode scripts import, and the access controls that constrain them.
- [Extension Manager capability](extension-manager.md) — the other lever for keeping the active tool count and context usage down.
- [Context engineering](../../agent-loop/context-engineering.md) — the broader picture of how BioRouter manages its context window.
- [Permission modes](../../security/permission-modes.md) — how to require approval before a script runs shell commands or edits files.
