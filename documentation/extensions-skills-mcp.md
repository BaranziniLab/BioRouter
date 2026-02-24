# UCSF BioRouter — Extensions, Skills, and MCP Agents

BioRouter is extensible through three complementary mechanisms: **Extensions** (MCP servers that add tools), **Skills** (reusable instruction sets), and **Platform extensions** (built-in agent capabilities). Together they allow BioRouter to connect to databases, APIs, file systems, and custom workflows.

---

## Extensions (MCP Servers)

Extensions are add-ons based on the [Model Context Protocol (MCP)](https://github.com/modelcontextprotocol). Each extension is an MCP server that exposes a set of tools BioRouter can invoke. BioRouter automatically scans extensions for known malware before activating them.

### Built-in Extensions

These extensions ship with BioRouter and are available immediately:

| Extension | Description | Default State |
|---|---|---|
| **Developer** | File operations, shell commands, text editing, code search — essential for software development | Enabled |
| **Computer Controller** | Web scraping, file caching, browser automation | Disabled |
| **Memory** | Remembers user preferences across sessions | Disabled |
| **Tutorial** | Interactive tutorials for learning BioRouter | Disabled |
| **Auto Visualiser** | Automatically generates data visualizations in conversations | Disabled |

### Built-in Platform Extensions

Platform extensions provide global agent capabilities and run inside the agent process:

| Extension | Description | Default State |
|---|---|---|
| **Todo** | Manage task lists and track progress across sessions | Enabled |
| **Skills** | Load and use agent skills from skill directories | Enabled |
| **Extension Manager** | Discover, enable, and disable extensions during a session | Enabled |
| **Chat Recall** | Search conversation content across all session history | Disabled |
| **Code Execution** | Execute JavaScript in a sandboxed environment for tool discovery and calling | Disabled |

### Adding External Extensions

Any MCP server can be added as a BioRouter extension.

**Desktop UI:**
1. Open the sidebar > Extensions > Add custom extension.
2. Enter the extension type, ID, name, command, arguments, and any required environment variables.
3. Click Add.

**CLI:**
```sh
biorouter configure
# Select "Add Extension" > "Command-line Extension"
```

**Example — adding the GitHub MCP server:**
```sh
biorouter session --with-extension "GITHUB_PERSONAL_ACCESS_TOKEN=<token> npx -y @modelcontextprotocol/server-github"
```

**Config file** (`~/.config/biorouter/config.yaml`):
```yaml
extensions:
  github:
    name: GitHub
    cmd: npx
    args: [-y @modelcontextprotocol/server-github]
    enabled: true
    envs:
      GITHUB_PERSONAL_ACCESS_TOKEN: "<your_token>"
    type: stdio
    timeout: 300
```

### Extension Types

| Type | Description |
|---|---|
| `stdio` | Standard I/O process (most common — Node/Python MCP servers) |
| `builtin` | Bundled with the BioRouter MCP server binary |
| `platform` | Runs in the agent process (platform extensions) |
| `streamable_http` | Remote server over HTTP |
| `inline_python` | Inline Python code executed via `uvx` |

### Managing Extensions

**Toggling (Desktop):** Sidebar > Extensions > toggle switch next to each extension.

**Toggling (CLI):** `biorouter configure` > Toggle Extensions.

**Removing (Desktop):** Sidebar > Extensions > gear icon > Remove Extension.

Extensions enabled dynamically during a session (via the Extension Manager) are only active for that session. To persist an extension across sessions, enable it through Settings or the config file.

### Developing Custom Extensions

Extensions are standard MCP servers. You can write one in any language (Python, TypeScript, Rust, etc.) that implements the MCP protocol.

- **Python:** `uvx mcp create my-extension` or use the `mcp` Python SDK.
- **TypeScript:** Use the `@modelcontextprotocol/sdk` npm package.
- **Reference:** https://modelcontextprotocol.io/quickstart/server

Once built, add it to BioRouter as a `stdio` or `streamable_http` extension.

---

## Skills

Skills are reusable instruction sets that teach BioRouter how to perform specific workflows. Unlike extensions (which add tools), skills add domain expertise and procedural knowledge — checklists, deployment procedures, API guides, etc.

The **Skills platform extension** must be enabled (it is by default) for skills to work.

### How Skills Work

When a session starts, BioRouter discovers all available skills and adds them to its context. During a session, BioRouter automatically loads a skill when your request clearly matches the skill's purpose. You can also invoke skills explicitly:

```
Use the code-review skill to review this PR
Follow the new-service skill to set up the auth service
Apply the deployment skill
```

You can ask BioRouter "What skills are available?" to see the loaded skill list.

### Skill Locations

BioRouter checks all of these directories (later directories take priority if the same skill name exists in multiple):

1. `~/.claude/skills/` — global, shared with Claude Desktop
2. `~/.config/agents/skills/` — global, portable across AI coding agents
3. `~/.config/BioRouter/skills/` — global, BioRouter-specific
4. `./.claude/skills/` — project-level, shared with Claude Desktop
5. `./.biorouter/skills/` — project-level, BioRouter-specific
6. `./.agents/skills/` — project-level, portable across agents

Use global skills for workflows that apply across projects. Use project-level skills for procedures tied to a specific codebase.

### Creating a Skill

Each skill lives in its own directory with a `SKILL.md` file:

```
~/.config/agents/skills/
└── code-review/
    └── SKILL.md
```

The `SKILL.md` file requires a YAML frontmatter block with `name` and `description`, followed by the instructions:

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

### Skills with Supporting Files

A skill directory can include helper scripts, templates, or config files:

```
~/.config/agents/skills/
└── api-setup/
    ├── SKILL.md
    ├── setup.sh
    └── templates/
        └── config.template.json
```

BioRouter can access these supporting files when executing the skill via the Developer extension's file tools.

### Best Practices

- Keep skills focused — one skill per workflow or domain.
- Write for clarity — use numbered steps and direct language.
- Include verification steps so BioRouter can confirm the workflow completed successfully.
- Split long skills into multiple focused skills rather than one large monolithic one.

---

## MCP Agent Integration

BioRouter supports connecting to any remote MCP server as an agent, enabling access to specialized capabilities:

- **Databases** — PostgreSQL, SQLite, Supabase
- **Web** — Fetch, Brave Search, Browserbase (headless browser), Firecrawl
- **Files** — Google Drive, PDF reading
- **Communication** — Slack, GitHub, Asana
- **Visualization** — Auto Visualiser, Blender
- **Memory** — Knowledge Graph Memory, Chat Recall
- **Data / Science** — custom science-domain MCP servers

MCP servers communicate over `stdio` (local process) or Streamable HTTP (remote service). Any server that implements the MCP specification is compatible with BioRouter.

A directory of available MCP servers is maintained at https://www.pulsemcp.com/servers.

### Connecting a Remote MCP Agent

```yaml
# In ~/.config/biorouter/config.yaml
extensions:
  my-remote-agent:
    name: My Remote Agent
    url: https://my-mcp-server.example.com/mcp
    type: streamable_http
    enabled: true
    timeout: 300
```

Or via CLI:
```sh
biorouter session --with-streamable-http-extension "https://my-mcp-server.example.com/mcp"
```
