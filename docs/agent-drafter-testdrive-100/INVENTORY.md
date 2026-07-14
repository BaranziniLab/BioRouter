# App and evidence inventory

All authored app stores are worktree-local under `.br-testdrive/runtime/config/biorouter/agent_drafter/`.

| Spec | App id | Static | Functional | Aesthetic | Screenshot |
|---:|---|---|---|---|---|
| 001 | `spec-001-variant-tribunal` | pass | partial | aligned | `shots/spec-001-pre-runtime-fix.png` |
| 002 | `spec-002-cohort-funnel-foundry` | pass | partial | partial | `shots/spec-002-initial.png` |
| 003 | `spec-003-pathway-seance` | pass | partial | partial | `shots/spec-003-refined.png` |
| 004 | `spec-004-trial-regia` | pass | partial | aligned | `shots/spec-004-initial.png` |
| 005 | `spec-005-omics-loom` | pass | partial | partial | `shots/spec-005-integrated.png` |
| 006 | `spec-006-ward-board` | pass | partial | aligned | `shots/spec-006-initial.png` |
| 007 | `spec-007-provenance-autopsy` | pass | partial | aligned | `shots/spec-007-initial.png` |
| 008 | `spec-008-manhattan-signal-room` | pass | partial | partial | `shots/spec-008-initial.png` |
| 009 | `spec-009-survival-atelier` | pass | fail | aligned | `shots/spec-009-initial.png` |
| 010 | `spec-010-diagnosis-odyssey` | pass | fail | partial | `shots/spec-010-initial.png` |
| 011 | `spec-011-reaction-diffusion-foundry` | pass | partial | aligned | `shots/spec-011/agent-loop.png` |
| 012 | `spec-012-contagion-studio` | pass | partial | partial | `shots/spec-012/fit-loop.png` |
| 013 | `spec-013-orbital-sandbox` | pass | partial | partial | `shots/spec-013/stabilize-loop.png` |
| 014 | `spec-014-serengeti-engine` | pass | partial | aligned | `shots/spec-014/balance-run.png` |
| 015 | `spec-015-foldscape` | pass | partial | partial | `shots/spec-015/mutation-state-split.png` |
| 016 | `spec-016-aerocanvas` | pass | partial | partial | `shots/spec-016/optimize-loop.png` |
| 017 | `spec-017-automata-loom` | pass (integration re-audit: invalid KB) | partial | partial | `shots/spec-017/glider-run.png` |
| 018 | `spec-018-systemdynamics-forge` | pass (integration re-audit: invalid KB + missing routes) | partial | partial | `shots/spec-018/autowire-run.png` |

Specs 011–018 were successfully authored after VPN restoration. Five controlled layout-diversity probes are complete; Specs 019–020 are in the active authoring batch. See [`PROVIDER-BLOCKER.md`](PROVIDER-BLOCKER.md) for the resolved outage record.

Every observed main and worker session and every manifest model reference used only `versa_azure/gpt-5.5-2026-04-24`. No fallback model was used.
