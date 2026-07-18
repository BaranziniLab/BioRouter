# Agent tool discovery hardening — July 2026

> **What this is.** The record of one hardening pass on code-execution tool discovery — why the agent spent five top-level tool calls finding a web-scrape tool for a simple request, the root causes in lazy module discovery, and the tightened `search_modules`/`read_module` contract that fixed it.
> **Status:** Historical record — the hardening shipped in July 2026 and the verification section below closes against a clean `main` checkout. The "Hardened contract" section describes behaviour that was live at that point; nothing here was still in flight.
> **Audience:** maintainers working on the agent loop and on the developer and computer-controller extensions.

BioRouter exposes extension tools to the model through *code execution*: rather than
listing every tool in the system prompt, the agent discovers tools lazily with
`search_modules`, reads their signatures with `read_module`, and then calls them from
inside an `execute_code` script. Lazy discovery keeps the context small, but it puts
the burden of a good answer on the search result. This review is the record of what
happened when that search result was not good enough, and what changed as a result.

**Date:** July 2026.

## Reproduction

The reproduction case is an exported chat session titled `Apple Watch news`. The
export itself is not stored in this repository and is not linked from here; it is
cited only as the origin of the trace below.

That session used five top-level tool calls for one simple request:

1. `search_modules(["web", "search", "browser", "news"])`
2. `read_module("computercontroller/web_scrape")`
3. `read_module("computercontroller")`
4. `execute_code(...)`, whose nested Python source failed to parse
5. A corrected `execute_code(...)`

The failed script was returned with `isError: false`, so the agent loop recorded
it as a successful action even though its text began with `Script failed`.

## Root causes and the hardened contract

Lazy loading remains enabled so every default extension does not consume the
model context on every request. The hardening changed what discovery *returns*,
not whether it happens.

| Root cause | Hardened behaviour |
| --- | --- |
| Lazy discovery returned only tool names and the first description line, then instructed the model to make another module-read call for signatures. | `search_modules` is now the complete unknown-tool discovery step. It ranks results, caps the list, and returns copy-ready imports plus full required and optional parameter signatures. `read_module` is reserved for a known module when the needed tool was not in the search result, and the prompts explicitly prohibit reading a signature that search already returned. |
| Built-in descriptions commonly start with a formatting newline. Selecting the literal first line therefore produced blank summaries such as `automation_script(...): string -`. | Description summaries use the first nonempty line. |
| Search results used unranked OR matching. A generic term such as `search` surfaced many clinical search tools alongside the web tools. | Search results are ranked and the returned list is capped. |
| `web_scrape` returned only a cache path, forcing another tool lookup/read even when the fetched text was small enough to use immediately. | Text and JSON web responses are returned inline up to 128 KiB while the full response remains cached. Known RSS/news URLs therefore need one underlying fetch, not a cache read or an improvised nested script. |
| Nonzero automation-script exits and MCP `is_error` results were flattened into successful JavaScript strings. | Underlying MCP errors and nonzero scripts fail `execute_code`, allowing the normal mistake/replan logic to observe them. |
| Multiline nested scripts had no guidance to use `String.raw`, allowing backslash escapes to alter the source passed to the inner interpreter. | Multiline script arguments use `String.raw` guidance so backslashes survive the JavaScript boundary. |

## Regression scenarios

- The exact Apple Watch discovery terms rank `computercontroller/web_scrape`
  ahead of unrelated clinical search tools and include a usable import and
  signature without a module read.
- A two-step deterministic loop (`search_modules` then `execute_code`) fetches a
  mocked RSS response and exposes its headline inline.
- A nonzero nested search script produces an error result containing stderr.
- Web responses preserve UTF-8 boundaries when the inline excerpt is capped.
- Existing plain-text and regex module searches, code execution, artifacts, and
  default extension behavior remain covered by their existing suites.

## Verification

All final checks ran from a clean checkout of `main` with an isolated Cargo
target directory.

> **Note.** No commit SHA was recorded for this run, so the counts below identify
> a July 2026 state of `main` rather than a re-checkable revision.

- `cargo fmt --all -- --check`: passed.
- `./scripts/clippy-lint.sh`: passed all workspace, baseline, line-count, and
  banned-TLS checks.
- `cargo test -p biorouter-mcp --lib -- --test-threads=1`: 792 passed, 2
  ignored.
- `cargo test -p biorouter --lib`: 1,400 passed.
- `cargo test -p biorouter --test code_execution_integration`: 24 passed.
- `cargo test -p biorouter-cli --lib session::output::tests -- --nocapture`: 6
  passed.
- A detached 120×40 `tmux` smoke launched the TUI of BioRouter 1.88.2 — the build
  under test at the time of this review — rendered its model/extensions/status UI,
  processed `/help`, and exited with `/exit`.

An initial parallel MCP run exposed an existing test-harness race in which a
global current directory was deleted by another test. The affected test passed
alone, and the full serialized MCP run passed; no production failure was
observed.

## Related documentation

- [Code execution extension](../../extensions/built-in/code-execution.md) — the living reference for `search_modules`, `read_module` and `execute_code`.
- [Computer Controller extension](../../extensions/built-in/computer-controller.md) — documents `web_scrape`, the tool the reproduction case failed to find.
- [Desktop reliability defects — July 2026](desktop-reliability-defects.md) — the sibling July 2026 review covering the desktop surface, including the same 1.88.1/1.88.2 build window.
- [Terminal UI stability audit — July 2026](terminal-ui-stability.md) — the sibling CLI audit, whose library test count differs from the one recorded here.
- [Core loop and tool dispatch review](../agent-loop-review/subsystem-reviews/core-loop-and-tool-dispatch.md) — the broader review of how tool results reach the agent loop, including error propagation.
