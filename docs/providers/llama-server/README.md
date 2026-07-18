# Llama Server provider

This folder covers the bundled **Llama Server** (`llamacpp`) provider — the zero-setup
local-inference path, where the desktop app ships a pinned `llama-server` binary from
llama.cpp and manages it as a sidecar process. The material here is verification-focused:
how to confirm that every model in the shipped catalog still loads, answers, calls tools,
and streams, and which security invariants around the sidecar must not regress.

Come here when you are changing the `llamacpp` provider or its managed sidecar, bumping
the pinned llama.cpp build, or altering `MODEL_CATALOG`, and you need to re-verify the
result. If you are instead deciding *which* provider to use, read
[choosing a model provider](../../getting-started/choosing-a-model-provider.md). Hosted,
API-key providers are documented as flat files one level up — [z.ai (GLM)](../zai-glm.md)
and [Xiaomi MiMo](../xiaomi-mimo.md) — and none of the sidecar, download, or local-memory
concerns on this page apply to them. For the environment variables themselves rather than
the tests that exercise them, see
[environment variables](../../configuration/environment-variables.md).

## Documents

| Document | What it covers |
|----------|----------------|
| [Llama Server model catalog QA checklist](model-catalog-qa-checklist.md) | A repeatable checklist, plus the automated harness that executes most of it, for verifying every catalog model's availability, capability, tool-calling, speed, and robustness. **Superseded in part** — it was written against an earlier pinned llama.cpp build and an earlier `MODEL_CATALOG`, so the test procedure still holds but the model list and expected launch flags must be re-derived from the provider and sidecar source before running it. |

## Related documentation

- [Environment variables](../../configuration/environment-variables.md) — the
  `BIOROUTER_LLAMACPP_BIN`, `LLAMACPP_EXTRA_ARGS`, and `LLAMACPP_EXTERNAL_HOST` knobs the
  checklist sets.
- [Choosing a model provider](../../getting-started/choosing-a-model-provider.md) — where
  Llama Server sits among the local, institutional, and commercial providers.
- [z.ai (GLM) provider](../zai-glm.md) — a sibling provider verification reference
  covering the same surfaces for a hosted provider.
- [Debugging the dev GUI with agent-browser](../../desktop-ui/agent-browser-debugging.md)
  — how to drive the dev app for the checklist's manual GUI section.
