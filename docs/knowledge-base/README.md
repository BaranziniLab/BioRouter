# Knowledge base

The Knowledge base is BioRouter's personal, LLM-maintained wiki: it ingests a file, URL or pasted text, converts it deterministically to markdown, and lets a bounded sub-agent digest that markdown into interlinked wiki pages backed by a git history. This folder holds the **live working documents** for that subsystem — surveys of how a layer works today, and plans for extending it.

Come here when you are changing the Knowledge subsystem and need to know the current shape of a layer or what work is still open on it. Two neighbours cover adjacent ground. The **build record** — the founding design and the six implementation plans that shipped the feature — lives in [`../history/knowledge-base-buildout/`](../history/knowledge-base-buildout/README.md); go there for why a module is shaped the way it is, not for what to do next. Anything about the shipped code's present behaviour that is not open work is documented in `CLAUDE.md` and in `crates/biorouter-mcp/src/knowledge/` itself.

## Documents

| Document | What it covers |
|---|---|
| [Knowledge ingestion format roadmap](ingestion-format-roadmap.md) | A survey of the conversion pipeline as it stood on 2026-06-10, a licensed comparison of open-source document converters, and a phased plan to extend ingestion to PowerPoint, Excel/ODS and a higher-fidelity PDF path. **Partially implemented** — Phases 1, 2, 3 and 4.1 shipped and were verified on 2026-06-10; Phases 4.2, 4.3 and 5 are open work. The architecture survey and the June 2026 licensing research inside it are reference snapshots of that date, not live registers. |

## Related documentation

- [Founding design](../history/knowledge-base-buildout/founding-design.md) — the origin design for git-backed markdown knowledge bases, and why the pipeline is convert-then-digest at all. Historical record; read it for the rationale behind a design decision.
- [Plan 1 — storage, git and graph derivation](../history/knowledge-base-buildout/plan-1-storage-git-and-graph.md) — the storage, conversion, credibility and graph layers behind `KnowledgeService` that the ingestion roadmap extends.
- [Plan 3 — HTTP routes and `.brkb` export/import](../history/knowledge-base-buildout/plan-3-http-routes-and-export.md) — defines the `/knowledge/bases/{id}/ingest` SSE endpoint every new format travels through.
- [System overview](../architecture/system-overview.md) — where the Knowledge base sits among BioRouter's other on-disk state and subsystems.
