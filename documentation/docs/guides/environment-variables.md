---
sidebar_position: 95
title: Environment Variables
sidebar_label: Environment Variables
---

biorouter supports various environment variables that allow you to customize its behavior. This guide provides a comprehensive list of available environment variables grouped by their functionality.

## Model Configuration

These variables control the [language models](/docs/getting-started/providers) and their behavior.

### Basic Provider Configuration

These are the minimum required variables to get started with biorouter.

| Variable | Purpose | Values | Default |
|----------|---------|---------|---------|
| `BIOROUTER_PROVIDER` | Specifies the LLM provider to use | [See available providers](/docs/getting-started/providers#available-providers) | None (must be [configured](/docs/getting-started/providers#configure-provider-and-model)) |
| `BIOROUTER_MODEL` | Specifies which model to use from the provider | Model name (e.g., "gpt-4", "claude-sonnet-4-20250514") | None (must be [configured](/docs/getting-started/providers#configure-provider-and-model)) |
| `BIOROUTER_TEMPERATURE` | Sets the [temperature](https://medium.com/@kelseyywang/a-comprehensive-guide-to-llm-temperature-%EF%B8%8F-363a40bbc91f) for model responses | Float between 0.0 and 1.0 | Model-specific default |

**Examples**

```bash
# Basic model configuration
export BIOROUTER_PROVIDER="anthropic"
export BIOROUTER_MODEL="claude-sonnet-4-20250514"
export BIOROUTER_TEMPERATURE=0.7
```

### Advanced Provider Configuration

These variables are needed when using custom endpoints, enterprise deployments, or specific provider implementations.

| Variable | Purpose | Values | Default |
|----------|---------|---------|---------|
| `BIOROUTER_PROVIDER__TYPE` | The specific type/implementation of the provider | [See available providers](/docs/getting-started/providers#available-providers) | Derived from BIOROUTER_PROVIDER |
| `BIOROUTER_PROVIDER__HOST` | Custom API endpoint for the provider | URL (e.g., "https://api.openai.com") | Provider-specific default |
| `BIOROUTER_PROVIDER__API_KEY` | Authentication key for the provider | API key string | None |

**Examples**

```bash
# Advanced provider configuration
export BIOROUTER_PROVIDER__TYPE="anthropic"
export BIOROUTER_PROVIDER__HOST="https://api.anthropic.com"
export BIOROUTER_PROVIDER__API_KEY="your-api-key-here"
```

### Lead/Worker Model Configuration

These variables configure a [lead/worker model pattern](/docs/tutorials/lead-worker) where a powerful lead model handles initial planning and complex reasoning, then switches to a faster/cheaper worker model for execution. The switch happens automatically based on your settings.

| Variable | Purpose | Values | Default |
|----------|---------|---------|---------|
| `BIOROUTER_LEAD_MODEL` | **Required to enable lead mode.** Name of the lead model | Model name (e.g., "gpt-4o", "claude-sonnet-4-20250514") | None |
| `BIOROUTER_LEAD_PROVIDER` | Provider for the lead model | [See available providers](/docs/getting-started/providers#available-providers) | Falls back to `BIOROUTER_PROVIDER` |
| `BIOROUTER_LEAD_TURNS` | Number of initial turns using the lead model before switching to the worker model | Integer | 3 |
| `BIOROUTER_LEAD_FAILURE_THRESHOLD` | Consecutive failures before fallback to the lead model | Integer | 2 |
| `BIOROUTER_LEAD_FALLBACK_TURNS` | Number of turns to use the lead model in fallback mode | Integer | 2 |

A _turn_ is one complete prompt-response interaction. Here's how it works with the default settings:
- Use the lead model for the first 3 turns
- Use the worker model starting on the 4th turn
- Fallback to the lead model if the worker model struggles for 2 consecutive turns
- Use the lead model for 2 turns and then switch back to the worker model

The lead model and worker model names are displayed at the start of the biorouter CLI session. If you don't export a `BIOROUTER_MODEL` for your session, the worker model defaults to the `BIOROUTER_MODEL` in your [configuration file](/docs/guides/config-files).

**Examples**

```bash
# Basic lead/worker setup
export BIOROUTER_LEAD_MODEL="o4"

# Advanced lead/worker configuration
export BIOROUTER_LEAD_MODEL="claude4-opus"
export BIOROUTER_LEAD_PROVIDER="anthropic"
export BIOROUTER_LEAD_TURNS=5
export BIOROUTER_LEAD_FAILURE_THRESHOLD=3
export BIOROUTER_LEAD_FALLBACK_TURNS=2
```

### Planning Mode Configuration

These variables control biorouter's [planning functionality](/docs/guides/creating-plans).

| Variable | Purpose | Values | Default |
|----------|---------|---------|---------|
| `BIOROUTER_PLANNER_PROVIDER` | Specifies which provider to use for planning mode | [See available providers](/docs/getting-started/providers#available-providers) | Falls back to BIOROUTER_PROVIDER |
| `BIOROUTER_PLANNER_MODEL` | Specifies which model to use for planning mode | Model name (e.g., "gpt-4", "claude-sonnet-4-20250514")| Falls back to BIOROUTER_MODEL |

**Examples**

```bash
# Planning mode with different model
export BIOROUTER_PLANNER_PROVIDER="openai"
export BIOROUTER_PLANNER_MODEL="gpt-4"
```

### Provider Retries

Configurable retry parameters for LLM providers. 

#### AWS Bedrock

| Variable | Purpose | Default |
|---------------------|-------------|---------|
| `BEDROCK_MAX_RETRIES` | The max number of retry attempts before giving up | 6 |
| `BEDROCK_INITIAL_RETRY_INTERVAL_MS` | How long to wait (in milliseconds) before the first retry | 2000 |
| `BEDROCK_BACKOFF_MULTIPLIER` | The factor by which the retry interval increases after each attempt | 2 (doubles every time) |
| `BEDROCK_MAX_RETRY_INTERVAL_MS` | The cap on the retry interval in milliseconds |  120000 |

**Examples**

```bash
export BEDROCK_MAX_RETRIES=10                    # 10 retry attempts
export BEDROCK_INITIAL_RETRY_INTERVAL_MS=1000    # start with 1 second before first retry
export BEDROCK_BACKOFF_MULTIPLIER=3              # each retry waits 3x longer than the previous
export BEDROCK_MAX_RETRY_INTERVAL_MS=300000      # cap the maximum retry delay at 5 min
```

#### Databricks

| Variable | Purpose | Default |
|---------------------|-------------|---------|
| `DATABRICKS_MAX_RETRIES` | The max number of retry attempts before giving up | 3 |
| `DATABRICKS_INITIAL_RETRY_INTERVAL_MS` | How long to wait (in milliseconds) before the first retry | 1000 |
| `DATABRICKS_BACKOFF_MULTIPLIER` | The factor by which the retry interval increases after each attempt | 2 (doubles every time) |
| `DATABRICKS_MAX_RETRY_INTERVAL_MS` | The cap on the retry interval in milliseconds |  30000 |

**Examples**

```bash
export DATABRICKS_MAX_RETRIES=5                      # 5 retry attempts
export DATABRICKS_INITIAL_RETRY_INTERVAL_MS=500      # start with 0.5 second before first retry
export DATABRICKS_BACKOFF_MULTIPLIER=2               # each retry waits 2x longer than the previous
export DATABRICKS_MAX_RETRY_INTERVAL_MS=60000        # cap the maximum retry delay at 1 min
```


## Session Management

These variables control how biorouter manages conversation sessions and context.

| Variable | Purpose | Values | Default |
|----------|---------|---------|---------|
| `BIOROUTER_CONTEXT_STRATEGY` | Controls how biorouter handles context limit exceeded situations | "summarize", "truncate", "clear", "prompt" | "prompt" (interactive), "summarize" (headless) |
| `BIOROUTER_MAX_TURNS` | [Maximum number of turns](/docs/guides/sessions/smart-context-management#maximum-turns) allowed without user input | Integer (e.g., 10, 50, 100) | 1000 |
| `BIOROUTER_SUBAGENT_MAX_TURNS` | Sets the maximum turns allowed for a [subagent](/docs/guides/subagents) to complete before timeout | Integer (e.g., 25) | 25 |
| `CONTEXT_FILE_NAMES` | Specifies custom filenames for [hint/context files](/docs/guides/context-engineering/using-biorouterhints#custom-context-files) | JSON array of strings (e.g., `["CLAUDE.md", ".biorouterhints"]`) | `[".biorouterhints"]` |
| `BIOROUTER_CLI_THEME` | [Theme](/docs/guides/biorouter-cli-commands#themes) for CLI response  markdown | "light", "dark", "ansi" | "dark" |
| `BIOROUTER_RANDOM_THINKING_MESSAGES` | Controls whether to show amusing random messages during processing | "true", "false" | "true" |
| `BIOROUTER_CLI_SHOW_COST` | Toggles display of model cost estimates in CLI output | "true", "1" (case insensitive) to enable | false |
| `BIOROUTER_AUTO_COMPACT_THRESHOLD` | Set the percentage threshold at which biorouter [automatically summarizes your session](/docs/guides/sessions/smart-context-management#automatic-compaction). | Float between 0.0 and 1.0 (disabled at 0.0) | 0.8 |

**Examples**

```bash
# Automatically summarize when context limit is reached
export BIOROUTER_CONTEXT_STRATEGY=summarize

# Always prompt user to choose (default for interactive mode)
export BIOROUTER_CONTEXT_STRATEGY=prompt

# Set a low limit for step-by-step control
export BIOROUTER_MAX_TURNS=5

# Set a moderate limit for controlled automation
export BIOROUTER_MAX_TURNS=25

# Set a reasonable limit for production
export BIOROUTER_MAX_TURNS=100

# Customize subagent turn limit
export BIOROUTER_SUBAGENT_MAX_TURNS=50

# Use multiple context files
export CONTEXT_FILE_NAMES='["CLAUDE.md", ".biorouterhints", ".cursorrules", "project_rules.txt"]'

# Set the ANSI theme for the session
export BIOROUTER_CLI_THEME=ansi

# Disable random thinking messages for less distraction
export BIOROUTER_RANDOM_THINKING_MESSAGES=false

# Enable model cost display in CLI
export BIOROUTER_CLI_SHOW_COST=true

# Automatically compact sessions when 60% of available tokens are used
export BIOROUTER_AUTO_COMPACT_THRESHOLD=0.6
```

### Model Context Limit Overrides

These variables allow you to override the default context window size (token limit) for your models. This is particularly useful when using [LiteLLM proxies](https://docs.litellm.ai/docs/providers/litellm_proxy) or custom models that don't match biorouter's predefined model patterns.

| Variable | Purpose | Values | Default |
|----------|---------|---------|---------|
| `BIOROUTER_CONTEXT_LIMIT` | Override context limit for the main model | Integer (number of tokens) | Model-specific default or 128,000 |
| `BIOROUTER_LEAD_CONTEXT_LIMIT` | Override context limit for the lead model in [lead/worker mode](/docs/tutorials/lead-worker) | Integer (number of tokens) | Falls back to `BIOROUTER_CONTEXT_LIMIT` or model default |
| `BIOROUTER_WORKER_CONTEXT_LIMIT` | Override context limit for the worker model in lead/worker mode | Integer (number of tokens) | Falls back to `BIOROUTER_CONTEXT_LIMIT` or model default |
| `BIOROUTER_PLANNER_CONTEXT_LIMIT` | Override context limit for the [planner model](/docs/guides/creating-plans) | Integer (number of tokens) | Falls back to `BIOROUTER_CONTEXT_LIMIT` or model default |

**Examples**

```bash
# Set context limit for main model (useful for LiteLLM proxies)
export BIOROUTER_CONTEXT_LIMIT=200000

# Set different context limits for lead/worker models
export BIOROUTER_LEAD_CONTEXT_LIMIT=500000   # Large context for planning
export BIOROUTER_WORKER_CONTEXT_LIMIT=128000 # Smaller context for execution

# Set context limit for planner
export BIOROUTER_PLANNER_CONTEXT_LIMIT=1000000
```

For more details and examples, see [Model Context Limit Overrides](/docs/guides/sessions/smart-context-management#model-context-limit-overrides).

## Tool Configuration

These variables control how biorouter handles [tool execution](/docs/guides/biorouter-permissions) and [tool management](/docs/guides/managing-tools/).

| Variable | Purpose | Values | Default |
|----------|---------|---------|---------|
| `BIOROUTER_MODE` | Controls how biorouter handles tool execution | "auto", "approve", "chat", "smart_approve" | "smart_approve" |
| `BIOROUTER_TOOLSHIM` | Enables/disables tool call interpretation | "1", "true" (case insensitive) to enable | false |
| `BIOROUTER_TOOLSHIM_OLLAMA_MODEL` | Specifies the model for [tool call interpretation](/docs/experimental/ollama) | Model name (e.g. llama3.2, qwen2.5) | System default |
| `BIOROUTER_CLI_MIN_PRIORITY` | Controls verbosity of [tool output](/docs/guides/managing-tools/adjust-tool-output) | Float between 0.0 and 1.0 | 0.0 |
| `BIOROUTER_CLI_TOOL_PARAMS_TRUNCATION_MAX_LENGTH` | Maximum length for tool parameter values before truncation in CLI output (not in debug mode) | Integer | 40 |
| `BIOROUTER_DEBUG` | Enables debug mode to show full tool parameters without truncation | "1", "true" (case insensitive) to enable | false |
| `BIOROUTER_SEARCH_PATHS` | Additional directories to search for executables when running extensions | JSON array of paths (e.g., `["/usr/local/bin", "~/custom/bin"]`) | System PATH only | No |

**Examples**

```bash
# Enable tool interpretation
export BIOROUTER_TOOLSHIM=true
export BIOROUTER_TOOLSHIM_OLLAMA_MODEL=llama3.2
export BIOROUTER_MODE="auto"
export BIOROUTER_CLI_MIN_PRIORITY=0.2  # Show only medium and high importance output
export BIOROUTER_CLI_TOOL_PARAMS_MAX_LENGTH=100  # Show up to 100 characters for tool parameters in CLI output

# Add custom tool directories for extensions
export BIOROUTER_SEARCH_PATHS='["/usr/local/bin", "~/custom/tools", "/opt/homebrew/bin"]'
```

These paths are prepended to the system PATH when extensions execute commands, ensuring your custom tools are found without modifying your global PATH.

### Enhanced Code Editing

These variables configure [AI-powered code editing](/docs/guides/enhanced-code-editing) for the Developer extension's `str_replace` tool. All three variables must be set and non-empty for the feature to activate.

| Variable | Purpose | Values | Default |
|----------|---------|---------|---------|
| `BIOROUTER_EDITOR_API_KEY` | API key for the code editing model | API key string | None |
| `BIOROUTER_EDITOR_HOST` | API endpoint for the code editing model | URL (e.g., "https://api.openai.com/v1") | None |
| `BIOROUTER_EDITOR_MODEL` | Model to use for code editing | Model name (e.g., "gpt-4o", "claude-sonnet-4") | None |

**Examples**

This feature works with any OpenAI-compatible API endpoint, for example:

```bash
# OpenAI configuration
export BIOROUTER_EDITOR_API_KEY="sk-..."
export BIOROUTER_EDITOR_HOST="https://api.openai.com/v1"
export BIOROUTER_EDITOR_MODEL="gpt-4o"

# Anthropic configuration (via OpenAI-compatible proxy)
export BIOROUTER_EDITOR_API_KEY="sk-ant-..."
export BIOROUTER_EDITOR_HOST="https://api.anthropic.com/v1"
export BIOROUTER_EDITOR_MODEL="claude-sonnet-4-20250514"

# Local model configuration
export BIOROUTER_EDITOR_API_KEY="your-key"
export BIOROUTER_EDITOR_HOST="http://localhost:8000/v1"
export BIOROUTER_EDITOR_MODEL="your-model"
```

## Security Configuration

These variables control security related features.

| Variable | Purpose | Values | Default |
|----------|---------|---------|---------|
| `BIOROUTER_ALLOWLIST` | Controls which extensions can be loaded | URL for [allowed extensions](/docs/guides/allowlist) list | Unset |
| `BIOROUTER_DISABLE_KEYRING` | Disables the system keyring for secret storage | Set to any value (e.g., "1", "true", "yes") to disable. The actual value doesn't matter, only whether the variable is set. | Unset (keyring enabled) |

:::tip
When the keyring is disabled, secrets are stored here:

* macOS/Linux: `~/.config/BioRouter/secrets.yaml`
* Windows: `%APPDATA%\Block\biorouter\config\secrets.yaml`
:::

## Observability

Beyond biorouter's built-in [logging system](/docs/guides/logs), you can export telemetry to external observability platforms for advanced monitoring, performance analysis, and production insights.

### OpenTelemetry Protocol (OTLP)

Configure biorouter to export traces and metrics to any OTLP-compatible observability platform. 
OTLP is the standard protocol for sending telemetry collected by [OpenTelemetry](https://opentelemetry.io/docs/). When configured, biorouter exports telemetry asynchronously and flushes on exit.

| Variable | Purpose | Values | Default |
|----------|---------|--------|---------|
| `OTEL_EXPORTER_OTLP_ENDPOINT` | OTLP endpoint URL | URL (e.g., `http://localhost:4318`) | None |
| `OTEL_EXPORTER_OTLP_TIMEOUT` | Export timeout in milliseconds | Integer (ms) | `10000` |

**When to use OTLP:**
- Diagnosing slow tool execution or LLM response times
- Understanding intermittent failures across multiple sessions
- Monitoring biorouter performance in production or CI/CD environments
- Tracking usage patterns, costs, and resource consumption over time
- Setting up alerts for performance degradation or high error rates

**Example:**
```bash
export OTEL_EXPORTER_OTLP_ENDPOINT="http://localhost:4318"
export OTEL_EXPORTER_OTLP_TIMEOUT=10000
```

### Langfuse Integration

These variables configure the [Langfuse integration for observability](/docs/tutorials/langfuse).

| Variable | Purpose | Values | Default |
|----------|---------|---------|---------|
| `LANGFUSE_PUBLIC_KEY` | Public key for Langfuse integration | String | None |
| `LANGFUSE_SECRET_KEY` | Secret key for Langfuse integration | String | None |
| `LANGFUSE_URL` | Custom URL for Langfuse service | URL String | Default Langfuse URL |
| `LANGFUSE_INIT_PROJECT_PUBLIC_KEY` | Alternative public key for Langfuse | String | None |
| `LANGFUSE_INIT_PROJECT_SECRET_KEY` | Alternative secret key for Langfuse | String | None |

## Recipe Configuration

These variables control recipe discovery and management.

| Variable | Purpose | Values | Default |
|----------|---------|---------|---------|
| `BIOROUTER_RECIPE_PATH` | Additional directories to search for recipes | Colon-separated paths on Unix, semicolon-separated on Windows | None |
| `BIOROUTER_RECIPE_GITHUB_REPO` | GitHub repository to search for recipes | Format: "owner/repo" (e.g., "BaranziniLab/biorouter-recipes") | None |
| `BIOROUTER_RECIPE_RETRY_TIMEOUT_SECONDS` | Global timeout for recipe success check commands | Integer (seconds) | Recipe-specific default |
| `BIOROUTER_RECIPE_ON_FAILURE_TIMEOUT_SECONDS` | Global timeout for recipe on_failure commands | Integer (seconds) | Recipe-specific default |

**Examples**

```bash
# Add custom recipe directories
export BIOROUTER_RECIPE_PATH="/path/to/my/recipes:/path/to/team/recipes"

# Configure GitHub recipe repository
export BIOROUTER_RECIPE_GITHUB_REPO="myorg/biorouter-recipes"

# Set global recipe timeouts
export BIOROUTER_RECIPE_RETRY_TIMEOUT_SECONDS=300
export BIOROUTER_RECIPE_ON_FAILURE_TIMEOUT_SECONDS=60
```

## Experimental Features

These variables enable experimental features that are in active development. These may change or be removed in future releases. Use with caution in production environments.

| Variable | Purpose | Values | Default |
|----------|---------|---------|---------|
| `ALPHA_FEATURES` | Enables experimental alpha features&mdash;check the feature docs to see if this flag is required | "true", "1" (case insensitive) to enable | false |

**Examples**

```bash
# Enable alpha features
export ALPHA_FEATURES=true

# Or enable for a single session
ALPHA_FEATURES=true biorouter session
```

## Development & Testing

These variables are primarily used for development, testing, and debugging biorouter itself.

| Variable | Purpose | Values | Default |
|----------|---------|---------|---------|
| `BIOROUTER_PATH_ROOT` | Override the root directory for all biorouter data, config, and state files | Absolute path to directory | Platform-specific defaults |

**Default locations:**
- macOS: `~/Library/Application Support/Block/BioRouter/`
- Linux: `~/.local/share/BioRouter/`
- Windows: `%APPDATA%\Block\biorouter\`

When set, biorouter creates `config/`, `data/`, and `state/` subdirectories under the specified path. Useful for isolating test environments, running multiple configurations, or CI/CD pipelines.

**Examples**

```bash
# Temporary test environment
export BIOROUTER_PATH_ROOT="/tmp/biorouter-test"

# Isolated environment for a single command
BIOROUTER_PATH_ROOT="/tmp/biorouter-isolated" biorouter run --recipe my-recipe.yaml

# CI/CD usage
BIOROUTER_PATH_ROOT="$(mktemp -d)" biorouter run --recipe integration-test.yaml

# Use with developer tools
BIOROUTER_PATH_ROOT="/tmp/biorouter-test" ./scripts/biorouter-db-helper.sh status
```

## Variables Controlled by biorouter

These variables are automatically set by biorouter during command execution.

| Variable | Purpose | Values | Default |
|----------|---------|---------|---------|
| `BIOROUTER_TERMINAL` | Indicates that a command is being executed by biorouter, enables customizing shell behavior | "1" when set | Unset |

### Customizing Shell Behavior

Sometimes you want biorouter to use different commands or have different shell behavior than your normal terminal usage. For example, you might want biorouter to use a different tool, prevent biorouter from running `git commit`, or block long-running development servers that could hang the AI agent. This is most useful when using biorouter CLI, where shell commands are executed directly in your terminal environment.

**How it works:**
1. When biorouter runs commands, `BIOROUTER_TERMINAL` is automatically set to "1"
2. Your shell configuration can detect this and change behavior while keeping your normal terminal usage unchanged

**Examples:**

```bash
# In ~/.zshenv (for zsh users) or ~/.bashrc (for bash users)

# Block git commit when run by biorouter
if [[ -n "$BIOROUTER_TERMINAL" ]]; then
  git() {
    if [[ "$1" == "commit" ]]; then
      echo "❌ BLOCKED: git commit is not allowed when run by biorouter"
      return 1
    fi
    command git "$@"
  }
fi
```

```bash
# Guide biorouter toward better tool choices
if [[ -n "$BIOROUTER_TERMINAL" ]]; then
  alias find="echo 'Use rg instead: rg --files | rg <pattern> for filenames, or rg <pattern> for content search'"
fi
```

## Notes

- Environment variables take precedence over configuration files.
- For security-sensitive variables (like API keys), consider using the system keyring instead of environment variables.
- Some variables may require restarting biorouter to take effect.
- When using the planning mode, if planner-specific variables are not set, biorouter will fall back to the main model configuration.

