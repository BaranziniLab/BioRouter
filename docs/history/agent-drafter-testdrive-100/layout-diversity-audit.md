# Layout diversity audit and controlled probes

> **What this is.** An investigation into why every app the 100-app test drive produced looked
> structurally alike, and the five purpose-built probe apps authored to establish whether that
> sameness came from the test corpus or from Agent Drafter itself.
> **Status:** Historical record — complete. The corpus audit, the authoring of all five probes, the
> 5/5 static audit, and desktop browser verification were all finished. The corrective prompting
> protocol it prescribed for specs 016–100 was applied from spec 016, but the run stopped at spec
> 025 and never reached 100.
> **Audience:** maintainers of Agent Drafter and the Apps SDK.

The audit carries no date of its own. It was written after the first 14 numbered apps had been
generated and before spec 016 was authored, so it sits mid-run in the July 2026 campaign.

Two terms are used throughout. **Archetype** is the starter shape Agent Drafter seeds an app from —
`explorer`, `dashboard`, `workbench`, `wizard`, `canvas`, or `chat`. **Starter gravity** is this
document's coined term for the tendency of a generated app to keep the geometry of whichever starter
it began from, even when the prompt asks for something else.

Because both the plan and the results for the probe experiment live in this one file, each section
below says explicitly whether it is describing what was prescribed or what was observed.

## The finding

The recurring left rail + center stage + right inspector + bottom transport shape is primarily
imposed by the 100-idea corpus. It is not yet evidence that Agent Drafter is incapable of other
layouts.

A mechanical audit of every `**Layout:**` line in the corpus
([hundred-app-test-specs.md](../../agent-drafter/testing/hundred-app-test-specs.md)) found:

| Corpus property | Count |
|---|---:|
| Explicitly mentions `Left` | 100 / 100 |
| Explicitly mentions `Center` | 100 / 100 |
| Explicitly mentions `Right` | 100 / 100 |
| Explicitly mentions `Bottom` | 100 / 100 |
| Uses the word `rail` | 89 / 100 |
| Uses the word `inspector` | 96 / 100 |

The first 14 generated apps therefore reflect their locked design orders. They do show different
themes and central widgets, but they do not constitute a fair structural-diversity test.

## What Agent Drafter claims to support

The shipped implementation exposes five structured non-chat starters, plus the legacy `chat` starter
— six rows in total below:

| Archetype | Starter shape |
|---|---|
| `explorer` | network/graph + search + detail |
| `dashboard` | KPI mosaic + narrative/detail |
| `workbench` | dense table + selected-row detail |
| `wizard` | centered staged form |
| `canvas` | author-registered full draw surface |
| `chat` | legacy chat card |

The [apps platform design](../../agent-drafter/apps-platform-design.md) explicitly says apps should
look and interact differently. The starters are not identical: dashboard and wizard do not seed the
same two-column arrangement used by explorer, workbench, and canvas. However, three of the five
structured starters do begin from a two-column card grid, so starter gravity is a secondary risk even
though the idea corpus is the dominant cause of the observed three-panel repetition.

## Corrective protocol prescribed for specs 016–100

This section records the protocol the audit prescribed at the time. It was applied from spec 016
onward; the run ended at spec 025.

The locked regions and dimensions remained mandatory. Starting with spec 016, the Agent Drafter order
also required concept-specific spatial hierarchy, responsive behavior, controls, and visual rhythm,
and explicitly prohibited copying a prior app's generic rail/card/inspector CSS. Per-app aesthetic
rubrics were to record both theme alignment and whether the composition is meaningfully distinct.

This did not rewrite the 100 ideas: every requested region remained present. Where a spec itself
mandates three columns, diversity had to come from the composition inside and between those regions
rather than from silently violating the spec.

## Controlled probes: what was required of them

The 100 prompts cannot prove structural range because all 100 prescribe the same four-way skeleton.
Five additional, clearly labeled probe apps were therefore authored through Agent Drafter and
retained in the worktree-local BioRouter app store. They are evidence probes, not substitutes for any
numbered application.

| Probe id | Required archetype | Layout constraint |
|---|---|---|
| `layout-probe-kpi-mosaic` | `dashboard` | asymmetric KPI mosaic with top command ribbon and bottom narrative drawer; no persistent sidebars |
| `layout-probe-centered-wizard` | `wizard` | single centered stepper with progressive disclosure and full-width completion canvas; no sidebars |
| `layout-probe-radial-canvas` | `canvas` | full-bleed radial canvas with floating tool petals and a modal sheet; no left/right columns |
| `layout-probe-tabletop-workbench` | `workbench` | full-width table under a top filter ribbon, with an expandable bottom detail drawer |
| `layout-probe-constellation` | `explorer` | full-bleed network, command palette overlay, and transient popover dossier rather than a persistent inspector |

Each probe was required to:

- be authored only through `create_app` / `update_app` / `configure_app` / `build_app` in a named
  Agent Drafter session;
- use only `versa_azure/gpt-5.5-2026-04-24` for main, worker, and route models;
- remain under `.br-testdrive/runtime/config/biorouter/agent_drafter/`;
- build and lint cleanly, include responsive media rules, and pass desktop browser checks (plus
  narrow visual checks when the browser surface supports resizing);
- demonstrate a direct-manipulation control and one agent-driven action; and
- have screenshot and per-probe evidence recorded beside the 100-app results.

## Controlled probes: what they showed

All five probe apps were authored through named Agent Drafter sessions, remained in the isolated
BioRouter store, and used the UCSF model. Static evidence is in
[`data/layout-probe-static-audit.json`](data/layout-probe-static-audit.json); the per-probe browser
rubrics are in `layout-probes/`, linked by name below. Screenshots were captured during the run but
not preserved in this repository.

| Probe | Static | Structural diversity | Functional | Aesthetic |
|---|---|---|---|---|
| [KPI mosaic](layout-probes/kpi-mosaic.md) | pass | pass | partial | aligned |
| [Centered wizard](layout-probes/centered-wizard.md) | pass | pass | partial | aligned |
| [Radial canvas](layout-probes/radial-canvas.md) | pass after theme refinement | pass | partial | aligned after refinement |
| [Tabletop workbench](layout-probes/tabletop-workbench.md) | pass after theme refinement | pass | partial | partial at runtime |
| [Constellation explorer](layout-probes/constellation.md) | pass after theme refinement | pass | partial | aligned baseline / partial runtime |

The controlled result is decisive: Agent Drafter can generate materially different configurations and
looks when the prompt asks for them. The observed numbered-app homogeneity is therefore primarily a
test-corpus constraint, with starter gravity as a secondary risk rather than a hard layout
limitation.

The probes also show that geometry diversity does not fix the runtime control bugs. All five lost the
first `probe_adjusted` signal, all five entered a repeated control-plane call pattern, and several
exposed blank or stale bindings. Those defects are tracked independently in the
[audit findings register](audit-findings-register.md).

## Completion status

Corpus audit, five-probe authoring, static audit (5/5), and desktop browser verification are
complete. The in-app browser did not expose viewport resizing, so narrow behavior is evidenced by
authored media rules rather than by a narrow screenshot. Numbered-app prompting from spec 016 onward
included the anti-template guard.

## Related documentation

- [Test drive README](README.md) — the index for this campaign and the reading order.
- [Audit findings register](audit-findings-register.md) — finding 1 is this audit's corpus finding;
  the runtime defects the probes reproduced are findings 6, 8, 12, and 13.
- [Hundred-app test specs](../../agent-drafter/testing/hundred-app-test-specs.md) — the corpus whose
  `Layout` lines this audit counted.
- [Agent Drafter apps platform design](../../agent-drafter/apps-platform-design.md) — the archetype
  starters and the design intent that apps should look different.
- [Authored-app verdict index](authored-app-verdict-index.md) — the numbered apps these probes are
  contrasted against.
