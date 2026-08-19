# OKF migration — progress

> **What this is.** The live tracker for the OKF/BioOKF migration. One row per stage; updated as
> each lands. A stage is DONE only when its gate in [`stages.md`](stages.md) has actually been run.
> **Status:** Current (in progress).
> **Audience:** Contributors working on the Knowledge subsystem.

**Branch:** `feature/okf-knowledge` · **Worktree:** `~/Desktop/BioRouter-okf`

| Stage | What | State | Gate run? |
| --- | --- | --- | --- |
| — | Research (OKF v0.2, LLM Wiki, BioOKF v0.5, BioRouter audit) | DONE | n/a |
| — | Design records DR-1…DR-11 | DONE | committee pending |
| 0 | `okf` core module (Rust) | NOT STARTED | — |
| 1 | BioOKF profile module (28 types / 35 predicates) | NOT STARTED | — |
| 2 | Graph derivation: typed nodes + edges, three link forms | NOT STARTED | — |
| 3 | Store, manifest, service, legacy migration | NOT STARTED | — |
| 4 | MCP tool surface + skills | NOT STARTED | — |
| 5 | Sub-agent macros (profile-aware) | NOT STARTED | — |
| 6 | HTTP routes + OpenAPI + TS client | NOT STARTED | — |
| 7 | Desktop UI: format chooser, typed graph, faceting | NOT STARTED | — |
| 8 | Verification in the real app | NOT STARTED | — |

## Measured baselines

Captured before any change, so a later "pre + N" assertion has something true to stand on.

| Suite | Count | Measured |
| --- | --- | --- |
| `cargo test -p biorouter-mcp --lib knowledge::` | 283 | 2026-08-19 |
| `cargo test -p biorouter-server --test knowledge_routes` | 46 | 2026-08-19 |
| Desktop `it()` under `components/knowledge` | 94 in 14 files | 2026-08-19 |

## Log

### 2026-08-19
- Worktree `~/Desktop/BioRouter-okf` created on `feature/okf-knowledge` off `main` (ea34ef57).
- 16-agent research sweep completed: OKF v0.2 spec digest, LLM Wiki lineage, BioOKF v0.5
  (layout, vocabulary, provenance, lint, core crate, MCP surface, Studio renderer), and a
  six-part audit of BioRouter's own knowledge subsystem.
- Design records DR-1…DR-11 written.
