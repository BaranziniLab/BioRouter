# Extensions and skills

This folder documents how BioRouter is extended: MCP extensions (external and built-in servers that add tools the agent can call), platform extensions (capabilities that run inside the agent process), and skills (reusable instruction sets that teach the agent how to apply those tools). It holds the end-user guide to all three, a reference page for each built-in extension, and an open investigation into a Slack integration that has not been built.

Come here when you want to add a capability BioRouter does not have yet, when you want to know what a shipped built-in extension actually does, or when you are authoring a skill. This folder does *not* cover the two built-in extensions large enough to have their own documentation: Agent Drafter is documented in [`docs/agent-drafter/`](../agent-drafter/README.md) and its typed app surface in [`docs/apps-sdk/`](../apps-sdk/README.md). If you are trying to restrict what an extension is allowed to do rather than add one, go to [`docs/security/`](../security/README.md); if you want a scripted, repeatable multi-step task rather than an agent capability, go to [`docs/workflows/`](../workflows/README.md).

## Documents in this folder

| Document | What it covers |
|---|---|
| [Extensions, skills, and MCP agents](extensions-and-skills-guide.md) | The end-user guide to the three ways BioRouter is extended — MCP extensions, platform extensions, and skills — covering how to add, configure, and author each, plus installing from the BAAM marketplace and connecting remote MCP agents. |
| [Reliable Slack posting from the agent](slack-posting-investigation.md) | An options memo comparing three ways to give the agent reliable Slack posting — incoming webhook, a Slack MCP server with a bot token, and a user token or the Slack CLI — with a recommendation. **Open investigation, not implemented:** no `slack_post` tool or Slack extension exists in the tree, and the recommendation has been neither accepted nor rejected. |

## Built-in extension reference

The [`built-in/`](built-in/README.md) subdirectory holds one user-facing reference page per built-in extension: Developer, Computer Controller, Memory, Auto Visualiser, Chat Recall, Code Execution, Extension Manager, Skills, Todo and Tutorial. It carries its own index describing each page, so read that index rather than a second copy of it kept here.

Two of those pages are worth knowing about before you go. [Developer](built-in/developer.md) carries the most substantive security guidance of any extension page, and [Computer Controller](built-in/computer-controller.md) documents the highest-blast-radius built-in, because it acts on your real desktop.

## Related documentation

- [Installation](../getting-started/installation.md) — the default-enabled extension list a new install starts from, before you add anything here
- [Permission modes](../security/permission-modes.md) — how to decide whether BioRouter asks before `shell`, `text_editor` or `computer_control` acts on your machine
- [Agent Drafter](../agent-drafter/README.md) — the built-in extension that builds interactive apps, documented separately because of its size
- [Integrations with other tools](../integrations/README.md) — the opposite direction: adapters that let another host application, such as JupyterLab's Jupyter AI chat, run a Biorouter agent with everything documented here already attached
- [Workflows](../workflows/README.md) — the scripted alternative to a skill when you want deterministic steps rather than agent judgement
