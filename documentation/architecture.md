# UCSF BioRouter — Architecture

UCSF BioRouter is an AI-powered integrated research environment that unifies commercial, institution-hosted, and local large language models (LLMs), AI agents, Information Commons databases, and customizable workflows into one extensible platform for explorative analysis, prototyping, automation, and federated cross-institution collaboration.

**Developed by:** Wanjun Gu (wanjun.gu@ucsf.edu), Baranzini Lab (https://baranzinilab.ucsf.edu/), UCSF
**Supported by:** UCSF IT and Information Commons
**GitHub:** https://github.com/BaranziniLab/BioRouter
**Releases:** https://github.com/BaranziniLab/BioRouter/releases

---

## High-Level Overview

BioRouter is built as a modular, plugin-based system. It consists of three main layers:

1. **Interface** — The desktop GUI or CLI that accepts user input and displays responses.
2. **Agent** — The core reasoning loop that manages LLM interaction, tool execution, and session state.
3. **Extensions** — Pluggable MCP servers that give the agent access to tools (file operations, database queries, web access, code execution, etc.).

In a typical session, the interface starts an agent instance, which connects to one or more extensions simultaneously and routes requests through the selected LLM provider.

---

## Tech Stack

### Backend — Rust

The backend is a Rust workspace (`crates/`) organized into several crates:

| Crate | Purpose |
|---|---|
| `biorouter` | Core agent library — agent loop, provider integrations, session management, recipes, scheduling |
| `biorouter-server` | REST API server (`biorouterd`) that the desktop UI communicates with |
| `biorouter-cli` | Command-line interface (`biorouter` binary) |
| `biorouter-mcp` | Built-in MCP servers (Developer, Computer Controller, Memory, Tutorial, Auto Visualiser) |
| `biorouter-acp` | Agent Communication Protocol support |
| `biorouter-bench` | Benchmarking tools |
| `biorouter-test` | Integration tests |

Key Rust dependencies:

- **tokio** — Async runtime
- **axum** — HTTP web framework for the API server
- **rmcp** — Model Context Protocol implementation
- **reqwest** — HTTP client for provider API calls
- **serde / serde_json** — Serialization
- **tiktoken-rs** — Token counting for context management
- **minijinja** — Jinja-style template engine for recipes
- **tokio-cron-scheduler** — Cron-based job scheduling
- **sqlx (SQLite)** — Persistent session and schedule storage
- **etcetera** — Cross-platform config path resolution (`~/.config/biorouter/` on macOS/Linux)

### Frontend — Electron + React

The desktop application is an Electron app built with React and TypeScript.

| Component | Details |
|---|---|
| Framework | Electron 39 + React 19 |
| Build tool | Vite + Electron Forge |
| Language | TypeScript (strict mode) |
| Styling | TailwindCSS v4 with custom design tokens |
| UI components | Radix UI primitives |
| Routing | React Router DOM v7 |
| Testing | Vitest (unit), Playwright (E2E) |

The frontend communicates with the `biorouterd` REST server (started in the background by the Electron main process) via a local HTTP API. The OpenAPI spec is generated from the Rust server and used to type-safe frontend API calls.

---

## Agent Interaction Loop

The agent operates in a continuous loop:

1. **Human request** — The user sends a message or task through the interface.
2. **Provider chat** — The agent forwards the request plus a list of available tools to the configured LLM provider.
3. **Tool call** — If the LLM decides to invoke a tool, the agent extracts the tool call (JSON) and executes it via the appropriate extension.
4. **Result feedback** — The tool result is returned to the LLM as context.
5. **Context revision** — Old or irrelevant messages are summarized or pruned to manage token usage efficiently.
6. **Final response** — Once all tool calls are complete, the LLM sends a final response to the user.

If a tool call produces an error (invalid JSON, missing tool, etc.), BioRouter captures and returns the error to the model as a tool response, allowing the LLM to self-correct without breaking the session.

---

## Configuration and Data Paths

| Location | Purpose |
|---|---|
| `~/.config/biorouter/config.yaml` | Primary config — providers, API keys, extensions, settings |
| `~/.config/biorouter/sessions/` | Session history (SQLite) |
| `~/.config/biorouter/recipes/` | Saved recipes |
| `~/.config/biorouter/skills/` | BioRouter-specific global skills |
| `~/Library/Application Support/BioRouter/` | Electron app state (macOS) |

The config file is shared between the Desktop UI and the CLI — changes in either interface are reflected in both.

---

## Multi-Model and Multi-Agent Support

BioRouter supports running multiple agents in parallel:

- **Sub-agents** — A recipe can spawn sub-agents to handle parallel tasks, each with its own LLM provider and extension set.
- **Lead/Worker orchestration** — A lead model delegates sub-tasks to worker models, enabling multi-model pipelines.
- **Subrecipes** — Recipes can call other recipes as sub-tasks, running them sequentially or in parallel.

---

## Security

- Extensions are scanned for known malware before activation.
- BioRouter enforces permission modes that control whether tool calls require user approval.
- `.biorouterignore` files can restrict which files and directories the agent is allowed to access.
- Allowlists can restrict which shell commands the agent may execute.
