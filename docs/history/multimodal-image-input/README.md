# Multimodal image input

This work happened and it still holds. The two documents here designed and built multimodal image input — letting a user paste, drop, or pick an image and have a vision-capable model actually receive the bytes — and the feature shipped in **v1.76.1** (released 2026-06-01), with three follow-up fixes landing 2026-05-31. It has not been removed or superseded. The pieces it introduced are still in the tree: `supports_vision: Option<bool>` on `ModelInfo` in `crates/biorouter/src/providers/base.rs`, the `readTempImageAsBase64` IPC binding in `ui/desktop/src/preload.ts`, and the Gemini `inline_data` arm in `crates/biorouter/src/providers/formats/google.rs`. Both files were written 2026-05-30 and are kept for the record and for provenance, not as current guidance — they describe the state of the code as it was being changed, so where they and the source disagree, the source wins.

Come here to reconstruct *why* image attachments are shaped the way they are: why the frontend reads temp files to base64 only at send time rather than passing paths, why `supports_vision` is an `Option<bool>` that must never be compared against `false`, why the legacy path-in-text extraction in `imageUtils.ts` is still present, and why PDFs and generic file attachments were deliberately left out. If instead you want to know what the feature does today, read the [v1.76.1 release notes](../../releases/notes/v1.76.1.md) or the code. If you want which providers and models can accept images at all, that is [choosing a model provider](../../getting-started/choosing-a-model-provider.md), not this folder — the model lists in these two documents are a mid-2026 snapshot and were never maintained afterwards.

## Documents

| Document | What it covers |
|---|---|
| [Image input design](image-input-design.md) | The design spec for sending pasted, dropped, or picked images to vision-capable models as inline base64 content blocks — adding a `supports_vision` capability flag to per-model metadata and fixing Gemini's silent image drop. This is the *why*: the problem statement, the chosen inline-base64 approach, the rejected path-reference and hybrid-blob-store alternatives, and the explicit non-goals. |
| [Image input and vision-flag implementation plan](image-input-plan.md) | The task-by-task execution of that spec across six phases (model metadata, Gemini wire format, IPC and message construction, chat input, renderer, verification) and twenty numbered tasks. This is the *how*, with the exact code that landed, plus implementation notes recording decisions that outlive the plan itself. |

> **Note on the plan's checkboxes.** The `- [ ]` steps in the implementation plan are left in their original unticked state. They record the plan as authored, not remaining work — the plan was executed on the day it was written.

## Related documentation

- [Historical records](../README.md) — the archive index this folder belongs to, and the fastest way to find which other subsystems have a written provenance trail.
- [v1.76.1 release notes](../../releases/notes/v1.76.1.md) — the user-facing account of what actually shipped, including the 56 vision-capable models across 5 providers and the drag-drop and per-session gating fixes that followed.
- [Choosing a model provider](../../getting-started/choosing-a-model-provider.md) — which providers and models are available today, and therefore which ones can accept images.
- [System overview](../../architecture/system-overview.md) — the renderer → `biorouterd` → provider path that this work threaded a new content type through.
