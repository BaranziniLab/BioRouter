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
| Stream/provider errors | Quota, connection, and decode failures replaced the conversation with a full-page “Failed to Load Session” error. | A turn-local stream failure was persisted and rethrown as a session-loading failure. | Store structured turn errors beside the affected user turn and render `ChatTurnError` inline while keeping earlier messages and the composer available. |
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

For a user message, the new branch ends immediately before that prompt so it can be
edited or resent. For an assistant message, the branch includes the triggering user
prompt and excludes the selected assistant response. Both divergence entry points
must calculate the same boundary and may never reinterpret assistant text as user
input.

### Provider detection is an end-to-end transaction

Detection is complete only when the API key is persisted under the provider's
declared configuration key, the provider catalog is refreshed, and the matching
model is selected on the next screen. A successful key probe alone is insufficient.

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

## Verification record

### Pre-integration automated checks

- Focused desktop regression suite: 41 tests passed.
- Provider auto-detection: 9 core tests and the server route test passed.
- TypeScript typecheck and ESLint passed.
- Notification contrast suite: 128 assertions passed.
- Rust formatting check and `scripts/clippy-lint.sh` passed.
- Full desktop suite: 970 of 971 tests passed. The remaining failure is the existing,
  date-sensitive `RecentChats.test.tsx` fixture, which currently resolves to
  “Yesterday” while the assertion expects “Today”; it also fails in isolation and is
  unrelated to this batch.

### Post-integration application checks

The integrated desktop application is tested after the initial `main` push, as
requested. Results, screenshots inspected, and any follow-up corrections are added
to this section in a subsequent documentation commit.

The manual/visual matrix is:

1. Launch and initiate a new conversation without crossing the application error boundary.
2. Diverge from a user turn, an assistant turn, and the conversation-title menu; verify text and roles.
3. Produce a recoverable provider failure and verify the error remains inline.
4. Expand successful, failed, empty, and malformed tool-call responses.
5. Configure a detected provider key through model selection without re-entering it.
6. Exercise success, info, warning, error, and loading notifications at narrow and normal widths.
7. Re-run focused and broad automated suites against the exact integrated tree.
