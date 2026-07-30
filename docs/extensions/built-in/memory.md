# Memory extension

> **What this is.** User guide to the built-in Memory extension: enabling it, the trigger words that store, recall and forget memories, where memories live on disk, and a worked example teaching BioRouter a lab's analysis standards.
> **Status:** Current. `crates/biorouter-mcp/src/memory` ships in the product and the local/global model described here still matches. This page predates the Knowledge feature; see [Knowledge and memory](#knowledge-and-memory) for how the two relate.
> **Audience:** end users.

The Memory extension lets you teach BioRouter personalized information — commands, code snippets, preferences, configurations, lab conventions — that it can recall and apply later. Knowledge can be project-specific (**local**) or user-wide (**global**), so BioRouter remembers what matters to you across sessions.

## Configuration

1. Run the `configure` command:

   ```bash
   biorouter configure
   ```

2. Choose `Toggle Extensions`, then enable `memory`:

   ```text
   ┌   biorouter-configure
   │
   ◇  What would you like to configure?
   │  Toggle Extensions
   │
   ◆  Enable extensions: (use "space" to toggle and "enter" to submit)
   │  ● memory
   └  Extension settings updated successfully
   ```

## Why use Memory

With the Memory extension you are not just storing static notes, you are teaching BioRouter how to assist you better. Imagine telling BioRouter:

> _learn everything about MCP servers and save it to memory._

Later, you can ask:

> _utilizing our MCP server knowledge help me build an MCP server._

BioRouter recalls everything you have saved, as long as you instruct it to remember. This makes results more consistent across sessions.

BioRouter loads all saved memories at the start of a session and includes them in every prompt sent to the LLM. For large or detailed instructions, store them in files and instruct BioRouter to reference those files:

> _Remember that if I ask for help writing JavaScript, I want you to refer to "/path/to/javascript_notes.txt" and follow the instructions in that file._

## Where memories are stored

Memories are plain files on your machine, organized by category. There are two scopes:

| Scope | Location |
|-------|----------|
| Global (user-wide) | `~/.config/biorouter/memory/` on macOS and Linux; `~\AppData\Roaming\BaranziniLab\Biorouter\config\memory` on Windows |
| Local (project-specific) | `.biorouter/memory/` inside the working directory |

Because they are ordinary files, you can inspect, edit or delete them directly when you want to audit or reset what BioRouter has learned. You do not have to, though — the desktop app lists both stores and lets you prune them.

## Seeing and deleting what was remembered

Open **Settings → Chat → Memory**. It lists both stores separately — the global one shared by every conversation on this computer, and the current project's local one — each with its directory path, its categories, and every memory inside them. Expand a category to read its contents.

This matters most for the global store. A conversation asking to read a global memory category has to be approved by you first, and that approval card names the category; this section is where you find out what is in it before deciding. The card names this path so you know where to look.

What each memory shows is what the store actually records, and no more:

| Shown | Where it comes from |
|-------|---------------------|
| Category name | The category file's name |
| Scope, and the store's absolute path | Which of the two directories the file is in |
| Tags | The memory's own `#` tag line, when the model attached one |
| Category size and "Updated" date | The category file's size and modification time on disk |

A memory is a line in a flat text file, so nothing records **when an individual memory was written, which conversation wrote it, or which model**. The date shown is per category — the last time anything was appended to that file — not per memory, and the section labels it that way rather than dating each row with a timestamp the file cannot support.

Two delete controls sit alongside the listing: one on each memory, one on each category. Both ask first, and the confirmation says what is about to be lost — the number of memories going, who could read them, and that it cannot be undone. Deleting the last memory in a category deletes the category too, so its name stops being offered to future sessions. Nothing is recoverable afterwards; BioRouter keeps no copy.

## Available tools

| Tool | Description |
|------|-------------|
| `remember_memory` | Store a memory in a category, with optional tags, in the local or global scope |
| `retrieve_memories` | Read back stored memories |
| `remove_memory_category` | Delete an entire category of memories |
| `remove_specific_memory` | Delete one stored memory |

## Trigger words and when to use them

BioRouter recognizes certain words as signals to store, retrieve or remove memory. These are **soft heuristics that guide the model**, not a parsed command grammar — a phrasing that is not in this table can still trigger a memory operation, and a phrasing that is in it will not always do so. State your intent plainly and the model will pick the right tool.

| **Trigger words**   | **When to use** |
|---------------------|----------------|
| remember            | Store useful info for later use |
| forget           | Remove a stored memory |
| memory           | General memory-related actions |
| save             | Save a command, config, or preference |
| remove memory    | Delete specific stored data |
| clear memory     | Wipe all stored memories |
| search memory    | Find previously stored data |
| find memory      | Locate specific saved information |

## Knowledge and memory

The Memory extension is best for short, durable facts and preferences that should ride along in every prompt. For larger bodies of structured knowledge — ingested documents, cross-referenced pages, a browsable graph — BioRouter's Knowledge feature is the better fit, and it stores its content separately under `~/.config/biorouter/knowledge/`. See [knowledge ingestion formats](../../knowledge-base/ingestion-format-roadmap.md) for what Knowledge can absorb.

## Example usage

This example makes BioRouter a knowledgeable research assistant by teaching it your lab's data-analysis standards. BioRouter stores the standards and recalls them when needed, so you do not have to repeat your project's requirements on every new task.

> **Tip.** If you frequently work with analysis standards or other structured knowledge, BioRouter may offer to save them to memory for you when it detects a pattern in your workflow.

To follow along, either open a new session in BioRouter Desktop, or start one from a terminal:

```bash
biorouter session
```

### Step 1: Teach BioRouter your analysis standards

#### BioRouter prompt #1

```text
biorouter, learn our cohort data-analysis standards:

- All cohorts must be defined with explicit OMOP concept IDs
- Continuous variables are reported as mean ± SD; categorical as n (%)
- Multiple-comparison correction uses Benjamini-Hochberg FDR
- Patient identifiers must be de-identified before analysis
- Every analysis needs a reproducible script and a methods summary

Remember this so you can help me follow these practices.
```

#### BioRouter output

```text
I've learned your analysis standards. I'll help enforce these requirements when working on cohort analyses.

Storing in:
    - Category: analysis_standards
    - Tags: #cohort #omop #statistics #reproducibility
    - Scope: global (these are general standards)

I can now help with:
    - Cohort definition with OMOP concepts
    - Descriptive and inferential statistics
    - Multiple-comparison correction
    - De-identification and methods reporting
```

### Step 2: Use stored knowledge to run a new cohort analysis

#### BioRouter prompt #2

```text
Help me compare baseline characteristics between the treatment and control cohorts
```

#### BioRouter output

```text
I see you need a baseline comparison. Based on our cohort data-analysis standards that I have stored in memory, I'll ensure it meets all our requirements.

[.. Checking stored analysis standards ...]

I've followed all our required elements:

✅ Cohorts defined via OMOP concept IDs
✅ Mean ± SD for continuous, n (%) for categorical
✅ Benjamini-Hochberg FDR correction
✅ De-identified patient data
✅ Reproducible script + methods summary

Here's your compliant analysis script:

# baseline_comparison.py
import pandas as pd
from scipy import stats
from statsmodels.stats.multitest import multipletests

treatment = load_cohort(concept_ids=[201826])   # OMOP concept
control = load_cohort(concept_ids=[4193704])

# Continuous: mean ± SD, two-sample t-test
# Categorical: n (%), chi-square test
pvals = run_baseline_tests(treatment, control)

# Benjamini-Hochberg FDR correction per our standard
_, qvals, _, _ = multipletests(pvals, method="fdr_bh")

Would you like me to run the full comparison and generate the Table 1 summary?
```

## Related documentation

- [Chat Recall extension](chat-recall.md) — the complementary way to recover past context, by searching old sessions rather than storing facts.
- [Knowledge ingestion formats](../../knowledge-base/ingestion-format-roadmap.md) — the Knowledge feature, which covers larger bodies of structured knowledge.
- [Extensions and skills guide](../extensions-and-skills-guide.md) — how to enable, disable and configure extensions generally.
- [Context engineering](../../agent-loop/context-engineering.md) — how memories, skills and other injected context share the window.
