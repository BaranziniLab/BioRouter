# Authored-app verdict index

> **What this is.** One row per app that Agent Drafter authored during the 100-app test drive,
> mapping each spec to its static, functional, and aesthetic verdict and to the screenshot captured
> for it.
> **Status:** Historical record — a frozen snapshot of a campaign that stopped. It covers specs
> 001–018, which are the apps that reached a full browser verdict; authoring itself continued to spec
> 025, and `app-results/` holds rubrics for spec-001 through spec-025. The run never reached spec 100.
> **Audience:** maintainers of Agent Drafter and the Apps SDK.

Use this table to find an app, then open its rubric in `app-results/` for the per-check detail behind
the verdict. The defects these verdicts aggregate are recorded once, de-duplicated, in the
[audit findings register](audit-findings-register.md); this index does not restate them.

## Verdict vocabulary

| Column | Values | Meaning |
|---|---|---|
| Static | `pass` | The built app satisfied the static reviewer: manifest shape, declared surface, model pin, theme pack, and layout regions. |
| Functional | `pass` / `partial` / `fail` | Whether the app's runtime checks — reactivity, agent-driven loop, multi-agent orchestration, signal round-trip — succeeded in a browser. `partial` means some checks passed and at least one failed. |
| Aesthetic | `aligned` / `partial` / `off` | Whether the rendered app matched the theme and composition its spec called for. |

Two specs carry an integration re-audit note in the Static column; that re-audit is the stricter
identifier check described in the [platform integration audit](platform-integration-audit.md).

## Apps and verdicts

All authored app stores were worktree-local under
`.br-testdrive/runtime/config/biorouter/agent_drafter/`, an ephemeral sandbox that was not checked
in. The `shots/` screenshot paths below record what was captured during the run; the images were not
preserved in this repository.

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
| 017 | `spec-017-automata-loom` | pass — integration re-audit: invalid KB | partial | partial | `shots/spec-017/glider-run.png` |
| 018 | `spec-018-systemdynamics-forge` | pass — integration re-audit: invalid KB and missing routes | partial | partial | `shots/spec-018/autowire-run.png` |

Specs 011–018 were authored after VPN restoration ended the provider outage; see the
[Azure 403 outage incident](azure-403-outage-incident.md) for the resolved outage record. The five
controlled layout-diversity probes were also complete by this point, and specs 019–020 were the
batch in progress when the run stopped.

Every observed main and worker session, and every manifest model reference, used only
`versa_azure/gpt-5.5-2026-04-24`. No fallback model was used.

## Related documentation

- [Test drive README](README.md) — the index for this campaign, including the full file inventory.
- [Audit findings register](audit-findings-register.md) — the de-duplicated defects behind every
  `partial` and `fail` above.
- [Layout diversity audit](layout-diversity-audit.md) — the five probe apps that sit alongside these
  numbered apps as evidence, and their separate verdict table.
- [Platform integration audit](platform-integration-audit.md) — the stricter identifier re-audit
  that produced the notes in the Static column.
