# Desktop Reliability and Regression Review — July 2026

## Scope

This review covers the conversation, provider setup, model compatibility, tool-call,
and notification defects reported against the July 2026 desktop build. It records
the observed failure modes, root causes, implementation changes, and the regression
checks required before and after publishing the integrated `main` branch.

## Defect and fix matrix

| Area | Reported behavior | Root cause | Resolution |
| --- | --- | --- | --- |
| Conversation startup | A new conversation replaced the whole app with `z is not a function`. | React's external-store subscription received an unstable or incorrectly shaped callback across chat-store lifecycles. | Added a stable subscription adapter and regression tests, and hardened the chat boundary so a malformed store update cannot take down the application shell. |
| Diverge | A branch could swap the user's prompt with the assistant response. The message action and title-menu action behaved differently. | The two entry points inferred the branch point and message role independently, and one path selected the adjacent message rather than the requested turn. | Centralized branch-point selection in `useDiverge`, made both entry points use the same turn semantics, and added role/order assertions. |
| Stream/provider errors | Quota, connection, and decode failures replaced the conversation with a full-page “Failed to Load Session” error. | A turn-local stream failure was persisted and rethrown as a session-loading failure. The integrated UI also rendered both the persisted assistant failure and the structured turn card. | Store structured turn errors beside the affected user turn, keep the transcript and composer available, classify wrapped provider failures from their preserved details, and suppress the structured card when the same failure is already visible in the transcript. |
| Tool-call expansion | Expanding an unsuccessful or partial tool call crashed while reading `undefined.length`. | The renderer assumed every response contained the complete content-array shape. | Normalize absent and legacy tool responses, provide readable fallbacks for malformed payloads, and test missing content, errors, and empty outputs. |
| Reasoning plus tools | Some OpenAI reasoning models received unsupported `/v1/chat/completions` requests with `reasoning_effort` and tools. | Model capability metadata did not participate in transport selection, so compatible reasoning settings could still use an incompatible endpoint. | Route reasoning-and-tool requests that require it through the Responses format, preserve Chat Completions for compatible models, and test the transport/capability combinations. |
| Provider auto-detection | A key was detected on onboarding, but the next model-selection screen did not retain the provider/model. | The UI reused a stale provider catalog, guessed the provider's key-storage field, and did not wait for persistence before advancing. | Return the authoritative `api_key_config_key`, trim and await key persistence, force one catalog refresh, and preselect the detected model. Coverage includes all configured commercial providers. |
| Session notifications | Delete/update notifications used different spacing and could overlap page controls. | Session views had bespoke toast markup and positioning outside the shared notification surface. | Migrate session and remaining raw notifications to `NotificationSurface` and the shared toast helpers. |
| Toast alignment | Notification text sat a few pixels above its status icon and close button. | The 28 px status chip, text line-height, and 20 px close affordance used different alignment origins. | Standardize notification geometry, including text padding and close-button offset, across success, info, warning, error, and loading states. |

## Integration notes

- `codex/homepage-activity-cache` and `codex/settings-reset-panel` were already
  ancestors of `main` when this batch was integrated.
- `fix/turn-error-hardening` supplies the backend structured-error contract and is
  merged with the richer inline-error UI in this batch.
- Generated OpenAPI files are refreshed from the server definition after the merge;
  they are not edited as source files.
- The landing site intentionally advertises `1.88.1`; `1.88.2` is treated as a
  retracted build by the consistency check and download metadata.

## Behavioral contracts

### A failed turn is not a failed session

Session-loading errors are reserved for failures that prevent the session itself
from being read. Provider, quota, network, stream-decode, and tool execution errors
belong to the active turn. They must remain inline, must not remove prior transcript
content, and must not disable retry, copy, diverge, or subsequent prompts.

### Divergence preserves role and ordering

Message-level Diverge is available on a completed assistant answer and includes that
answer in the new branch. Title-menu Diverge uses the same inclusive boundary at the
most recent completed assistant answer. Editing a user message is a separate
edit-diverge path that truncates immediately before the edited prompt. Every path
must preserve message ordering and roles; assistant text may never be reinterpreted
as user input.

### Provider detection is an end-to-end transaction

Detection is complete only when the API key is persisted under the provider's
declared configuration key, the provider catalog is refreshed, and the matching
model is selected on the next screen. A successful key probe alone is insufficient.

### A generated session title is renderer-wide state

The backend may generate a title after any completed turn, including a tool-first or
failed turn. Once persisted, that title must replace the default placeholder in the
conversation header, desktop window title, sidebar Recents, Home recent chats, and
full History. Session identifiers use a variable-width daily counter such as
`20260716_1`; renderer refresh logic must not assume a fixed-width suffix.

### Notification geometry is shared

All desktop notifications use the same status chip, text baseline, close affordance,
width constraints, stacking region, and progress treatment. Feature views supply
content and severity, not custom toast layout.

## Regression risk review

| Risk | Mitigation |
| --- | --- |
| Duplicate stream failures after reconnect | Turn errors have stable identities and replace/update the affected turn rather than appending session failures. |
| Legacy sessions containing incomplete tool payloads | Rendering accepts missing arrays and unknown payload shapes without assuming `.length` exists. |
| Provider refresh loops | The model switcher performs one explicit refresh after successful persistence, not a render-driven poll. |
| Reasoning behavior changes for models that already worked | Endpoint selection is capability-driven; ordinary chat and known-compatible tool calls keep their prior format. |
| Toast text wrapping into close controls | Shared right padding and width constraints reserve the close-button area at every supported severity. |
| Branching from the latest or failed message | Tests cover message-level and title-menu divergence with user, assistant, and error turns. |
| Backend title persists while renderer views remain stale | Every completed turn refreshes session lists, while default-name sessions poll for the backend rename and broadcast the resolved name to all renderer consumers. |

## Verification record

### Automated checks

- Focused desktop reliability suite: 12 files and 114 tests passed.
- Full desktop suite: 131 files and all 978 tests passed. The run initially exposed
  a fixed-date `RecentChats` fixture and load-sensitive timeouts; the date fixture is
  now relative to the test runtime, and the timeout cases passed both alone and in
  the clean full rerun.
- Provider auto-detection: 9 core tests passed; onboarding, provider-guard, model
  context, and model-switcher coverage also passed in the desktop suite.
- OpenAI/Azure reasoning capability matrix: 9 tests passed, including o4-mini tool
  calls through the Responses API.
- Wrapped provider-error classification: 3 tests passed.
- Server configuration routes: 3 tests passed.
- TypeScript typecheck, ESLint, Prettier, and all 128 notification contrast
  assertions passed.
- Full `biorouter` library suite: all 1,396 tests passed. Two timing-sensitive test
  harnesses were made deterministic: the GCP token test now asserts the stable
  error variant, and the hook history cap is tested directly without subprocess
  scheduling assumptions.
- `cargo fmt --check` and the complete `scripts/clippy-lint.sh` workflow passed,
  including the baseline rule and banned-TLS checks.

### Post-integration application checks

The integrated desktop application was launched with an isolated BioRouter data root
and inspected through its accessibility tree, screenshots, renderer events, and API
traffic. No real provider credential or existing user session was used.

- A new conversation opened without the `z is not a function` error boundary.
- A detected OpenAI key was persisted under `OPENAI_API_KEY`, the provider catalog
  refreshed exactly once, `gpt-4.1` was preselected in the model dialog, and the home
  composer retained it after selection.
- A controlled provider 401 kept the transcript and composer visible. It exposed a
  duplicate persisted/structured error and a generic error classification; both were
  corrected during this pass. The resulting view contains one inline failure and no
  full-page “Failed to Load Session” or “Honk!” state.
- The final provider 401 check was repeated after rebuilding and restarting the real
  development backend. The new turn again produced exactly one inline authentication
  failure, left the composer enabled, and reported no renderer or console errors.
- Message-level and title-menu Diverge both included the selected/latest completed
  assistant answer and preserved the user and assistant roles.
- An imported failed-tool fixture expanded to its error detail without crashing;
  the long diagnostic wrapped inside the tool card.
- Success, information, warning, error, and loading notifications were inspected as
  a five-toast stack. Measured icon/text center deltas were `0px` for all severities,
  and no notification text intersected its close control.
- Renderer monitoring reported no uncaught page errors or console errors throughout
  the provider, divergence, tool, and notification checks.

All automated and application checks above were run against the follow-up tree before
its final push.

### Session-title synchronization follow-up

- Reproduced a completed tool-using conversation whose database row was already
  named while the header and sidebar still displayed `New Session`.
- Added a tool-first controller regression covering the real variable-width session
  identifier format and the History/Recents completion event.
- Repeated the conversation against an isolated local provider. The generated,
  duplicate-safe title `Apple Watch News 2` appeared identically in the desktop
  window title, conversation header, sidebar Recents, Home recent chats, and full
  History without reopening the session.
- The focused title/sidebar suite passed 39 tests. The full desktop suite passed 988
  of 989 tests in one single-worker run; the only timeout, an unrelated extension
  modal test, passed immediately when rerun alone. Typecheck, ESLint, Prettier, and
  all 128 contrast assertions passed.
