# Knowledge base buildout

This folder is the build record of BioRouter's Knowledge feature: personal, git-backed markdown knowledge bases that an LLM maintains incrementally, with credibility classification, graph derivation, an MCP tool surface, HTTP routes, the `.brkb` portability format, and a desktop UI. It holds the founding design, approved 2026-05-30, and the six implementation plans that executed it through 2026-06-01. **All of it happened.** The design was built, every plan shipped, and the feature is live — `crates/biorouter-mcp/src/knowledge/`, `crates/biorouter-server/src/routes/knowledge.rs` and `ui/desktop/src/components/knowledge/` are the code these documents produced. Nothing here was abandoned or later removed.

These are completed records, kept for provenance and not as current guidance. Come here to find out *why* a module, a route or a component is shaped the way it is, and to recover the reasoning behind a decision that the code alone does not explain. Do not come here for what the system does today: the shipped behaviour is documented in the Knowledge section of `CLAUDE.md` and in the source itself, and the live working documents — surveys of how a layer works now, and plans for extending it — are one folder over in [`docs/knowledge-base/`](../../knowledge-base/README.md). Two further cautions apply when reading the plans. Their unticked `- [ ]` checkboxes are the plans as written, not outstanding work. And their worktree paths, pinned dependency versions, source line anchors and expected test counts are all point-in-time; each file's own header says which of its details have since drifted, and the shipped code is authoritative wherever the two disagree.

## Documents

| Document | What it covers |
|---|---|
| [Founding design](founding-design.md) | The origin design document for the Knowledge feature — git-backed markdown knowledge bases maintained by an LLM, a credibility classifier, graph derivation, the MCP tool surface, the HTTP routes, the `.brkb` portability format, and the desktop UI. Approved 2026-05-30 and built out across the six plans below. |
| [Plan 1 — knowledge storage, git and graph derivation](plan-1-storage-git-and-graph.md) | The storage, git, format-conversion, credibility-classification and graph-derivation layers behind a shared `KnowledgeService`. No UI, no macros, no chat integration. |
| [Plan 2 — knowledge macros and the sub-agent loop](plan-2-macros-and-subagent-loop.md) | The `kb_ingest_source` / `kb_query` / `kb_lint` macros running over a bounded sub-agent loop, the primitives Plan 1 deferred (`kb_search`, active-KB state, `kb_append_log`, the MCP-exposed transaction tools), and the real agentic credibility fallback. |
| [Plan 3 — knowledge HTTP routes and `.brkb` export/import](plan-3-http-routes-and-export.md) | Exposing the Knowledge backend over HTTP from `biorouter-server` with SSE-streamed macros, adding the `.brkb` export/import format, and regenerating the TypeScript client. |
| [Plan 4 — Knowledge view and ingest panel](plan-4-knowledge-view-and-ingest.md) | The sidebar entry, the top-level `KnowledgeView` shell, the multi-KB command-palette selector, and the ingest panel with dropzone, paste box, staged list, model picker and live SSE progress. |
| [Plan 5 — knowledge graph view and change-log drawer](plan-5-graph-view-and-change-log.md) | Replacing the Plan-4 right-column placeholder with a `react-force-graph-2d` credibility-coloured graph, and a git-history change-log drawer with preview and restore. |
| [Plan 6 — knowledge chat integration and closeout](plan-6-chat-integration-and-closeout.md) | The last plan: persisting the active KB to disk, a chat-side KB chip in `ChatInput`, a `/knowledge` slash command, the retracted-node badge Plan 5 deferred, and the closing `CLAUDE.md` documentation. |

## Related documentation

- [Knowledge base](../../knowledge-base/README.md) — the live working documents for the Knowledge subsystem; go there for the current shape of a layer and what work is still open on it.
- [Knowledge view redesign](../../design/ui-overhaul/knowledge-view-redesign.md) — the later visual redesign of the surface Plans 4 and 5 built, signed off 2026-07-10; it is also the best surviving record of the intent behind those plans' unrecoverable UI mockup.
- [Nested `KnowledgeProvider`: the chat-groups nesting blocker](../../design/chat-groups/knowledge-provider-nesting-blocker.md) — an open defect in the active-KB state that Plan 6 shipped, still unfixed.
- [Historical records](../README.md) — the archive index this folder sits in, covering the other completed campaigns and designs from May to July 2026.
