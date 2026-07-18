# Image input design

> **What this is.** The design spec for sending pasted, dropped, or picked images to vision-capable models as inline base64 content blocks — adding a `supports_vision` capability flag to per-model metadata and fixing Gemini's silent image drop.
> **Status:** Historical record — written 2026-05-30 and implemented the same day; the work shipped in **v1.76.1** (released 2026-06-01). `supports_vision: Option<bool>` is on `ModelInfo` in `crates/biorouter/src/providers/base.rs`, the `readTempImageAsBase64` IPC binding exists in `ui/desktop/src/preload.ts`, and the Gemini `inline_data` arm is in `crates/biorouter/src/providers/formats/google.rs`.
> **Audience:** developers working on providers, chat input, and message rendering.

BioRouter shipped several vision-capable models but had no way to get image bytes to them: the chat box collected images and then flattened them into file-path text. This spec describes the fix that landed — structured `ImageContent` blocks end to end, plus a per-model capability flag so the UI can hide the attach button on text-only models. The companion task-by-task plan is [the image input implementation plan](image-input-plan.md); this document is the *why*, that one is the *how*.

> **Note on `v-next`.** The original draft used the placeholder token `v-next` for "the release this ships in" and `pre-v-next` for "sessions saved before this change". Those tokens are resolved throughout this rewrite to **v1.76.1**, the release the work actually shipped in. The backward-compatibility semantics are unchanged.

> **Scope.** Images only. PDF, generic file attachments, audio, video, and image generation are explicit non-goals — see [Non-goals](#non-goals).

**Area of change:**

| Layer | Paths |
|---|---|
| Chat input UI | `ui/desktop/src/components/ChatInput.tsx` |
| Message construction | `ui/desktop/src/types/message.ts` |
| Electron main process | `ui/desktop/src/main.ts` |
| Gemini wire format | `crates/biorouter/src/providers/formats/google.rs` |
| Model metadata | `crates/biorouter/src/providers/` (across providers) |

## Problem

BioRouter ships several multimodal LLMs (Claude, GPT-4o, Gemini, Bedrock-Claude) but users today cannot send images to any of them through the chat box. The frontend has substantial UI for pasting, dropping, and picking images — thumbnails, retry/error states, 5-image / 5-MB caps — but `createUserMessage()` collapses every attachment into a file-path token embedded in the message text. The model receives `"describe this /tmp/abc.png"`, not the image.

The backend is in better shape: `MessageContent::Image` exists, sessions store it, and four providers (Anthropic, OpenAI, Bedrock, Databricks) already translate it to wire format. Google/Gemini silently drops it via a catch-all match arm. Snowflake is explicitly skipped with a TODO comment. No provider/model exposes a `supports_vision` capability flag, so the UI cannot gate on it.

The user goal is direct multimodal input: drop a screenshot, the model sees the bytes. No OCR fallback, no auto-routing through tools.

## Goals

- User can paste, drop, or pick a PNG/JPEG/GIF/WebP and the active multimodal model sees the image content.
- UI hides the attach button when the active model lacks vision capability.
- Gemini honors `MessageContent::Image` instead of silently dropping it.
- Sessions saved before this change continue to render correctly (paths embedded in text).
- Existing 5-image / 5-MB-per-image caps and retry/error UX are preserved.

## Non-goals

- PDF attachments (separate design; per-provider wire format differs).
- Generic file attachments (txt, csv, code) — needs separate routing decision.
- Snowflake or other non-vision providers — they stay non-vision; UI gating handles them.
- Auto-downscale of oversized images — user manually resizes; we don't silently mutate input.
- In-UI override of `supports_vision` for custom/local models — for v1 users edit config YAML.
- OCR fallback — explicitly out; the point is native vision.
- Audio/video/webcam/image-generation.
- Storage-layer optimization (deduplicated blob store) — base64 inline matches the existing backend model.
- Hiding behind `ALPHA=true` — ships enabled.

## Chosen approach and rejected alternatives

**Approach A — inline base64 end-to-end.** Frontend reads attached images from temp files into base64 only at send time, builds a structured `content[]` with `TextContent` + `ImageContent` blocks, and sends as-is. The backend's existing `MessageContent::Image` path and the four working providers' format converters handle it without change. The renderer learns to display `ImageContent.data` as a data-URL.

Rejected alternatives:

- **Path reference + server-side encode** — diverges from the backend's "self-contained Message JSON in SQLite" model, adds a path-validation surface on the server, and doesn't reduce provider-call payload size (which is what dominates).
- **Hybrid blob store** — premature; bounded by the existing 5×5 MB UI caps for v1.

## Architecture and data flow

```text
1. User pastes / drops / picks image in ChatInput.
2. ChatInput calls window.electron.saveDataUrlToTemp(...)   [unchanged]
   → temp file written, path returned, thumbnail rendered locally.
3. On Send, for each attached image:
   → window.electron.readTempImageAsBase64(path)            [NEW IPC]
   → returns { data: "<base64>", mimeType: "image/png" }
4. createUserMessage() builds structured content[]:
   [
     { type: 'text', text: "<prompt with path tokens stripped>" },
     { type: 'image', data: '<base64>', mimeType: 'image/png' },
     ...
   ]
5. ChatRequest.user_message sent as-is to /reply.
   Backend stores Message JSON in SQLite (no schema change).
6. Agent calls provider.complete(messages):
   - Anthropic / OpenAI / Bedrock / Databricks: existing converters [unchanged]
   - Gemini: FIXED to emit { inline_data: { mime_type, data } }
   - Non-vision providers: not reachable because UI hides attach.
7. On session reload, renderer sees ImageContent blocks and decodes
   data-URLs inline. The temp-file render path is retained for the
   live-session "image just attached" case and for backward-compat
   with pre-v-next sessions that store paths in text.
```

**Key invariants:**

- Temp file remains the source of truth in the current session. Base64 is read **only at send time**, never held in React state.
- After send, transcript rendering reads from structured `ImageContent.data` — same code path as a reloaded session.
- Pre-v1.76.1 sessions (image paths embedded in text) keep working via the existing `imageUtils.ts` path-extraction fallback. No migration.

## Capability metadata: `supports_vision`

**Where it lives.** A new optional field on the per-model metadata struct:

```rust
supports_vision: Option<bool>  // None → treated as false
```

> **Where it landed.** The spec left the exact home to the implementation plan. It landed on `ModelInfo` in `crates/biorouter/src/providers/base.rs`, alongside a `with_vision()` builder that providers chain onto their known-model lists.

Surfaced through the existing model-listing endpoint to the generated OpenAPI TS client. The frontend exposes a `currentModelSupportsVision` selector derived from the active model.

**Initial vision-true set.** This was a *seed list*, written to be refined against vendor docs during implementation. It is a snapshot of what was believed vision-capable on 2026-05-30, not a maintained catalogue — for the current truth, read the `.with_vision()` calls in each provider module under `crates/biorouter/src/providers/`.

- **Anthropic:** Claude 3, 3.5, 3.7, and 4 families (Opus, Sonnet, Haiku as documented vision-capable).
- **OpenAI:** gpt-4o, gpt-4o-mini, gpt-4-turbo, gpt-4.1*, o1/o3/o4 vision variants.
- **Google/Gemini:** gemini-1.5-pro/flash, gemini-2.0-flash, gemini-2.5-*.
- **Bedrock:** Claude and Llama vision variants only.
- **Databricks:** vision-capable served models only; default false otherwise.
- **Ollama / local / unknown:** default `false`. Users with local llava/qwen-vl override per-model in their config YAML.

**Why static, not auto-detect.** Probing adds startup latency, fails offline for local providers, and is wrong after model renames. Static declarations matched against model ID are dumb and reliable.

**UI gating behavior.** When `currentModelSupportsVision` is `false`:

- Attach button is hidden.
- Image-MIME pastes are ignored (text paste still works).
- Image-file drops are ignored.

## Frontend changes

### New IPC: `read-temp-image-as-base64`

Added in `ui/desktop/src/main.ts`, alongside the existing `save-data-url-to-temp`, `get-temp-image`, and `delete-temp-file` handlers:

```ts
ipcMain.handle('read-temp-image-as-base64', async (_, filePath: string) => {
  // 1. validate filePath is inside the BioRouter temp dir
  //    (reuse the validation pattern from delete-temp-file)
  // 2. fs.readFile(filePath)
  // 3. derive mimeType from extension: png/jpeg/gif/webp only
  // 4. return { data: <base64>, mimeType }
  // 5. on error → throw; surfaced via existing error channel
})
```

Preload exposes it as `window.electron.readTempImageAsBase64(path)`.

> **Why a new handler instead of reusing `get-temp-image`.** `get-temp-image` returns a data-URL string for `<img src>`. The API needs raw base64 + mimeType. Two shapes → two handlers. Avoids string-parsing on the renderer.

### `createUserMessage()` rewrite

The current implementation in `ui/desktop/src/types/message.ts` wraps everything in one `TextContent` and embeds image paths in the text. New signature:

```ts
async function createUserMessage(
  text: string,
  attachments: { path: string; kind: 'image' }[] = []
): Promise<Message>
```

Steps:

1. `Promise.all` over attachments → call `readTempImageAsBase64` for each.
2. Strip image-path tokens from `text` using existing `imageUtils` logic.
3. Build `content: [{type:'text', text:stripped}, ...imageBlocks]`. Omit the text block if stripped text is empty.
4. If any read fails → throw; the existing per-image retry UI catches it and blocks Send until resolved. Don't auto-send a partial set.

`createUserMessage` becomes async, so every renderer call site must `await` it. The implementation plan enumerates the callers it found.

### ChatInput integration

In `ui/desktop/src/components/ChatInput.tsx`:

- Read `supportsVision` from the model-context selector.
- If `false`: hide attach button, skip image branch in `handlePaste`, drop image files in `useFileDrop` callback.
- If model switches `true → false` with images already attached: keep thumbnails, show inline banner ("Current model can't see images. Switch model or remove attachments."), disable Send.
- On Send, pass attached temp-paths to `createUserMessage()` instead of stringifying them into the text.

### Message rendering

In `ui/desktop/src/components/BioRouterMessage.tsx` and `UserMessage.tsx`:

- For each `ImageContent` block in `message.content`, render an inline image from `data:${mimeType};base64,${data}`. Extract `ImagePreview`'s styling into a small `InlineImage` component accepting either a temp path OR a `{data, mimeType}` pair.
- Keep `imageUtils.ts` path-stripping ONLY for backward-compat with pre-v1.76.1 sessions. For new messages, paths never appear in the text.

### Frontend behaviour left unchanged

5×5 MB caps, drag-drop / paste / picker UX, temp-file lifecycle, retry/error per-image UI.

## Backend changes

### Fix Gemini's silent image drop

In `crates/biorouter/src/providers/formats/google.rs`, the catch-all `_ => {}` arm in the content match currently swallows `MessageContent::Image`. Replace with explicit handling that emits Gemini's wire format:

```json
{ "parts": [
    { "text": "..." },
    { "inline_data": { "mime_type": "image/png", "data": "<base64>" } }
]}
```

`ImageContent { data, mime_type }` maps 1:1. Gemini's 20 MB total-request limit is well above the UI's 5×5 MB cap; no new validation needed.

### Add `supports_vision` to model metadata

- **Data model:** add `supports_vision: Option<bool>` to the per-model struct. `None` → treated as `false`. Plumb through the OpenAPI schema.
- **Per-provider seed lists:** declare `supports_vision: true` for the vision-capable models listed in the capability section. Default `false` everywhere else.
- The implementation plan pins down the canonical location for the model list. If the metadata is scattered across providers without a single shape, a small refactor lands here.

### Regenerate the OpenAPI client

After adding `supports_vision`, run `just generate-openapi`. The TS client must expose the field on the model-listing endpoint or the frontend gating won't work.

### Backend surfaces left unchanged

`MessageContent::Image` variant, `ImageContent` struct, Anthropic/OpenAI/Bedrock formatters, SQLite schema, `/reply` route, session manager, Snowflake's existing skip.

The Databricks formatter handles images today but coverage is partial. The implementation plan verifies it against a vision-capable Databricks-served model and adds explicit handling if gaps are found.

## Edge cases

- **Model switch with images attached.** Keep thumbnails, show banner, disable Send. Don't silently drop user's work; don't block the model switch itself.
- **Mid-send read failure.** Per-image error UI fires (existing); whole send is blocked until retry or removal. No partial sends.
- **Oversized images.** Existing 5 MB cap is a hard ceiling. No auto-downscale.
- **Provider rejection.** Surface the provider error verbatim through the existing reply-error path. Don't retry-without-image.
- **Session reload with mixed history.** Pre-v1.76.1 sessions render via the path-extraction fallback; v1.76.1-and-later sessions render via `ImageContent` blocks. No migration.
- **Replay cost.** `conversation_so_far` carries base64 every turn. A 5-image conversation hits ~25 MB per turn after the first. Providers accept this. Mitigation deferred to a future blob-store design.
- **Tool-returned images.** Already flow through `MessageContent::Image`. Gemini fix + capability gating help them incidentally; no extra work.
- **User mismarks a local model as `supports_vision: true`** — provider call may error or hallucinate. Documented as a user contract.

## Testing

### Backend tests (Rust)

- Extend `crates/biorouter/tests/providers.rs` with Gemini coverage: round-trip `ImageContent` through `format_messages`, assert the JSON contains `inline_data` with the right `mime_type` and non-empty base64 body.
- Table-driven unit test asserting Claude 3+, GPT-4o, Gemini 1.5+, and Bedrock-Claude variants seed as `supports_vision: true`, and a known text-only model as `false`.
- No new tests for Anthropic/OpenAI/Bedrock/Databricks formatters — already covered.

### Frontend tests (Vitest)

`createUserMessage` cases (mock `window.electron.readTempImageAsBase64`):

- Text only → one `TextContent`.
- Text + 1 image → 2 content blocks; path token stripped from text.
- Text + 3 images → 4 content blocks in order.
- Empty text + 1 image → just the image block.
- One image read fails → throws; no message constructed.

### Playwright end-to-end tests

Golden path:

1. Launch with a vision-capable model selected (mocked provider with canned response).
2. Paste an in-memory PNG into ChatInput.
3. Assert thumbnail appears.
4. Click Send.
5. Assert the outgoing request payload contains an `ImageContent` block with non-empty `data`.
6. Assert the message renders in transcript with image visible.

Gating:

1. Launch with text-only model selected.
2. Assert attach button is not visible.
3. Attempt image paste → no thumbnail; text paste still works.

### Manual verification gates

Per CLAUDE.md "UI changes must be tested in the running app." The implementing agent runs `just run-ui` and confirms:

1. Paste a screenshot to Claude Sonnet → model describes it.
2. Drop a PNG file to GPT-4o → model describes it.
3. Pick a JPEG via attach button on Gemini → model describes it.
4. Switch to a text-only Ollama model → attach button disappears.
5. Reload a session containing images → transcript renders them.

The spec declared that if any of these five fail, implementation is not done regardless of unit-test status.

> **Note.** No pass/fail record for these five gates was written back into this document or the plan. The feature shipped in v1.76.1, and follow-up fixes on 2026-05-31 (`fix(image-input): drag-dropped images now actually reach the model`, `fix(image-input): per-session vision gating + parent-drop filter + banner restyle`, `fix(openai-responses): include image content blocks in responses-API requests`) suggest the gates were exercised and produced defects that were then repaired. Treat the checklist as executed but unrecorded.

## Files touched

- `crates/biorouter/src/providers/formats/google.rs` — Gemini image conversion.
- `crates/biorouter/src/providers/base.rs` (or per-provider model lists) — `supports_vision: Option<bool>` field + seeded `true` values per vendor docs.
- `crates/biorouter-server/src/routes/` — expose `supports_vision` on the model-listing response if not already.
- `ui/desktop/openapi.json` and `ui/desktop/src/api/` — regenerated client.
- `ui/desktop/src/main.ts` — new `read-temp-image-as-base64` IPC handler.
- `ui/desktop/src/preload.ts` — preload binding.
- `ui/desktop/src/types/message.ts` — async `createUserMessage` with structured content.
- `ui/desktop/src/components/ChatInput.tsx` — capability gating, pass attachments to `createUserMessage`, model-switch banner.
- `ui/desktop/src/components/BioRouterMessage.tsx`, `UserMessage.tsx`, `ImagePreview.tsx` — render `ImageContent` blocks from data-URLs.
- `crates/biorouter/tests/providers.rs` — Gemini image-format test, model-metadata table test.
- `ui/desktop/src/types/message.test.ts` (new) — `createUserMessage` unit tests.
- Playwright suite — golden-path and gating end-to-end tests.

## Related documentation

- [Image input implementation plan](image-input-plan.md) — the task-by-task execution of this spec, with the exact code that landed.
- [Choosing a model provider](../../getting-started/choosing-a-model-provider.md) — which providers and models are available, and therefore which ones can accept images.
- [System overview](../../architecture/system-overview.md) — how the renderer, `biorouterd`, and provider layer fit together, which this design threads a new content type through.
- [Sessions](../../sessions/README.md) — how conversation state is persisted, relevant to the pre-/post-v1.76.1 rendering split.
