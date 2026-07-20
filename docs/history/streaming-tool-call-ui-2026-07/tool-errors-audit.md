# Tool-errors audit — every tool failure in QA rounds 1 and 2

> **What this is.** A sweep of every tool error the QA rounds surfaced in the daemon logs, each
> classified as an intended rejection or a genuine defect, plus a follow-up sweep of the
> `code_execution` sandbox's module errors.
> **Status:** Historical record (rounds 1–2 completed 2026-07-19; follow-up sweep on the
> sandbox module errors added 2026-07-20).
> **Audience:** developers working on tool dispatch, the developer extension's path jail, and
> permission gating.

The audit was commissioned because a user hit `Path '…' is outside the working directory` during
a BioOKF build and it was not clear whether the gate was doing its job. Eleven distinct errors
appear below; six of them turn out to be the same single defect, and the rest are the system
correctly refusing something. The audit also exposed a logging gap — no single log target carried
tool name, outcome and error text together — which was closed as part of the same work.

Scope: every tool error surfaced during the 2026-07-19 QA rounds (rounds 1–2, incl. the
BioOKF `/tmp/biookf-rebuild` parallel build), swept from the daemon logs at
`~/.local/state/biorouter/logs/server/2026-07-19/*.log`. Each is classified
**INTENDED** (model misuse correctly rejected, an approval decline, or a genuine
user-/environment-level error) vs **DEFECT** (an architectural gate contradicting
the intended policy, or a wrong/misleading error surface).

Two error surfaces appear in the logs:
- `WARN rmcp::service: … response error … ErrorData { code: -32602/-32603, message: … }`
  — a tool returning a hard `Err(ErrorData)`.
- `"Error: Module error: Error: Tool error: …"` / `"Error: Parse error: …"` — a
  `code_execution__execute_code` script's result (`is_error`) wrapping the failure
  of a tool it imported and called *inside* the sandbox. "Module error" = an inner
  imported tool (e.g. `developer.text_editor`) failed; "Parse error" = the Boa JS
  engine rejected the script source.

## Classified errors

| # | Error message (as logged) | Count | Surface / origin | Classification | Rationale |
|---|---------------------------|-------|------------------|----------------|-----------|
| 1 | `Path '/tmp/biookf-rebuild/SCHEMA.md' is outside the working directory` | 4 | code_execution → inner `developer.text_editor` write → `resolve_path` jail (`rmcp_developer.rs:1785/1805/1816`) | **DEFECT** | The user-reported BioOKF error. Working root was `/tmp/biookf-rebuild`, but the session cwd differed, so the developer `text_editor` jail rejected a legitimate write. The jail is **mode-blind** (see verdict below): in Fully-Automatic mode broad filesystem access is intended, approval-gated only for sensitive ops. `/tmp/biookf-rebuild` is neither sensitive nor secret. Wrong gate for the mode. |
| 2 | `Path '/tmp/biookf-rebuild/README.md' is outside the working directory` | 1 | same as #1 | **DEFECT** | Same jail, same BioOKF build, different target file. |
| 3 | `Path '/tmp/qa2a.txt' is outside the working directory` | 1 (rmcp) + 24 (code_exec wrap) | same jail | **DEFECT** | Round-2 setup write to `/tmp`. Same mode-blind jail. The high wrapped count reflects the model retrying the write through `execute_code`. |
| 4 | `Path '/tmp/qa-r1/calc.py' is outside the working directory` | 1 (rmcp) + 2 (wrap) | same jail | **DEFECT** | Round-1 write to `/tmp/qa-r1`. Same jail. |
| 5 | `Path '/tmp/br_b2_test.py' is outside the working directory` | 1 | same jail | **DEFECT** | Round-2 write to `/tmp`. Same jail. |
| 6 | `Path '/tmp/biookf-s2-task.txt' is outside the working directory` | 1 | same jail | **DEFECT** | BioOKF session-2 task-file write to `/tmp`. Same jail. |
| 7 | `HTTP request failed with status: 403 Forbidden` | 1 (rmcp) + 12 (wrap) | `computercontroller/web_scrape` (round-1/2 "capital of Australia" web fetch) | **INTENDED** | A real upstream HTTP 403 from the fetched site. Genuine environment/network error, surfaced accurately. The model retried against other sources and answered correctly (Canberra). No architectural issue. |
| 8 | `Shell command was cancelled by user` | 1 | `developer/shell` cancellation | **INTENDED** | Correct, accurate surfacing of a user-initiated cancel (also the path that validated mid-turn `cancel`). Working as designed. |
| 9 | `Parse error: SyntaxError: … got 'raw' in object literal` | 4 | Boa JS engine rejecting `String.raw` tagged-template literals in an `execute_code` body | **INTENDED (with a filed minor gap)** | Not an architectural policy violation — the sandbox JS engine genuinely does not support `String.raw` tagged templates (filed as R2-03). The error is accurate; the model self-recovered by retrying with plain strings. Ergonomics gap, not a defect in the gate sense. |
| 10 | `Parse error: SyntaxError: … got 'id' in object literal` | 8 | Boa JS engine rejecting a script's object-literal syntax | **INTENDED** | Genuine model-authored JS syntax error correctly rejected by the sandbox parser. Model-level mistake, accurately surfaced. |
| 11 | `Parse error: SyntaxError: unexpected token '%' …` | 8 | Boa JS engine rejecting a script's syntax | **INTENDED** | Genuine model-authored JS syntax error, correctly rejected. |

Non-tool errors also seen in the logs and excluded as out of scope (not tool-call
results): repeated `extension_manager` "Failed to fetch secret … SPOKEAGENT_PASSCODE
/ CLINICAL_RECORDS_USERNAME" (uninstalled-extension credentials — expected in this
env), `local_workflows` "missing field `title`" while scanning stray JSON files in
`$HOME`, and `tunnel::lapstone` WebSocket resets. None are tool-dispatch outcomes.

## The defect, in one line

Errors #1–#6 are the **same single defect**: the `developer/text_editor` path jail
(`resolve_path` in `crates/biorouter-mcp/src/developer/rmcp_developer.rs`) is a hard,
**BioRouterMode-unaware** confinement to the session working directory. It fires on
every write outside cwd even in Fully-Automatic mode, where the intended policy is
broad access with approval only for sensitive ops (`security/sensitive_ops.rs`,
commit `1079f909`). It is also **inconsistent with `developer/shell`**, which takes a
per-call `working_directory` and runs anywhere with no such jail — so the model can
write `/tmp/…` via `shell` but not via `text_editor`, an arbitrary asymmetry the model
cannot predict.

## Why this is not a routing defect

The tool the model chose for the SCHEMA.md write was `text_editor` (write), invoked
from inside a `code_execution__execute_code` script (hence the "Module error" wrapper).
Choosing `text_editor` for a single-file write is the **correct** primitive per
[tool routing](../../agent-loop/tool-routing.md); the `execute_code` wrapper is *forced*, not a routing miss —
with `code_execution` enabled (default) `prepare_tools_and_prompt` strips the bare
`developer/*` tools from the callable set (round-2 observation R2-06), so the model has
no way to call `text_editor` directly. **Not a routing defect.**

## Logging gap this audit exposed (now fixed)

Before this audit, no single log target carried *tool name + ok/error + error text*
together. The failures had to be reconstructed from three disjoint sources: raw
`rmcp::service` `WARN response error` lines (no tool name, no ok/error field),
`TOOL_EXEC_START/END` `debug` markers (timing only), and the quoted error strings
buried inside `code_execution` result payloads in `LLM_REQUEST` dumps. A single
always-on `info`-level line keyed `target: "tool_result"` was added at the universal
dispatch choke point (`Agent::dispatch_tool_call`, `crates/biorouter/src/agents/agent.rs`)
— see [tool routing](../../agent-loop/tool-routing.md), section "Tool-result logging". Filter with
`RUST_LOG=tool_result=info`.

## Follow-up sweep — `Module could not be found` (2026-07-20)

A user reported `Module error: TypeError: Module could not be found.` as happening "very
often". This sweep re-ran the audit method over everything available since: the session
store (`~/.local/share/biorouter/sessions/sessions.db`), the daemon logs
(`~/.local/state/biorouter/logs/server/**`, `llm_request.*.jsonl`) and the Electron main
log. It found the reported error is **rare and secondary**, and that the error the user
was actually drowning in is a different one that precedes it.

### What the sandbox's module resolution actually does

`code_execution__execute_code` runs the script in Boa with one synthetic module per
**extension (server) name** — `developer`, `computercontroller`, `autovisualiser`, and
whatever else is enabled — each exporting that extension's tools plus a namespace object
under the server's own name. Resolution is a plain map lookup on the import specifier.
There are no other modules: no Node or browser standard library, no filesystem behind the
loader, no `require`, no `fetch`. Anything not in that map is a miss.

### Frequency table (unique real failures, all sources de-duplicated)

Log hits over-count badly — each `LLM_REQUEST` dump replays the whole conversation, so one
failure reappears in every later turn. The counts below are unique tool results.

| Error, as the model saw it | Unique | Offending specifier / cause | Classification |
|---|---|---|---|
| `TypeError: Module could not be found.` | 2 | `fs` (×1), `node:child_process` (×1) — both Node builtins | **DEFECT (error surface)** |
| `TypeError: not a callable function` | 7 | a string method called on a tool result that came back as parsed JSON | **DEFECT (false contract + error surface)** |

Across all 184 `execute_code` calls in the store, the import-specifier census is
`developer` 166, `computercontroller` 9, `autovisualiser` 8, `agent_drafter` 8, `fs` 1,
`./sdk` 2, `skills` 1, `jupytermcpserver` 1. Real extension names dominate; the model is
not routinely inventing modules.

### Root cause — the two errors are one incident

Session `20260720_1` shows the causal chain. The model imported `{ shell, text_editor }`
from `developer` — the correct module, the correct tools — and got `not a callable
function`. It retried the same import as a namespace, then with bracket access; same
error each time, because the error names neither the value nor the call site, so there was
nothing to correct. Only after three identical dead ends did it abandon `developer`
entirely and guess `import fs from "fs"` — which produced `Module could not be found`.

So the user-reported error is a **downstream symptom**. The `not a callable function`
error is the one to fix, and its cause is a false promise in the tool's own description:
it stated "All calls are synchronous, return strings", while `parse_result_to_js`
JSON-parses any result that parses. A tool answering with JSON therefore returns an
*object*, and `shell({…}).trim()` throws. Reproduced directly against Boa: calling
`.trim()` on a parsed result yields exactly `TypeError: not a callable function`.

Neither error is model misuse in the usual sense — the guidance was wrong, and both error
messages were dead ends. **Both are DEFECTs**, in the audit's "wrong or misleading error
surface" sense, and the first is also a false contract.

### What changed

In `crates/biorouter/src/agents/code_execution_extension.rs`:

- Replaced Boa's `MapModuleLoader` with a `ToolModuleLoader` that matches specifiers
  verbatim (there are no path semantics here) and, on a miss, reports the module that
  failed, the exact importable set, a case-insensitive "did you mean", and — for the
  Node/browser builtins that are the guesses actually observed — the `developer` import
  that replaces them.
- `annotate_opaque_js_error` appends the likely cause and the recovery to
  `not a callable function`, which Boa emits with no other context.
- Corrected the `execute_code` description: a call returns a parsed object for a JSON
  result and a string otherwise. Added a `MODULES:` block stating the importable set is
  closed and naming the absent builtins.
- The live module inventory (`get_moim`) now says those modules are the only importable
  ones and restates the return-type rule.

In `crates/biorouter/src/prompts/system.md`, one bullet in `# Tool Routing` carries the
same "those and only those" rule; the three `prompt_manager` snapshots were regenerated
and their diff is that bullet alone.

### Gates

Six unit tests in `code_execution_extension.rs` and two end-to-end tests in
`tests/code_execution_integration.rs` (`case24`, `case25`), the latter driving a real
`ExtensionManager` through `dispatch_tool_call` — the same path the agent loop uses, so
they assert the text the model actually receives. Each is proven by revert: restoring
`MapModuleLoader` reproduces the user's verbatim string, and neutering the annotator
reproduces the other.

| Under revert | Now |
|---|---|
| `Error: Module error: TypeError: Module could not be found.` | `Error: Module error: TypeError: Module "fs" could not be found. This sandbox has no Node.js or browser standard library, so there is no "fs", "path", "os", "child_process", "http", or "fetch". For filesystem and command work import from "developer" instead: import { shell, text_editor } from "developer"; Importable modules are exactly: developer — nothing else can be imported. Call search_modules to find which one holds the tool you need, or read_module("<module>") to list its tools.` |
| `Error: Module error: TypeError: not a callable function` | the same, plus: `— you called something that is not a function. A tool call returns a parsed object when the tool's result is JSON, and a string otherwise, so string methods such as .trim()/.split() fail on a JSON result. Inspect the shape first (record_result(value)) or convert it with JSON.stringify(value).` |

The module list is the session's live inventory, not a hardcoded set — `case24` asserts it
reports the extensions that manager really has.

### Live verification

Two headless CLI runs against the configured provider (`versa_azure`,
`gpt-5.5-2026-04-24`), sessions `20260720_24` and `20260720_25`.

**Prevention.** Told explicitly to make `import fs from "fs";` the first line of its
script, the model instead imported `{ shell, text_editor } from "developer"` and never
attempted `fs` at all. Zero module errors in the session.

**Self-correction.** The second run reproduced the real-world `not a callable function`
naturally — the model wrote `shell({…}).trim()` (message 17848) and got the annotated
error (17849). Its very next script (17850) applied the remedy the error names, checking
the shape before calling a string method:

```javascript
dependencyCount: typeof depRaw === "string" ? depRaw.trim() : depRaw,
```

It kept that pattern for the rest of the run and answered correctly. One error, one
recovery, no loop — against the historical pattern of three identical retries followed by
a fallback to a Node builtin.

Not verified: the desktop GUI path. The dispatch path these runs exercise is the same one
the GUI drives, and the error text is produced below any interface, but no Electron run
was made.

## Related documentation

- [Streaming tool-call UI campaign](README.md) — the campaign index this audit belongs to.
- [Tool routing](../../agent-loop/tool-routing.md) — the living guidance on which tool the agent should reach for, and the home of the tool-result logging reference.
- [QA round 2 results](qa-round-2-results.md) — the round whose BioOKF build produced most of the errors classified here.
- [Campaign final report](campaign-final-report.md) — where the defect found here is recorded against its fix.
