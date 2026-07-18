# Chat Recall extension

> **What this is.** User guide to the built-in Chat Recall extension: searching your past session history by keyword or session ID so BioRouter can pull earlier context into the current conversation.
> **Status:** Current.
> **Audience:** end users.

The Chat Recall extension lets BioRouter search across your saved session history. When you reference past work, or ask a question that needs historical context, BioRouter searches for relevant conversations or loads a summary of a specific session — you do not have to invoke it explicitly.

Search runs entirely against session history stored on your own machine, under `~/.config/biorouter/sessions/` (a SQLite database). Nothing is sent to a remote search service, and sessions from other machines or other users are not visible.

> **Note.** Unlike most built-in extensions, Chat Recall ships **disabled by default** (`default_enabled: false` in `crates/biorouter/src/agents/extension.rs`), so the configuration step below is required before you can use it.

## Configuration

1. Run the `configure` command:

   ```bash
   biorouter configure
   ```

2. Choose `Toggle Extensions`, then enable `chatrecall`:

   ```text
   ┌   biorouter-configure
   │
   ◇  What would you like to configure?
   │  Toggle Extensions
   │
   ◆  Enable extensions: (use "space" to toggle and "enter" to submit)
   │  ● chatrecall
   └  Extension settings updated successfully
   ```

## Available tool

The extension registers a single tool.

| Tool | Description |
|------|-------------|
| `chatrecall` | Search past chat, or load session summaries. In **search mode** (a `query`) it returns messages grouped by session and ordered by recency, and supports date filters. In **load mode** (a `session_id`) it returns the first and last three messages of that session. |

You can obtain session IDs from the [CLI command reference](../../cli/command-reference.md).

## Example usage

Ask BioRouter to find recent conversations about a topic.

### BioRouter prompt

```text
What did we discuss about the cohort QC pipeline last week?
```

### BioRouter output

> **Note.** The transcript below is an illustration. Its session titles and dates are made up.

I found discussions about the cohort QC pipeline in your recent sessions. Here's what we covered:

**Cohort QC Pipeline Discussions - Last Week**

#### Session: Fix QC filtering scripts (Nov 26, 2024)

**Working Dir:** /projects/cohort-study

We debugged a quality-control step that was dropping more samples than expected. The key points were:

- The filter was excluding samples with missing values in an optional biomarker column
- We added conditional logic to skip the filter when the column is absent
- Re-ran the pipeline locally and committed the fix

#### Session: Cohort schema updates (Nov 27, 2024)

**Working Dir:** /projects/cohort-study

We discussed adding a derived eligibility flag:

- Planned to add a new `meets_inclusion_criteria` field to the cohort table
- Discussed whether to default it to false until criteria are evaluated
- Created the script to populate the new field

Both sessions were focused on safely refining the cohort dataset. Would you like me to load the full details from either of these sessions?

## Compatibility caveat

> **Note.** Compacted sessions from BioRouter versions prior to v1.14.0 may not be searchable, because conversation history is stored differently from that release onwards. If your installation has never run a pre-v1.14.0 release, this does not apply.

## Related documentation

- [Sessions](../../getting-started/managing-sessions.md) — how sessions are created, stored and resumed, and where the SQLite history lives.
- [CLI command reference](../../cli/command-reference.md) — the `session` subcommands that list sessions and reveal their IDs.
- [Extensions and skills guide](../extensions-and-skills-guide.md) — how to enable, disable and configure extensions generally.
- [Memory extension](memory.md) — the complementary way to carry knowledge forward, by teaching BioRouter facts rather than searching old chats.
