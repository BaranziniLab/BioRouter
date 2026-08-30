# Extension Manager capability

> **What this is.** User guide to the built-in Extension Manager: how BioRouter discovers, enables and disables other extensions mid-session so the active tool count stays small.
> **Status:** Current. The capability is enabled by default, so no manual setup is normally needed.
> **Audience:** end users.

You don't always need to manage extensions by hand. The Extension Manager lets BioRouter discover, enable and disable extensions during an active session. Based on the task you give it, BioRouter recognizes when it needs a specific extension, enables it, and suggests disabling unused ones when the tool bloat starts eating your context window. Describe your task and BioRouter handles the extension management.

> **Note.** This capability is **enabled by default**. Its internal registration still uses the legacy `PlatformExtensionDef` type; that storage name does not make it an installed extension. The configuration walkthrough below is only needed if you previously disabled it, or want to confirm its state.

## Configuration

1. Run the `configure` command:

   ```bash
   biorouter configure
   ```

2. Choose `Toggle Extensions`, then confirm `extensionmanager` is enabled:

   ```text
   ┌   biorouter-configure
   │
   ◇  What would you like to configure?
   │  Toggle Extensions
   │
   ◆  Enable capabilities and extensions: (use "space" to toggle and "enter" to submit)
   │  ● extensionmanager
   └  Extension settings updated successfully
   ```

## Why use the Extension Manager

BioRouter can work with many extensions, but having too many enabled at once can:

- Overwhelm the LLM with too many tool choices
- Reduce the quality of tool selection
- Slow down response times
- Exceed the recommended budget of 5 extensions or 50 tools

The Extension Manager addresses this by letting BioRouter:

- **Discover** what extensions are available
- **Enable** extensions only when needed for a specific task
- **Disable** extensions when they're no longer required

The result is a more focused session where BioRouter has exactly the tools it needs, when it needs them.

> **Note.** The "5 extensions / 50 tools" figure is a rule of thumb for keeping tool selection sharp, not an enforced limit. Nothing fails when you exceed it; tool-choice quality and response time degrade gradually.

## Available tools

| Tool | Description | Use Case |
|------|-------------|----------|
| `search_available_extensions` | Discover extensions that can be enabled or disabled | Finding the right extension for a task |
| `manage_extensions` | Enable or disable an extension by name | Loading/unloading extensions dynamically |
| `install_extension` | Install a BAAM marketplace extension end to end | The extension is not installed at all |
| `list_resources` | List resources from extensions (if supported) | Discovering available data sources |
| `read_resource` | Read specific resource content (if supported) | Accessing extension-provided data |

> **Tip.** The resource tools (`list_resources` and `read_resource`) are only available when at least one enabled extension supports resources.

## Installing one the user does not have

`manage_extensions` only enables what is already installed. `install_extension` handles the rest: it downloads and validates the bundle, builds its Python environment, collects any credentials the extension needs, registers it, and attaches it to the current chat — so the agent never shells out to `curl` and the CLI.

**The agent never sees a credential.** If the extension needs an API key, passcode or token, the install pauses and BioRouter opens its own dialog; the agent learns only which key *names* were configured. It must never ask for a value in chat — a credential in a chat message cannot configure anything and exposes it to every model that reads the transcript. The full design is in [Installing an extension, and where its credentials go](../installing-an-extension.md).

Privacy Gate F1 applies to the **attach**, not the install: a public-model chat may install a private connector at the user's request and will not be able to use it in that chat.

## Malware scanning when an extension is enabled

Enabling an extension that launches a package runner triggers a security check before the extension starts. BioRouter infers the ecosystem from the command — a command ending in `npx` is treated as npm, one ending in `uvx` as PyPI — parses the first package argument, and queries the OSV vulnerability database for `MAL-*` advisories. If the package is flagged as malicious, enabling it is denied.

Two consequences are worth knowing:

- Commands from other ecosystems are not recognized, and the check **fails open** — it is skipped rather than blocking the extension.
- The endpoint can be overridden with the `OSV_ENDPOINT` environment variable.

## Example usage

In this example BioRouter enables the GitHub extension — an extension you install separately, rather than one of the built-ins documented in this folder — because the task needs it.

### BioRouter prompt

```text
List all my GitHub repositories
```

### BioRouter output

> **Note.** The transcript below is stylized. The uppercase `MANAGE_EXTENSIONS` and `LIST_REPOSITORIES` markers stand in for tool calls; they are not real tool names or a real tool-call format. The actual tool is `manage_extensions`, listed in [Available tools](#available-tools) above.

```text
I'll enable the GitHub extension for you so we can work with repositories.

MANAGE_EXTENSIONS
action: enable
extension_name: github

✅ The extension 'github' has been installed successfully

The GitHub extension is now active.

I'll list your GitHub repositories using the GitHub extension.

LIST_REPOSITORIES

Here are your repositories:

...

Would you like to work with any of these repositories?
```

## Related documentation

- [Extensions and skills guide](../extensions-and-skills-guide.md) — installing, configuring and removing extensions yourself.
- [Code Execution capability](code-execution.md) — the other lever for reducing context pressure, by batching tool calls instead of trimming the tool list.
- [Context engineering](../../agent-loop/context-engineering.md) — why the active tool count matters to the context window.
- [Extension trait design](../../history/legacy-architecture/extension-trait-design.md) — historical design record for how extensions are modelled internally.
