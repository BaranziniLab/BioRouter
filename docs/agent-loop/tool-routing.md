# Tool routing — which tool for which job

> **What this is.** The canonical reference for tool selection: which tool the agent should
> reach for, in what order of preference, where the overlaps are, and where every dispatched
> tool call's outcome can be inspected.
> **Status:** Current, with two items open — the tier model in the next section is an
> interpretation awaiting confirmation, and the deprecation proposal near the end has been
> approved by nobody and removed nothing.
> **Audience:** developers working on the agent loop, extension authors deciding how to describe
> a tool, and agents choosing a tool at runtime.

BioRouter exposes several tools that can each list a directory, read a file, or run a command.
Without explicit routing guidance the model picks the wrong one — writing a JavaScript script to
`ls` a directory or copy a file, or calling a tool-discovery search to answer a web question. This
page states the preference order once, and names the three places in the source where the same
guidance is mirrored so they can be kept in sync.

The user's directive: *"when the situations are somewhat more straightforward and
easy, we should almost always rely on the most basic fundamental tools inside of
BioRouter — the tools without any extensions. Only when there are more complicated
tasks should we think about using the extensions."*

## Interpretation: a tier model, pending confirmation

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
| **1 — always-on platform** | `platform__ingest_conversation`, `platform__manage_schedule`, `platform__read_session_blob`, `subagent` | Saving a conversation to a knowledge base (KB); scheduling a workflow; re-reading a large externalized tool output; delegating a bounded sub-task. |
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
- **Use for:** two or more **dependent** MCP (Model Context Protocol) tool calls in one round-trip; running
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

## Deprecation proposal — awaiting approval, nothing removed yet

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

## Tool-result logging (observability of every tool call)

There is now **one always-on place where every dispatched tool call's outcome is
inspectable**, regardless of extension, error surface, or interface (GUI/CLI).
`Agent::dispatch_tool_call` (`crates/biorouter/src/agents/agent.rs`) emits a single
structured `tracing` line at **`info`** level, keyed **`target: "tool_result"`**,
immediately after the tool future resolves — the universal choke point every tool
call passes through:

- `tool` — the tool name (e.g. `code_execution__execute_code`, `developer__text_editor`)
- `id` — the request id
- `ok` — `true` / `false`
- `dur_ms` — execution duration (excludes time parked on the concurrency semaphore)
- `error` — present only on failure; the error message text

It captures **both** error surfaces: a hard `Err(ErrorData)` (e.g. the developer
`text_editor` jail's `INVALID_PARAMS "… is outside the working directory"`) **and**
an `Ok(CallToolResult)` flagged `is_error: Some(true)` (the MCP "tool ran and reported
a failure" path, e.g. a `code_execution` script whose inner tool failed). Kept cheap:
the error string is only materialised on the failure path; the success path logs no
payload.

Filter the logs with `RUST_LOG=tool_result=info` (or grep `target="tool_result"`)
to get a clean, one-line-per-call ledger of what the agent did and what failed. This
complements the pre-existing `TOOL_EXEC_START`/`TOOL_EXEC_END` **`debug`** markers
(timing only, no ok/error) and the raw `rmcp::service` `WARN response error …` lines
(per-server, no tool-name/ok context).

## Related documentation

- [The agent loop](README.md) — the loop that dispatches every tool call routed by this page.
- [Extensions and skills](../extensions/extensions-and-skills-guide.md) — how the extensions providing these tools are installed, enabled and described.
- [Built-in extensions](../extensions/built-in/README.md) — the reference page for each shipped extension named in the tier table.
- [Streaming tool-call UI campaign](../history/streaming-tool-call-ui-2026-07/README.md) — the July 2026 campaign that wrote this guidance and the tool-result logging described above.
- [Tool-errors audit](../history/streaming-tool-call-ui-2026-07/tool-errors-audit.md) — the log sweep that motivated the always-on `tool_result` line.
