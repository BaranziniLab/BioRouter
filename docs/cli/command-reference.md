# biorouter CLI command reference

> **What this is.** The reference for every `biorouter` command-line subcommand and its flags, plus the slash commands, themes, and keyboard shortcuts available inside an interactive session.
> **Status:** Current.
> **Audience:** end users

biorouter ships a command-line interface (CLI) for managing sessions, configuration, and extensions, and for running tasks headlessly. This page lists the commands you can run from your shell, then the controls available once you are inside an interactive session. For the manual verification script that exercises this same surface, see the [CLI QA checklist](qa-checklist.md).

## Contents

- [Flag naming conventions](#flag-naming-conventions)
- [Core commands](#core-commands)
- [Session management](#session-management)
- [Task execution](#task-execution)
- [Project management](#project-management)
- [Interface](#interface)
- [Terminal integration](#terminal-integration)
- [Interactive session features](#interactive-session-features)
- [Navigation and controls](#navigation-and-controls)

## Flag naming conventions

The biorouter CLI follows consistent patterns for flag naming to make commands intuitive and predictable:

- **`--session-id`**: Used for session identifiers (e.g., `20251108_1`)
- **`--schedule-id`**: Used for schedule job identifiers (e.g., `daily-report`)
- **`-n, --name`**: Used for human-readable names
- **`--path`**: Used for file paths (legacy support)
- **`-o, --output`**: Used for output file paths
- **`-r, --resume` or `-r, --regex`**: Context-dependent (resume for sessions, regex for filters)
- **`-v, --verbose`**: Used for verbose output
- **`-l, --limit`**: Used for limiting result counts
- **`-f, --format`**: Used for specifying output formats
- **`-w, --working_dir`**: Used for working directory filters

## Core commands

### help

Display the help menu.

**Usage:**

```bash
biorouter --help
```

### configure

Configure biorouter settings - providers, extensions, etc.

**Usage:**

```bash
biorouter configure
```

### info [options]

Shows biorouter information, including the version, configuration file location, session storage, and logs.

**Options:**

- **`-v, --verbose`**: Show detailed configuration settings, including environment variables and enabled extensions

**Usage:**

```bash
biorouter info
```

### models

Inspect and update model/provider configuration from the CLI.

**Usage:**

```bash
# Show the configured provider and model
biorouter models current

# List available providers
biorouter models providers

# List known models for a provider
biorouter models list openai
biorouter models list ollama --format json

# Set the default provider and model
biorouter models set --provider openai --model gpt-5.5
```

### version

Check the current biorouter version you have installed.

**Usage:**

```bash
biorouter --version
```

## Session management

> **Note.** biorouter stores sessions in a SQLite database (`sessions.db`) rather than individual `.jsonl` files, a change introduced in version 1.10.0. Sessions that predate the change are automatically imported into the database. Legacy `.jsonl` files remain on disk but are no longer managed by biorouter.

### session [options]

Start or resume interactive chat sessions.

**Basic Options:**

- **`--session-id <session_id>`**: Specify a session by its ID (e.g., '20251108_1')
- **`-n, --name <name>`**: Give the session a name
- **`--path <path>`**: Legacy parameter for specifying session by file path
- **`-r, --resume`**: Resume a previous session
- **`--history`**: Show previous messages when resuming a session
- **`--debug`**: Enable debug mode to output complete tool responses, detailed parameter values, and full file paths
- **`--max-tool-repetitions <NUMBER>`**: Set the maximum number of times the same tool can be called consecutively with identical parameters. Helps prevent infinite loops.
- **`--max-turns <NUMBER>`**: Set the maximum number of turns allowed without user input (default: 1000)

**Extension Options:**

- **`--with-extension <command>`**: Add stdio extensions
- **`--with-streamable-http-extension <url>`**: Add remote extensions over Streamable HTTP
- **`--with-builtin <id>`**: Enable built-in extensions (e.g., 'developer', 'computercontroller')

**Usage:**

```bash
# Start a basic session
biorouter session -n my-project

# Resume a previous session
biorouter session --resume -n my-project
biorouter session --resume --session-id 20251108_2
biorouter session --resume --path ./session.json    # exported session
biorouter session --resume --path ./session.jsonl   # legacy session storage

# Start with extensions
biorouter session --with-extension "npx -y @modelcontextprotocol/server-memory"
biorouter session --with-builtin developer
biorouter session --with-streamable-http-extension "http://localhost:8080/mcp"

# Advanced: Mix multiple extension types
biorouter session \
  --with-extension "echo hello" \
  --with-streamable-http-extension "http://localhost:8080/mcp" \
  --with-builtin "developer"

# Control session behavior
biorouter session -n my-session --debug --max-turns 25
```

### session list [options]

List all saved sessions.

**Options:**

- **`-f, --format <format>`**: Specify output format (`text` or `json`). Default is `text`
- **`--ascending`**: Sort sessions by date in ascending order (oldest first)
- **`-w, --working_dir <path>`**: Filter sessions by working directory
- **`-l, --limit <number>`**: Limit the number of results

**Usage:**

```bash
# List all sessions in text format (default)
biorouter session list

# List sessions in JSON format
biorouter session list --format json

# Sort sessions by date in ascending order
biorouter session list --ascending

# Filter sessions by working directory
biorouter session list -w ~/projects/myapp

# List only the 10 most recent sessions
biorouter session list --limit 10
```

### session remove [options]

Remove one or more saved sessions.

**Options:**

- **`--session-id <session_id>`**: Remove a specific session by its session ID
- **`-n, --name <name>`**: Remove a specific session by its name
- **`-r, --regex <pattern>`**: Remove sessions matching a regex pattern
- **`--path <path>`**: Remove a specific session by its file path (legacy)

**Usage:**

```bash
# Interactive removal (prompts you to choose sessions)
biorouter session remove

# Remove a specific session by ID
biorouter session remove --session-id 20251108_3

# Remove a specific session by name
biorouter session remove -n my-project

# Remove all sessions starting with "project-"
biorouter session remove -r "project-.*"

# Remove all sessions containing "migration"
biorouter session remove -r ".*migration.*"
```

> **Caution.** Session removal is permanent and cannot be undone. biorouter will show which sessions will be removed and ask for confirmation before deleting.

### session export [options]

Export sessions in different formats for backup, sharing, migration, or documentation purposes.

**Options:**

- **`--session-id <session_id>`**: Export a specific session by ID
- **`-n, --name <name>`**: Export a specific session by name
- **`--path <path>`**: Export a specific session by file path (legacy)
- **`-o, --output <file>`**: Save exported content to a file (default: stdout)
- **`--format <format>`**: Output format: `markdown`, `json`, `yaml`. Default is `markdown`

**Export Formats:**

- **`json`**: Complete session backup preserving all data including conversation history, metadata, and settings
- **`yaml`**: Complete session backup in YAML format
- **`markdown`**: Default format that creates a formatted, readable version of the conversation for documentation and sharing

**Usage:**

```bash
# Interactive export
biorouter session export

# Export specific session as JSON for backup
biorouter session export -n my-session --format json -o session-backup.json

# Export specific session as readable markdown
biorouter session export -n my-session -o session.md

# Export to stdout in different formats
biorouter session export --session-id 20251108_4 --format json
biorouter session export -n my-session --format yaml

# Export session by path (legacy)
biorouter session export --path ./my-session.jsonl -o exported.md
```

### session diagnostics [options]

Generate a comprehensive diagnostics bundle for troubleshooting issues with a specific session.

**Options:**

- **`--session-id <session_id>`**: Generate diagnostics for a specific session by ID
- **`-n, --name <name>`**: Generate diagnostics for a specific session by name
- **`--path <path>`**: Generate diagnostics for a specific session by file path (legacy)
- **`-o, --output <file>`**: Save diagnostics bundle to a specific file path (default: `diagnostics_{session_id}.zip`)

**What's included:**

- **System Information**: App version, operating system, architecture, and timestamp
- **Session Data**: Complete conversation messages and history for the specified session
- **Configuration Files**: Your [configuration files](../configuration/config-file-reference.md) (if they exist)
- **Log Files**: Recent application logs for debugging

**Usage:**

```bash
# Generate diagnostics for a specific session by ID
biorouter session diagnostics --session-id 20251108_5

# Generate diagnostics for a session by name
biorouter session diagnostics -n my-project-session

# Save diagnostics to a custom location
biorouter session diagnostics --session-id 20251108_5 -o /path/to/my-diagnostics.zip

# Interactive selection (prompts you to choose a session)
biorouter session diagnostics
```

> **Warning.** Diagnostics bundles contain your session messages and system information. If your session includes sensitive data (API keys, personal information, proprietary code), review the contents before sharing publicly.

> **Tip.** Generate diagnostics before reporting bugs to provide technical details that help with faster resolution. The ZIP file can be attached to GitHub issues or shared with support.

## Task execution

### run [options]

Execute commands from an instruction file or stdin.

**Input Options:**

- **`-i, --instructions <FILE>`**: Path to instruction file containing commands. Use `-` for stdin
- **`-t, --text <TEXT>`**: Input text to provide to biorouter directly
- **`--system <TEXT>`**: Provide additional system instructions to customize the agent's behavior
- **`--workflow <WORKFLOW_NAME_OR_PATH> <OPTIONS>`**: Load a custom workflow in current session
- **`--params <KEY=VALUE>`**: Key-value parameters to pass to the workflow file. Can be specified multiple times
- **`--sub-workflow <WORKFLOW>`**: Specify sub-workflows to include alongside the main workflow. Can be specified multiple times

**Session Options:**

- **`-s, --interactive`**: Continue in interactive mode after processing initial input
- **`-n, --name <name>`**: Name for this run session (e.g. `daily-tasks`)
- **`-r, --resume`**: Resume from a previous run
- **`--path <PATH>`**: Path for this run session (e.g. `./playground.jsonl`). Used for legacy file-based session storage.
- **`--no-session`**: Run biorouter commands without creating or storing a session file

**Extension Options:**

- **`--with-extension <COMMAND>`**: Add stdio extensions (can be used multiple times)
- **`--with-streamable-http-extension <URL>`**: Add remote extensions over Streamable HTTP (can be used multiple times)
- **`--with-builtin <name>`**: Add builtin extensions by name (e.g., 'developer' or multiple: 'developer,github')

**Control Options:**

- **`--debug`**: Output complete tool responses, detailed parameter values, and full file paths
- **`--max-tool-repetitions <NUMBER>`**: Maximum number of times the same tool can be called consecutively with identical parameters. Helps prevent infinite loops
- **`--max-turns <NUMBER>`**: Maximum number of turns allowed without user input (default: 1000)
- **`--explain`**: Show a workflow's title, description, and parameters
- **`--render-workflow`**: Print the rendered workflow instead of running it
- **`-q, --quiet`**: Quiet mode. Suppress non-response output, printing only the model response to stdout
- **`--output-format <FORMAT>`**: Output format (`text`, `json`, or `stream-json`). Default is `text`. Use JSON structured output for automation and scripting: `json` for results after completion, `stream-json` for events as they occur
- **`--provider`**: Specify the provider to use for this session (overrides environment variable)
- **`--model`**: Specify the model to use for this session (overrides environment variable)

**Usage:**

```bash
# Run from instruction file
biorouter run --instructions plan.md

# Load a workflow with a prompt that biorouter executes and then exits  
biorouter run --workflow workflow.yaml

# Load a workflow and stay in an interactive session
biorouter run --workflow workflow.yaml --interactive

# Load a workflow in debug mode
biorouter run --workflow workflow.yaml --debug

# Show workflow details
biorouter run --workflow workflow.yaml --explain

# Run a workflow with parameters
biorouter run --workflow workflow.yaml --params environment=production --params region=us-west-2

# Run instructions from a file without session storage
biorouter run --no-session -i instructions.txt

# Run with a specified provider and model
biorouter run --provider anthropic --model claude-4-sonnet -t "initial prompt"

# Run with limited turns before prompting user
biorouter run --workflow workflow.yaml --max-turns 10
```

### bench

Used to evaluate system-configuration across a range of practical tasks.

**Usage:**

```bash
biorouter bench ...etc.
```

### workflow

Used to validate workflow files, manage workflow sharing, list available workflows, and open workflows in biorouter desktop.

**Commands:**

- **`deeplink <WORKFLOW_NAME>`**: Generate a shareable link for a workflow file
  - **`-p, --param <KEY=VALUE>`**: Pre-fill workflow parameter (can be specified multiple times)
- **`list [OPTIONS]`**: List all available workflows from local directories and configured GitHub repositories
  - **`--format <FORMAT>`**: Output format (`text` or `json`). Default is `text`
  - **`-v, --verbose`**: Show verbose information including workflow titles and full file paths
- **`open <WORKFLOW_NAME>`**: Open a workflow file directly in biorouter desktop
  - **`-p, --param <KEY=VALUE>`**: Pre-fill workflow parameter (can be specified multiple times)
- **`validate <WORKFLOW_NAME>`**: Validate a workflow file

**Usage:**

```bash
# Generate a shareable link
biorouter workflow deeplink my-workflow.yaml

# Generate a deeplink and provide parameter values
biorouter workflow deeplink my-workflow.yaml -p environment=production -p region=us-west-2

# List all available workflows
biorouter workflow list

# List workflows with detailed information
biorouter workflow list --verbose

# List workflows in JSON format for automation
biorouter workflow list --format json

# Open a workflow in biorouter desktop
biorouter workflow open my-workflow.yaml

# Open a workflow by name
biorouter workflow open my-workflow

# Open a workflow and provide parameter value
biorouter workflow open my-workflow --param name=myproject

# Validate a workflow file
biorouter workflow validate my-workflow.yaml

# Get help about workflow commands
biorouter workflow help
```

### schedule

Automate workflows by running them on a [schedule](../workflows/creating-and-sharing-workflows.md#schedule-workflow).

**Commands:**

- `add <OPTIONS>`: Create a new scheduled job. Copies the current version of the workflow to the `scheduled_workflows` directory in Biorouter's data directory
- `list`: View all scheduled jobs
- `remove`: Delete a scheduled job
- `sessions`: List sessions created by a scheduled workflow
- `run-now`: Run a scheduled workflow immediately
- `cron-help`: Show cron expression examples and help

**Options:**

- `--schedule-id <NAME>`: A unique ID for the scheduled job (e.g. `daily-report`)
- `--cron "* * * * * *"`: Specifies when a job should run using a [cron expression](https://en.wikipedia.org/wiki/Cron#Cron_expression)
- `--workflow-source <PATH>`: Path to the workflow YAML file
- `-l, --limit <NUMBER>`: Max number of sessions to display when using the `sessions` command

**Usage:**

```bash
biorouter schedule <COMMAND>

# Add a new scheduled workflow which runs every day at 9 AM
biorouter schedule add --schedule-id daily-report --cron "0 0 9 * * *" --workflow-source ./workflows/daily-report.yaml

# List all scheduled jobs
biorouter schedule list

# List the 10 most recent biorouter sessions created by a scheduled job
biorouter schedule sessions --schedule-id daily-report -l 10

# Run a workflow immediately
biorouter schedule run-now --schedule-id daily-report

# Remove a scheduled job
biorouter schedule remove --schedule-id daily-report
```

### mcp

Run an enabled MCP server specified by `<name>` (e.g. `'Google Drive'`). MCP is the Model Context Protocol, the standard biorouter extensions speak.

**Usage:**

```bash
biorouter mcp <name>
```

### acp

Run biorouter as an Agent Client Protocol (ACP) agent server over stdio. This enables biorouter to work with ACP-compatible clients like Zed.

ACP is an emerging protocol specification that standardizes communication between AI agents and client applications, making it easier for clients to integrate with various AI agents.

**Usage:**

```bash
biorouter acp
```

> **Note.** This command is automatically invoked by ACP-compatible clients and is not typically run directly by users. The client manages the lifecycle of the `biorouter acp` process.

## Project management

### project

Start working on your last project or create a new one.

**Alias**: `p`

**Usage:**

```bash
biorouter project
```

### projects

Choose one of your projects to start working on.

**Alias**: `ps`

**Usage:**

```bash
biorouter projects
```

## Interface

### web

Start a new session in biorouter Web, a lightweight web-based interface launched via the CLI that mirrors the desktop app's chat experience.

biorouter Web is particularly useful when:

- You want to access biorouter with a graphical interface without installing the desktop app
- You need to use biorouter from different devices, including mobile
- You're working in an environment where installing desktop apps isn't practical

> **Warning.** Don't expose the web interface to the internet without proper security measures.

**Options:**

- **`-p, --port <PORT>`**: Port number to run the web server on. Default is `3000`
- **`--host <HOST>`**: Host to bind the web server to. Default is `127.0.0.1`
- **`--open`**: Automatically open the browser when the server starts
- **`--auth-token <TOKEN>`**: Require a password to access the web interface

**Usage:**

```bash
# Start web interface at `http://127.0.0.1:3000` and open the browser
biorouter web --open

# Start web interface at `http://127.0.0.1:8080` 
biorouter web --port 8080

# Start web interface accessible from local network at `http://192.168.1.7:8080`
biorouter web --host 192.168.1.7 --port 8080

# Start web interface with authentication required
biorouter web --auth-token <TOKEN>
```

> **Note.** Use `Ctrl+C` to stop the server.

**Limitations:**

While the web interface provides most core features, be aware of these limitations:

- Some file system operations may require additional confirmation
- Extension management must be done through the CLI
- Certain tool interactions might need extra setup
- Configuration changes require a server restart

## Terminal integration

### @biorouter / @g

Ask biorouter questions directly from your shell prompt, with command history included in the context. These aliases are created when you set up terminal integration.

**Examples:**

```bash
# Ask questions with command history context
@biorouter create a python script to process these files
@biorouter create a PR description summarizing these changes
@g how do I fix these permission denied errors?
```

## Interactive session features

### Slash commands

Once you're in an interactive session (via `biorouter session` or `biorouter run --interactive`), you can use these slash commands. All commands support tab completion. Press `/ + <Tab>` to cycle through available commands.

**Available Commands:**

- **`/?` or `/help`** - Display the help menu
- **`/builtin <names>`** - Add builtin extensions by name (comma-separated)
- **`/clear`** - Clear the current chat history
- **`/endplan`** - Exit plan mode and return to 'normal' biorouter mode
- **`/exit` or `/quit`** - Exit the session
- **`/extension <command>`** - Add a stdio extension (format: ENV1=val1 command args...)
- **`/mode <name>`** - Set the biorouter mode to use ('auto', 'approve', 'chat', 'smart_approve')
- **`/plan <message_text>`** - Enter 'plan' mode with optional message. Create a plan based on the current messages and ask user if they want to act on it
- **`/workflow [filepath]`** - Generate a workflow from the current conversation and save it to the specified filepath (must end with .yaml). If no filepath is provided, it will be saved to ./workflow.yaml
- **`/compact`** - Compact and summarize the current conversation to reduce context length while preserving key information
- **`/t`** - Toggle between `light`, `dark`, and `ansi` themes. [More info](#themes).
- **`/t <name>`** - Set theme directly (light, dark, ansi)

**Examples:**

```bash
# Create a plan for triaging test failures
/plan let's create a plan for triaging test failures

# Switch to chat mode
/mode chat

# Add a builtin extension during the session
/builtin developer

# Clear the current conversation history
/clear
```

You can also create custom slash commands for running workflows in biorouter Desktop or the CLI.

### Themes

The `/t` command controls the syntax highlighting theme for markdown content in biorouter CLI responses. This affects the styles used for headers, code blocks, bold/italic text, and other markdown elements in the response output.

**Commands:**

- `/t` - Cycles through themes: `light` → `dark` → `ansi` → `light`
- `/t light` - Sets `light` theme (subtle light colors)
- `/t dark` - Sets `dark` theme (subtle darker colors)
- `/t ansi` - Sets `ansi` theme (most visually distinct option with brighter colors)

**Configuration:**

- The default theme is `dark`
- The theme setting is saved to the [configuration file](../configuration/config-file-reference.md) as `BIOROUTER_CLI_THEME` and persists between sessions
- The saved configuration can be overridden for the session using the `BIOROUTER_CLI_THEME` [environment variable](../configuration/environment-variables.md#session-management)

> **Note.** Syntax highlighting styles only affect the font, not the overall terminal interface. The `light` and `dark` themes have subtle differences in font color and weight.

The biorouter CLI theme is independent from the biorouter Desktop theme.

**Examples:**

```bash
# Start a named session
biorouter session --name use-custom-theme

# Toggle theme during a session
/t

# Set the light theme during a session
/t light
```

> **Note.** The first example starts a session only; set `BIOROUTER_CLI_THEME` in the environment, as described under Configuration above, to choose the theme for that session.

## Navigation and controls

### Keyboard shortcuts

**Session Control:**

- **`Ctrl+C`** - Interrupt the current request
- **`Ctrl+J`** - Add a newline

**Navigation:**

- **`Cmd+Up/Down arrows`** - Navigate through command history
- **`Ctrl+R`** - Interactive command history search (reverse search). [More info](#command-history-search).

### Command history search

The `Ctrl+R` shortcut provides interactive search through your stored CLI command history. This feature makes it easy to find and reuse recent commands without retyping them. When you type a search term, biorouter searches backwards through your history for matches.

**How it works:**

1. Press `Ctrl+R` in your biorouter CLI session
2. Type a search term
3. Navigate through the results using:
   - `Ctrl+R` to cycle backwards through earlier matches
   - `Ctrl+S` to cycle forward through newer matches
4. Press `Return` (or `Enter`) to run the found command, or `Esc` to cancel

For example, instead of retyping this long command:

```text
analyze the GWAS summary statistics file and suggest follow-up enrichment analyses
```

Use the `"GWAS summary"` or `"enrichment"` search term to find and rerun it.

**Search tips:**

- **Distinctive terms work best**: Choose unique words or phrases to help filter the results
- **Partial matches and multiple words are supported**: You can search for phrases like `"gith"` and `"run the unit test"`

## Related documentation

- [CLI QA checklist](qa-checklist.md) — the manual and headless verification script covering every command on this page.
- [Configuration file reference](../configuration/config-file-reference.md) — the `config.yaml` keys behind `biorouter configure` and the theme setting.
- [Environment variables](../configuration/environment-variables.md) — per-invocation overrides for the same settings, including `BIOROUTER_CLI_THEME`.
- [Managing sessions](../getting-started/managing-sessions.md) — how sessions are stored, resumed, and pruned behind the `session` subcommands.
- [Creating and sharing workflows](../workflows/creating-and-sharing-workflows.md) — how to author the workflow files that `run --workflow`, `workflow`, and `schedule` consume.
