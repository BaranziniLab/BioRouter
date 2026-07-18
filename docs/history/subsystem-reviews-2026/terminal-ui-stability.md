# Terminal UI stability audit — July 2026

> **What this is.** An audit of whether the July 2026 desktop and provider hardening work destabilized the CLI's interactive terminal UI, recording two fixes — a retry-budget correction for permanent provider errors, and a narrow-terminal status layout fix — plus a live tmux verification matrix.
> **Status:** Historical record — both findings were fixed in July 2026 and the verification section closes with passing CLI and library suites. This is a point-in-time audit against one change batch, not living reference for the terminal UI.
> **Audience:** maintainers working on `biorouter-cli` and on the shared provider and agent-loop code.

BioRouter ships two interfaces over one shared core: the Electron desktop
application and the interactive terminal UI in the `biorouter-cli` crate. When the
July 2026 desktop batch changed provider error classification, retry policy, OpenAI
request routing and conversation persistence, those changes landed in the shared
core — so they reach terminal sessions even though no CLI code was edited. This
audit asked whether they broke anything there.

**Date:** July 2026.

## Terms used here

- **Reply loop** — the CLI-side loop that issues model calls within a single turn;
  its *recovery* budget is how many additional attempts one turn may make after a
  failure.
- **Wrapped error** — a provider failure whose HTTP-level cause (a `401`
  authentication rejection, an `insufficient_quota` response) is carried inside an
  outer `ProviderError::ExecutionError` or `ProviderError::UsageError` variant
  rather than being the variant itself.
- **Classified kind** — the underlying category the provider layer derives from a
  wrapped error's preserved details: `server`, `network` or `unknown` (transient,
  worth retrying) versus authentication, quota, invalid-request and other permanent
  kinds.

## Scope

This review checks whether the July 2026 desktop and provider hardening changes
alter the terminal UI's stability or agent execution loop. It covers the shared
provider, retry, persistence, and OpenAI transport paths used by both interfaces,
the terminal UI's rendering and input loop, and a live tmux session against an
isolated OpenAI-compatible test server.

## Impact assessment

The integrated desktop changes did not directly modify `biorouter-cli`, but four
shared-core changes are visible to terminal sessions:

| Shared area | Terminal UI effect | Audited result |
| --- | --- | --- |
| Provider error classification | Determines whether a failed turn is retried or returned inline. | Wrapped authentication and quota failures are now fatal at the agent-loop boundary; network and server failures retain the bounded retry. |
| Reply-loop recovery | Controls the number and ordering of model calls within one turn. | Permanent failures issue one provider request and stop. The input loop remains active for the next command or message. |
| OpenAI request routing | Selects Chat Completions or Responses and decides whether to send reasoning settings. | Ordinary `gpt-4.1` tool turns remain on Chat Completions without `reasoning_effort`; reasoning-and-tool combinations that require Responses use that transport. |
| Conversation persistence | Supplies transcript ordering when a session is reopened or diverged. | Database insertion order preserves user/assistant role order when timestamps are equal. |

No unbounded retry, recursive input-loop re-entry, duplicate submission, or
terminal-mode leak was observed.

## Findings and fixes

| Finding | Fix |
| --- | --- |
| Wrapped permanent provider errors received an extra agent-loop attempt | The reply loop retries `ExecutionError`/`UsageError` only when the classified kind is server, network or unknown. |
| Narrow terminals could concatenate status regions | The status renderer picks full or compact labels from measured display width, and omits the right section when it cannot fit with separation. |

### Wrapped permanent provider errors received an extra agent-loop attempt

`ProviderError::ExecutionError` and `ProviderError::UsageError` were treated as
recoverable solely because of their outer enum variant. The provider layer now
recognizes embedded authentication and quota details, but the reply-loop policy
did not consult that classification. A wrapped 401 or `insufficient_quota` could
therefore consume the one-turn recovery budget before stopping.

The reply loop now retries those variants only when their classified kind is
server, network, or unknown. Authentication, quota, invalid-request, and other
permanent kinds stop immediately. Unit coverage asserts that wrapped 401 and quota
errors never spend the retry budget.

### Narrow terminals could concatenate status regions

At 72 columns, the resource counts and right-aligned context meter could occupy
more cells than the available row. Saturating padding prevented an arithmetic
failure but left no separator, producing text such as `knowledge basescontext`.
The bottom hint could also clip midway through its final phrase.

The status renderer now selects full or compact resource labels based on measured
Unicode display width, falls back to a compact context percentage when necessary,
and omits the right section if it cannot fit with separation. The hint renderer
uses a complete compact variant at narrower widths. A 72-by-24 buffer regression
test protects the layout.

## Live tmux verification

The current debug binary was launched in tmux at 72 by 24 cells with an isolated
`BIOROUTER_PATH_ROOT` and a local OpenAI-compatible streaming server. The fake key
and loopback endpoint prevented access to real credentials, sessions, or provider
traffic.

| Scenario | Result |
| --- | --- |
| Startup and alternate-screen entry | Header, transcript, status, composer, and hints rendered without panic. |
| Slash-command completion | `/help` completed and executed; the UI remained responsive. |
| Streaming response | Partial text appeared before the completed assistant response. |
| Request shape | One `gpt-4.1` request reached `/v1/chat/completions`, included four tools, and omitted `reasoning_effort`. |
| Controlled 401 | Exactly one request was issued; the failure remained inline and the composer stayed usable. |
| Post-error command | `/help` worked after the failed turn, confirming the input loop continued. |
| Narrow layout | Counts, context meter, and complete compact hint rendered without overlap. |
| Exit | Ctrl-D returned exit status 0 and restored the shell, alternate screen, and terminal input mode. |

## Automated verification

> **Note.** The `biorouter --lib` count recorded here (1,396) differs from the
> 1,400 recorded in the tool-discovery hardening review. The two runs were taken
> against different trees at different points in July 2026; neither figure
> supersedes the other.

- `cargo test -p biorouter-cli`: all 192 tests passed.
- `cargo test -p biorouter --lib`: all 1,396 tests passed.
- The CLI suite covers TUI rendering, resize-sensitive layout, streaming previews,
  completion, input/history, queued messages, tool-result collapse, selection,
  paste handling, markdown, and permission dialogs.
- `cargo fmt --check`, `git diff --check`, and the complete
  `scripts/clippy-lint.sh` workflow passed, including baseline-rule and banned-TLS
  checks.

## Related documentation

- [Desktop reliability defects — July 2026](desktop-reliability-defects.md) — the desktop-side record of the same change batch this audit checked the CLI against.
- [Agent tool discovery hardening — July 2026](tool-discovery-hardening.md) — a sibling July 2026 review whose verification section records the differing library test count noted above.
- [CLI QA checklist](../../cli/qa-checklist.md) — the living manual checklist for exercising the terminal UI.
- [CLI command reference](../../cli/command-reference.md) — what `/help` and the other slash commands exercised in the tmux matrix actually do.
- [Agent error model](../../architecture/agent-error-model.md) — the provider error taxonomy behind "classified kind".
