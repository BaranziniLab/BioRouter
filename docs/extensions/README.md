# Extensions and skills

This folder documents how BioRouter is extended: MCP extensions (external and built-in servers that add tools the agent can call), platform extensions (capabilities that run inside the agent process), and skills (reusable instruction sets that teach the agent how to apply those tools). It holds the end-user guide to all three, a reference page for each built-in extension, and an open investigation into a Slack integration that has not been built.

Come here when you want to add a capability BioRouter does not have yet, when you want to know what a shipped built-in extension actually does, or when you are authoring a skill. This folder does *not* cover the two built-in extensions large enough to have their own documentation: Agent Drafter is documented in [`docs/agent-drafter/`](../agent-drafter/README.md) and its typed app surface in [`docs/apps-sdk/`](../apps-sdk/README.md). If you are trying to restrict what an extension is allowed to do rather than add one, go to [`docs/security/`](../security/README.md); if you want a scripted, repeatable multi-step task rather than an agent capability, go to [`docs/workflows/`](../workflows/README.md).

## Documents in this folder

| Document | What it covers |
|---|---|
| [Extensions, skills, and MCP agents](extensions-and-skills-guide.md) | The end-user guide to the three ways BioRouter is extended — MCP extensions, platform extensions, and skills — covering how to add, configure, and author each, plus installing from the BAAM marketplace and connecting remote MCP agents. |
| [Reliable Slack posting from the agent](slack-posting-investigation.md) | An options memo comparing three ways to give the agent reliable Slack posting — incoming webhook, a Slack MCP server with a bot token, and a user token or the Slack CLI — with a recommendation. **Open investigation, not implemented:** no `slack_post` tool or Slack extension exists in the tree, and the recommendation has been neither accepted nor rejected. |

## Built-in extension reference

The [`built-in/`](built-in/) subdirectory holds one user-facing reference page per built-in extension. It has no index of its own, so its pages are listed here.

| Document | What it covers |
|---|---|
| [Developer extension](built-in/developer.md) | A walkthrough of the Developer extension (its `shell`, `text_editor`, `analyze`, `screen_capture` and `image_processor` tools) plus a reference on constraining it with permission modes, tool permissions and `.biorouterignore`. Carries the most substantive security guidance of any extension page. |
| [Computer Controller extension](built-in/computer-controller.md) | Enabling the Computer Controller, which tools it provides, and a worked example combining web research with macOS system automation. The highest-blast-radius built-in, because it acts on your real desktop. |
| [Memory extension](built-in/memory.md) | The trigger words that store, recall and forget memories, where memories live on disk, and a worked example teaching BioRouter a lab's analysis standards. Predates the Knowledge feature; the page explains how the two relate. |
| [Auto Visualiser extension](built-in/auto-visualiser.md) | How to enable the Auto Visualiser, which figures it produces, and a worked cohort-data example. **Superseded in part:** the chart catalogue documents only 8 of the 34 tools the code now registers, so `crates/biorouter-mcp/src/autovisualiser/` is the current truth. |
| [Chat Recall extension](built-in/chat-recall.md) | Searching your saved session history by keyword or session ID so BioRouter can pull earlier context into the current conversation. Unlike most built-ins, it ships disabled by default. |
| [Code Execution extension](built-in/code-execution.md) | Code Mode: instead of calling MCP tools one at a time, the model writes a short JavaScript program that batches many tool calls into a single execution. Enabled by default, so the setup walkthrough is normally unnecessary. |
| [Extension Manager extension](built-in/extension-manager.md) | How BioRouter discovers, enables and disables other extensions mid-session so the active tool count — and the context it consumes — stays small. Enabled by default. |
| [Skills extension](built-in/skills.md) | Where skills are discovered from on disk, how to get more of them, and a worked GWAS-pipeline example showing a skill steering the agent. Enabled by default. |
| [Todo extension](built-in/todo.md) | How BioRouter breaks multi-step work into a tracked checklist and reports progress as it goes. Enabled by default. |
| [Tutorial extension](built-in/tutorial.md) | The interactive, step-by-step walkthroughs of BioRouter features that the agent can load on request. The tutorial list has been regenerated from `crates/biorouter-mcp/src/tutorial/tutorials/`, which remains authoritative. |

## Related documentation

- [Installation](../getting-started/installation.md) — the default-enabled extension list a new install starts from, before you add anything here
- [Permission modes](../security/permission-modes.md) — how to decide whether BioRouter asks before `shell`, `text_editor` or `computer_control` acts on your machine
- [Agent Drafter](../agent-drafter/README.md) — the built-in extension that builds interactive apps, documented separately because of its size
- [Workflows](../workflows/README.md) — the scripted alternative to a skill when you want deterministic steps rather than agent judgement
