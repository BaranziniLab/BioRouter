# Architecture

This folder holds the orientation-level description of how Biorouter is put together: the three
layers (interface, agent, extensions), the crate and process boundaries between them, the agent
interaction loop, and the cross-cutting policies — such as error handling — that every subsystem
inherits. These are maps, not references. Where a topic has a page of its own, the documents here
link out rather than duplicating it.

Come here first, before descending into any subsystem document — the pages in this folder name the
vocabulary and boundaries that the rest of `docs/` assumes you already know. Go elsewhere if you
want the mechanics rather than the map: the deeper workings of the reasoning loop live in
[`docs/agent-loop/`](../agent-loop/context-engineering.md), and a single component's own behaviour
lives in its subsystem folder (`docs/providers/`, `docs/extensions/`, `docs/desktop-ui/`,
`docs/knowledge-base/`). Architecture designs that have been replaced are kept under
[`docs/history/legacy-architecture/`](../history/legacy-architecture/extension-trait-design.md), not
here.

## Documents

| Document | What it covers |
|---|---|
| [System overview](system-overview.md) | The orientation-level map of UCSF Biorouter — its three-layer architecture, its Rust and Electron tech stack, the agent interaction loop, where configuration and data live, and its security posture. Current; the starting point for anyone new to the codebase. |
| [Agent error model](agent-error-model.md) | A design note on the two-tier error model: infrastructure failures raised to the caller, versus model-generated "agent errors" fed back to the LLM as recoverable prompts. **Superseded** — the concept still governs the agent loop, but every concrete type name is obsolete (`AgentError` no longer exists; rmcp's `ErrorData` replaced it), so the page carries a mapping table to the current names. |

## Related documentation

- [Installation](../getting-started/installation.md) — how to get the platform described here onto a machine; the natural next step after the system overview.
- [Context engineering](../agent-loop/context-engineering.md) — the next level down on step 5 of the agent loop: what happens to messages as the context window fills.
- [Security overview](../security/README.md) — expands each bullet of the overview's security section into an enforced mechanism.
- [Extension trait design](../history/legacy-architecture/extension-trait-design.md) — the historical extension API the error model was designed alongside, sharing the same superseded vocabulary.
