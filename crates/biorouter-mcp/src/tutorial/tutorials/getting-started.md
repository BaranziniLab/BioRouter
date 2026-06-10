# Getting Started with Biorouter: set up a model provider, learn the interface, and run your first session

## Purpose

Walk a new user through their first 15 minutes with Biorouter: connecting an
LLM provider, understanding the desktop app and CLI, and having a productive
first conversation. Adapt depth to the user — ask early whether they have used
AI agent tools before.

## Phase 1: Check the current setup

1. Ask the user whether they are in the desktop app or the CLI.
2. Determine whether a provider is already configured. If you can converse,
   one is — tell the user which provider/model is active if they ask
   (Settings → Models in the app, or `biorouter models current` in a terminal).
3. If they want to change or add a provider, continue to Phase 2; otherwise
   skip to Phase 3.

## Phase 2: Configure a provider and model

Biorouter supports commercial providers (Anthropic, OpenAI, Azure, AWS
Bedrock, GCP Vertex AI, Google, OpenRouter, GitHub Copilot, xAI, …),
institution-hosted providers (e.g. UCSF Versa), and local models via Ollama.

- **Desktop:** Settings (bottom of the left sidebar) → Providers → choose a
  provider and enter its API key, then pick a default model under Models.
- **CLI:** run `biorouter configure` for the interactive flow, or
  `biorouter models set --provider <provider> --model <model>`.

Notes to share when relevant:
- API keys go into the OS credential store by default (macOS Keychain,
  Windows Credential Manager, Linux Secret Service). On macOS, choosing
  "Always Allow" at the Keychain prompt prevents repeated prompts.
- Local/no-cost option: install Ollama, pull a model, then select the
  `ollama` provider — no API key needed.
- Any provider key can also be supplied as an environment variable (e.g.
  `OPENAI_API_KEY`), which takes precedence.

## Phase 3: Tour the interface

If in the desktop app, briefly orient the user around the left sidebar:

- **Chat** — the conversation you are in now.
- **History** — every past session, searchable and resumable.
- **Workflows** — saved, reusable automation definitions.
- **Scheduler** — run workflows automatically on a schedule.
- **Extensions** — toggle built-in tools (Developer, Memory, Knowledge, …)
  and add third-party MCP extensions from <http://biorouter.ucsf.edu/baam>.
- **Skills** — reusable instruction sets that shape how the agent works.
- **Knowledge** — personal knowledge bases built from papers and documents.
- **Apps** — launchable MCP apps.
- **Settings** — providers, models, permissions, appearance.

If in the CLI, introduce the core commands instead: `biorouter session`
(interactive chat), `biorouter run --text "..."` (headless one-shot),
`biorouter configure`, `biorouter models`, `biorouter workflow`,
`biorouter schedule`, `biorouter skill`.

## Phase 4: A productive first task

Offer to do something real, scaled to the user's interests. Good first tasks:

- Summarize a paper: have them paste a URL or drop a PDF, optionally storing
  it into a knowledge base (offer the knowledge-bases tutorial).
- Explore a dataset: with the Developer extension enabled, load a CSV and
  produce quick statistics and a chart (Auto Visualiser renders it inline).
- Automate something small: draft a workflow from this conversation and save
  it for reuse (offer the create-workflows tutorial).

## Notes for the agent

- One step at a time; confirm success before moving on.
- Never echo API keys back to the user, and don't ask them to paste keys into
  chat — point them at Settings or `biorouter configure`, which store keys in
  the secrets backend.
- If a tool you need is missing, check the Extensions page state before
  assuming a bug.
- Suggest the other tutorials (knowledge-bases, create-workflows,
  schedule-automations, create-skills, build-mcp-extension) where they fit
  the user's goals, not all at once.
