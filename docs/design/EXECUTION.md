# UI overhaul — execution status

**Branch:** `worktree-redesign-ui-cohesion` · **Worktree:** `.claude/worktrees/redesign-ui-cohesion`
**Started:** 2026-07-16 · **Last updated:** 2026-07-17

**This file is the single source of truth for where the work stands.** If you
read one document, read this one. It links out to the others.

| Doc | What it is |
|---|---|
| **EXECUTION.md** (this file) | Status, step list, what's done, what's left, what's known-broken |
| [`ui-cohesion-redesign.html`](ui-cohesion-redesign.html) | The approved visual spec. Interactive: Current ⇄ Redesigned, light/dark, Highlight |
| [`chat-groups-plan.md`](chat-groups-plan.md) | The chat-groups plan (3 designs → adversarial judge). Carries the MEASURED banner for R1/R4 |
| [`chat-groups-r7-spike.md`](chat-groups-r7-spike.md) | Proof that two KnowledgeProviders clobber each other through the server |
| [`../../design.md`](../../design.md) | The design system. Decisions D-01…D-34; Part 6b is this pass; Part 7 is the drift register |

---

## Where we stand: 15 of 20 steps done

The list grew from 15 steps to 20. The user added the browser-tab keyboard model
(⌘T / ⌘N / ⌃Tab), made **lag a first-class acceptance criterion**, and asked for
**progressive conversation loading** (paint the transcript first, let extensions
finish behind it); D-31 turned out to have a second half (the preview's chrome,
D-33). New work is added as new rows rather than folded into existing ones, so
the denominator stays honest — the fraction should get *worse* when scope grows,
not quietly better.

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
| 12 | **User-reported bugs** (tab-per-click + Cmd+W, sidebar collapse, centering, icon parity) | ✅ done |
| 13 | **D-31** tab labels → sans — spec + code | ✅ done |
| 14 | **D-33** the other half of D-31: the preview's chrome → sans | ✅ done |
| 15 | **D-34** kill the "New Session" flash — a tab opens already named | ✅ done |
| 16 | **Browser keys** — ⌘T new tab, ⌘N new window, ⌃Tab cycle (chat + preview) | 🔄 in flight |
| 17 | **Lag** — measure first, then fix; leave a repeatable perf gate | 🔄 in flight |
| 18 | **Progressive load** — paint the transcript first; extensions/model finish behind it; toast on ready (naming partial failures) | 🔄 in flight — **premise is being measured before anything is rebuilt** |
| 19 | **D-32** the yield ladder — responsive collapse at every window size | ⬜ next |
| 20 | Final gate + full visual QA sweep (**lag is now an acceptance criterion, not a nice-to-have**) | ⬜ not started |

---

## Commits (every step reversible)

```
2e5c9262  design       the other half of D-31 — the preview's chrome → sans (D-33)
c9cff940  fix(tabs)    open a history chat already named, not "New Session" (D-34)
d0257a6a  design(tabs) set tab labels in the app's own sans — the D-31 code
427ea3d4  docs         EXECUTION.md — the suite is 1133/1133
621326ac  test         fix the suite's two real root causes, no timeout crutch
c22710f2  docs         add EXECUTION.md — the single status doc
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
   Fixed twice over — first by listening for the load (which corrected the name
   but only *after* a visible second), then properly, by handing the name over
   at open time (D-34). The first fix was treating the symptom: I made the
   correction arrive reliably instead of asking why a correction was needed for
   a string the sidebar was already rendering.

### Still to verify by driving

- **D-31 / D-33 (the fonts) and D-34 (the flash) are committed and unit-tested,
  but have NOT yet been confirmed in the running app.** jsdom applies no CSS, so
  the font change in particular is exactly the class of thing that "passes" in a
  test and is wrong on screen — the logo-square bug below was precisely that.
  Two agents are holding the Electron single-instance lock; this gets checked in
  the step-19 sweep, and until then it is unproven, not done.

---

## Known broken / open

| Item | State |
|---|---|
| Sidebar collapse can't recover | ✅ **fixed.** Not the cause I guessed. The rail translated correctly at every width and the toggle always hit-tested itself — it needed a *resize* to reproduce: `AppLayout`'s auto-collapse effect listed `open` in its deps, so the user's own click re-triggered it and set `open` straight back to `false` in the same tick. Now gated on a width-bucket **crossing**. Pure `sidebarAutoCollapseAction` + 8 tests, because the bug was **state, not geometry** — so it is genuinely provable in jsdom |
| Chat column flush-left, ~158px dead space right | ✅ **fixed, measured 250/250, delta 0** (was 24/158). Two causes: BaseChat's root had no `flex-1` (it used to mount only in a flex *column*, where stretch gave it full width for free; a chat *group* mounts it in a flex **row**, where width is the main axis — so it hugged its content and `mx-auto` centred inside *that*), and `contentClassName` opened with `pr-1`, a right-only 4px inset. **No jsdom test**, deliberately: jsdom computes no layout, so a centering assertion there would pass with the bug present. Verified by measurement in the real app |
| Tab icons at 13/12px, off §3.9's 16/20/24 scale | ✅ **fixed** — both 16px. Glyphs already came from `app-icons` (stroke 1.5 confirmed *rendered*, which catches a raw `lucide-react` swap) |
| History click replaces instead of opening a tab | ✅ **fixed** — tab per click, deduped across groups, closable via × / ⌘W. **User override of the VS Code preview-tab design**; the whole preview-tab concept (`preview` field, `pinTab`, pin-on-run, double-click-to-pin) is retired, with a test that fails if it creeps back |
| ⌘W closed the **window**, not the tab | ✅ **fixed, and it was a landmine.** `{ role: 'close' }` in `main.ts` silently claimed `CmdOrCtrl+W` with no `accelerator:` line to grep for. A renderer keydown listener could **never** have won — a menu accelerator is consumed before the web contents sees the key, so the window would have closed regardless, taking every tab with it. ⌘W = Close Tab (via IPC), ⇧⌘W = Close Window, per Safari/Chrome. Verified by dumping the built menu |
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
- **D-33 — mono for data, sans for chrome.** D-31 was only half the answer: the
  user said the font was still wrong in the chat *and the preview*. The tab
  strips were already fixed (one line covered both — the preview shares chat's
  `.br-tabstrip`), but the preview's status strip still set *everything* in
  mono. The test, applied per-usage: **mono is a claim that the glyphs matter.**
  Paths, git refs and `tabular-nums` counts keep it; the language chip and the
  status legend don't.
- **D-34 — a tab opens already named.** The "New Session" flash was a
  round-trip to the server for a string that was already on screen. The name
  now travels with the click. The late rename stays — deep links, reloads and
  fresh chats carry no name, and only the load knows `user_set_name`.
- **The browser model.** ⌘T = new tab (a new chat), ⌘N = new window, ⌘W =
  close tab, ⌃Tab = next tab — in the preview's tab stack as well as chat's.
  ⌃Tab and not ⌘Tab because the OS owns ⌘Tab and will not give it up; this is
  also what Safari and Chrome do on macOS.

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
