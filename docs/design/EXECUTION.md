# UI overhaul — execution status

**Branch:** `worktree-redesign-ui-cohesion` · **Worktree:** `.claude/worktrees/redesign-ui-cohesion`
**Started:** 2026-07-16 · **Last updated:** 2026-07-16

**This file is the single source of truth for where the work stands.** If you
read one document, read this one. It links out to the others.

| Doc | What it is |
|---|---|
| **EXECUTION.md** (this file) | Status, step list, what's done, what's left, what's known-broken |
| [`ui-cohesion-redesign.html`](ui-cohesion-redesign.html) | The approved visual spec. Interactive: Current ⇄ Redesigned, light/dark, Highlight |
| [`chat-groups-plan.md`](chat-groups-plan.md) | The chat-groups plan (3 designs → adversarial judge). Carries the MEASURED banner for R1/R4 |
| [`chat-groups-r7-spike.md`](chat-groups-r7-spike.md) | Proof that two KnowledgeProviders clobber each other through the server |
| [`../../design.md`](../../design.md) | The design system. Decisions D-01…D-32; Part 6b is this pass; Part 7 is the drift register |

---

## Where we stand: 11 of 15 steps done

| # | Step | Status |
|---|---|---|
| 1 | Audit the app against the design system (sidebar / chat / preview / terminal / markdown / pop-ups) | ✅ done |
| 2 | Approved visual spec (rev 1→5) | ✅ done |
| 3 | Cohesion pass implemented (prose tokens, tables, math, headings, surfaces, tabs, sidebar, titlebar, icons) | ✅ done |
| 4 | Docs: design.md Part 6b, drift register given a status column, supersession banner | ✅ done |
| 5 | Chat-groups design: 3 approaches + adversarial judge | ✅ done |
| 6 | R1 measured — drag inside the drag region | ✅ done (PASS) |
| 7 | R4 measured — N mounted BaseChats | ✅ done (affordable) |
| 8 | Stage 0 — session-scope the broadcast events | ✅ done |
| 9 | Stages 1–2 — tabs in one group | ✅ done |
| 10 | Stage 3 — split, drag, drop zones, global dock | ✅ done |
| 11 | Theme default → auto | ✅ done |
| 12 | **User-reported bugs** (tab-per-click + Cmd+W, sidebar collapse, centering, icon parity) | 🔄 in flight |
| 13 | **D-31** tab labels → sans (spec ✅ committed; code pending — agents hold the files) | 🔄 in flight |
| 14 | **D-32** the yield ladder — responsive collapse at every window size | ⬜ next |
| 15 | Final gate + full visual QA sweep | ⬜ not started |

---

## Commits (every step reversible)

```
609849b2  fix(chat)    session-scope the broadcast events so N chats can coexist
751f06db  feat(chat)   tabs — one group, on the shared .br-tab classes
191fbc28  feat(chat)   split the chat area into groups — drag a tab to an edge
76882cd6  docs         R4 measured — N-mount affordable, split unblocked
68fc7a24  design       tab labels are sans, not mono (D-31) — user override
081de99e  fix(theme)   default to auto (follow the OS) instead of light
1c6df30d  feat(ui)     implement the UI cohesion pass
6ca66fe7  fix(ui)      the Biorouter logo was a solid square, not a glyph
c6f7551e  docs         record the cohesion pass; drift register status column
4e6a4121  docs         the chat-groups plan, the R7 spike, the R1 measurement
```

---

## Gates

| Gate | State |
|---|---|
| `tsc --noEmit` | ✅ clean |
| `eslint --max-warnings 0` | ✅ clean |
| `check-contrast.mjs` | ✅ 140/140 (was 128 — +12 guard the code ground) |
| `vitest run` | ✅ **1133/1133, 0 failures** — no timeout flags |

---

## Verified by driving the real app (not by unit test)

jsdom applies no CSS and computes no layout, so none of this could be proven in
a test. Each was observed:

- the tab strip renders in the 52px header; deep-link `?resumeSessionId=` loads
- **the split works**: drag a tab to an edge → 2 groups, each with its own strip,
  transcript and composer; 2 chat columns mounted
- **the drop zone tints live, mid-flight**: `top → top → center → center → right`
  as the cursor crosses, ghost up, source dimmed at 35%
- prose colours are warm tokens (`<strong>` = `#2a2520`, not `#101828`)
- the tab-strip ground is byte-identical to the sidebar's (`rgb(243,237,225)`)
- dark mode via the real persistence path

### The "pre-existing test failures" were real, and are fixed

I called them flakes three times. They were not. Two genuine defects in the
shared test setup:

1. **`findBy*` was racing a 1s timeout.** Testing Library's async helpers use
   their own `asyncUtilTimeout` (default 1000ms), which is INDEPENDENT of
   vitest's `testTimeout` — which is why raising `--testTimeout` to 30s never
   helped. Under parallel load the list took >1s to render, so `findByText`
   gave up while the test still had 29s left. Fixed globally with
   `configure({ asyncUtilTimeout: 5000 })`.
2. **jsdom has no `matchMedia` and nothing stubbed it.** Any component reaching
   a responsive hook died. It was being re-stubbed per file — I added one of
   those workarounds myself instead of fixing the root, which was the tell.

### Bugs only the running app found

1. **The logo was a solid square.** Vite inlines `glyph.svg` as a `data:` URI
   containing `'` and `(`; an *unquoted* CSS `url()` cannot contain those, so
   the tokenizer emitted a bad-url-token and Chrome discarded `mask-image`
   entirely. The mark paints `currentColor` and relies on the mask to cut its
   shape → a solid accent block. Pre-existing; affects every use of the mark.
   **The old test asserted the property was SET and passed** — jsdom's CSS
   parser accepts what Chrome rejects. Green test, broken feature.
2. **The tab read "New Session"** for a loaded chat while `document.title` had
   the real name: nothing announces a session name on *load*, only on rename.

---

## Known broken / open

| Item | State |
|---|---|
| Sidebar collapse can't recover | 🔄 being fixed (step 12) — worst of the reported bugs |
| Chat column flush-left, ~158px dead space right | 🔄 being fixed — measured, `mx-auto` defeated by something reserving width |
| Tab icons at 13/12px, off §3.9's 16/20/24 scale | 🔄 being fixed |
| History click replaces instead of opening a tab | 🔄 being fixed — **user override of the VS Code preview-tab design** |
| Preview panel not window-pinned in a split | 📌 **known partial, deliberate.** It stays per-pane; in a left/right split it sits inside the active group's box, not at the window edge. Hoisting it drops the artifact tab stack on every group switch (plan Stage 5, its own change) |
| `KnowledgeProvider` nesting | 🚫 blocked on the R7 prerequisite fix — I wrote it, could not demonstrate it with a green test, and **reverted it** rather than ship an unverified change to a server-write path |
| `ChatStreamController` never evicted | 📋 pre-existing leak; tabs make it easier to hit but do not cause it. File separately |
| `WorkflowsView.tsx:167` still routes to `/dashboard` | 📋 removing the titlebar control did not remove the last entry point |

---

## Decisions the user overrode (recorded, not argued)

- **D-31 — tab labels: mono → sans.** I borrowed otty.sh's mono-for-UI-labels
  idea and justified it with P6. In the app it read as a special thin font that
  belonged to nothing else on screen — the exact incoherence this pass exists to
  remove. Mono keeps the jobs it earns (code, terminal, paths); a tab label is a
  name. **Mono for data, sans for chrome.**
- **Tab-per-click.** VS Code preview tabs (single click = italic, reuses the
  slot) were deliberate, to stop history browsing leaving twelve tabs behind.
  The user looked at it and wants a tab per click, deduped, closable with ×/⌘W.
- **D-32 — the yield ladder.** The active chat always wins; everything else
  yields in a fixed order: sidebar → preview panel → tab labels → the split
  itself.

---

## What is NOT built, and why (each has a reason, not a shrug)

- **Splitting past 4 groups** — `MAX_GROUPS=4` is the edge of the R4 evidence
  (measured with small windows and short transcripts), not a memory cliff.
- **⌥-duplicate a tab** — two tabs on one sessionId = two composers submitting
  into one turn.
- **Harvesting Dashboard's `createdHere` delete-on-close** — combined with
  preview tabs, browsing history could DELETE sessions. The judge called it the
  highest-consequence bug in the packet.
- **The artifact-panel hoist** — see the known partial above.
