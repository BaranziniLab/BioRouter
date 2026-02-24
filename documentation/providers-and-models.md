# UCSF BioRouter — LLM Providers and Models

BioRouter connects to a wide range of LLM providers — commercial cloud APIs, institution-hosted services, and local models. You select and configure providers through the Provider Settings panel in the app (Settings > Models > Providers).

**UCSF users:** For institution-managed access, start with **Azure OpenAI** (UCSF ChatGPT) or **Amazon Bedrock** (UCSF-hosted Anthropic). For fully local, air-gapped inference, use **Ollama**.

---

## Provider Configuration Panel

Providers are managed in Settings > Models. Each provider card shows:
- Provider name and status (configured / not configured)
- A "Configure" button to enter API keys or credentials
- A "Launch" button to switch to that provider and choose a model

The panel ordering reflects recommended providers for UCSF users:

1. Azure OpenAI
2. Amazon Bedrock
3. Anthropic
4. OpenAI
5. Google Gemini
6. (all others alphabetically)

You can also add fully custom providers (e.g. any OpenAI-compatible endpoint) via the "Add Custom Provider" card.

---

## Supported Providers

### Azure OpenAI

**Environment variable:** `AZURE_OPENAI_API_KEY` (or Azure credential chain)

Start from this profile to access UCSF-hosted ChatGPT models. Uses Azure credential chain by default, making it compatible with institutional single sign-on.

Default model: `gpt-5-2025-08-07`

Available models include:
- gpt-4o, gpt-4o-mini, gpt-4

---

### Amazon Bedrock

**Environment variables:** `AWS_PROFILE`, `AWS_REGION` (or standard AWS credential chain)

Start from this profile to access UCSF-hosted Anthropic models. Supports AWS SSO profiles — run `aws sso login --profile <profile-name>` before using.

Default model: `us.anthropic.claude-sonnet-4-5-20250929-v1:0`

Available models include:
- Claude Sonnet 4.5 (via Bedrock)
- Multiple Claude 3/4 variants

---

### Anthropic

**Environment variable:** `ANTHROPIC_API_KEY`

Direct API access to Anthropic's Claude models.

Default model: `claude-sonnet-4-5`

Available models include:
- claude-opus-4-5
- claude-sonnet-4-5
- claude-haiku-4-5

---

### OpenAI

**Environment variable:** `OPENAI_API_KEY`

Direct API access to OpenAI models.

Default model: `gpt-4o`

Available models include:
- gpt-4o, gpt-4o-mini
- gpt-4.1
- o1, o3

Optional configuration: `OPENAI_ORG_ID`, `OPENAI_PROJECT_ID`

---

### Google Gemini

**Environment variable:** `GOOGLE_API_KEY`

Direct API access to Google's Gemini models.

Default model: `gemini-2.5-pro`

Available models include:
- gemini-2.5-pro
- gemini-2.5-flash
- gemini-2.0-flash variants

---

### GCP Vertex AI

**Authentication:** Service account or application default credentials

Runs Google and Anthropic models through Google Cloud's Vertex AI infrastructure.

Default model: `gemini-2.5-flash`

---

### Databricks

**Environment variable:** `DATABRICKS_HOST`, `DATABRICKS_TOKEN`

Access models through Databricks. Supports OAuth.

Default model: `databricks-claude-sonnet-4`

Available models include:
- Claude variants
- Llama models
- DBRX Instruct

---

### Snowflake Cortex

**Environment variable:** Snowflake credentials

Access Claude and other models through Snowflake's Cortex integration.

Default model: `claude-sonnet-4-5`

---

### Ollama (Local)

**No API key required** — runs fully on your machine.

Use Ollama for completely local, private inference. No data leaves your device.

Default model: `qwen3`

Available models include:
- qwen3, qwen3-coder variants
- Any model available in the Ollama library

To use: install Ollama (https://ollama.com), pull a model (`ollama pull qwen3`), then configure BioRouter to use the Ollama provider. The endpoint defaults to `http://localhost:11434`.

---

### OpenRouter

**Environment variable:** `OPENROUTER_API_KEY`

A proxy service that provides access to many providers through a single API.

Default model: `anthropic/claude-sonnet-4`

Available models include access to Anthropic, Google, Deepseek, Qwen, and many others.

---

### LiteLLM

**Environment variable:** depends on backend

A proxy/gateway supporting many providers through a unified OpenAI-compatible interface.

Default model: `gpt-4o-mini`

---

### Venice AI

**Environment variable:** `VENICE_API_KEY`

Privacy-focused inference provider.

Default model: `llama-3.3-70b`

Available models include:
- Llama 3.2 / 3.3 variants
- Mistral variants

---

### GitHub Copilot

**Authentication:** Device code OAuth flow

Access GPT-4.1, Claude, Gemini, and Grok models through GitHub Copilot infrastructure.

Default model: `gpt-4.1`

---

### X.AI (Grok)

**Environment variable:** `XAI_API_KEY`

Access Grok models from xAI.

Default model: `grok-code-fast-1`

---

### AWS SageMaker TGI

**Authentication:** AWS credential chain

Run models deployed on AWS SageMaker endpoints using TGI (Text Generation Inference).

---

### Custom / Declarative Providers

Any OpenAI-compatible endpoint can be added as a custom provider through the "Add Custom Provider" card. You specify:
- Display name
- API base URL
- API key environment variable name
- Model list
- Streaming support

Custom providers are stored in `~/.config/biorouter/config.yaml` and available in all future sessions.

---

## Switching Providers and Models

**Desktop:** Settings > Models > select a provider card > Configure or Launch > choose a model.

**CLI:**
```sh
biorouter configure
# Select "Configure Providers"
```

You can also specify provider and model on a per-session or per-recipe basis without changing your default configuration.

---

## Multi-Model Orchestration

BioRouter supports routing tasks across multiple models:

- **Lead/Worker pattern** — A lead model orchestrates tasks and delegates sub-tasks to worker models (potentially different providers).
- **Per-recipe model override** — A recipe can specify `settings.biorouter_provider` and `settings.biorouter_model` to use a different model for that recipe without changing your default.
- **Per-session override** — The CLI supports `--provider` and `--model` flags when starting a session.
