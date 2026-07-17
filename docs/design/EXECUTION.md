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
| [`../../design.md`](../../design.md) | The design system. Decisions D-01…D-37; Part 6b is this pass; Part 7 is the drift register |

---

## Where we stand: 20 of 20 steps done — the branch is complete

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
| 16 | **Browser keys** — ⌘T new tab, ⌘N new window, ⌃Tab cycle (chat + preview) | ✅ done (D-35) — 13/13 driven; preview-side ⌃Tab jsdom-only |
| 17 | **Lag** — measure first, then fix; leave a repeatable perf gate | ✅ done — **measured; no refactor shipped, because nothing measured indicted app code** |
| 18 | **Progressive load** — paint the transcript first; extensions/model finish behind it; toast on ready (naming partial failures) | ✅ done — **2667ms → 738ms to read** |
| 19 | **D-32** the yield ladder — responsive collapse at every window size | ✅ done (+ D-36, D-37) |
| 20 | Final gate + full visual QA sweep (**lag is an acceptance criterion, not a nice-to-have**) | ✅ done — 6/6 on screen, 1 real bug found + fixed |

---

## Commits (every step reversible)

```
94125bbc  refactor      1120 had three homes; give it one (+ correct D-32's rationale)
0c329018  fix(sidebar)  the Recents hidden-count badge is sans, not mono (D-31)
0e75258f  fix(tabs)     move the titlebar reserve OUT of the strip's scroll box
4621d654  test(harness) a git-repository fixture — D-33's split was unverifiable without it
8bcfcbe5  feat(preview) rung 3 for the preview's strip, through the SHARED rule
85fe794c  feat(groups)  rung 4's trigger — the split's width watcher
c611c154  feat(groups)  rung 4's state half — merge to one, and give it back
3f71f504  feat(tabs)    rung 3 — tabs collapse into a ▾ once they scroll out of sight
6158c6ae  feat(chat)    rung 2 — the preview panel yields its column to the transcript
ea68495d  feat(layout)  the yield ladder, as pure rules (D-32)
d9028330  feat(chat)   paint the conversation before the model and extensions load
0a701721  feat(toasts) extension readiness: multi-chat aware, partials reported honestly
c5676eae  fix(build)   land closeActiveTabRegistry — HEAD imported a module not in the tree
84b0c5a4  test(tabs)   the chat side of the Ctrl+Tab arbitration
fa725c70  design       D-35 — the tab model is a browser's, and so is the keyboard
acef5969  fix(chat)    a submit fills the blank tab you typed in, not the leftmost
4fc34bb3  fix(preview) Ctrl+Tab in the panel is scoped to focus, not to "is it open"
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
| `lint:check` | ✅ clean (tsc + eslint 0 warnings + contrast). **Use `npm run lint:check`, not a bare `npx eslint .`** — the latter lints `.vite/build/main.js`, a build artifact, and reports thousands of phantom errors |
| `check-contrast.mjs` | ✅ 140/140 (was 128 — +12 guard the code ground) |
| `vitest run` | ✅ **1266/1266 (151 files), 0 failures** — no timeout flags. **Run it on an idle machine; see below** |

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

### The suite is load-sensitive, and it fooled me

I saw 17 failures and diagnosed a global ⌃Tab keydown listener swallowing
Escape across the app. The cluster looked damning: menu dismissal, modal
dismissal, focus restore. **I was wrong, and the way it was disproved is the
lesson.**

The failing *sets differed between runs*, every failing file *passed in
isolation*, `ChatGroupsProvider` only mounts under `/pair` so most of those
components never had the listener at all — and `uptime` showed **load average
112–166 on 8 cores**, because I had two heavy agents driving Electron while a
third ran the suite. At load 7: **1181/1181**. Exactly one of the 17 was real
(the ArtifactViewer ⌃Tab contract, since fixed).

Two things follow, and they generalise:

1. **A coherent-looking story is not evidence.** My hypothesis explained the
   cluster beautifully. It was still wrong. The cheap discriminator —
   *does it fail in isolation?* — costs seconds and would have killed it
   immediately. Run that before theorising.
2. **A green suite under heavy load is not a pass, and a red one is not a
   fail.** `asyncUtilTimeout: 5000` bought headroom, not immunity; at load 166
   nothing saves you. **Check `uptime` before believing a suite result.**

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

### The browser tab model — ⌘T / ⌘N / ⌘W / ⌃Tab (D-35)

**⌘T was already claimed by the menu**, exactly as ⌘W had been: "Go → New Chat"
held `CmdOrCtrl+T` and merely navigated Home. That is now the *second* time a
menu accelerator silently owned a key we wanted — a renderer listener could
never have won either. Both go through menu + IPC. Worth generalising: **before
binding any key in the renderer, dump the built menu.** `scripts/debug/menu-dump.mjs`
does it, and it confirmed ⌃Tab is claimed by nothing, so ⌃Tab is an honest DOM
listener.

**Verified by driving, 13/13** (`scripts/debug/tab-shortcuts-probe.mjs`): ⌘T
0→1→2 blank tabs each with a composer; a real ⌃Tab keypress moving active 1→0;
⌃⇧Tab back; wrap; ⌃Tab from *inside* the composer (a browser switches tabs while
you type — no text-input guard, deliberately); plain Tab inert; ⌘N 1→2 windows.

**Two bugs this work found, neither of them the feature:**
1. **The preview panel was hijacking ⌃Tab app-wide.** Its listener was gated on
   `isOpen`, not focus — with a panel open it cycled *previews* from anywhere,
   including the composer. **Its test dispatched at `window`, so it passed while
   the bug was live** — theatre, and rewritten to dispatch from inside the panel.
   Both listeners now consult one predicate (`isWithinArtifactPanel`), because
   both listen on `window` in capture and capture order is merely mount order.
2. **Empty-tab adoption filled the *leftmost* blank tab.** ⌘T makes two blanks a
   keystroke away, so your first message landed in the wrong one.

### Lag — measured, and the honest outcome was "don't refactor"

**The tabs/split architecture is not a lag source.** Typing latency
(keydown→paint) with the split at full stretch, on a quiet machine, mount
verified (`groups=4 tabs=4 composers=4 messagesInDom=1211`):

| | p50 | p95 | long tasks |
|---|---|---|---|
| 1 group (355 msgs) | 25.7ms | 33.5ms | **0** |
| 4 groups (1211 msgs) | 35.4ms | 38.7ms | **0** |

**Ratio 1.16×.** That was the headline risk of this branch — mounting N chats —
and it is answered.

**No perf refactor was shipped, deliberately.** Nothing measured indicts app
code. A refactor with no measured delta is a risk, not a fix.

**Two traps invalidated the first round of numbers**, and both generalise:

1. **Machine load.** Same probe, same build: **load 93 → p95 70ms + 41 long
   tasks; load 11 → p95 25.7ms + 0 long tasks.** Contention was nearly reported
   as an app bug — the same trap that produced my phantom "17 failures". The
   probe now asserts load and prints it next to every number.
2. **The dev build is not the app.** A CPU profile indicted `jsxDEV` /
   `logComponentRender` / owner stacks — **~646ms of a ~1650ms typing burst is
   React dev-only work that does not exist in the packaged app.** Every
   absolute number here is dev-inflated; the *ratio* is what's trustworthy.

The probe **refuses to emit a number until it has proved the thing under test
is on screen**, and that caught four real voids (including a "production"
number that was silently measured against the dev bundle). This is the direct
answer to the earlier N-mount probe that reported "affordable" while measuring
an empty page.

**The standing gate:** `cd ui/desktop && node scripts/perf/chat-perf-probe.mjs`
(needs vite on :5173, non-zero exit on breach, docs in `scripts/perf/README.md`).
Load-bearing budgets are the **ratio (1.6×)** and **long tasks (2)** — both
near-immune to dev overhead. Absolutes are coarse on purpose.

### The premise behind progressive loading is confirmed, with numbers

Measured against the real backend on a 355-message session:

| `/agent/resume` | Time |
|---|---|
| `load_model_and_extensions: false` (transcript only) | **0.50s** (warm 0.067s) |
| `load_model_and_extensions: true` (what the app calls) | **5.07s** |
| a *different* session, `true` — i.e. chat #2 | **2.64s** |

The conversation is fetched **first** and then held ~4.6s behind 9 extensions
that contribute **359 bytes**. And extensions are **per-session, not global**,
so every new tab re-pays ~2.5s — a cost tabs and splits multiply.

The user called this from the outside, hedged it ("perhaps"), and was right.

### Progressive loading — built, and it was the user's call

Split `/agent/resume` in two at the **existing** `load_model_and_extensions`
seam: phase 1 (`false`) paints the transcript, phase 2 (`true`) loads the model
and extensions behind it and toasts. No route change, no OpenAPI regen — the
seam was already there; nobody had used it.

**Observed, real app + real backend, n=3, load 8–12:**

| | before | after |
|---|---|---|
| time to transcript | median **2667ms** | median **738ms** |
| time to ready (toast) | median 2667ms | median 2651ms |

**3.6× faster to read, ~1.9s earlier, and readiness arrives no later than
before.** The BEFORE column is the proof of the coupling the user suspected:
paint time **== ready time to the millisecond**. They were literally the same
event.

**The premise was confirmed but the numbers recalibrated.** Same setup (9
extensions, 373 bytes, gating 355 messages), but cold full resume measures
**1.25–1.42s here, not the 5.07s** the perf agent reported — that figure was
most likely taken under machine load. Flagged rather than repeated: **a number
you cannot reproduce is not a number.**

**Submit-before-ready: HELD, not dropped and not blocked.** The message lands in
the transcript, the chat shows Streaming with a live abortController, and Stop
works throughout — so the user sees their message and sees it working. Observed:
submitted at 962ms (2s *before* ready), sent at 3167ms, text intact. Both submit
tests fail when the hold is removed.

**Multi-tab toasts, two rules:** a clean load only toasts for the *focused* chat;
a **failure always toasts and can never be overwritten by a later success**.
Verified in a real 2×2 split: max 1 toast on screen, background successes
silent, a background *failure* still spoke up.

**Two pre-existing bugs found on the way, both fixed, both would have got worse:**
- the transcript LRU cache (process-lifetime, no TTL) could reach a submit
  having **never loaded the agent at all** — a live-looking chat over a backend
  with no extensions;
- `useToolCount` fetches on `sessionId` alone with nothing to re-trigger it. It
  already raced; progressive loading would have made it *reliably* read zero
  tools and cache that forever, silently killing the "too many tools" alert.

**Regression:** a deliberately broken extension (isolated config root) still
painted the transcript at 686ms and toasted *"8 of 9 extensions loaded — Failed:
Ucsfomopagent"* on the correct popover surface. Navigating away mid-load leaves
no unmount warnings, no orphaned stream. The artifact panel, KB chip and model
selector never depended on the old ordering (they derive from the transcript or
from global state) — checked, fine.

### The yield ladder (D-32) — measured at every width

Each rung is a **pure function of (width, state)** in `Layout/yieldLadder.ts`
(34 unit tests), following `sidebarAutoCollapseAction`'s shape on purpose:
these are **state bugs waiting to happen, not layout bugs**, and the sidebar
taught us that the testable part is the rule, not the geometry.

| window | sidebar | pane | chat col | preview | tabs | rows | ▾ |
|---|---|---|---|---|---|---|---|
| 1400 | expanded | 1160 | 584 | side, 520 | 5×88 | 1 | no |
| 1120 | expanded | 880 | 464 | side, **360 floor** | 5×88 | 1 | **yes** |
| 1000 | **compact** | 1000 | 584 | side, 360 | 5×88 | 1 | yes |
| 860 | overlay | 860 | **760** | **overlay** | 5×98 | 1 | no |
| 800 | overlay | 800 | 744 | overlay | 5×89 | 1 | no |

**It reverses exactly** — the same numbers at every width on the way back up.
**`rows = 1` in every run at every width: the strip never wrapped**, which was
the one thing the spec forbade outright.

640/760 are absent because they are **unreachable** — the OS window floors at
800 (`main.ts`). Sweeping there would have been three identical rows dressed up
as data.

**Rung 4 composing with rung 1, which is the ladder actually working:** a 3-up
split (needs 1082) at **1400** → 3 groups @289px; at **1300** (shell 1060) →
**merged**, chat back to 760; at **1100** → the sidebar collapses, which hands
the shell 1100 back → **the split is restored**. Rung 1 yielding buys back the
room rung 4 needed, *while the window is still shrinking*.

**A bug rung 3 exposed:** the preview panel's strip was `overflow-hidden` — its
tabs never shrank-then-scrolled at all; past the floor they were **clipped and
unreachable**. That is squarely what the user asked about ("if certain elements
don't fit, then collapse the element"), so rung 3 covers both strips through a
**shared hook** — the spec's claim that both strips behave alike is only true
if the *rule* is shared, not merely similar. An existing test had pinned the
clipping as if it were intended; it was flipped deliberately, not quietly.

**Mutation: 9 run.** M4 (remove the crossing gate) killed 4 — including *"keeps
a split the user made BY HAND"*. **M7 initially SURVIVED**: a "leaf order" test
was decorative, because the fixture's object order accidentally matched leaf
order. Rewritten with a `left`-split fixture where the two genuinely disagree;
it kills the mutant now. That is the second decorative test found on this
branch by mutating rather than trusting a green tick.

**Perf gate: 1.19× against a 1.6× budget**, p95 33.8/40.3ms on a verified-quiet
box — a ResizeObserver-driven ladder was exactly the kind of thing that could
have regressed typing, and it didn't.

### Step 20 — the visual sweep: 6/6 on screen, and one real bug

Driven in the real Electron app (Playwright, real userData) plus the artifact
harness in Chrome. **Observed values, not impressions.** 144 screenshots.

| # | Item | Verdict | The evidence |
|---|---|---|---|
| 1 | **Fonts** (D-31/D-33) | **PASS** | `.br-tab__label`, **both** strips: `ui-sans-serif, -apple-system, "system-ui", …` @ **13px**, tracking `normal`. The D-33 split, checked item by item: chip "TypeScript" **sans**; path + `app.ts` + "1 line" **mono** (+`tabular-nums`); git ref **mono**; all 5 legend labels incl. "Modified" **sans**. App-wide: **5 of 136** visible text leaves are mono — 4 file paths and the live `$1.97`. Every one earned |
| 2 | **No "New Session" flash** (D-34) | **PASS** | 3 runs, **402 frames sampled**, tab already named at first paint. **0 placeholder frames.** The sampler was proven live by catching each transition |
| 3 | **Artifact panel / code view** | **PASS** | 15 types. The documented Prism/Tailwind collision reproduced: **26 `token table` spans, every one `display: inline`** — the `main.css` guard beats Tailwind. Long line **763ch** (added: the shipped fixtures max 70ch, so they proved nothing): `scrollWidth 6038 vs clientWidth 1179`, `white-space: pre`, line numbers aligned, **0 flex lines** |
| 4 | **Light + dark × parchment + alma-mater** | **PASS** | All four via the real persistence path + reload, 11 tabs mounted each |
| 5 | **Yield ladder 1400→800** | **PASS** | 11 tabs: never wraps (single `tabTops [9]`), nothing clipped, ▾ reachable at every rung, menu lists all 11 |
| 6 | **Collapsed sidebar + traffic lights** | **BUG → FIXED** | below |

#### The bug: the reserve was *inside the thing that moves*

The 172px traffic-light reserve was `padding-left` **on the scrolling element**.
Padding is scrollable content — so any scroll carried the reserve away with it.
And selecting a tab scrolls it into view, so **this was the ordinary path, not an
edge case.** Measured clip-aware at 800px, sidebar collapsed, 11 tabs,
`scrollLeft: 428` *as found*:

- **before** — the scroll box clips `0..674`: tabs visibly rendered at **x=0**,
  **40..128**, **131..219** — under the traffic lights (which end at 72) and under
  the controls (100–164), each hit-testing to itself
- **after** — clips `172..674`: 6 visible tabs, all ≥172, **zero violations**

A/B'd by stashing the fix and re-running the identical probe.
**`getBoundingClientRect` alone is not evidence here — it ignores clipping**, and
the sweep's first probe was wrong for exactly that reason.

**The unit tests asserted the padding property was SET. It was set. It just could
not hold.** This is the third time on this branch that a green test guarded a
property instead of the behaviour — the logo's `mask-image`, the preview panel's
`window`-dispatched ⌃Tab, and now this. The tests now assert the *structural*
invariant (the reserve lives on an ancestor of the scroll box; the scroll box
carries no left inset), which is the strongest claim jsdom can honestly make.

The spec called this one in advance: *"a reserve that fails silently is worse
than no reserve."*

### One last thing the sweep flagged, and it was worse than reported

`BaseChat` re-declared `SIDEBAR_COMPACT_TITLE_WIDTH = 1120` instead of importing
it. In fact **1120 was declared three times** — `AppLayout` (rung 1's threshold),
`TitlebarControls` (the titlebar reserve), and that third copy. All three agreed,
so nothing was broken. **That is exactly why it was worth fixing before it broke:**
rung 1 fires on this width and rungs 2–4 inherit the room it frees, so a drifted
copy would desynchronise the ladder from the chrome *with nothing failing*.

The guard is a **source-text assertion**, deliberately — the defect is textual (a
duplicate literal), and no runtime assertion can see a constant that was never
imported. Mutation-checked: re-introducing a fourth copy fails exactly that test.

The same commit corrected `CHAT_MIN_WIDTH`'s comment, which still carried the
68ch rationale D-36 records as false. **A comment repeating a justification we
have formally documented as wrong is worse than no comment.**

### Still to verify by driving

- **The preview side of ⌃Tab is jsdom-only.** Opening the panel needs a live
  agent turn to produce an artifact. The arbitration is a pure
  `event.target`/`closest()` question, which jsdom models faithfully and two
  mutations caught — but the two listeners have not been proven to coexist in
  the real app.
- **Inherent, not a bug to fix:** with focus inside a preview's *sandboxed
  iframe*, ⌃Tab does nothing — the keydown never reaches the parent document.
  Clicking the panel's chrome restores it. Documented in `tabCycle.ts`.
- **Production-bundle timing numbers were never obtained** — every absolute
  above is dev-inflated. The ratio and long-task counts are the trustworthy
  parts.
- **4-group scroll shows a 442.9ms worst frame** (3/117 dropped). Real but
  narrow — and **unverified**: the probe's scroller selector was wrong, so this
  is a lead, not a finding.
- ✅ **Resolved.** D-31 / D-33 / D-34 are now confirmed on screen — see the
  step-20 sweep above. The caution was warranted: the sweep found a real bug
  (the traffic-light reserve) that every unit test passed straight through.
- **A live agent turn was never exercised** — no provider is configured in this
  environment (`XIAOMI_MIMO_API_KEY` missing; the composer reads "Unavailable").
  The sweep worked around it with saved transcripts, real file links and the
  harness, so **no artifact was produced by a live turn**.
- **Split / multi-group layouts were not screen-verified** — the sweep drove the
  single-group case only. The `firstLeaf` reserve aiming is unit-tested, not
  seen.
- **Widths below 800px are unreachable** (`main.ts` sets `minWidth: 800`). Not
  faked.
- macOS only.

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
| **The app's own `biorouterd` cannot read this machine's provider secrets** | 📋 **pre-existing, verified identical on old code** (by reverting). `provider_restore_failed: XIAOMI_MIMO_` → the route returns `extension_results: None` → extensions are never attempted → no readiness toast fires. All toast verification therefore ran against an **external** backend (`EXTERNAL=1`). This is not caused by progressive loading, but progressive loading is the first feature to *depend* on that field. Worth its own investigation |
| **Extensions are per-session, not global** | 📌 **the deeper cost, and bigger than this branch.** 4 splits = 4 × ~1.0s of duplicate extension startup, spawning 4 copies of the same MCP servers. Proven, not inferred: a 4-split window fired **4 separate `resume(load_model_and_extensions=true)` calls**, and three cold sessions each paid independently. **Progressive loading hides this latency; it does not remove the work.** A shared/pooled extension manager is worth more than the reordering — and is a backend change of real size. Recommended, not built |
| **A split created at a narrow width keeps its slivers** | 📌 **deliberate — D-37.** A 4-up dragged out at 1400px sits at 169px panes and stays. Rung 4 merges only on a *crossing*, because a watcher that dissolved a split the instant you made it would be fighting the drop that just happened. If slivers should never exist, the fix belongs in the **drop** (refuse it, where the user can see why), not in a watcher that undoes it afterwards. Needs a decision, not a patch |
| **No virtualization in the transcript** | 📋 **count-proven, left alone deliberately.** 4 tabs mount **1211** message components at once and never unmount them (`ProgressiveMessageList` batches 20 at a time until *all* are mounted). It does not hurt typing today — 0 long tasks at 4 groups — so fixing it now would be a refactor with no measured delta. **This is the scaling cliff**: the number that grows is messages × tabs, and nothing currently bounds it. File it before someone opens a 5000-message chat in four tabs |
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
