# Tool-errors audit — 2026-07-19 QA rounds

Scope: every tool error surfaced during today's QA rounds (rounds 1–2, incl. the
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

## Routing note

The tool the model chose for the SCHEMA.md write was `text_editor` (write), invoked
from inside a `code_execution__execute_code` script (hence the "Module error" wrapper).
Choosing `text_editor` for a single-file write is the **correct** primitive per
`docs/tool-routing.md`; the `execute_code` wrapper is *forced*, not a routing miss —
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
— see `docs/tool-routing.md` § "Tool-result logging". Filter with
`RUST_LOG=tool_result=info`.
