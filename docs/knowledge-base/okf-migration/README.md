# OKF migration

> **What this is.** The design, decision records and progress tracker for moving BioRouter's
> knowledge bases off the informal "LLM wiki" page format and onto the Open Knowledge Format
> (OKF v0.2), with the Biomedical Open Knowledge Format (BioOKF v0.5) as an optional strict
> profile a user selects at creation time.
> **Status:** In progress.
> **Audience:** Contributors working on the Knowledge subsystem.

## Documents

| Document | What it holds |
| --- | --- |
| [design.md](design.md) | The decision records (DR-1…) and the target format contract. |
| [stages.md](stages.md) | The stepwise implementation plan: nine stages, each with its own gate. |
| [progress.md](progress.md) | Live progress tracker: what has landed, what is in flight, what is blocked. |
| [ui-spec.md](ui-spec.md) | The binding UI specification for Stage 7's Knowledge-section redesign, delivered in slices. |

## The one-paragraph summary

A BioRouter knowledge base becomes an **OKF bundle**: a git-shippable tree of Markdown files whose
every non-reserved `.md` carries YAML frontmatter with a non-empty `type`. Two profiles ship. **OKF**
mode is maximally permissive — any `type` string, any edge predicate, nothing rejected — and is the
default for general-purpose memory, retrieval and development work. **BioOKF** mode is the strict
biomedical profile: `type` must be one of 28 controlled values, edge predicates one of 35, and every
edge carries a provenance triplet. The user picks the profile when the base is created, from the
desktop UI or from the `kb_create_base` MCP tool; the profile is recorded in `manifest.yaml` and
governs validation, lint, the sub-agent's prompt, and how the graph is rendered.

## Related documentation

- [`../README.md`](../README.md) — the knowledge-base document index.
- [`../ingestion-format-roadmap.md`](../ingestion-format-roadmap.md) — the earlier survey of the ingestion pipeline.
- [`../multi-kb-implementation-plan.md`](../multi-kb-implementation-plan.md) — the visible-set / primary-pointer model this work must not disturb.
