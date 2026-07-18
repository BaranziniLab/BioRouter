# Architecture

This folder holds the orientation-level description of how Biorouter is put together: the three
layers (interface, agent, extensions), the crate and process boundaries between them, and the
agent runtime — how one request becomes assembled context, inspected tool work, durable state and
a verified answer. These are maps of the whole, not references to one part. Where a topic has a
page of its own, the documents here link out rather than duplicating it.

Come here first, before descending into any subsystem document — the pages in this folder name the
vocabulary and boundaries that the rest of `docs/` assumes you already know. Go elsewhere if you
want the mechanics rather than the map: the deeper workings of the reasoning loop and the designs
behind its guardrails live in [`docs/agent-loop/`](../agent-loop/README.md), and a single
component's own behaviour lives in its subsystem folder (`docs/providers/`, `docs/extensions/`,
`docs/desktop-ui/`, `docs/knowledge-base/`). Architecture designs that have been replaced are kept
under [`docs/history/legacy-architecture/`](../history/legacy-architecture/README.md), not here —
including the [agent error model](../history/legacy-architecture/agent-error-model.md), whose
two-tier concept still governs the loop even though every type name it uses is gone.

## Documents

| Document | What it covers |
|---|---|
| [System overview](system-overview.md) | The orientation-level map of UCSF Biorouter — its three-layer architecture, its Rust and Electron tech stack, the agent interaction loop, where configuration and data live, and its security posture. Current; the starting point for anyone new to the codebase. |
| [Biorouter agentic system explorer](agentic-system-explorer.md) | The written companion to the explorer page: a code-aligned account of the agent runtime in sixteen parts — turn lifecycle, entry paths, request assembly, the inspection pipeline, vault substitution, dispatch, hook lanes, recovery and transport — each naming its implementing Rust module. Current, and follows the runtime's present behaviour. |

## Rendered pages

`agentic-system-explorer.html` is a self-contained page that **must be opened in a browser to be
useful**: it carries seventeen rendered SVG architecture diagrams of the agent runtime — the turn
lifecycle, entry paths, request assembly, inspection pipeline, vault substitution, dispatch, hook
lanes, safety escalation, recovery paths and transport lanes. It shows nothing meaningful as source
text. Its Markdown companion above carries the reasoning and the specifications, so anyone working
without a browser should read that instead.

## Related documentation

- [Installation](../getting-started/installation.md) — how to get the platform described here onto a machine; the natural next step after the system overview.
- [The agent loop](../agent-loop/README.md) — the next level down from the explorer: the designs behind the loop's guardrails, its lifecycle hooks, and how durable context reaches the model.
- [Context engineering](../agent-loop/context-engineering.md) — the next level down on step 5 of the agent loop: what happens to messages as the context window fills.
- [Security overview](../security/README.md) — expands each bullet of the overview's security section into an enforced mechanism.
- [Legacy architecture](../history/legacy-architecture/README.md) — the superseded architecture designs, including the extension trait framework that was never shipped and the agent error model whose vocabulary the current code has replaced.
