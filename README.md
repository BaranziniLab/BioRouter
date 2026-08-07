<div align="center">

<img src="ui/desktop/src/images/icon.png" alt="Biorouter" width="120"/>

# UCSF Biorouter

**An AI-powered integrated research environment for biomedical discovery**

<p>
  <a href="https://opensource.org/licenses/Apache-2.0"><img src="https://img.shields.io/badge/License-Apache_2.0-blue.svg" alt="Apache 2.0 License"></a>
  <img src="https://img.shields.io/badge/version-1.88.6-tan.svg" alt="Version 1.88.6">
</p>

<a href="http://biorouter.ucsf.edu/">Website</a> ·
<a href="http://biorouter.ucsf.edu/download">Download</a> ·
<a href="http://biorouter.ucsf.edu/docs">Docs</a> ·
<a href="http://biorouter.ucsf.edu/baam">BAAM Marketplace</a>

</div>

## What is Biorouter?

[UCSF Biorouter](http://biorouter.ucsf.edu/) is an AI-powered integrated research environment for **biomedical discovery**, built by the [Baranzini Lab](https://baranzinilab.ucsf.edu/) at UCSF. It unifies commercial, institution-hosted, and fully local LLMs together with AI agents, biomedical databases and knowledge graphs, personal knowledge bases, and customizable workflows into a single extensible tool.

Think of Biorouter as your biomedical research co-pilot — one that can read and synthesize papers, query biomedical databases and knowledge graphs (like SPOKE), explore clinical/EHR/OMOP data, build cohorts, run genomics and bioinformatics pipelines, analyze drug–disease relationships, visualize results, and carry out complex multi-step research tasks — all from one unified interface.

Biorouter runs as a desktop app, a full-screen terminal CLI, or a headless REST/WebSocket server, sharing the same agent core across all three.

## Key Features

### Bring your own model — commercial, institutional, or fully local

- **40+ LLM providers** — Anthropic Claude, OpenAI GPT, Google Gemini, Amazon Bedrock, Azure OpenAI, Databricks, Ollama, and many more.
- **UCSF institution-hosted options** — built-in support for UCSF-hosted ChatGPT (Azure OpenAI) and UCSF-hosted Anthropic (Amazon Bedrock) for compliant access on sensitive research.
- **Zero-setup local models** — a bundled **Llama Server** (llama.cpp sidecar) ships a curated Qwen/Gemma catalog with one-click download and runs entirely on your machine; Ollama is also supported. Ideal for **air-gapped, private inference** where no data leaves your device.

### Biomedical agents & the MCP extension ecosystem

- **Model Context Protocol (MCP)** — connect Biorouter to biomedical databases, web tools, file systems, and APIs through pluggable extensions, and install third-party agents.
- **Biomedical agents via the BAAM marketplace** ([biorouter.ucsf.edu/baam](http://biorouter.ucsf.edu/baam)) — including **SPOKEAgent** (the SPOKE biomedical knowledge graph), the **UCSF OMOP Agent** and **CDWAgent** (clinical/EHR/OMOP data and cohort building), plus a growing library of bioinformatics and clinical skills (ATAC-seq, ChIP-seq, alternative splicing, causal genomics, chemoinformatics, clinical biostatistics, and more).
- **Built-in extensions** — Developer (shell, files, code execution), Computer Controller (web/computer automation), Memory, Tutorial, Auto Visualiser, and Knowledge.

### Personal, LLM-maintained knowledge bases

- Build personal knowledge bases backed by **markdown trees + git history** that an LLM curates as it ingests.
- **Ingest** papers and documents from **PDF, HTML, DOCX, PowerPoint, CSV/XLSX, and URLs**.
- **Source credibility classification** via Crossref / OpenAlex, a **knowledge graph view** of cross-linked pages, **BM25 search**, full change history, and **`.brkb` export/import** to share a base.

### Auto Visualiser — publication-ready figures in chat

Turn structured data into self-contained, interactive HTML figures rendered inline in chat — **33 tools** spanning scientific plots (**volcano**, **Manhattan**, **Kaplan–Meier**, **forest**), charts (histogram, box, bubble, area, radar, donut, gauge), relationships and hierarchies (network, Sankey, chord, heatmap, treemap, sunburst, dendrogram, word cloud, calendar heatmap), diagrams (Mermaid flowchart/gantt/sequence/mindmap/timeline/ER/state/class), and geographic maps (Leaflet map, choropleth).

### Computer Controller & vision

- Drive the web and your computer for research automation, with **multi-monitor screen capture** (enumerate displays and re-capture any screen by index) and **vision input** so the model can read screenshots and figures.

### Workflows, skills & automation

- **Workflows** — package any multi-step task into a shareable, reusable file with Jinja-style templating; compose **sub-workflows**.
- **Scheduling** — run workflows and agent automations on a **cron schedule**, unattended.
- **Skills** — teach Biorouter your lab's reusable instruction sets and best practices; built-in authoring skills (`develop-biorouter-extension`, `develop-biorouter-skill`) help you create your own.
- **Lifecycle hooks** — fire custom commands at `SessionStart`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `Stop`, and `SessionEnd` for logging, policy, and automation.
- **Agent goals & ACP** — agents track goals across a session, and the Agent Communication Protocol enables multi-agent orchestration.

### Three surfaces, one core

- **Desktop app** (Electron + React) for interactive work.
- **`biorouter` CLI** — a full-screen TUI at parity with the GUI: slash-command palette, model/provider setup, knowledge bases, and extension/skill/workflow install, all from the terminal. `biorouter doctor` checks prerequisites and flags when a newer release is available.
- **`biorouterd` server** — REST + WebSocket API with an OpenAPI-generated TypeScript client.

### Built for the desktop

- **One-click "Restart & Update"** on macOS (electron-updater) — downloads in the background and swaps the app in place, leaving your settings, sessions, and knowledge bases untouched.
- **Native installers** for macOS (Apple Silicon + Intel), Windows, and Linux (deb/rpm), plus **headless CLI-only Linux packages** for servers, HPC nodes, and containers.
- **Secret storage** via the OS keychain (macOS Keychain / Windows Credential Manager / Linux Secret Service), configurable **permission modes**, and a **`.biorouterignore`** to keep sensitive files out of the agent's reach.

## Download

Native installers for all major platforms are available in every release:

| Platform | Package |
|----------|---------|
| **macOS** (Apple Silicon) | `Biorouter-*-arm64.dmg` — open and drag to `/Applications` |
| **macOS** (Intel) | `Biorouter-*-x64.dmg` — open and drag to `/Applications` |
| **Windows** (x64) | `Biorouter-win32-x64-*.zip` — unzip and run `Biorouter.exe` |
| **Linux** Ubuntu / Pop!_OS (x64) | `biorouter_*_amd64.deb` — `sudo dpkg -i biorouter_*.deb` |
| **Linux** Fedora / RHEL (x64) | `Biorouter-*-1.x86_64.rpm` — `sudo rpm -i Biorouter-*.rpm` |
| **Linux — CLI only** Debian/Ubuntu (x64) | `biorouter-cli_*_amd64.deb` — `sudo apt install ./biorouter-cli_*.deb` |
| **Linux — CLI only** Fedora/RHEL (x64) | `biorouter-cli-*-1.x86_64.rpm` — `sudo dnf install ./biorouter-cli-*.rpm` |

**[Download Biorouter →](http://biorouter.ucsf.edu/download)** or grab assets from the [Releases page](https://github.com/BaranziniLab/biorouter/releases).

The `biorouter` command-line tool ships **inside** the desktop app. On macOS and Windows, install the app above, then accept the in-app "Install Biorouter CLI" prompt (or run `biorouter setup-path`) to add the bundled `biorouter` binary to your `PATH`. On Linux you can install the CLI on its own with the headless `biorouter-cli` package (`biorouter-cli_*_amd64.deb` or `biorouter-cli-*-1.x86_64.rpm`) — no desktop app required.

Always install the newest version for the latest features and fixes.

## Getting Started in 3 Steps

**1. Download and install** Biorouter for your platform from the table above.

**2. Connect a model** — on first launch, Biorouter walks you through choosing a provider:
- **UCSF users** — select Azure OpenAI (UCSF ChatGPT) or Amazon Bedrock (UCSF Anthropic).
- **Your own API key** — enter your Anthropic, OpenAI, or Google key directly.
- **Fully local** — pick the bundled Llama Server (zero setup) or install [Ollama](https://ollama.com); no API key, no data leaves your device.

**3. Start exploring** — ask a research question, ingest papers into a knowledge base, query SPOKE, build a cohort, or load a workflow. Biorouter takes it from there.

## Who is Biorouter For?

- **Bench and computational researchers** analyzing data, reviewing literature, and running genomics/bioinformatics pipelines with AI assistance.
- **Clinical researchers and data scientists** who need secure, institution-compliant AI access for sensitive EHR/OMOP and cohort work.
- **Labs and teams** sharing reusable AI workflows, skills, and knowledge bases across their group.

## Working with Sensitive Data

Biorouter routes your inputs to an LLM provider. For patient data, PHI, or other sensitive research data:

- **Use institution-managed services** (UCSF Azure OpenAI or UCSF Amazon Bedrock) or **fully local models** (bundled Llama Server or Ollama).
- **Do not** use personal commercial API keys with patient data.
- **Always verify** with your institution's compliance office before processing sensitive data.

See the [Data Privacy Guide](docs/security/data-privacy-and-phi.md) for full details.

## Documentation

Full documentation lives at [biorouter.ucsf.edu/docs](http://biorouter.ucsf.edu/docs) and in the [docs/](docs/) folder:

| Guide | Description |
|---|---|
| [Architecture](docs/architecture/system-overview.md) | How Biorouter is built — backend, frontend, agent loop |
| [Providers & Models](docs/getting-started/choosing-a-model-provider.md) | All supported LLM providers and models |
| [Extensions, Skills & MCP](docs/extensions/extensions-and-skills-guide.md) | Adding tools, agents, and reusable skills |
| [Workflows](docs/workflows/README.md) | Creating and sharing automated workflows |
| [Schedulers](docs/workflows/scheduled-jobs.md) | Running workflows on a schedule |
| [Hooks](docs/agent-loop/hooks/hooks-reference.md) | Lifecycle hooks for logging, policy, and automation |
| [Secret Storage](docs/security/secret-storage.md) | How credentials are kept in your OS keychain |
| [Installation & Setup](docs/getting-started/installation.md) | Step-by-step setup guide |
| [Data Privacy](docs/security/data-privacy-and-phi.md) | Guidelines for handling patient and sensitive data |

## Acknowledgments

Biorouter's agentic environment was built on the foundation of, and with reference to, the following open-source AI tools — we are grateful to their authors and communities:

- **[Goose](https://block.github.io/goose/)** — CLI/Desktop agent for full developer workflows (Block) — Biorouter's primary upstream foundation
- **[Aider](https://aider.chat/)** — open-source, Git-native CLI AI coding agent
- **[Cline](https://github.com/cline/cline)** — open-source interactive CLI coding agent
- **[OpenCode](https://opencode.ai/)** — open-source coding agent with multi-session and multi-provider support
- **[ForgeCode](https://forgecode.dev/)** — terminal AI assistant for task planning and code generation

## Citation

If you use Biorouter in your research, please cite:

```bibtex
@software{biorouter2025,
  title  = {UCSF Biorouter: An AI-Powered Integrated Research Environment},
  author = {Gu, Wanjun and Bellucci, Gianmarco and Baranzini, Sergio E.},
  year   = {2025},
  url    = {https://github.com/BaranziniLab/biorouter}
}
```

## About

UCSF Biorouter is developed by **Wanjun Gu** ([wanjun.gu@ucsf.edu](mailto:wanjun.gu@ucsf.edu)) at the [Baranzini Lab](https://baranzinilab.ucsf.edu/), Department of Neurology, UCSF Bakar Computational Health Sciences Institute. Development is supported by **UCSF IT** and **Information Commons**.

Licensed under the [Apache License 2.0](LICENSE).

<div align="center">
  <p>
    <a href="https://github.com/BaranziniLab/biorouter/releases">Download</a> ·
    <a href="docs/getting-started/installation.md">Setup Guide</a> ·
    <a href="https://github.com/BaranziniLab/biorouter/issues">Report an Issue</a> ·
    <a href="mailto:wanjun.gu@ucsf.edu">Contact</a>
  </p>
</div>
