# Todo capability

> **What this is.** User guide to the built-in Todo capability, which makes BioRouter break multi-step work into a tracked checklist and report progress as it goes.
> **Status:** Current. The capability is enabled by default, so no manual setup is normally needed.
> **Audience:** end users.

The Todo capability keeps BioRouter organized on long tasks. BioRouter reaches for it automatically when a task has two or more steps, touches multiple files or components, or has uncertain scope. At the start it creates a checklist, updates the checklist as it works, and verifies at the end that every item is done — so you can see where it is rather than waiting for a single opaque answer.

> **Note.** This capability is **enabled by default**. Its internal registration still uses the legacy `PlatformExtensionDef` type; that storage name does not make it an installed extension. The configuration walkthrough below is only needed if you previously disabled it, or want to confirm its state.

## Configuration

1. Run the `configure` command:

   ```bash
   biorouter configure
   ```

2. Choose `Toggle Extensions`, then confirm `todo` is enabled:

   ```text
   ┌   biorouter-configure
   │
   ◇  What would you like to configure?
   │  Toggle Extensions
   │
   ◆  Enable capabilities and extensions: (use "space" to toggle and "enter" to submit)
   │  ● todo
   └  Extension settings updated successfully
   ```

## Available tools

| Tool | Description |
|------|-------------|
| `todo_write` | Replace the entire checklist with a markdown checklist. Used to seed the initial list. |
| `todo_add` | Append one or more pending items without rewriting the existing ones. Each new item gets a fresh `#N` id. |
| `todo_update` | Update a single item by its `#N` id — change its status, its text, or both — without touching the rest of the list. |
| `plan_write` | Set or update the living plan: a step-by-step plan kept current as work proceeds, re-injected into the model's context each turn alongside the checklist. An empty string clears it. |

## Checklist item states

Items carry one of three states, written in the familiar markdown checkbox syntax:

| Marker | State |
|--------|-------|
| `- [ ] task` | pending |
| `- [~] task` | in progress |
| `- [x] task` | completed |

The `[~]` in-progress marker is what makes the checklist useful while a long task is still running: it shows you which single item BioRouter is working on right now.

## Example usage

In this example BioRouter fixes formatting inconsistencies across two documentation files, and uses the Todo capability to work through them systematically.

> **Note.** This example is self-referential — it shows BioRouter editing BioRouter's own documentation, so its checklist items refer to an internal docs style guide ("reference pattern", "reference format") that you do not need to know. Read it for the *shape* of the checklist, not for its content. Note also that every item is already ticked `[x]`, because the transcript was captured after the task finished; during a real run you would see a mix of `[ ]`, `[~]` and `[x]`.

### BioRouter prompt

```text
I need to update our documentation to fix formatting inconsistencies between two tutorial files. There are several sections that don't match the expected pattern. Can you help me systematically work through these issues and track our progress?
```

### BioRouter output

```md
I'll help you fix the documentation inconsistencies systematically. Let me break this down into organized tasks and track our progress.

## Fix Documentation Tutorial Inconsistencies

### Missing Components (vs Reference Pattern)
- [x] Add video embed at the top
- [x] Add LLM info box in Example Usage section  
- [x] Remove extra sections not in reference pattern

### Configuration Section Issues
- [x] Change terminology to match reference format
- [x] Fix CLI configuration section format
  - [x] Show exact command flow
  - [x] Include proper menu output with syntax highlighting
  - [x] Remove vague instructions

### Introduction/Description Issues
- [x] Simplify intro paragraph to match reference style
- [x] Fix tutorial description line to match pattern
- [x] Fix capitalization consistency throughout

### Example Structure Issues
- [x] Simplify example to match reference format
  - [x] Use clear "Prompt" and "Output" sections
  - [x] Add timing information
  - [x] Show actual response in proper code blocks
- [x] Use focused, achievable example
- [x] Show clear before/after states

### Formatting Consistency
- [x] Ensure code blocks use proper syntax highlighting
- [x] Match tone and structure of reference
- [x] Use consistent terminology throughout
- [x] Fix capitalization issues

## Documentation Update Complete! ✅

All tutorial inconsistencies have been resolved:

✅ **Structure aligned** - Matches reference pattern exactly
✅ **Components added** - Video embed, info boxes, proper tabs  
✅ **Terminology fixed** - Consistent language throughout
✅ **Example updated** - Simple, focused, with clear timing
✅ **CLI instructions** - Exact command flow with highlighting
✅ **Formatting consistent** - Proper code blocks and syntax

The documentation now follows a consistent pattern and provides a clear, organized experience for users.
```

## Related documentation

- [Extensions and skills guide](../extensions-and-skills-guide.md) — how to enable, disable and configure extensions generally.
- [Installation](../../getting-started/installation.md) — the list of capabilities enabled out of the box.
- [Subagents](../../agent-loop/subagents.md) — the other mechanism for structuring long, multi-step work.
- [Context engineering](../../agent-loop/context-engineering.md) — how the checklist and living plan are re-injected into the model's context each turn.
