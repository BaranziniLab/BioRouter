# Computer Controller capability

> **What this is.** User guide to the built-in Computer Controller capability: how to enable it, which tools it provides, and a worked example combining web research with macOS system automation.
> **Status:** Current. `crates/biorouter-mcp/src/computercontroller/` ships in the product and the tools described below are real. The worked example is macOS-specific.
> **Audience:** end users.

The Computer Controller capability automates everyday computer tasks and web interactions — searching the web, controlling system settings, processing data files, and driving applications — without you having to write code. It is the highest-blast-radius built-in capability, because it acts on your actual desktop rather than on a sandboxed workspace.

> **Warning.** `computer_control` drives your real machine: it can launch and quit applications, click buttons, type text, change system settings, and reorganize files. Before running an unattended automation, decide whether you want BioRouter to ask first — see [permission modes](../../security/permission-modes.md) for how to switch out of Autonomous mode, and the [security guide](../../security/README.md) for the wider picture.

> **Tip.** Let BioRouter complete its tasks without interruption — avoid using your mouse or keyboard until it is done.

## Platform support

The `computer_control` tool exists on every platform, but its automation backend and therefore its capabilities differ:

| Platform | Backend | Notes |
|----------|---------|-------|
| macOS | AppleScript | Application control, UI automation, system settings, web and email, media, file operations, Calendar/Reminders/Messages integration. |
| Windows | PowerShell | PowerShell system control and UI automation, file and system management, Windows-specific features and settings. |
| Linux | Shell scripting | X11/Wayland window management, D-Bus system services, desktop-environment control (GNOME, KDE), process management, system settings. |
| Other | System automation | Available features depend on the operating system. |

The `automation_script` tool runs PowerShell or Batch scripts on Windows. The worked example later on this page is **macOS-only** — it uses Safari, Numbers and macOS screen brightness — so Linux and Windows users should read it as an illustration of shape, not as a runnable recipe.

## Configuration

1. Run the `configure` command:

   ```bash
   biorouter configure
   ```

2. Choose `Toggle Extensions`, then enable `computercontroller`:

   ```text
   ┌   biorouter-configure
   │
   ◇  What would you like to configure?
   │  Toggle Extensions
   │
   ◆  Enable capabilities and extensions: (use "space" to toggle and "enter" to submit)
   │  ● computercontroller
   └  Extension settings updated successfully
   ```

## Available tools

| Tool | Description | Risk |
|------|-------------|------|
| `computer_control` | Control the computer through the platform's automation backend (AppleScript, PowerShell or shell). Launch and quit applications, simulate clicks and typing, manage system settings, open URLs, organize files. Combines well with a screenshot tool for visual assistance. | ⚠️ High — acts on your real desktop |
| `automation_script` | Create and run small PowerShell or Batch scripts. Also usable for network-aware scripts (web, API, RSS, news searches) when no dedicated search tool exists. The script is saved to a temporary file and executed. | ⚠️ High — runs arbitrary scripts |
| `web_scrape` | Fetch an HTTP(S) URL for web research, APIs, RSS/Atom feeds, and search-result URLs. Text and JSON are returned inline and a cached copy is saved; large responses are truncated inline but stay complete in the cache. Prefer this over an automation script when you already know the URL. | ✅ Low — read-only fetch |
| `xlsx_tool` | Read and manipulate Excel workbooks: `list_worksheets`, `get_columns`, `get_range`, `find_text`, `get_cell`, `update_cell`, `save`. | Moderate — can modify workbooks |
| `docx_tool` | Extract text and structure from DOCX files (`extract_text`), or create/update documents (`update_doc`, with `append`, `replace`, `structured` and `add_image` modes). | Moderate — can modify documents |
| `pdf_tool` | Extract text (`extract_text`) or embedded images (`extract_images`, saved as PNG) from PDF files. | ✅ Low — read-only extraction |
| `cache` | Manage cached files and data: `list`, `view`, `delete`, `clear`. | ✅ Low |

> **Note.** When embedding a multiline script inside `execute_code` (Code Mode), wrap it in a `` String.raw`...` `` JavaScript template literal so backslashes survive intact.

## Example usage

This example shows BioRouter multitasking across system controls, music playback, web research and data organization. It was run on macOS.

To follow along, either open a new session in BioRouter Desktop, or start one from a terminal:

```bash
biorouter session
```

> **Note.** This transcript was produced with Anthropic's Claude 4 Sonnet. Any tool-capable model can drive the capability; the exact wording of the response will differ by model.

### BioRouter prompt

```text
biorouter, I need to decompress while researching candidate drugs for tomorrow's lab meeting. Can you create a relaxing environment and help me research?

1. Play classical music in Safari
2. Dim screen brightness (2 levels)
3. Research current approved therapies for a target indication:
    * Verify approval status
    * Collect: approval date, mechanism of action, indication, dosing
    * Use official sources only (e.g. FDA, DailyMed)
4. Create "drug-candidates-comparison.csv" on desktop with collected data
5. Launch Numbers and open the CSV file
6. Reset brightness and close Safari
```

### BioRouter output

```md
I'll help you create a relaxing research environment and gather information about AI models. I'll break this down into steps:

1. First, let's set up the relaxing environment with classical music and dimmed screen:

─── computer_control | computercontroller ──────────────────────────
script: ...

2. Now, let's research the candidate drugs. I'll use web search and scraping to gather
 information from official sources:

─── web_search | computercontroller ──────────────────────────
query: drug A approval indication

─── web_search | computercontroller ──────────────────────────
query: drug B mechanism of action

─── web_search | computercontroller ──────────────────────────
query: drug C dosing

3. Let's create the CSV file with the collected information:

─── automation_script | computercontroller ──────────────────────────
language: shell
script: ...

4. Now, let's open the CSV file in Numbers and reset the environment:

─── computer_control | computercontroller ──────────────────────────
script: ...

I've completed all the requested tasks:
1. ✅ Played classical music in Safari
2. ✅ Dimmed screen brightness
3. ✅ Researched current approved therapies for the indication
4. ✅ Created the CSV file on desktop
5. ✅ Launched Numbers and opened the CSV file
6. ✅ Reset brightness and closed Safari

The CSV file has been created on your desktop with the following information for each drug:
- Drug name
- Approval status
- Approval date
- Mechanism of action
- Indication
- Dosing information

The file is now open in Numbers for your review. The environment has been reset to normal settings. Is there anything specific about any of the drugs you'd like me to elaborate on?
```

### Results

The run produced `drug-candidates-comparison.csv` on the desktop and opened it in Numbers. (The screenshot that originally accompanied this section is no longer part of the documentation.)

## Related documentation

- [Developer capability](developer.md) — the other high-privilege built-in capability, and the fullest description of BioRouter's layered access controls.
- [Permission modes](../../security/permission-modes.md) — how to make BioRouter ask before it acts on your machine.
- [Security guide](../../security/README.md) — using BioRouter safely, including what a high-privilege extension implies.
- [Computer Controller hardening: test plan and root causes](../../history/computer-controller-hardening/test-plan-and-root-causes.md) — the historical campaign that exercised these tools and the failure modes it found.
