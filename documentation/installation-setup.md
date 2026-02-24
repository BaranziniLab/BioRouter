# UCSF BioRouter — Installation and Setup

This guide covers how to install BioRouter, connect an LLM provider, and optionally configure MCP extensions and agents.

**GitHub:** https://github.com/BaranziniLab/BioRouter
**Latest releases:** https://github.com/BaranziniLab/BioRouter/releases

Always install the newest release for the latest features and bug fixes.

---

## Step 1 — Install BioRouter

### macOS (Desktop App — Recommended)

1. Go to https://github.com/BaranziniLab/BioRouter/releases and download the latest macOS release (`.dmg` or `.zip`).
2. Open the downloaded file and move BioRouter to your Applications folder.
3. Launch BioRouter from Applications.

**Note:** If you see a security warning on an Apple Silicon Mac, ensure `~/.config` has read and write permissions. BioRouter uses this directory for its configuration and log files.

### macOS (CLI)

```sh
curl -fsSL https://github.com/BaranziniLab/BioRouter/releases/download/stable/download_cli.sh | bash
```

To install without interactive configuration:
```sh
curl -fsSL https://github.com/BaranziniLab/BioRouter/releases/download/stable/download_cli.sh | CONFIGURE=false bash
```

To update:
```sh
biorouter update
```

### Linux and Windows

Linux and Windows builds are currently in development. Check the GitHub releases page for updates:
https://github.com/BaranziniLab/BioRouter/releases

---

## Step 2 — Configure an LLM Provider

BioRouter needs an LLM provider to function. On first launch, you will be prompted to configure one.

### UCSF Users — Recommended Options

**Option A: Azure OpenAI (UCSF ChatGPT)**
- Access is managed through your UCSF institutional account.
- Select "Azure OpenAI" in the provider setup and follow the credential prompts.
- This provider routes through UCSF's Azure tenant — no separate API key required for most users.

**Option B: Amazon Bedrock (UCSF-hosted Anthropic)**
- Uses AWS SSO with your UCSF AWS account.
- Run `aws sso login --profile <your-profile>` before launching BioRouter.
- Select "Amazon Bedrock" and configure `AWS_PROFILE` and `AWS_REGION`.

**Option C: Local / Air-gapped (Ollama)**
- Install Ollama: https://ollama.com
- Pull a model: `ollama pull qwen3`
- Select "Ollama" in BioRouter — no API key needed.
- Data stays entirely on your device.

### Commercial Cloud Providers

For direct access using your own API key:

| Provider | Where to get a key |
|---|---|
| Anthropic | https://console.anthropic.com |
| OpenAI | https://platform.openai.com/api-keys |
| Google Gemini | https://aistudio.google.com/app/apikey |
| X.AI (Grok) | https://console.x.ai |
| Venice AI | https://venice.ai |
| OpenRouter | https://openrouter.ai/keys |

**Desktop setup:**
1. On the welcome screen, enter your API key in the Quick Setup panel.
2. BioRouter will test the key and prompt you to choose a model.
3. You're ready to start a session.

**CLI setup:**
```sh
biorouter configure
# Select "Configure Providers"
# Choose your provider and enter the API key when prompted
```

### Switching or Updating Your Provider

**Desktop:** Settings > Models > select a provider card > Configure or Launch.

**CLI:**
```sh
biorouter configure
# Select "Configure Providers"
```

---

## Step 3 — Verify the Setup

After configuration, start a session and send a test message:

**Desktop:** Type a message in the chat input and press Enter.

**CLI:**
```sh
biorouter session
> Hello, can you confirm you're working?
```

If BioRouter responds, the setup is complete.

---

## Step 4 (Optional) — Enable or Add Extensions

Extensions give BioRouter access to tools like file operations, web search, databases, and more.

The **Developer** extension is enabled by default and provides core capabilities (reading/writing files, running shell commands, etc.).

### Enabling Built-in Extensions

**Desktop:** Sidebar > Extensions > toggle the extension on.

**CLI:**
```sh
biorouter configure
# Select "Add Extension" > "Built-in Extension"
```

Available built-in extensions: Developer (default), Computer Controller, Memory, Tutorial, Auto Visualiser, Chat Recall, Code Execution, Extension Manager (default), Skills (default), Todo (default).

### Adding External MCP Servers

Any MCP-compatible server can be added as an extension.

**Desktop:** Sidebar > Extensions > Add custom extension > enter the command and configuration.

**CLI example (GitHub MCP server):**
```sh
biorouter configure
# Select "Add Extension" > "Command-line Extension"
# Name: GitHub
# Command: npx -y @modelcontextprotocol/server-github
# Environment variable: GITHUB_PERSONAL_ACCESS_TOKEN = <your token>
```

**Manual config** (`~/.config/biorouter/config.yaml`):
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

---

## Step 5 (Optional) — Set Up MCP Agents for Specialized Tasks

BioRouter can connect to remote MCP agents that provide specialized research capabilities (database access, literature retrieval, bioinformatics tools, etc.).

To connect a remote MCP agent over HTTP:
```yaml
# In ~/.config/biorouter/config.yaml
extensions:
  my-research-agent:
    name: My Research Agent
    url: https://my-agent.example.com/mcp
    type: streamable_http
    enabled: true
    timeout: 300
```

Or via CLI:
```sh
biorouter session --with-streamable-http-extension "https://my-agent.example.com/mcp"
```

---

## Configuration File Location

The main configuration file is at:

```
~/.config/biorouter/config.yaml       (macOS / Linux)
```

This file stores provider settings, API keys (encrypted), extension configurations, and model preferences. It is shared between the Desktop app and the CLI.

**Electron app state** (Desktop only) is stored at:
```
~/Library/Application Support/BioRouter/     (macOS)
```

---

## Updating BioRouter

**Desktop:** BioRouter checks for updates automatically. You can also manually download the latest release from https://github.com/BaranziniLab/BioRouter/releases.

**CLI:**
```sh
biorouter update
```

---

## Troubleshooting

**App shows no window on launch (macOS M3):**
Ensure `~/.config` has read and write permissions.
```sh
chmod 755 ~/.config
```

**Extension fails to activate:**
- Check that the required runtime is installed (Node.js for `npx`, Python/`uvx` for Python extensions).
- In corporate or air-gapped networks, extensions may need pre-installation rather than on-demand downloads.

**API key not working:**
- Verify the key is correct and has not expired.
- Check that the provider account has sufficient credits or quota.
- Try setting the key as an environment variable instead of via the keyring:
  ```sh
  export ANTHROPIC_API_KEY=your_key_here
  biorouter configure
  ```

**Log files:**
Logs are stored in `~/.config/biorouter/logs/`. Review them for detailed error messages.
