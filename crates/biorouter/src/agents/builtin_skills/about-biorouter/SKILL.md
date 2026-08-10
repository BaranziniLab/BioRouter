---
name: about-biorouter
description: "Built-in self-knowledge about Biorouter. Load this skill whenever the user asks about Biorouter itself: what it is, how it works, who built it, or how to use or configure any of its features (extensions, skills, workflows, scheduler, knowledge bases, models and providers, secrets, CLI, or the desktop app)."
---

# About Biorouter

This is the authoritative self-knowledge reference for Biorouter. Use it to answer
questions about Biorouter accurately instead of guessing from general knowledge.
If a question goes deeper than this document, point the user to the documentation
at <http://biorouter.ucsf.edu/docs>.

## What Biorouter is

Biorouter is an open-source, AI-powered integrated research environment for
biomedical discovery, created by Wanjun Gu and the Baranzini Lab at UCSF. It
unifies commercial, institution-hosted, and local LLMs, AI agents, MCP-based
extensions, personal knowledge bases, and customizable workflows into one
extensible tool for exploratory analysis, prototyping, and automation.

- Website: <http://biorouter.ucsf.edu/>
- Extension & skill marketplace (BAAM): <http://biorouter.ucsf.edu/baam>
- Documentation: <http://biorouter.ucsf.edu/docs>
- Downloads: <http://biorouter.ucsf.edu/download>

## Architecture

Three layers:

1. **Interface**: the desktop app (Electron + React) or the `biorouter` CLI.
2. **Agent**: the reasoning loop holding session state, talking to the
   configured LLM provider, and dispatching tool calls.
3. **Extensions**: pluggable MCP (Model Context Protocol) servers that provide
   tools and context.

The desktop app spawns a local REST/WebSocket server (`biorouterd`) and talks to
it over a generated, type-safe API client. The CLI calls the agent library
directly. Sessions are persisted under `~/.config/biorouter/sessions/`.

## The major pillars

### Capabilities and extensions

Capabilities are the tools compiled into Biorouter. **Developer**, **Computer
Controller**, **Auto Visualiser**, **Code Execution**, **Extension Manager**,
**Skills**, **Todo**, **Memory**, **Knowledge**, and **Agent Drafter** are enabled
by default. **Chat Recall** and **Tutorial** are disabled by default.
Manage them in **Settings → Chat → Capabilities**.

User-installed extensions are MCP servers added through `stdio` (external
process), `streamable_http` (remote), or `inline_python`. Manage them from the
**Extensions** page in the desktop sidebar, via `biorouter configure`, ad-hoc with
`biorouter session --with-extension <cmd>`, or in
`~/.config/biorouter/config.yaml` under the `extensions:` key.
- Third-party extensions are browsable at <http://biorouter.ucsf.edu/baam>.

### Skills

Skills are reusable instruction sets: a folder containing a `SKILL.md` file
with YAML frontmatter (`name`, `description`) followed by markdown
instructions. The agent sees the list of enabled skills and loads one with the
`loadSkill` tool when relevant.

- Primary location: `~/.config/biorouter/skills/<slug>/SKILL.md`. Also
  discovered from `~/.claude/skills`, `~/.config/agents/skills`, extension
  bundles, and project-local `.biorouter/skills`, `.claude/skills`,
  `.agents/skills` directories.
- Manage from the **Skills** page in the sidebar (add, toggle, delete) or with
  `biorouter skill install|list|enable|disable|remove` in the CLI.
- Toggles persist in `~/.config/biorouter/skills-config.json`, written by both
  the GUI toggle and `biorouter skill enable|disable` (which accept a skill
  name, bundle name, or directory slug).
- This `about-biorouter` skill is built in: it ships with Biorouter, can be
  toggled off, and is restored automatically if its folder is removed.

### Workflows

Workflows are declarative YAML/JSON automation definitions executed by the
agent. Key fields: `version`, `title`, `description`, plus `instructions`
(system prompt for the session) and/or `prompt` (initial message, required for
headless/scheduled runs). Optional: `parameters` (typed inputs templated into
text with `{{ name }}` Jinja syntax), `extensions`, `skills`,
`knowledge_bases`, `settings` (per-workflow provider/model overrides),
`activities` (clickable starter buttons in the desktop UI), `response` (JSON
schema for structured output), `sub_workflows`, and `retry`.

- Manage from the **Workflows** page in the sidebar.
- CLI: `biorouter run --workflow my.yaml --param key=value`,
  `biorouter workflow validate <file>`, `biorouter workflow list`.

### Scheduler

The scheduler runs workflows on a cron cadence (standard 5-field cron, e.g.
`0 9 * * 1` = 9am Mondays). Jobs persist in `~/.config/biorouter/schedule.json`
and always run headless, so the workflow must define a `prompt`.

- Manage from the **Scheduler** page in the sidebar (create, pause, resume,
  edit, delete, run-now) or with `biorouter schedule list|delete|cron-help`.
- The agent itself can manage schedules via the platform schedule-management
  tool (list, create, run_now, pause, unpause, delete, inspect, sessions).

### Knowledge bases

Personal, LLM-maintained knowledge bases backed by markdown page trees with
full git history. Each KB lives at `~/.config/biorouter/knowledge/<kb-id>/`
with `raw/` (original sources), `knowledge/` (curated pages cross-linked with
`[[wiki-links]]`), `index.md`, `log.md`, and `schema.md` (editable conventions
that steer the ingestion sub-agent).

- Ingest sources (URLs, pasted text, PDFs, HTML, DOCX, CSV) from the
  **Knowledge** page in the sidebar, with live streaming progress. Sources are
  credibility-classified (peer-reviewed > preprint > book > gray literature >
  web > personal) via Crossref/OpenAlex lookup.
- In chat, the Knowledge extension provides `kb_search` (BM25 over curated
  pages), `kb_read_page`, `kb_write_page`, graph view, history/restore, and
  `.brkb` export/import for sharing whole KBs.
- A graph view visualizes pages as nodes connected by their `[[…]]`
  cross-references. An "active KB" can be selected per session from the chat
  composer.
- **Soul** is a built-in personal KB (`kb_id` "soul") installed on first run. It
  holds durable facts about the user (how they work, tools/commands they prefer,
  personal details) and is grown automatically by a "Meditation" workflow and a
  daily 3:00 AM "Daily Meditation" scheduled job, guided by the built-in
  `update-soul` skill. Consult it (`kb_search` with `kb_id="soul"`) to
  personalise answers; it may be hidden, so search it by explicit id.

### Models & providers

Biorouter supports 17+ LLM providers: Anthropic, OpenAI, Azure OpenAI, AWS
Bedrock, GCP Vertex AI, Google, Ollama (local), OpenRouter, Databricks,
SageMaker, Snowflake, GitHub Copilot, LiteLLM, xAI, institution-hosted Versa
(UCSF), and more.

- Defaults are set with the `BIOROUTER_PROVIDER` and `BIOROUTER_MODEL` config
  keys; workflows can override per-run in their `settings` block.
- Configure in **Settings → Providers/Models** in the desktop app, or via CLI:
  `biorouter models current`, `biorouter models providers`,
  `biorouter models list <provider>`,
  `biorouter models set --provider X --model Y`, or `biorouter configure`.

### Secrets

API keys are stored via a pluggable secrets backend (`secrets_backend` in
config.yaml or `BIOROUTER_SECRETS_BACKEND`):

- `keyring` (default): OS credential store (macOS Keychain, Windows
  Credential Manager, Linux Secret Service); read once per process.
- `encrypted-file`: passphrase-encrypted `secrets.enc` (Argon2id +
  XChaCha20-Poly1305).
- `file`: plaintext `secrets.yaml` (for headless/dev use).

Environment variables (e.g. `OPENAI_API_KEY`) override the backend entirely.

## Interfaces

### Desktop app

Left sidebar navigation: **Chat** (conversation), **History** (past sessions),
**Workflows**, **Scheduler**, **Extensions**, **Skills**, **Knowledge**,
**Apps** (MCP apps), and **Settings** (providers, models, permissions, theme).
Chat renders markdown, syntax-highlighted code, inline visualizations, and
expandable tool-call messages.

### CLI

Main commands: `biorouter session` (interactive chat, `--name`/`--resume`),
`biorouter run` (headless: `--text "prompt"` or `--workflow file.yaml --param
k=v`), `biorouter configure` (interactive setup), `biorouter models …`,
`biorouter schedule …`, `biorouter workflow …`, `biorouter skill …`,
`biorouter session-list`, `biorouter info`, `biorouter setup-path`.

## Configuration file map

| Path | Purpose |
|---|---|
| `~/.config/biorouter/config.yaml` | Providers, model defaults, extensions, secrets backend |
| `~/.config/biorouter/sessions/` | Session history |
| `~/.config/biorouter/skills/` | Installed skills |
| `~/.config/biorouter/workflows/` | Workflows |
| `~/.config/biorouter/knowledge/` | Knowledge bases |
| `~/.config/biorouter/schedule.json` | Scheduled jobs |
| `~/.config/biorouter/skills-config.json` | Skill enable/disable state |
| `.biorouterhints` / `AGENTS.md` (project) | Project-specific agent guidance |
| `.biorouterignore` (project) | Files the agent must not read |

## How to use this skill

When answering questions about Biorouter, ground your answer in this document.
If the user wants hands-on guidance, suggest enabling the **Tutorial**
extension, which offers interactive walkthroughs (getting started, knowledge
bases, workflows, scheduling, skills, and building MCP extensions). For
anything not covered here, refer them to <http://biorouter.ucsf.edu/docs>.
