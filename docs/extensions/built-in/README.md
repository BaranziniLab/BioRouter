# Built-in extensions

This folder holds one user guide per extension that ships inside BioRouter. Each page covers the same ground for its extension: how to confirm or change whether it is enabled, which tools it gives the agent, and a worked example of it doing real work. Most of these extensions are enabled by default, so the configuration walkthroughs are there to confirm or restore state rather than to perform first-time setup — the pages say so individually where it matters.

Come here when you want to know what a specific built-in extension can do, or why BioRouter reached for a tool you did not ask for. Go elsewhere if you are adding an extension BioRouter does not ship — installing a third-party MCP server, or writing a skill — which is covered by the [extensions, skills, and MCP agents guide](../extensions-and-skills-guide.md) one level up. If your question is about how much the agent is allowed to do rather than what it can do, the answer is in [security](../../security/README.md), not here.

## Documents

| Document | What it covers |
|----------|----------------|
| [Auto Visualiser](auto-visualiser.md) | How to enable the Auto Visualiser and which figures it produces, with a worked cohort-data example. The page carries a warning that its chart catalogue covers only 8 of the 34 tools the code registers — the source under `crates/biorouter-mcp/src/autovisualiser/` is the current truth for the full list. |
| [Chat Recall](chat-recall.md) | Searching your past session history by keyword or session ID so BioRouter can pull earlier context into the current conversation. Unlike most built-ins, this one ships disabled by default. |
| [Code Execution](code-execution.md) | Code Mode: instead of calling MCP tools one at a time, the model writes a short JavaScript program that batches many tool calls into a single execution. |
| [Computer Controller](computer-controller.md) | Enabling the Computer Controller, its tools, and a worked example combining web research with macOS system automation — the highest-blast-radius built-in, because it acts on your real desktop. |
| [Developer](developer.md) | A walkthrough of the Developer extension and its five tools, plus a reference on constraining it with permission modes, tool permissions, and `.biorouterignore`. |
| [Extension Manager](extension-manager.md) | How BioRouter discovers, enables, and disables other extensions mid-session so the active tool count stays small. |
| [Memory](memory.md) | The trigger words that store, recall, and forget memories, where memories live on disk, and a worked example teaching BioRouter a lab's analysis standards. Predates the Knowledge feature; the page explains how the two relate. |
| [Skills](skills.md) | Where skills are discovered from on disk, how to get more of them, and a worked GWAS-pipeline example showing a skill steering the agent. |
| [Todo](todo.md) | How BioRouter breaks multi-step work into a tracked checklist and reports progress as it goes. |
| [Tutorial](tutorial.md) | Loading interactive, step-by-step walkthroughs of BioRouter features, and what each of the seven shipped tutorials covers. `crates/biorouter-mcp/src/tutorial/tutorials/` remains the authoritative list. |

## Related documentation

- [Extensions, skills, and MCP agents](../extensions-and-skills-guide.md) — the guide to the three ways BioRouter is extended, and where to go to add or author an extension rather than use a bundled one.
- [Security](../../security/README.md) — permission modes, `.biorouterignore`, and secret storage; read it before letting the Developer or Computer Controller extensions run unattended.
- [Permission modes](../../security/permission-modes.md) — the specific mechanism for deciding whether BioRouter asks before acting, referenced from several pages in this folder.
- [Installation](../../getting-started/installation.md) — carries the authoritative list of which extensions are enabled by default on a fresh install.
