# Tool Routing — which tool for which job

This document is the canonical reference for **tool selection** in BioRouter: which
tool the agent should reach for, in what order of preference, and where the overlaps
are. It exists because the platform exposes several tools that can each list a
directory, read a file, or run a command, and without explicit routing guidance the
model picks the wrong one — e.g. writing a JavaScript script to `ls` a directory or
copy a file, or calling a tool-discovery search to answer a web question.

The user's directive: *"when the situations are somewhat more straightforward and
easy, we should almost always rely on the most basic fundamental tools inside of
BioRouter — the tools without any extensions. Only when there are more complicated
tasks should we think about using the extensions."*

## Interpretation: a tier model (please confirm)

There is an important ground-truth wrinkle behind the directive. **BioRouter has no
truly extension-free `shell`, `edit`, or `find` tool.** The tools the user thinks of
as "fundamental" — `shell` and `text_editor` — are provided by the **`developer`
extension** (`crates/biorouter-mcp/src/lib.rs`, `BUILTIN_EXTENSIONS["developer"]`),
which is a real, loadable, disable-able extension that merely happens to be enabled
by default in essentially every session. The only always-present, genuinely
extension-free tools are the gated `platform__*` tools (`ingest_conversation`,
`manage_schedule`, `read_session_blob`), `workflow__final_output`, and
`subagent`/`subagent_status` (which paradoxically require at least one extension to
be loaded). None of those do file or shell work.

So "tools without any extensions" cannot be read literally. The faithful reading of
the directive is a **tier model** by simplicity, not by extension-vs-not:

- **Tier 1 — primitives.** The simplest tool that directly does the job. For file
  and system work this is the `developer` extension: `shell` and `text_editor`. These
  are the default, first-choice tools for anything "straightforward and easy."
- **Tier 2 — specialized extensions.** Reach here only when a task genuinely needs a
  capability a primitive lacks: real computation / control flow / multi-call chaining
  (`code_execution`), figures (`autovisualiser`), a knowledge base (`knowledge` /
  `bokf`), browser automation (`playwright`), data queries, app building
  (`agent_drafter`), and so on.

**This tier reading is an interpretation — please confirm or correct it.** If instead
you want the literal set (only `platform__*` and friends treated as "fundamental"),
the routing guidance would look very different, because those tools cannot list or
edit files at all.

## Tier table

| Tier | Tools | Reach for it when… |
|------|-------|--------------------|
| **1 — primitives** | `developer/shell`, `developer/text_editor` | Listing, reading, writing, editing, copying, moving, deleting, or finding files; running one-off commands; anything straightforward. **Default.** |
| **1 — always-on platform** | `platform__ingest_conversation`, `platform__manage_schedule`, `platform__read_session_blob`, `subagent` | Saving a conversation to a KB; scheduling a workflow; re-reading a large externalized tool output; delegating a bounded sub-task. |
| **2 — computation / chaining** | `code_execution` (`execute_code`, `search_modules`, `read_module`) | Several **dependent** tool calls whose outputs feed each other in one round-trip, or real loops/aggregation/conditionals over their results. |
| **2 — specialized domains** | `autovisualiser/*`, `knowledge`/`bokf_*`, `computercontroller/*`, `playwright/*`, `agent_drafter` (`files_server`, `compute_server`, `appcontrol`), data-query extensions | The task is squarely in that extension's domain (a figure, a knowledge base, GUI/browser automation, an app sandbox). |

## Per-tool "when to use / when not to"

### `developer/shell`
- **Use for:** running commands; `ls`/`cp`/`mv`/`rm`/`mkdir`; finding files or text
  with `rg` (never `find` or `ls -r`); chaining a couple of commands with `&&`;
  long-lived processes via `background=true` + `shell_wait`/`shell_output`/`shell_kill`.
- **Don't:** use it to dump a whole file to read it (`cat`/`head`) — use
  `text_editor` view; produce huge output (pipe to a file); busy-loop with `sleep`
  (use `shell_wait`).

### `developer/text_editor`
- **Use for:** reading a file (`view`), creating/overwriting (`write`, full content),
  editing (`str_replace`, or a multi-file unified `diff` in one call; `insert`).
- **Don't:** read/write files via shell `cat`/`sed`/`echo >`, or via a code-execution
  script, when this tool does it directly.

### `code_execution/execute_code`
- **Use for:** two or more **dependent** MCP tool calls in one round-trip; running
  computation, loops, conditionals, or aggregation over tool outputs.
- **Don't:** list a directory, read/write a single file, or copy/move/delete — call
  `developer/shell` or `developer/text_editor` directly. Don't wrap a single tool call
  in a script.

### `code_execution/search_modules`, `read_module`
- **Use for:** discovering which installed MCP tool/module to `import` inside an
  `execute_code` script, and its signature.
- **Don't:** treat `search_modules` as a web/knowledge/general search. It searches the
  **local tool catalog** only and answers no questions. For a factual or web-research
  question, use a web tool or answer directly.

### `computercontroller/automation_script`, `web_scrape`, `computer_control`
- **Use for:** GUI/system automation (`computer_control`); a fetch of a known URL
  (`web_scrape`); a small saved script where a one-off `shell` command won't do
  (`automation_script`).
- **Don't:** use `automation_script` for what a single `developer/shell` command does;
  don't fetch a URL three different ways (shell `curl`, `web_scrape`, and a script).

### `files_server` / `compute_server` (Agent Drafter app sandboxes)
- **Use for:** file and shell/Python work **scoped to a built app's sandbox**.
- **Don't:** use them as general file/shell tools for the user's own workspace — that
  is `developer`'s job.

## Overlap matrix

Rows are capabilities; cells mark tools that can do it. **Bold** = the tool to prefer.

| Capability | developer | code_execution | computercontroller | files/compute_server |
|------------|-----------|----------------|--------------------|----------------------|
| List a directory | **shell `ls`** | execute_code | automation_script | list_dir (sandbox) |
| Read a file | **text_editor view** | execute_code | — | read_text_file (sandbox) |
| Write a file | **text_editor write** | execute_code | — | write_text_file (sandbox) |
| Copy/move/delete | **shell** | execute_code | automation_script | — |
| Find files/text | **shell `rg`** | execute_code | — | — |
| Run one command | **shell** | execute_code | automation_script | compute_server/shell (sandbox) |
| Chain N dependent calls + logic | (serial calls) | **execute_code** | — | — |
| Fetch a URL | shell `curl` | execute_code fetch | **web_scrape** | — |
| Run Python | shell `python3` | execute_code | automation_script | **compute_server/python** (sandbox) |

The rule the matrix encodes: **the leftmost bold cell wins for a simple task**;
`execute_code` wins only in the "chain N dependent calls + logic" row.

## Where the guidance now lives (so it stays in sync)

- **System prompt** — `crates/biorouter/src/prompts/system.md`, `# Tool Routing`
  section (renders in every mode, including code-execution mode where per-extension
  instructions are hidden). A one-line version is in
  `crates/biorouter/src/prompts/system_small_local.md`.
- **code_execution extension** — server `instructions` and the `execute_code` /
  `search_modules` tool descriptions in
  `crates/biorouter/src/agents/code_execution_extension.rs`.
- **developer extension** — base `instructions` and the `shell` tool description in
  `crates/biorouter-mcp/src/developer/rmcp_developer.rs`.

---

## DEPRECATION PROPOSAL — awaiting user approval (nothing removed yet)

The following are **candidates** to reduce tool overlap. **None has been removed,
disabled, or changed** by this work — this is a proposal only. Do not act on it
without explicit approval; each carries a migration note.

1. **`computercontroller/automation_script` (shell mode).**
   - *Rationale:* overlaps `developer/shell` almost entirely; its own description
     already says "Consider using shell script (bash) for most simple tasks first."
     The shell/script split confuses routing.
   - *Migration:* keep the Ruby/PowerShell path if anything depends on it; route
     plain shell scripts to `developer/shell`. Or narrow its description to
     "non-shell scripting only."

2. **`computercontroller/web_scrape` vs. shell `curl` vs. execute_code fetch.**
   - *Rationale:* three surfaces fetch a URL. `web_scrape` is the nicest (caching,
     text/JSON extraction) but the redundancy invites inconsistent choices.
   - *Migration:* designate `web_scrape` the single canonical URL-fetch tool in
     docs/prompts (done, in the routing text); consider dropping the "fetch web
     content" line from the shell description if `web_scrape` is always present.

3. **`files_server` / `compute_server` naming.**
   - *Rationale:* `list_dir` / `read_text_file` / `write_text_file` / `shell` /
     `python` read as generic file/shell tools but are **sandbox-scoped** to Agent
     Drafter apps; the descriptions don't state that boundary.
   - *Migration:* not a removal — **rename/redescribe** to make the sandbox scope
     explicit (e.g. "…inside the app sandbox") so the model never picks them for the
     user's own workspace. (No code change proposed here beyond the doc note.)

4. **`code_execution` few-shot examples that modeled file-copy.**
   - *Status:* **already fixed** in this change — the "copy a file" and bare "ls a
     directory" examples were replaced with genuine dependent-chaining examples, and
     explicit negative guidance was added. Listed here for the record, not as a
     pending action.

No tool is deprecated by merging this document. Approval is required before any of
candidates 1–3 are implemented.
