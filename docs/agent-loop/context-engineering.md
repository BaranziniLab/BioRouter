# Context engineering

> **What this is.** An index of the BioRouter features you use to give the agent durable background knowledge, preferences, and workflows, so you do not re-explain yourself every session.
> **Status:** Current — the page body below is a live routing table. (Its original docs-site body was stripped by the 2026-05-07 plain-markdown migration; what you are reading replaces it.)
> **Audience:** end users

Context engineering is about building background knowledge, preferences, and workflows that help biorouter work more effectively. Instead of repeating instructions, you define them once and teach biorouter how you work. Each mechanism below covers one way of doing that — persistent memory, reusable instruction sets, packaged session configurations, lifecycle hooks, and delegation — and each has its own guide.

## Where the guides live

| Mechanism | Guide | What it gives you |
|---|---|---|
| Memory | [Memory extension](../extensions/built-in/memory.md) | Teach biorouter key information — commands, code snippets, preferences, configurations — that it recalls and applies later, scoped either per-project (local) or globally. |
| Skills | [Skills extension](../extensions/built-in/skills.md) | Load reusable sets of instructions that teach biorouter how to perform a specific task, discovered automatically at startup from `.agents/skills/` and `~/.config/agents/skills/`. |
| Extensions and skills together | [Extensions, skills, and MCP agents](../extensions/extensions-and-skills-guide.md) | How the three extensibility mechanisms — MCP-server extensions, skills, and built-in platform extensions — relate to one another. |
| Workflows | [Workflows](../workflows/README.md) | Package instructions, prompts, extension requirements, parameters, and model settings into one shareable file that launches a reproducible, pre-configured session. |
| Configuration files | [Configuration file reference](../configuration/config-file-reference.md) | Persist default behaviours, model choices, tool permissions, and extensions in `config.yaml`. |
| Environment variables | [Environment variables](../configuration/environment-variables.md) | Set the same kinds of settings per-invocation rather than persistently. |
| Hooks | [Hooks reference](hooks/hooks-reference.md) | Run your own shell commands, or an LLM judge, at points in the agent lifecycle — including injecting context before a tool runs or around compaction. |
| Delegation | [Subagents](subagents.md) | Offload work to temporary instances so tool output does not accumulate in the main conversation. |

## Related documentation

- [Subagents](subagents.md) — the delegation mechanism for keeping a long session's context clean.
- [Hooks reference](hooks/hooks-reference.md) — programmatic context injection at defined lifecycle points.
- [Workflows](../workflows/README.md) — the packaged, shareable form of a configured session.
- [Usage tips](../getting-started/usage-tips.md) — practical habits, including why to keep sessions short.
- [Managing sessions](../getting-started/managing-sessions.md) — how session context and history are maintained.
