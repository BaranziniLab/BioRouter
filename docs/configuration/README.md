# Configuration

This folder documents how you tell biorouter what to do before it starts a session: which provider and model to use, how much autonomy the agent has, where it looks for extensions and workflows, and how it reports telemetry. The same settings surface in two forms — persistent YAML files in your user config directory, and environment variables that override them for a single invocation — and this folder holds the complete reference for both.

Come here when you know *which* setting you want to change and need its name, accepted values, and default. Go elsewhere if you are still deciding what to set: [installation](../getting-started/installation.md) covers first-run setup and [choosing a model provider](../getting-started/choosing-a-model-provider.md) covers which provider to pick. Settings whose *consequences* are security-relevant are defined here but explained in [security](../security/README.md) — this folder tells you that `BIOROUTER_MODE` accepts `"approve"`, that folder tells you what approving actually gates. A handful of variables belong to individual features and live with those features rather than here; the [environment variables](environment-variables.md#variables-documented-elsewhere) page lists which ones and where they went.

## Documents

| Document | What it covers |
|----------|----------------|
| [Configuration file reference](config-file-reference.md) | The reference for biorouter's YAML configuration files — where they live, every global setting they accept, how extensions and search paths are declared, and how config values interact with environment variables. |
| [Environment variables](environment-variables.md) | The grouped reference of every environment variable that customizes biorouter's behaviour, from provider selection through observability, workflow discovery, and test isolation. Note that its example command blocks were lost in the 2026-05 docs migration; the tables are complete and authoritative, but several sections retain only the captions of examples that no longer follow. |

Between the two, precedence runs environment variables → config file → built-in defaults. Enterprise deployments add an admin-owned tier above all three; the two pages here and [managed policy](../security/managed-policy.md) describe orderings that have not been reconciled, and where an admin-installed policy file is present the managed policy page is authoritative.

## Related documentation

- [Security](../security/README.md) — what the permission modes and keyring settings defined here actually control, including [secret storage](../security/secret-storage.md) for API keys you should not put in `config.yaml`.
- [Choosing a model provider](../getting-started/choosing-a-model-provider.md) — the provider names and model IDs the provider settings on both pages expect.
- [biorouter CLI command reference](../cli/command-reference.md) — the commands these settings modify, including `biorouter configure` and the themes selected by `BIOROUTER_CLI_THEME`.
- [Extensions, skills, and MCP agents](../extensions/extensions-and-skills-guide.md) — background on the extensions you declare under the `extensions` key in `config.yaml`.
