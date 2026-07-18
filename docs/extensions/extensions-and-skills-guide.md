# Extensions, skills, and MCP agents

> **What this is.** The end-user guide to the three ways Biorouter is extended — MCP extensions (tools), platform extensions (built-in agent capabilities), and skills (reusable instruction sets) — covering how to add, configure, and author each.
> **Status:** Current.
> **Audience:** end users.

Biorouter connects to databases, APIs, file systems, and custom workflows through pluggable parts rather than built-in special cases. Extensions add *tools* the agent can call; skills add *procedural knowledge* telling the agent how to use them. MCP — the [Model Context Protocol](https://github.com/modelcontextprotocol) — is the open standard that extensions speak, so any MCP server written by anyone can become a Biorouter extension. Read the extension sections if you want new capabilities; read the skills sections if the capabilities already exist and you want Biorouter to apply them consistently.

---

## Extensions (MCP servers)

Extensions are add-ons based on MCP. Each extension is an MCP server that exposes a set of tools Biorouter can invoke. Biorouter automatically scans extensions for known malware before activating them.

### Built-in MCP extensions

These extensions ship with Biorouter and are available immediately:

| Extension | Description | Default state |
|---|---|---|
| **Developer** | File operations, shell commands, text editing, code search — essential for software development | Enabled |
| **Computer Controller** | Web scraping, file caching, browser automation | Disabled |
| **Memory** | Remembers user preferences across sessions | Disabled |
| **Tutorial** | Interactive tutorials for learning Biorouter | Disabled |
| **Auto Visualiser** | Automatically generates data visualizations in conversations | Disabled |
| **Knowledge** | Personal, LLM-maintained knowledge bases backed by markdown and git history | Enabled |
| **Agent Drafter** | Builds interactive artifacts and exports them as standalone projects | Enabled |

> **Note.** The default-state column for the first five rows predates the current
> capability defaults in `ui/desktop/src/components/settings/capabilities/capabilities.ts`
> and may be out of date — check that file, or Settings → Chat → Capabilities in the
> desktop app, for what your build actually ships enabled.

Most built-ins have their own reference page in this folder — for example [Developer](built-in/developer.md), [Computer Controller](built-in/computer-controller.md), [Memory](built-in/memory.md), [Auto Visualiser](built-in/auto-visualiser.md), and [Tutorial](built-in/tutorial.md).

### Built-in platform extensions

Platform extensions provide global agent capabilities and run inside the agent process:

| Extension | Description | Default state |
|---|---|---|
| **Todo** | Manage task lists and track progress across sessions | Enabled |
| **Skills** | Load and use agent skills from skill directories | Enabled |
| **Extension Manager** | Discover, enable, and disable extensions during a session | Enabled |
| **Chat Recall** | Search conversation content across all session history | Disabled |
| **Code Execution** | Execute JavaScript in a sandboxed environment for tool discovery and calling | Disabled |

### Adding an external extension

Any MCP server can be added as a Biorouter extension. Pick whichever of the three routes below fits how you work — they write to the same configuration.

#### From the desktop app

1. Open the sidebar, then **Extensions** → **Add custom extension**.
2. Enter the extension type, ID, name, command, arguments, and any required environment variables.
3. Click **Add**.

#### From the CLI

```bash
biorouter configure
# Select "Add Extension" > "Command-line Extension"
```

To add one for a single session — here, the GitHub MCP server:

```bash
biorouter session --with-extension "GITHUB_PERSONAL_ACCESS_TOKEN=<token> npx -y @modelcontextprotocol/server-github"
```

#### In the config file

Add an entry under `extensions:` in `~/.config/biorouter/config.yaml`:

```yaml
extensions:
  github:
    name: GitHub
    cmd: npx
    args: ["-y", "@modelcontextprotocol/server-github"]
    enabled: true
    envs:
      GITHUB_PERSONAL_ACCESS_TOKEN: "<your_token>"
    type: stdio
    timeout: 300
```

### Extension types

| Type | Description |
|---|---|
| `stdio` | Standard I/O process (most common — Node/Python MCP servers) |
| `builtin` | Bundled with the Biorouter MCP server binary |
| `platform` | Runs in the agent process (platform extensions) |
| `streamable_http` | Remote server over HTTP |
| `inline_python` | Inline Python code executed via `uvx` |

### Managing extensions

| Action | Where |
|---|---|
| Toggle (desktop) | Sidebar → **Extensions** → toggle switch next to each extension |
| Toggle (CLI) | `biorouter configure` → **Toggle Extensions** |
| Remove (desktop) | Sidebar → **Extensions** → gear icon → **Remove Extension** |

Extensions enabled dynamically during a session (via the Extension Manager) are only active for that session. To persist an extension across sessions, enable it through Settings or the config file.

### Installing from the BAAM marketplace

BAAM is the Biorouter agent, extension, and skill marketplace. Its catalog is the machine-readable [`landing/registry.json`](../../landing/registry.json), published at <https://biorouter.ucsf.edu/registry.json> and browsable at <https://biorouter.ucsf.edu/baam.html>. Biorouter reads that registry to list and one-click-install extensions (`.brxt` bundles) and skills (`.zip`), so the marketplace is the shortest path to a curated extension — reach for the manual steps above when you are adding something that is not in the catalog.

### Developing a custom extension

Extensions are standard MCP servers. You can write one in any language (Python, TypeScript, Rust, etc.) that implements the MCP protocol.

- **Python:** `uvx mcp create my-extension`, or use the `mcp` Python SDK.
- **TypeScript:** use the `@modelcontextprotocol/sdk` npm package.
- **Reference:** the [MCP server quickstart](https://modelcontextprotocol.io/quickstart/server).

Once built, add it to Biorouter as a `stdio` or `streamable_http` extension.

---

## Skills (reusable instruction sets)

Skills are reusable instruction sets that teach Biorouter how to perform specific workflows. Unlike extensions (which add tools), skills add domain expertise and procedural knowledge — checklists, deployment procedures, API guides, and the like.

The **Skills platform extension** must be enabled (it is by default) for skills to work.

### How skills work

When a session starts, Biorouter discovers all available skills and adds them to its context. During a session, Biorouter automatically loads a skill when your request clearly matches the skill's purpose. You can also invoke skills explicitly:

```text
Use the code-review skill to review this PR
Follow the new-service skill to set up the auth service
Apply the deployment skill
```

Ask Biorouter "What skills are available?" to see the loaded skill list.

### Where skills live

Biorouter checks all of these directories. Later directories take priority when the same skill name exists in more than one:

1. `~/.claude/skills/` — global, shared with Claude Desktop
2. `~/.config/agents/skills/` — global, portable across AI coding agents
3. `~/.config/biorouter/skills/` — global, Biorouter-specific
4. `./.claude/skills/` — project-level, shared with Claude Desktop
5. `./.biorouter/skills/` — project-level, Biorouter-specific
6. `./.agents/skills/` — project-level, portable across agents

Use global skills for workflows that apply across projects. Use project-level skills for procedures tied to a specific codebase.

### Creating a skill

Each skill lives in its own directory with a `SKILL.md` file:

```text
~/.config/agents/skills/
└── code-review/
    └── SKILL.md
```

`SKILL.md` requires a YAML frontmatter block with `name` and `description`, followed by the instructions:

```markdown
---
name: code-review
description: Comprehensive code review checklist for pull requests
---

# Code Review Checklist

## Functionality
- [ ] Code does what the PR description claims
- [ ] Edge cases are handled
- [ ] Error handling is appropriate

## Code Quality
- [ ] Follows project style guide
- [ ] No hardcoded values that should be configurable
- [ ] Functions are focused and well-named

## Testing
- [ ] New functionality has tests
- [ ] Tests are meaningful, not just for coverage
```

### Skills with supporting files

A skill directory can include helper scripts, templates, or config files:

```text
~/.config/agents/skills/
└── api-setup/
    ├── SKILL.md
    ├── setup.sh
    └── templates/
        └── config.template.json
```

Biorouter can access these supporting files when executing the skill, via the Developer extension's file tools.

### Best practices for skills

- Keep skills focused — one skill per workflow or domain.
- Write for clarity — use numbered steps and direct language.
- Include verification steps so Biorouter can confirm the workflow completed successfully.
- Split long skills into multiple focused skills rather than one large monolithic one.

---

## Connecting remote MCP agents

Biorouter supports connecting to any remote MCP server as an agent. MCP servers communicate over `stdio` (local process) or Streamable HTTP (remote service), and any server that implements the MCP specification is compatible with Biorouter.

### Capabilities people commonly connect

- **Databases** — PostgreSQL, SQLite, Supabase
- **Web** — Fetch, Brave Search, Browserbase (headless browser), Firecrawl
- **Files** — Google Drive, PDF reading
- **Communication** — Slack, GitHub, Asana
- **Visualization** — Auto Visualiser, Blender
- **Memory** — Knowledge Graph Memory, Chat Recall
- **Data / science** — custom science-domain MCP servers

For servers curated for Biorouter, start with the BAAM marketplace above. For the wider ecosystem, the [PulseMCP server directory](https://www.pulsemcp.com/servers) catalogs third-party MCP servers.

### Configuring a remote agent

In `~/.config/biorouter/config.yaml`:

```yaml
extensions:
  my-remote-agent:
    name: My Remote Agent
    url: https://my-mcp-server.example.com/mcp
    type: streamable_http
    enabled: true
    timeout: 300
```

Or for a single session, via the CLI:

```bash
biorouter session --with-streamable-http-extension "https://my-mcp-server.example.com/mcp"
```

## Related documentation

- [Developer extension reference](built-in/developer.md) — the default-enabled extension most sessions actually use
- [Skills platform extension reference](built-in/skills.md) — the mechanics behind the skill loading described above
- [Config file reference](../configuration/config-file-reference.md) — the full schema for the `extensions:` block shown above
- [Secret storage](../security/secret-storage.md) — where extension tokens such as `GITHUB_PERSONAL_ACCESS_TOKEN` are kept, and how to avoid plaintext
- [Slack posting investigation](slack-posting-investigation.md) — a worked example of choosing between a webhook and a third-party MCP extension
