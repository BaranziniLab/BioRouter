# Getting started

This folder is the end-user on-ramp to biorouter: how to install the desktop app and its bundled `biorouter` command-line tool, how to connect an LLM provider and pick a model, how to run a first biomedical research task, and the day-to-day habits that make the agent work well for you. Everything here assumes no prior knowledge of the app — MCP, extensions, and skills are introduced from scratch where they first appear.

Come here if you have not yet got biorouter running, or if you have it running and want to use it better. Two paths cover the same install: [biorouter in 5 minutes](quickstart.md) is the fast one, [Installation and setup](installation.md) is the thorough one that adds UCSF institutional providers, remote MCP agents, and file locations. Go elsewhere once you are past setup: [configuration](../configuration/README.md) holds the reference for individual setting names and accepted values, [command-line interface](../cli/README.md) documents every subcommand and flag, [extensions, skills, and MCP agents](../extensions/extensions-and-skills-guide.md) covers adding new capabilities in depth, and [troubleshooting](../troubleshooting/README.md) is where a specific error message gets looked up.

## Documents in this folder

| Document | What it covers |
|---|---|
| [biorouter in 5 minutes](quickstart.md) | A five-minute onboarding path: install biorouter, configure a model provider, start a session, write a first biomedical prompt, and enable an extension. |
| [Installation and setup](installation.md) | The step-by-step guide to installing Biorouter, connecting an LLM provider (including UCSF institutional options), verifying the setup, adding extensions and remote MCP agents, and finding config and log locations. |
| [Choosing a model provider](choosing-a-model-provider.md) | A reference of Biorouter's supported LLM providers: the credentials each needs, its default model, a representative model list, and how to switch provider or override the choice per session. **Superseded in part** — the provider inventory and panel ordering no longer match the shipping app (four shipping providers have no entry, and the model lists are undated hand-maintained snapshots); the switching, orchestration, and custom-provider sections at the end remain accurate. |
| [Usage tips](usage-tips.md) | Short, independent tips for working with biorouter day to day — prompting, model choice, context and session hygiene, extensions, safety, cost, and workflows. |

## Related documentation

- [Configuration](../configuration/README.md) — once you know *which* setting you want to change, this folder gives its name, accepted values, and default.
- [Extensions, skills, and MCP agents](../extensions/extensions-and-skills-guide.md) — the full end-user guide to the three ways biorouter is extended, picking up where the quickstart's single extension example stops.
- [Command-line interface](../cli/README.md) — the complete `biorouter` subcommand and flag reference, plus the slash commands and shortcuts inside an interactive session.
- [Troubleshooting](../troubleshooting/README.md) — known problems and their fixes, and how to produce a diagnostics bundle when setup does not go as described here.
