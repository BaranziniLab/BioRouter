# Astryx adoption

> **What this is.** The folder index for the interface revision that rebuilds Biorouter's UI on the construction of Meta's Astryx design system while keeping Biorouter's palette, theming architecture and calm register.
> **Status:** Current — approved and largely built. Phases 1–7, 9 and 10 have landed; phase 8 is partly landed. See [Status](#status) before reading any of these documents as a description of the running app.
> **Audience:** maintainers reviewing the direction; developers and agents who will execute what remains.

Biorouter's design language is documented in [`design.md`](../../../design.md) but was never built as parts, so every view re-derived it: five row-title treatments, four error dialects, five spinner constructions, six modal shells, 140 hand-written `text-[11px]` classes. Astryx is the corrective — not for its colours, but for its discipline: one control height, one easing, one state model, one radius ladder, all expressed as theme tokens.

## Contents

Three documents make the proposal proper, plus one feature specification written against it. They answer different questions, and which one you want depends on what you are doing.

| Document | Read it when | Format |
|---|---|---|
| **[Design of record](astryx-ui-adoption-design.md)** | you want the *argument* — why each change, what it replaces, what evidence backs it | Markdown |
| **[Implementation specification](astryx-implementation-spec.html)** | you are **writing the code** — every token value, every measurement, the file each phase touches, the gates each must pass | HTML, browser |
| **[Design showcase](astryx-design-showcase.html)** | you want to *see* it, or to review quickly | HTML, browser |
| **[Tab tear-off and merge](tab-tear-off-and-merge.md)** | you are implementing the window-management gesture — dragging a chat tab out into its own window and back into another window's strip, plus the session list's "Open in new tab" | Markdown |

- **[Astryx UI adoption — comprehensive interface revision](astryx-ui-adoption-design.md)** — the design of record. Fixes the contract that constrains the work (palette, theming, calm, squared corners, chat density), then specifies foundations, elements, compositions and motion; lists what already agrees and what gets deleted; and ends with a ten-phase execution plan and ten decisions (`A-01`…`A-10`) for sign-off.
- **[Astryx UI adoption — implementation specification](astryx-implementation-spec.html)** — the buildable half, written to be executed top to bottom without needing the argument behind it. **Open in a browser.** Every token with its value; every element with its measurements; the composition specs including the sidebar tally, the terminal grounds per family, the tool-state ladder and the toast layouts; a **per-phase file index** naming what each of the ten phases touches; the verification gates including the known pre-existing test failure; and the ten decisions. Also hosted at <https://claude.ai/code/artifact/9a6a1969-ae6c-4720-b757-4479395335c4>.
- **[Tab tear-off and merge](tab-tear-off-and-merge.md)** — the window-management gesture, specified against the design of record's motion and token vocabulary. Establishes the ground truth of today's drag implementation, then decides the questions that actually govern the feature: why a tab with a turn in flight cannot cross windows (the daemon is shared but a turn's SSE stream has exactly one subscriber, and dropping it cancels the turn), how a drop is resolved when HTML5 drag events cannot cross window boundaries, what the visual for "this will become a new window" is, and which phases can land before the contested files clear. Closes with the session list's "Open in new tab" menu item — which turns out to *name* a gesture the row click already performs.
- **[Astryx design showcase](astryx-design-showcase.html)** — the rendered companion, and the faster way to review the proposal. **Must be opened in a browser**: every control is live and built from the proposed tokens, so the page is a specimen of the system it argues for. Three switches drive it — *Family* (Parchment / Alma Mater / Roche Limit, on the real shipped values) proves the construction survives every palette; *System* flips the whole page between today's geometry and the proposal, so the diff is the page itself; light and dark follow the viewer. Also hosted at <https://claude.ai/code/artifact/c24a639a-9e18-40b5-9cbb-62381acf4bd3>.

## Status

**Approved, and mostly executed.** This line used to read *"Proposed. No implementation has started"*; four separate audits read it, believed it, and drew wrong conclusions about the running app on the strength of it. It was stale by nine phases.

Nine of the ten phases have landed as commits on this worktree:

| Phase | State | Commit |
|---|---|---|
| 1–3 · Motion root, type tokens, radius + tints | Landed | `4d0adfcc` laid the token foundations; `5e644381` completed the layer that phases 5–9 needed |
| 4 · Typeface | Landed, then retuned | `d193dd37` bundled Figtree and switched `--font-sans`. `15f7e7c0` + `89e3e054` then moved the face to Arial by operator preference and dropped the Figtree payload — **only the face changed**; every size, weight, line-height and letter-spacing role is still the one §2.2 specifies, and the `@font-face` is left in place so one line restores it |
| 5 · Controls | Landed | `732f8ca7` |
| 6 · Overlays | Landed | `d0074c08` |
| 7 · Shell | Landed | `9b5b4d84` compacted the sidebar; the 44px band followed separately, because that commit deliberately deferred it (it was written three ways in three files owned by three stewards) and all three had to move in one edit |
| 8 · Views | **Partly landed** | `48b9e5a4` converged Chat history, Settings and Knowledge on the header recipe, the row spec and the table spec. The rest of the phase is outstanding |
| 9 · Chat | Landed | `1d6ecb72` |
| 10 · Sweep | Landed | `4729418f` |

**What is still to execute**, and why the documents below are not tidied down to match the app: the parts describing unbuilt work are kept **verbatim** so they stay executable step by step. Outstanding, at least:

- **The rest of phase 8** — the file explorer's directory and preview panels (§9 of the design of record), and `ScheduleDetailView`, which still spells its title `text-2xl font-semibold tracking-tight` by hand.
- The tool-state ladder, the long-message clamp, the file-preview header, the directory tree's indent guides and status letters, and the code-block "numbers plus horizontal scroll everywhere" rule.
- The remaining dialog-shell adoption and the menu/`Select` row unification.

A landed phase does **not** mean every bullet inside it shipped — the table records which commit carried the phase, not that nothing was left behind. Check the code before assuming an element matches its specification here.

### Amended since the design was written

Four things in the documents below have been overtaken by decisions taken while building, and are corrected in place rather than left to mislead:

- **The 44px chrome band is wired**, not pending. `--chrome-height` is read by all three bands — `BioRouterSidebar/AppSidebar.tsx`, `BaseChat.tsx` and `artifacts/ArtifactViewer.tsx`. The 8px was expected to need the sidebar compaction to pay for it; measured, it did not.
- **The rail gave 20 of those pixels back.** Dropping the band brought the wordmark up under the hairline with the first nav row directly beneath it, which read as crowded rather than dense. The brand block now takes 16px above and 8px below (`px-2 pt-4 pb-2`), and rail rows sit at a 2px gap rather than flush — the same gap the menu recipe uses, so the rail and the menus agree, and Recents matches. `--row-height-rail` is unchanged at 32px; only the gaps moved.
- **The chat measure is a flat `760px`**, not a clamp. It was briefly `clamp(760px, 78%, 1180px)`; a 1180px composer is a longer line, not a more capable input. `--measure-page` stays fluid, because a wide document view genuinely buys content.
- **The landing state has no suggestion chips.** They were built in phase 9 and then **removed by decision** — see §4.4 of the design of record.

## Related documentation

- [Design](../README.md) — the parent folder index.
- [Biorouter Design System](../../../design.md) — the current source of truth this proposal amends.
- [Theme system architecture](../theming/theme-system-architecture.md) — the pipeline the proposal treats as fixed infrastructure.
