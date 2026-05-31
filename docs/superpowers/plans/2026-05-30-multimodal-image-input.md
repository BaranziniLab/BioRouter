# Multimodal Image Input Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let users drop a screenshot or image into the chat and have the active multimodal model see the image directly (no OCR).

**Architecture:** Frontend reads attached temp files as base64 only at send time and builds a structured `content[]` with `TextContent` + `ImageContent` blocks. Backend `MessageContent::Image` path already works for Anthropic / OpenAI / Bedrock / Databricks; this plan adds Gemini support and a per-model `supports_vision` flag so the UI can hide the attach button on text-only models.

**Tech Stack:** Rust (axum/sqlx/utoipa), TypeScript / React 19 / Electron (Forge + Vite), Vitest + Playwright. Spec: [docs/superpowers/specs/2026-05-30-multimodal-image-input-design.md](../specs/2026-05-30-multimodal-image-input-design.md).

---

## Phase 1 — Backend: `supports_vision` on `ModelInfo`

### Task 1: Add `supports_vision` field + `with_vision()` builder

**Files:**

- Modify: `crates/biorouter/src/providers/base.rs:40-84`

- [ ] **Step 1: Add field to `ModelInfo` struct**

In `crates/biorouter/src/providers/base.rs`, replace the struct definition starting at line 40:

```rust
/// Information about a model's capabilities
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
pub struct ModelInfo {
    /// The name of the model
    pub name: String,
    /// The maximum context length this model supports
    pub context_limit: usize,
    /// Cost per token for input (optional)
    pub input_token_cost: Option<f64>,
    /// Cost per token for output (optional)
    pub output_token_cost: Option<f64>,
    /// Currency for the costs (default: "$")
    pub currency: Option<String>,
    /// Whether this model supports cache control
    pub supports_cache_control: Option<bool>,
    /// Whether this model accepts image inputs (multimodal vision)
    #[serde(default)]
    pub supports_vision: Option<bool>,
}
```

- [ ] **Step 2: Default the field in all constructors**

Update `ModelInfo::new` (line 57) and `ModelInfo::with_cost` (line 69) to set `supports_vision: None`:

```rust
    pub fn new(name: impl Into<String>, context_limit: usize) -> Self {
        Self {
            name: name.into(),
            context_limit,
            input_token_cost: None,
            output_token_cost: None,
            currency: None,
            supports_cache_control: None,
            supports_vision: None,
        }
    }

    pub fn with_cost(
        name: impl Into<String>,
        context_limit: usize,
        input_cost: f64,
        output_cost: f64,
    ) -> Self {
        Self {
            name: name.into(),
            context_limit,
            input_token_cost: Some(input_cost),
            output_token_cost: Some(output_cost),
            currency: Some("$".to_string()),
            supports_cache_control: None,
            supports_vision: None,
        }
    }
```

- [ ] **Step 3: Add `with_vision()` builder**

After the `with_cost` method in `impl ModelInfo`, add:

```rust
    /// Mark this model as supporting image inputs (multimodal vision).
    pub fn with_vision(mut self) -> Self {
        self.supports_vision = Some(true);
        self
    }
```

- [ ] **Step 4: Default in `ProviderMetadata::new` map closure**

Update the closure on line 131-141 to include the new field:

```rust
            known_models: model_names
                .iter()
                .map(|&name| ModelInfo {
                    name: name.to_string(),
                    context_limit: ModelConfig::new_or_fail(name).context_limit(),
                    input_token_cost: None,
                    output_token_cost: None,
                    currency: None,
                    supports_cache_control: None,
                    supports_vision: None,
                })
                .collect(),
```

- [ ] **Step 5: Update existing ModelInfo struct literals in tests**

In `crates/biorouter/src/providers/base.rs` lines 694-740 there are three direct `ModelInfo` struct literals in tests. Add `supports_vision: None,` to each (after `supports_cache_control`).

- [ ] **Step 6: Write a unit test for `with_vision()`**

Append to the tests module at the bottom of `crates/biorouter/src/providers/base.rs`:

```rust
    #[test]
    fn test_with_vision_sets_flag() {
        let info = ModelInfo::new("claude-3-5-sonnet", 200_000).with_vision();
        assert_eq!(info.supports_vision, Some(true));
    }

    #[test]
    fn test_default_vision_is_none() {
        let info = ModelInfo::new("text-only-model", 8_000);
        assert_eq!(info.supports_vision, None);
    }
```

- [ ] **Step 7: Run tests**

Run: `cargo test -p biorouter providers::base`
Expected: PASS (including the two new tests).

- [ ] **Step 8: Commit**

```bash
git add crates/biorouter/src/providers/base.rs
git commit -m "feat(providers): add supports_vision to ModelInfo"
```

---

### Task 2: Declare vision-capable Anthropic models

**Files:**

- Modify: `crates/biorouter/src/providers/anthropic.rs:167-191`

- [ ] **Step 1: Find the const that lists Anthropic model names**

Run: `grep -n "ANTHROPIC_KNOWN_MODELS" crates/biorouter/src/providers/anthropic.rs`

Note the file/lines where `ANTHROPIC_KNOWN_MODELS: &[&str]` is defined (likely a `pub const` near the top of the file).

- [ ] **Step 2: Update `Provider::metadata()` to mark Claude 3+ as vision**

All currently shipped Claude models are 3.x or 4.x and support vision. Replace the `models` builder at line 168-171:

```rust
        let models: Vec<ModelInfo> = ANTHROPIC_KNOWN_MODELS
            .iter()
            .map(|&model_name| ModelInfo::new(model_name, 200_000).with_vision())
            .collect();
```

If a future text-only Claude is added to the const, this becomes wrong — flag this in a comment:

```rust
        // All current Claude models (3.x and 4.x) are vision-capable.
        // If a text-only Claude ships, switch this to a per-model match.
        let models: Vec<ModelInfo> = ANTHROPIC_KNOWN_MODELS
            .iter()
            .map(|&model_name| ModelInfo::new(model_name, 200_000).with_vision())
            .collect();
```

- [ ] **Step 3: Run provider tests**

Run: `cargo test -p biorouter providers::anthropic`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/biorouter/src/providers/anthropic.rs
git commit -m "feat(anthropic): declare vision support on Claude models"
```

---

### Task 3: Declare vision-capable OpenAI models

**Files:**

- Modify: `crates/biorouter/src/providers/openai.rs:236+`

- [ ] **Step 1: Read the OpenAI `metadata()` block**

Run: `sed -n '230,290p' crates/biorouter/src/providers/openai.rs`

Identify the const/list of model names and whether the metadata uses `ProviderMetadata::new(Vec<&str>, ...)` or `with_models(Vec<ModelInfo>, ...)`.

- [ ] **Step 2: If using `ProviderMetadata::new`, switch to `with_models`**

Build an explicit `Vec<ModelInfo>` with `.with_vision()` chained on vision-capable IDs. Vision-capable set (verify against OpenAI docs before merging):

- gpt-4o, gpt-4o-mini, gpt-4o-2024-*
- gpt-4-turbo, gpt-4-turbo-*
- gpt-4.1, gpt-4.1-mini, gpt-4.1-nano
- o1, o1-mini, o3, o3-mini, o4-mini (those that document image input)

Example (adapt to the actual model const):

```rust
        let vision_models: &[&str] = &[
            "gpt-4o", "gpt-4o-mini", "gpt-4-turbo",
            "gpt-4.1", "gpt-4.1-mini", "gpt-4.1-nano",
            "o1", "o3", "o4-mini",
        ];
        let models: Vec<ModelInfo> = OPENAI_KNOWN_MODELS
            .iter()
            .map(|&name| {
                let info = ModelInfo::new(name, ModelConfig::new_or_fail(name).context_limit());
                if vision_models.contains(&name) {
                    info.with_vision()
                } else {
                    info
                }
            })
            .collect();

        ProviderMetadata::with_models(
            "openai",
            "OpenAI",
            // ... existing description, default, doc_url, config keys
            models,
            // ...
        )
```

- [ ] **Step 3: Run provider tests**

Run: `cargo test -p biorouter providers::openai`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/biorouter/src/providers/openai.rs
git commit -m "feat(openai): declare vision support on GPT-4o/4.1/o-series models"
```

---

### Task 4: Declare vision-capable Google/Gemini models

**Files:**

- Modify: `crates/biorouter/src/providers/google.rs:114-150`

- [ ] **Step 1: Read the Google `metadata()` block**

Run: `sed -n '110,160p' crates/biorouter/src/providers/google.rs`

It currently uses `ProviderMetadata::new(Vec<&str>, ...)`. Switch to `with_models(Vec<ModelInfo>, ...)`.

- [ ] **Step 2: All current Gemini models are multimodal**

Apply `.with_vision()` blanket:

```rust
    fn metadata() -> ProviderMetadata {
        // All current Gemini models (1.5+, 2.0, 2.5) are multimodal.
        let models: Vec<ModelInfo> = GOOGLE_KNOWN_MODELS
            .iter()
            .map(|&name| {
                ModelInfo::new(name, ModelConfig::new_or_fail(name).context_limit())
                    .with_vision()
            })
            .collect();

        ProviderMetadata::with_models(
            "google",
            "Google",
            // ... existing description, default, doc_url, config keys
            models,
            // ...
        )
    }
```

- [ ] **Step 3: Run provider tests**

Run: `cargo test -p biorouter providers::google`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/biorouter/src/providers/google.rs
git commit -m "feat(google): declare vision support on Gemini models"
```

---

### Task 5: Declare vision-capable Bedrock + Databricks models

**Files:**

- Modify: `crates/biorouter/src/providers/bedrock.rs:213+`
- Modify: `crates/biorouter/src/providers/databricks.rs:240+`

- [ ] **Step 1: Bedrock — selectively flag Claude / Llama vision variants**

Read: `sed -n '210,260p' crates/biorouter/src/providers/bedrock.rs`

Bedrock hosts a mix. Vision-capable IDs (verify against AWS docs):

- `anthropic.claude-3-*` (sonnet/opus/haiku, 3.5, 3.7)
- `anthropic.claude-4-*` if present
- `meta.llama-3-2-*-vision-instruct-v1:0` variants
- `meta.llama-4-*` if present

Apply same `vision_models` + `contains` pattern as Task 3.

- [ ] **Step 2: Databricks — flag served Claude models**

Read: `sed -n '240,290p' crates/biorouter/src/providers/databricks.rs`

Databricks serves a curated set; flag any `databricks-claude-*` or `databricks-meta-llama-*-vision` IDs. Leave others as `None`.

- [ ] **Step 3: Run provider tests**

Run: `cargo test -p biorouter providers::bedrock providers::databricks`
Expected: PASS.

- [ ] **Step 4: Add a table-driven cross-provider vision assertion**

Append to the tests module in `crates/biorouter/src/providers/base.rs`:

```rust
    #[test]
    fn known_vision_models_have_supports_vision_true() {
        use crate::providers::anthropic::AnthropicProvider;
        use crate::providers::google::GoogleProvider;
        use crate::providers::openai::OpenAiProvider;
        // Add other vision-capable providers as their imports are confirmed.

        let cases: Vec<(&str, &str, &str)> = vec![
            ("anthropic", "claude-3-5-sonnet-latest", "Anthropic Claude 3.5 Sonnet"),
            ("openai", "gpt-4o", "OpenAI GPT-4o"),
            ("google", "gemini-1.5-pro-latest", "Google Gemini 1.5 Pro"),
        ];

        for (provider_kind, model_name, label) in cases {
            let metadata = match provider_kind {
                "anthropic" => AnthropicProvider::metadata(),
                "openai" => OpenAiProvider::metadata(),
                "google" => GoogleProvider::metadata(),
                _ => unreachable!(),
            };
            let info = metadata
                .known_models
                .iter()
                .find(|m| m.name == model_name)
                .unwrap_or_else(|| panic!("model {model_name} not in known_models for {label}"));
            assert_eq!(
                info.supports_vision,
                Some(true),
                "{label} should declare supports_vision: true"
            );
        }
    }

    #[test]
    fn text_only_provider_does_not_claim_vision() {
        use crate::providers::ollama::OllamaProvider;
        let metadata = OllamaProvider::metadata();
        for info in &metadata.known_models {
            assert_ne!(
                info.supports_vision,
                Some(true),
                "Ollama default-listed model {} should not claim vision (user overrides in config)",
                info.name
            );
        }
    }
```

(Adjust provider struct names if they differ — e.g., `OpenAIProvider` vs `OpenAiProvider`. Run a `grep -n "impl Provider for" crates/biorouter/src/providers/anthropic.rs` to confirm before pasting.)

Run: `cargo test -p biorouter providers::base::tests::known_vision_models_have_supports_vision_true providers::base::tests::text_only_provider_does_not_claim_vision`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/biorouter/src/providers/bedrock.rs crates/biorouter/src/providers/databricks.rs crates/biorouter/src/providers/base.rs
git commit -m "feat(providers): declare vision support on Bedrock and Databricks models"
```

---

### Task 6: Regenerate OpenAPI spec

**Files:**

- Modify: `ui/desktop/openapi.json`
- Modify: `ui/desktop/src/api/types.gen.ts` (auto-generated)
- Modify: `ui/desktop/src/api/sdk.gen.ts` (auto-generated)

- [ ] **Step 1: Run the OpenAPI regen command**

Run: `just generate-openapi`
Expected: exits 0; `ui/desktop/openapi.json` and `ui/desktop/src/api/*.gen.ts` show diffs that add `supportsVision?: boolean | null;` to the `ModelInfo` type.

- [ ] **Step 2: Verify the field appears in the generated client**

Run: `grep -n "supportsVision" ui/desktop/src/api/types.gen.ts`
Expected: at least one match inside the `ModelInfo` type definition.

- [ ] **Step 3: Commit**

```bash
git add ui/desktop/openapi.json ui/desktop/src/api/
git commit -m "chore(api): regenerate OpenAPI client for supports_vision"
```

---

## Phase 2 — Backend: Fix Gemini's silent image-drop

### Task 7: Add Gemini image-format test

**Files:**

- Modify: `crates/biorouter/src/providers/formats/google.rs` (tests module at bottom)

- [ ] **Step 1: Locate or create the tests module**

Run: `grep -n "#\[cfg(test)\]" crates/biorouter/src/providers/formats/google.rs`

If a tests module exists, append. Otherwise add one at the bottom of the file:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::message::Message;
    // adjust imports as needed
}
```

- [ ] **Step 2: Write the failing test**

```rust
    #[test]
    fn format_messages_emits_inline_data_for_user_image() {
        let msg = Message::user()
            .with_image("BASE64DATA".to_string(), "image/png".to_string());
        let formatted = format_messages(&[msg], false);
        let json_str = serde_json::to_string(&formatted).unwrap();

        assert!(
            json_str.contains("\"inline_data\""),
            "expected inline_data in {json_str}"
        );
        assert!(
            json_str.contains("\"image/png\""),
            "expected mime_type in {json_str}"
        );
        assert!(
            json_str.contains("\"BASE64DATA\""),
            "expected base64 body in {json_str}"
        );
    }
```

(The exact `with_image` signature is verified at [crates/biorouter/src/conversation/message.rs:246](../../../crates/biorouter/src/conversation/message.rs#L246) — adjust args if it takes `(data, mime_type)` in a different order.)

- [ ] **Step 3: Run the test and verify it fails**

Run: `cargo test -p biorouter --lib providers::formats::google::tests::format_messages_emits_inline_data_for_user_image`
Expected: FAIL — the assertion fires because the current catch-all `_ => {}` at line 215 swallows `MessageContent::Image`.

---

### Task 8: Implement Gemini image handling

**Files:**

- Modify: `crates/biorouter/src/providers/formats/google.rs:206-216`

- [ ] **Step 1: Add an explicit `MessageContent::Image` arm**

The existing `MessageContent::Thinking` arm ends at line 213 and is followed by the `_ => {}` catch-all on line 215. Insert a new arm between them so the catch-all only swallows unknown variants:

```rust
                    MessageContent::Thinking(thinking) => {
                        let mut part = Map::new();
                        part.insert("text".to_string(), json!(thinking.thinking));
                        if include_signature {
                            part.insert("thoughtSignature".to_string(), json!(thinking.signature));
                        }
                        parts.push(json!(part));
                    }
                    MessageContent::Image(image) => {
                        parts.push(json!({
                            "inline_data": {
                                "mime_type": image.mime_type,
                                "data": image.data,
                            }
                        }));
                    }

                    _ => {}
```

(The wire shape matches the existing `RawContent::Image` arm at line 126 of the same file, which already emits this format for tool-returned images.)

- [ ] **Step 2: Run the test and verify it passes**

Run: `cargo test -p biorouter --lib providers::formats::google::tests::format_messages_emits_inline_data_for_user_image`
Expected: PASS.

- [ ] **Step 3: Run the full Google provider test set**

Run: `cargo test -p biorouter providers::formats::google`
Expected: PASS (no regressions).

- [ ] **Step 4: Commit**

```bash
git add crates/biorouter/src/providers/formats/google.rs
git commit -m "fix(google): emit inline_data for user-attached images"
```

---

## Phase 3 — Frontend: IPC + `createUserMessage` refactor

### Task 9: Add `read-temp-image-as-base64` IPC handler

**Files:**

- Modify: `ui/desktop/src/main.ts` (alongside existing temp-image handlers, near line 1617)
- Modify: `ui/desktop/src/preload.ts:296-307`
- Modify: `ui/desktop/src/types/electron.d.ts` or wherever `Window['electron']` is typed (find with `grep -n "saveDataUrlToTemp" ui/desktop/src/`)

- [ ] **Step 1: Locate the existing temp-image handlers**

Run: `grep -n "save-data-url-to-temp\|get-temp-image\|delete-temp-file" ui/desktop/src/main.ts`

Note the line range. The new handler goes right after `delete-temp-file`.

- [ ] **Step 2: Add the main-process IPC handler**

In `ui/desktop/src/main.ts`, append after the `delete-temp-file` handler:

```ts
ipcMain.handle('read-temp-image-as-base64', async (_event, filePath: string) => {
  // Reuse the same path validation used by delete-temp-file: filePath must
  // sit inside the BioRouter temp directory.
  const tempDir = path.join(os.tmpdir(), 'biorouter-images');
  const resolved = path.resolve(filePath);
  if (!resolved.startsWith(path.resolve(tempDir) + path.sep)) {
    throw new Error('refusing to read file outside temp dir');
  }

  const buf = await fs.promises.readFile(resolved);
  const ext = path.extname(resolved).toLowerCase();
  const mimeType = (
    {
      '.png': 'image/png',
      '.jpg': 'image/jpeg',
      '.jpeg': 'image/jpeg',
      '.gif': 'image/gif',
      '.webp': 'image/webp',
    } as const
  )[ext as '.png' | '.jpg' | '.jpeg' | '.gif' | '.webp'];

  if (!mimeType) {
    throw new Error(`unsupported image extension: ${ext}`);
  }

  return { data: buf.toString('base64'), mimeType };
});
```

(If the existing `delete-temp-file` handler uses a different temp-dir constant or validation helper, reuse that same helper here instead of re-deriving the path.)

- [ ] **Step 3: Expose in preload**

In `ui/desktop/src/preload.ts`, in the `window.electron` object literal (look for `saveDataUrlToTemp`):

```ts
    readTempImageAsBase64: (filePath: string) =>
      ipcRenderer.invoke('read-temp-image-as-base64', filePath) as Promise<{
        data: string;
        mimeType: string;
      }>,
```

- [ ] **Step 4: Add the TypeScript declaration**

In the `Window.electron` type declaration (grep for `saveDataUrlToTemp:` to find the file — likely `ui/desktop/src/types/electron.d.ts` or `preload.ts`):

```ts
readTempImageAsBase64: (filePath: string) => Promise<{
  data: string;
  mimeType: string;
}>;
```

- [ ] **Step 5: Type-check**

Run: `cd ui/desktop && npm run lint:check`
Expected: PASS (no new type errors).

- [ ] **Step 6: Commit**

```bash
git add ui/desktop/src/main.ts ui/desktop/src/preload.ts ui/desktop/src/types/
git commit -m "feat(ipc): add read-temp-image-as-base64 handler"
```

---

### Task 10: Write failing tests for new `createUserMessage` signature

**Files:**

- Create: `ui/desktop/src/types/message.test.ts`

- [ ] **Step 1: Write the test file**

```ts
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { createUserMessage } from './message';

declare global {
  interface Window {
    electron: {
      readTempImageAsBase64: (path: string) => Promise<{ data: string; mimeType: string }>;
    };
  }
}

describe('createUserMessage', () => {
  beforeEach(() => {
    (globalThis as unknown as { window: Partial<Window> }).window = {
      electron: {
        readTempImageAsBase64: vi.fn(async (p: string) => ({
          data: `B64-${p}`,
          mimeType: p.endsWith('.jpg') ? 'image/jpeg' : 'image/png',
        })),
      },
    } as Window;
  });

  it('text only produces a single TextContent block', async () => {
    const msg = await createUserMessage('hello world');
    expect(msg.content).toEqual([{ type: 'text', text: 'hello world' }]);
    expect(msg.role).toBe('user');
  });

  it('text + 1 image produces text + image blocks', async () => {
    const msg = await createUserMessage('describe this', [
      { path: '/tmp/biorouter-images/foo.png', kind: 'image' },
    ]);
    expect(msg.content).toEqual([
      { type: 'text', text: 'describe this' },
      { type: 'image', data: 'B64-/tmp/biorouter-images/foo.png', mimeType: 'image/png' },
    ]);
  });

  it('text + 3 images preserves order', async () => {
    const msg = await createUserMessage('compare', [
      { path: '/tmp/biorouter-images/a.png', kind: 'image' },
      { path: '/tmp/biorouter-images/b.jpg', kind: 'image' },
      { path: '/tmp/biorouter-images/c.png', kind: 'image' },
    ]);
    expect(msg.content).toHaveLength(4);
    expect(msg.content[0]).toEqual({ type: 'text', text: 'compare' });
    expect(msg.content[1]).toMatchObject({ type: 'image', mimeType: 'image/png' });
    expect(msg.content[2]).toMatchObject({ type: 'image', mimeType: 'image/jpeg' });
    expect(msg.content[3]).toMatchObject({ type: 'image', mimeType: 'image/png' });
  });

  it('empty text + 1 image omits the text block', async () => {
    const msg = await createUserMessage('', [
      { path: '/tmp/biorouter-images/foo.png', kind: 'image' },
    ]);
    expect(msg.content).toEqual([
      { type: 'image', data: 'B64-/tmp/biorouter-images/foo.png', mimeType: 'image/png' },
    ]);
  });

  it('throws if an image read fails', async () => {
    (window.electron.readTempImageAsBase64 as ReturnType<typeof vi.fn>).mockRejectedValueOnce(
      new Error('boom')
    );
    await expect(
      createUserMessage('hi', [{ path: '/tmp/biorouter-images/broken.png', kind: 'image' }])
    ).rejects.toThrow(/boom/);
  });
});
```

- [ ] **Step 2: Run and verify it fails**

Run: `cd ui/desktop && npm run test:run -- src/types/message.test.ts`
Expected: FAIL — `createUserMessage` is currently sync and doesn't accept attachments.

---

### Task 11: Refactor `createUserMessage` to async with structured content

**Files:**

- Modify: `ui/desktop/src/types/message.ts:10-18`

- [ ] **Step 1: Replace `createUserMessage`**

```ts
export type UserAttachment = { path: string; kind: 'image' };

export async function createUserMessage(
  text: string,
  attachments: UserAttachment[] = []
): Promise<Message> {
  const imageBlocks = await Promise.all(
    attachments
      .filter((a) => a.kind === 'image')
      .map(async (a) => {
        const { data, mimeType } = await window.electron.readTempImageAsBase64(a.path);
        return { type: 'image' as const, data, mimeType };
      })
  );

  const trimmed = text.trim();
  const content: Message['content'] = [];
  if (trimmed.length > 0) {
    content.push({ type: 'text', text });
  }
  for (const block of imageBlocks) {
    content.push(block);
  }

  return {
    id: generateMessageId(),
    role: 'user',
    created: Math.floor(Date.now() / 1000),
    content,
    metadata: { userVisible: true, agentVisible: true },
  };
}
```

- [ ] **Step 2: Run tests**

Run: `cd ui/desktop && npm run test:run -- src/types/message.test.ts`
Expected: PASS (all 5 cases).

- [ ] **Step 3: Commit**

```bash
git add ui/desktop/src/types/message.ts ui/desktop/src/types/message.test.ts
git commit -m "feat(message): async createUserMessage with structured image blocks"
```

---

### Task 12: Update `createUserMessage` callers to await

**Files:**

- Modify: `ui/desktop/src/hooks/useChatStream.ts:487` (and the `createUserMessage` import at line 27)
- Modify: `ui/desktop/src/hooks/useWorkflowManager.ts:265`

- [ ] **Step 1: Update `useChatStream.ts`**

Read line 480-495 first to see the surrounding context:

```bash
sed -n '480,495p' ui/desktop/src/hooks/useChatStream.ts
```

The line `? createUserMessage(userMessage)` is inside a ternary. Wrap whatever surrounding sync logic depends on the call in an async path. Since `useChatStream` is already async (it's a hook with async functions), `await` should be straightforward:

```ts
const msg = typeof userMessage === 'string'
  ? await createUserMessage(userMessage)
  : userMessage;
```

The exact shape depends on the surrounding expression — preserve it; just add `await`. If the call is inside `useMemo` or similar sync hook, that hook needs to move into a `useEffect` + state pattern. Verify before editing.

- [ ] **Step 2: Update `useWorkflowManager.ts:265`**

Read line 260-275 first:

```bash
sed -n '260,275p' ui/desktop/src/hooks/useWorkflowManager.ts
```

Change:

```ts
      const userMessage = createUserMessage(finalPrompt);
```

to:

```ts
      const userMessage = await createUserMessage(finalPrompt);
```

Confirm the enclosing function is already `async`. If not, mark it so.

- [ ] **Step 3: Type-check the whole frontend**

Run: `cd ui/desktop && npm run lint:check`
Expected: PASS (no errors). If TypeScript complains that an enclosing function is not `async`, mark it `async` and update its caller if needed.

- [ ] **Step 4: Commit**

```bash
git add ui/desktop/src/hooks/useChatStream.ts ui/desktop/src/hooks/useWorkflowManager.ts
git commit -m "refactor(hooks): await async createUserMessage"
```

---

## Phase 4 — Frontend: ChatInput integration

### Task 13: Surface `supportsVision` from `ModelAndProviderContext`

**Files:**

- Modify: `ui/desktop/src/components/ModelAndProviderContext.tsx`

- [ ] **Step 1: Read the current context shape**

Run: `cat ui/desktop/src/components/ModelAndProviderContext.tsx`

Identify where `currentModel` is set and where the model list (`ModelInfo[]`) is available.

- [ ] **Step 2: Add a `currentModelSupportsVision` selector**

Inside the context provider, derive:

```ts
const currentModelSupportsVision = useMemo(() => {
  if (!currentModel || !currentProvider) return false;
  const providerModels = knownModelsByProvider[currentProvider] ?? [];
  const info = providerModels.find((m) => m.name === currentModel);
  return info?.supportsVision === true;
}, [currentModel, currentProvider, knownModelsByProvider]);
```

(The exact source of `knownModelsByProvider` depends on how the context already fetches metadata — likely from `/agent/providers` or similar. Reuse the existing query; don't add a new fetch.)

Expose it on the context value:

```ts
return (
  <ModelAndProviderContext.Provider
    value={{
      currentModel,
      currentProvider,
      currentModelSupportsVision,
      // ... existing fields
    }}
  >
```

Update the context type to include the new field.

- [ ] **Step 3: Type-check**

Run: `cd ui/desktop && npm run lint:check`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add ui/desktop/src/components/ModelAndProviderContext.tsx
git commit -m "feat(context): expose currentModelSupportsVision"
```

---

### Task 14: Gate ChatInput on `supportsVision`

**Files:**

- Modify: `ui/desktop/src/components/ChatInput.tsx`

- [ ] **Step 1: Read the relevant ChatInput regions**

Run: `grep -n "handlePaste\|useFileDrop\|selectFileOrDirectory\|Attach" ui/desktop/src/components/ChatInput.tsx | head -30`

Identify:

- The attach button JSX (around line ~1098 per spec).
- The `handlePaste` function (around line 656-766).
- The `useFileDrop` hook call site.

- [ ] **Step 2: Pull the flag**

Near the other context destructures at the top of the component:

```ts
const { currentModelSupportsVision } = useModelAndProvider();
```

- [ ] **Step 3: Hide attach button when vision unavailable**

Wrap the attach button in `{currentModelSupportsVision && ( ... )}` or add a condition to the existing render.

- [ ] **Step 4: Skip image branch in `handlePaste`**

At the top of `handlePaste`:

```ts
const handlePaste = async (e: React.ClipboardEvent<HTMLTextAreaElement>) => {
  if (!currentModelSupportsVision) {
    // Allow default text paste behavior; just skip the image extraction.
    return;
  }
  // ... existing image-handling code
};
```

- [ ] **Step 5: Filter image drops in `useFileDrop` callback**

In the `onDrop` callback passed to `useFileDrop`, filter out image-typed entries when vision is off:

```ts
const filtered = currentModelSupportsVision
  ? droppedFiles
  : droppedFiles.filter((f) => !f.type.startsWith('image/'));
```

Then proceed with `filtered` for the rest of the handler.

- [ ] **Step 6: Type-check + run existing unit tests**

Run: `cd ui/desktop && npm run lint:check && npm run test:run`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add ui/desktop/src/components/ChatInput.tsx
git commit -m "feat(chat-input): hide attach + ignore image paste/drop when model lacks vision"
```

---

### Task 15: Send images as structured content + model-switch banner

**Files:**

- Modify: `ui/desktop/src/components/ChatInput.tsx`

- [ ] **Step 1: Find the current Send path**

Run: `grep -n "onSubmit\|handleSubmit\|stringifyAttachments\|attachedImages" ui/desktop/src/components/ChatInput.tsx`

Identify where the user prompt is currently merged with image paths (the existing path-embedding-in-text logic).

- [ ] **Step 2: Replace path-embedding with structured handoff**

The component sends to `reply()` (from `useChatStream`). The current call shape (per spec research): `reply(stringifiedPromptIncludingPaths)`. Change `reply` and its callers to accept an attachments array:

In `useChatStream.ts`, where `reply` is defined (line ~502):

```ts
async function reply(text: string, attachments: UserAttachment[] = []): Promise<void> {
  // ...
  const userMessage = await createUserMessage(text, attachments);
  // ... existing send logic
}
```

In `ChatInput.tsx` `handleSubmit`:

```ts
const attachments: UserAttachment[] = attachedImages.map((img) => ({
  path: img.tempPath,
  kind: 'image' as const,
}));
await reply(textWithoutPathTokens, attachments);
```

`textWithoutPathTokens` is the raw user textarea value — DO NOT call the existing path-injection helper. Image paths now flow only through `attachments`.

- [ ] **Step 3: Surface `createUserMessage` errors per-image**

`createUserMessage` (Task 11) throws if any `readTempImageAsBase64` fails. Wrap the call in `handleSubmit` and route the failure into the existing per-image error state instead of swallowing it:

```ts
try {
  await reply(textWithoutPathTokens, attachments);
} catch (err) {
  // The existing attached-image UI already has per-image error/retry state;
  // re-mark all currently-attached images as errored so the user can retry
  // or remove them. The send is aborted.
  setAttachedImagesError((err as Error).message ?? 'Failed to read image');
  return;
}
```

(`setAttachedImagesError` — or whatever the existing setter is — should already exist on the component; the spec research confirmed retry/error UI is in place. If the existing setter is per-image, mark each attached image as errored individually.)

- [ ] **Step 4: Add model-switch banner**

When `currentModelSupportsVision` is `false` AND `attachedImages.length > 0`, render an inline banner above the input and disable Send:

```tsx
{!currentModelSupportsVision && attachedImages.length > 0 && (
  <div className="px-3 py-2 mb-2 rounded-md bg-amber-100 dark:bg-amber-900/40 text-amber-900 dark:text-amber-100 text-sm">
    Current model can&apos;t see images. Switch to a vision-capable model
    or remove attachments to send.
  </div>
)}
```

For the Send button disabled condition, add: `(!currentModelSupportsVision && attachedImages.length > 0)`.

- [ ] **Step 5: Type-check**

Run: `cd ui/desktop && npm run lint:check`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add ui/desktop/src/components/ChatInput.tsx ui/desktop/src/hooks/useChatStream.ts
git commit -m "feat(chat-input): send images as structured content + vision-mismatch banner"
```

---

## Phase 5 — Frontend: Renderer

### Task 16: `InlineImage` component supporting both temp-path and base64

**Files:**

- Modify: `ui/desktop/src/components/ImagePreview.tsx` (or create a sibling `InlineImage.tsx` if `ImagePreview` is too coupled to file-path UX)

- [ ] **Step 1: Inspect the existing component**

Run: `cat ui/desktop/src/components/ImagePreview.tsx`

- [ ] **Step 2: Extend props to accept either source shape**

```tsx
type InlineImageProps =
  | { kind: 'temp-path'; path: string; alt?: string }
  | { kind: 'data'; data: string; mimeType: string; alt?: string };

export function InlineImage(props: InlineImageProps) {
  const [src, setSrc] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    if (props.kind === 'data') {
      setSrc(`data:${props.mimeType};base64,${props.data}`);
      return;
    }
    window.electron
      .getTempImage(props.path)
      .then((dataUrl) => { if (!cancelled) setSrc(dataUrl); })
      .catch((err: Error) => { if (!cancelled) setError(err.message); });
    return () => { cancelled = true; };
  }, [props]);

  if (error) {
    return <div className="text-sm text-red-500">Image failed to load: {error}</div>;
  }
  if (!src) {
    return <div className="text-sm text-muted">Loading image…</div>;
  }
  // Preserve the existing expand/collapse styling from ImagePreview.
  return <img src={src} alt={props.alt ?? ''} className="rounded-md max-h-96 cursor-zoom-in" />;
}
```

If `ImagePreview` already does expand/collapse, keep `ImagePreview` as-is and have it delegate to a renamed inner `<InlineImage>`. The point: one component that takes either source.

- [ ] **Step 3: Type-check**

Run: `cd ui/desktop && npm run lint:check`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add ui/desktop/src/components/ImagePreview.tsx ui/desktop/src/components/InlineImage.tsx
git commit -m "feat(renderer): InlineImage component supporting temp-path or base64"
```

---

### Task 17: Render `ImageContent` blocks in transcript

**Files:**

- Modify: `ui/desktop/src/components/UserMessage.tsx`
- Modify: `ui/desktop/src/components/BioRouterMessage.tsx`

- [ ] **Step 1: Find where each renderer walks `message.content`**

Run: `grep -n "message.content\|getTextContent\|imageUtils" ui/desktop/src/components/UserMessage.tsx ui/desktop/src/components/BioRouterMessage.tsx`

- [ ] **Step 2: Add an image-block branch**

In each renderer's content-block map (or equivalent loop), insert a case:

```tsx
{message.content.map((block, idx) => {
  if (block.type === 'image') {
    return (
      <InlineImage
        key={idx}
        kind="data"
        data={block.data}
        mimeType={block.mimeType}
      />
    );
  }
  // ... existing branches for text, toolRequest, toolResponse, etc.
})}
```

- [ ] **Step 3: Backward-compat with pre-v-next sessions**

`imageUtils.ts` already extracts paths from text and renders via the path-based `ImagePreview`. Keep that code unchanged. It runs only on `TextContent` blocks; new messages send their image paths through the structured route, so the regex matches nothing and the renderer is a no-op for them.

Confirm by tracing: in `UserMessage.tsx`, the text-block branch passes `block.text` through `extractImagePaths(text)` → for new messages `block.text` doesn't contain paths → empty result → renders text only. Good.

- [ ] **Step 4: Type-check**

Run: `cd ui/desktop && npm run lint:check`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add ui/desktop/src/components/UserMessage.tsx ui/desktop/src/components/BioRouterMessage.tsx
git commit -m "feat(renderer): display ImageContent blocks in user and assistant messages"
```

---

## Phase 6 — E2E + manual verification

### Task 18: Playwright golden-path e2e test

**Files:**

- Create: `ui/desktop/playwright/multimodal-image.spec.ts` (adjust path if existing e2e tests live elsewhere — `grep -rn "test.describe" ui/desktop/playwright/ 2>/dev/null` to confirm)

- [ ] **Step 1: Find the existing e2e harness pattern**

Run: `ls ui/desktop/playwright/ 2>/dev/null && head -40 ui/desktop/playwright/*.spec.ts 2>/dev/null | head -60`

If no `playwright/` directory exists, fall back to wherever `npm run test-e2e` runs from (check `package.json`).

- [ ] **Step 2: Write the golden-path test**

```ts
import { test, expect } from '@playwright/test';
import { launchBioRouter, mockProviderReply } from './fixtures'; // adapt to existing fixture names

test('user can paste an image and the model receives it', async () => {
  const app = await launchBioRouter({ model: 'claude-3-5-sonnet-latest' });
  const recordedRequests = mockProviderReply(app, {
    canned: { content: [{ type: 'text', text: 'I see a screenshot.' }] },
  });

  // Paste a 1x1 PNG. The data URL is deterministic.
  const dataUrl =
    'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+ip1sAAAAASUVORK5CYII=';
  await app.evaluate(async (url) => {
    const blob = await (await fetch(url)).blob();
    const file = new File([blob], 'paste.png', { type: 'image/png' });
    const dt = new DataTransfer();
    dt.items.add(file);
    document.querySelector('textarea')!.dispatchEvent(new ClipboardEvent('paste', { clipboardData: dt }));
  }, dataUrl);

  await expect(app.locator('[data-testid="attached-thumbnail"]')).toBeVisible();
  await app.locator('[data-testid="send-button"]').click();

  await expect.poll(() => recordedRequests.length).toBeGreaterThan(0);
  const lastReq = recordedRequests[recordedRequests.length - 1];
  const userMsg = lastReq.user_message;
  const imageBlocks = userMsg.content.filter((c: { type: string }) => c.type === 'image');
  expect(imageBlocks).toHaveLength(1);
  expect(imageBlocks[0].data).not.toEqual('');

  await expect(app.locator('[data-testid="transcript-image"]').first()).toBeVisible();
});
```

Adapt selectors (`data-testid` names) to whatever ChatInput currently uses, or add them in the same commit. Adapt `launchBioRouter` / `mockProviderReply` to the existing fixture helpers.

- [ ] **Step 3: Run the test**

Run: `cd ui/desktop && npm run test-e2e -- multimodal-image.spec.ts`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add ui/desktop/playwright/multimodal-image.spec.ts
git commit -m "test(e2e): paste image golden path"
```

---

### Task 19: Playwright gating test (text-only model hides attach)

**Files:**

- Modify: `ui/desktop/playwright/multimodal-image.spec.ts` (extend the suite)

- [ ] **Step 1: Add the gating test**

```ts
test('attach button is hidden when current model lacks vision', async () => {
  const app = await launchBioRouter({ model: 'ollama-text-only-stub' });
  // The stub model is declared with supports_vision unset in metadata.

  await expect(app.locator('[data-testid="attach-button"]')).toBeHidden();

  // Paste of plain text still works.
  await app.evaluate(() => {
    const dt = new DataTransfer();
    dt.setData('text/plain', 'hi from test');
    document.querySelector('textarea')!.dispatchEvent(new ClipboardEvent('paste', { clipboardData: dt }));
  });
  await expect(app.locator('textarea')).toHaveValue('hi from test');
});
```

- [ ] **Step 2: Run both e2e tests**

Run: `cd ui/desktop && npm run test-e2e -- multimodal-image.spec.ts`
Expected: PASS (both cases).

- [ ] **Step 3: Commit**

```bash
git add ui/desktop/playwright/multimodal-image.spec.ts
git commit -m "test(e2e): attach hidden for non-vision model"
```

---

### Task 20: Manual verification checklist

**Files:** None — this is a runtime check.

- [ ] **Step 1: Start the app**

Run: `just run-ui`
Expected: BioRouter window opens.

- [ ] **Step 2: Paste a screenshot to Claude Sonnet**

Select Claude Sonnet as the model. Take a screenshot (Cmd+Shift+4 on macOS) and paste (Cmd+V) into the chat box. Confirm thumbnail appears, then send "What do you see?"
Expected: Model describes the screenshot content (not the file path).

- [ ] **Step 3: Drop a PNG to GPT-4o**

Switch model to gpt-4o. Drag a `.png` file from Finder onto the chat box. Send "Describe this image."
Expected: Model describes the image.

- [ ] **Step 4: Pick a JPEG via attach on Gemini**

Switch to gemini-1.5-pro (or 2.5-flash). Click the attach button, pick a `.jpg`. Send "What is this?"
Expected: Model describes the image. (Pre-fix this would silently strip the image.)

- [ ] **Step 5: Switch to a text-only Ollama model**

Switch to any Ollama model whose `supports_vision` was not declared `true`.
Expected: Attach button disappears. Paste of an image is ignored. Paste of text still works.

- [ ] **Step 6: Reload a session containing images**

Quit BioRouter. Relaunch (`just run-ui`). Open the session from step 2 (or any of 2-4) from the recent sessions list.
Expected: The transcript renders the images inline. The text is intact.

- [ ] **Step 7: Tag the work as complete**

If all six checks pass, this plan is done. If any fail, the failure mode goes back to the relevant task and gets fixed before the plan is marked complete.

---

## Self-Review Notes (for the agent executing this plan)

- **Type names:** TS `ImageContent` is `{ data: string; mimeType: string }` (camelCase). Rust `ImageContent` uses `mime_type`. Serde maps automatically via `#[serde(rename_all = "camelCase")]` on the relevant struct — verify if a wire-format issue appears.
- **`supports_vision` is `Option<bool>`.** `None` → treated as `false` in the UI (`info?.supportsVision === true`). Don't compare to `false` directly.
- **Path-embed legacy code path stays.** `imageUtils.ts` keeps extracting paths from text for backward-compat with old sessions. It's a no-op for new messages.
- **No new fetch.** `currentModelSupportsVision` is derived from already-loaded provider metadata. Don't add a fetch.
- **Databricks coverage caveat from the spec:** if the manual checklist on a Databricks-served Claude model fails, treat that as a separate sub-task — likely a missing image-format branch in `databricks.rs`. Out of scope unless it actually fails.
