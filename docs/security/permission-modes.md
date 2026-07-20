# Permission modes

> **What this is.** A guide to the four permission modes that decide how much autonomy
> BioRouter has when modifying files, using extensions, and performing automated actions, and
> how to switch between them in the desktop app and the CLI.
> **Status:** Current. The four modes and their CLI values match the `/mode` slash command and
> the `BIOROUTER_MODE` setting.
> **Audience:** end users.

BioRouter's permissions determine how much autonomy it has when modifying files, using
extensions, and performing automated actions. By selecting a permission mode you control how
BioRouter interacts with your development environment. The mode applies to the whole session
and takes effect immediately — you can change it before or during a session.

> **Note.** A permission mode is the *user-owned* tier. An administrator can deploy a
> [managed policy](managed-policy.md) that overrides every mode described here, including
> Completely Autonomous, by forcing specific tools to be denied or to require approval. If a
> tool is blocked with *"Blocked by your organization's managed policy,"* that is the managed
> tier, not your mode setting.

## The four modes

The **Mode** column below is the name shown in the desktop app; **CLI value** is what you pass
to `/mode`; **Configure name** is how the mode appears in `biorouter configure`.

| Mode | CLI value | Configure name | Description | Best for |
|---|---|---|---|---|
| **Completely Autonomous** | `auto` | Auto Mode | BioRouter can modify files, use extensions, and delete files **without requiring approval** | Users who want **full automation** and seamless integration into their workflow |
| **Manual Approval** | `approve` | Approve Mode | BioRouter **asks for confirmation** before using any tools or extensions (supports granular tool permissions) | Users who want to **review and approve** every change and tool usage |
| **Smart Approval** | `smart_approve` | Smart Approve Mode | BioRouter uses a risk-based approach to **automatically approve low-risk actions** and **flag others** for approval (supports granular tool permissions) | Users who want a **balanced mix of autonomy and oversight** based on the action's impact |
| **Chat Only** | `chat` | Chat Mode | BioRouter **only engages in chat**, with no extension use or file modifications | Users who prefer a **conversational AI experience** for analysis, writing, and reasoning tasks without automation |

> **Warning.** Completely Autonomous (`auto`) is applied by default.

> **Note.** In Manual Approval and Smart Approval modes you will see "Allow" and "Deny" buttons
> in your session windows during tool calls. BioRouter only asks for permission for tools it
> deems are 'write' tools — for example any 'text editor write', 'text editor edit', or
> 'bash - rm, cp, mv' command. Read/write approval makes a best-effort attempt at classifying
> read or write tools, and that classification is interpreted by your LLM provider.

## Changing the mode in the desktop app

You can change modes before or during a session, and the change takes effect immediately.

From the chat window, click the mode button in the bottom menu and pick a mode.

Or, from Settings:

1. Click the sidebar button in the top-left to open the sidebar.
2. Click the `Settings` button on the sidebar.
3. Click `Chat`.
4. Under `Mode`, choose the mode you'd like.

## Changing the mode in the CLI

To change modes mid-session, use the `/mode` command:

- Autonomous: `/mode auto`
- Smart Approve: `/mode smart_approve`
- Approve: `/mode approve`
- Chat: `/mode chat`

To set the default mode, use `biorouter configure`:

1. Run the following command:

   ```bash
   biorouter configure
   ```

2. Select `biorouter settings` from the menu and press Enter.

   ```text
   ┌ biorouter-configure
   │
   ◆ What would you like to configure?
   | ○ Configure Providers
   | ○ Add Extension
   | ○ Toggle Extensions
   | ○ Remove Extension
   | ● biorouter settings (Set the biorouter mode, Tool Output, Tool Permissions, Experiment, biorouter workflow github repo and more)
   └
   ```

3. Choose `biorouter mode` from the menu and press Enter.

   ```text
   ┌   biorouter-configure
   │
   ◇  What would you like to configure?
   │  biorouter settings 
   │
   ◆  What setting would you like to configure?
   │  ● biorouter mode (Configure biorouter mode)
   │  ○ Router Tool Selection Strategy 
   │  ○ Tool Permission 
   │  ○ Tool Output 
   │  ○ Max Turns 
   │  ○ Toggle Experiment 
   │  ○ biorouter workflow github repo 
   │  ○ Scheduler Type 
   └
   ```

4. Choose the biorouter mode you would like to configure.

   ```text
   ┌   biorouter-configure
   │
   ◇  What would you like to configure?
   │  biorouter settings
   │
   ◇  What setting would you like to configure?
   │  biorouter mode
   │
   ◆  Which biorouter mode would you like to configure?
   │  ● Auto Mode (Full file modification, extension usage, edit, create and delete files freely)
   |  ○ Approve Mode
   |  ○ Smart Approve Mode    
   |  ○ Chat Mode
   |
   └  Set to Auto Mode - full file modification enabled
   ```

## Related documentation

- [Managed enterprise policy](managed-policy.md) — the admin-owned tier that overrides every
  mode on this page.
- [CLI command reference](../cli/command-reference.md) — the `/mode` slash command alongside
  the rest of the CLI surface.
- [Configuration file reference](../configuration/config-file-reference.md) — where the
  persisted mode and `permission.yaml` live.
- [Hooks reference](../agent-loop/hooks/hooks-reference.md) — lifecycle hooks, which gate tool
  calls independently of the permission mode.
- [Data privacy and patient data](data-privacy-and-phi.md) — the other decision that matters
  before a session touches sensitive data.
