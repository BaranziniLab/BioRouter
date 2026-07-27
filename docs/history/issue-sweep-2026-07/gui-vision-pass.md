# GUI vision pass — issue sweep 2026-07

> **What this is.** The record of the in-app visual verification of the sweep's
> UI-visible fixes, driven over CDP against the dev GUI built from merged main
> (post B1/B4/B5/B6/B7/B8/B9), with a sandboxed `XDG_CONFIG_HOME` and the real
> llama.cpp/Ollama local-model stack per the campaign's local-model policy.
> **Status:** Historical record — pass executed 2026-07-26 evening.

## Environment

- Dev GUI: vite renderer (`BIOROUTER_NO_HMR=1`, port 5173) + Electron against a
  freshly rebuilt `.vite/build/main.js`, CDP on 9333, driven via agent-browser.
- Config sandboxed (fresh onboarding); data home real, so local models and the
  shared session store behave as on a user machine.
- Local model: Gemma 4 E4B through the Llama Server card (Homebrew llama-server
  discovered on PATH; Ollama blob → HF QAT fallback download, ~4 min).

## Results

| Issue | Verdict | Evidence |
|---|---|---|
| #35 default + speed hints | PASS | 128 GiB machine preselects **Gemma 4 12B** (7.6 GB, "Fast — dense 12B"), not the 24 GB Qwen3.6 MoE; every onboarding option and inventory row shows size + speed hint + context (e.g. "Fastest — ~2B active parameters"). |
| #34 progress survives navigation | PASS | Started the Gemma 4 E2B install in Settings→Models (3%), navigated to Home for 25 s, returned: progress live at 33% (later 48%), never frozen or lost. Onboarding op flow also ran end-to-end (warm-up → auto-complete into the app). |
| #36 friendly artifact error | PASS | Real IPC `read-artifact-file` on a missing path returns `{"error":"This file was moved, renamed, or deleted, so it can't be previewed anymore.","code":"ENOENT"}` — no raw Node error. |
| #37 accent strip + spring | PASS | Active tab computed style shows `inset 0 -2px 0 0 <--accent-bar>` in dark (`#ee6c1a`) and light (`#d95b08`); strip follows the active tab on switch; `--ease-spring: cubic-bezier(0.34,1.56,0.64,1)` present; terminal dock's small tabs carry the strip consistently. |
| #38 last-tab close → Home | PASS | Closing tab 1 of 2 stayed on /pair; closing the last tab navigated to `#/` showing the heatmap Home; the closed session remained in Recents (not deleted). |
| #21 terminal | PASS (typing + lifecycle) | Typed `echo terminal-typing-works-$((6*7))` into a real pane → `terminal-typing-works-42`; per-pane close works across two panes; closing the last pane destroys the dock with the window intact. The Cmd+W ladder itself is Electron-menu-driven (unreachable over CDP) and is pinned by the 7 registry-driven unit tests. |
| #22 typing during streaming | PASS | 84-char string typed into the composer mid-turn stayed intact; rAF frame-gap probe during streaming: worst 12.1 ms / p95 12.0 / median 10.0 over 180 frames (main thread at full frame rate). |
| #39 working dir | UNIT-COVERED | The chooser opens a native macOS dialog (not automatable over CDP). Wiring is pinned by `BaseChat.workingDir.test.ts` + `ChatInput.workingDir.test.tsx`. |
| #27/#28 tool-call detail | UNIT/INTEGRATION-COVERED | Sandbox config has no extensions enabled and the small local model is unreliable for coordinated tool calls; the surfaces are pinned by ToolCallWithResponse tests (executed-calls rows, plain-text args, real error attribution) + 3 Rust integration cases. |

## Observations (not sweep regressions)

- The staged `ui/desktop/src/bin/llamacpp/` binaries on this machine are Windows
  artifacts from a prior cross-build; the sidecar found llama-server on PATH
  instead. Restage with `just copy-binary`-equivalent for mac before packaging.
- A local gemma4 chat turn sat in "Thinking" for 10+ minutes with llama-server
  idle and Ollama busy (the concurrent E2B pull); the turn's UI (elapsed timer,
  stop affordance, streaming state) behaved correctly and Stop worked. Local
  model routing/perf quirk, tracked outside this sweep.
