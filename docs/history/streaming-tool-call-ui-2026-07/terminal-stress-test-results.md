# In-app terminal: functional and stress test results

> **What this is.** A full functional and stress sweep of the in-app terminal dock — panes,
> per-tab scoping, working-directory correctness, process reaping, and behaviour under load —
> run live against the dev GUI on 2026-07-20, plus the two defects it fixed.
> **Status:** Historical record (completed 2026-07-20). The two fixes it describes are current.
> **Audience:** developers working on the terminal dock and the Electron main process.

The terminal dock is the shell panel that opens under a chat from the session header's terminal
button. It is scoped **per chat tab** — `terminalKey = tabId` — so every tab has its own terminal
with its own panes, its own frozen working directory, and its own scrollback. That scoping is the
part most likely to be wrong in a subtle way, so it is the part this sweep tested hardest.

Findings carry `TERM-NN` identifiers, used in the commit messages for the fixes below. Severity is
one of `blocker` / `major` / `minor` / `polish`.

The headline result: **the working-directory contract holds exactly**, and **normal close reaps
every shell**. The two real defects were both in teardown-and-limit accounting, and both are now
fixed: a PTY leak on renderer reload, and a session cap that was low enough to hit in ordinary use.

## What the terminal actually is

Worth stating up front, because two assumptions in the test brief turned out not to match the
code:

- **There is one terminal "type", not several.** `terminal:create` in
  [`ui/desktop/src/main.ts`](../../../ui/desktop/src/main.ts) spawns the user's `$SHELL` through
  `node-pty`, with a piped-`spawn` fallback used only when `node-pty` fails to load. Nothing in the
  UI chooses between them; the renderer just learns which one it got via `backend: 'pty' | 'process'`.
- **The terminal has no splits.** A dock holds a horizontal strip of pane **tabs**, rendered into a
  single-cell grid where only the active pane is visible. There is no horizontal/vertical/nested
  split gesture, and no keybinding for one. Splitting exists one level up, in the *chat group*
  layout (`ChatGroupsShell`), and the dock deliberately renders **below** the whole group tree
  rather than inside a pane — so a split chat layout has one dock spanning all groups, not one per
  group. Split-specific tests in the brief were therefore not applicable; group-split interaction
  was tested instead.

## Coverage

| Area | Exercised | Result |
|---|---|---|
| Terminal types | PTY backend (`node-pty`); process-pipe fallback read from source only | Pass — PTY is the live path |
| Splits | N/A — no split gesture exists; chat-group splits tested instead | Not applicable |
| Pane tabs | Open, add, switch, close, close-active, close-background | Pass |
| Per-session terminals | 3 chat tabs, each with its own dock | Pass |
| **cwd consistency** | 3 simultaneous sessions in 3 distinct dirs, `pwd` vs. composer chip | **Pass — exact** |
| cwd after a dir change | Session dir changed via `/agent/update_working_dir`, then terminal opened | Pass |
| Tab-switch survival | Scrollback + cwd across repeated tab switches | Pass |
| Busy terminals | 2 concurrent `while true` loops + bulk `yes` output | Pass — renderer ~3.7% CPU |
| Rapid churn | 10 cycles of add-2 / close-2 panes | Pass — no leak |
| Closing a busy terminal | Closed panes running infinite loops | Pass — reaped |
| PTY reaping (normal) | 7 shells open → close all panes | **Pass — 7 → 0** |
| PTY reaping (reload) | Renderer document replaced with shells open | **Fail → fixed (TERM-02)** |
| Session cap | Opened past the per-window limit | **Fail → fixed (TERM-03)** |
| Focus on tab switch | Focus location after switching chat tabs | **Fail — documented (TERM-04)** |
| Auto-open pane count | One "open terminal" click | **Fail — documented (TERM-01)** |
| Theme family switch | Not run — theme files were being edited concurrently | Not tested |

## The working-directory verdict

This was the load-bearing check, and it passes cleanly.

Three sessions were given three distinct working directories through the same endpoint the
composer's directory chip uses (`POST /agent/update_working_dir`), opened as three chat tabs, and
given a terminal each:

| Chat tab | Composer dir chip | `pwd` in that tab's terminal |
|---|---|---|
| Shell and Python request | `/tmp/br-term-a` | `/tmp/br-term-a` |
| BLIPBASE instruction test | `/tmp/br-term-b` | `/tmp/br-term-b` |
| Code execution request | `/tmp/br-term-c` | `/tmp/br-term-c` |

Each terminal reported **its own** session's directory with all three open simultaneously — no
cross-talk, and the dock's own cwd label agreed with both. Switching tabs and returning preserved
each pane's scrollback and prompt. The `terminalKey = tabId` scoping and the frozen-cwd contract
both hold.

One nuance worth recording, because it looks like a bug and is not: a terminal captures its cwd
**once, when it is first opened**, and never re-reads it. A tab whose session directory changes
*after* its terminal is already open keeps the old directory until that terminal is destroyed and
reopened. That is the documented frozen-cwd contract in
[`TerminalDockContext.tsx`](../../../ui/desktop/src/contexts/TerminalDockContext.tsx) — respawning a
live shell under a running command would be worse — and a terminal opened *after* the change picks
up the new directory correctly, which was verified.

## Findings

### TERM-02 — PTY sessions leak on every renderer reload (major, fixed)

**Symptom.** Every shell a window has open survives a renderer reload as an orphaned process that
no UI can reach, and permanently consumes that window's terminal-session budget. This is the direct
cause of the user report that the session cap "fires when nowhere near 8 terminals are open".

**Repro (verified live).**

1. Open two chat tabs, open a terminal in each — three panes total.
2. Confirm three shells: `ps -eo pid,ppid,command | awk -v p=<electron-pid> '$2==p' | grep -c zsh` → `3`.
3. Reload the renderer (Cmd+R, View > Reload, or the `reload-app` IPC).
4. The terminal docks are gone from the UI (`docks: 0`), but the shell count is still `3`, and it
   stays `3` indefinitely.
5. Those three slots are now permanently spent: with the old cap of 8, only five more terminals
   could ever be opened in that window.

**Root cause.** `registerSession` in `main.ts` freed a slot on exactly two events: an explicit
`terminal:dispose` from the renderer, and `webContents` `destroyed`. A reload is neither — it
replaces the document while the `webContents` lives on, so React never runs the effect cleanups
that would have sent `terminal:dispose`. All three reload paths are user-reachable in the shipped
app: Cmd+R is wired directly in `createChat`, View > Reload and Force Reload are in the menu, and
`reload-app` is exposed to the renderer via `preload.ts`.

**Fix.** A new `releaseOwner` path frees every session a renderer owns, called from
`did-start-navigation` (filtered to `isMainFrame && !isSameDocument`, so the app's own hash-router
navigation does **not** kill terminals), from `render-process-gone`, and still from `destroyed`.

**Files.** [`ui/desktop/src/terminalSessionRegistry.ts`](../../../ui/desktop/src/terminalSessionRegistry.ts) (new),
[`ui/desktop/src/main.ts`](../../../ui/desktop/src/main.ts).

**Screenshot.** `/tmp/br-shots-term/04-session-cap-dead-pane.png` (the resulting dead pane).

### TERM-03 — the session cap was too low, unexplained, and contradicted by the UI (major, fixed)

**Symptom.** A window refused the 9th terminal with *"A window can run at most 8 terminal
sessions."* The message named neither a knob nor a reason, and the refusal arrived as a dead pane
that still looked like a normal tab in the strip.

**Repro (verified live).** Open three chat tabs; open a terminal in each (two panes each by
TERM-01) for six shells; add three more panes to one dock. The 8th succeeds; the 9th and 10th open
as panes whose only content is the red refusal text. The dock's "+" button stays enabled the whole
time, because it is gated on a *per-dock* cap of 8 while the real limit is *per-window*.

**Root cause.** Two caps that did not know about each other:
`MAX_TERMINAL_SESSIONS_PER_OWNER = 8` in the main process (the real one, across all docks in a
window) and `MAX_TERMINAL_PANES = 8` in the dock (per dock). Any window with terminals in more than
one chat tab could exceed the former while the latter still said yes.

The cap is also simply a fork-bomb rail, not a product limit — 8 is low enough that ordinary use
with a few chat tabs reaches it, which is exactly what the user hit.

**Fix.** The rail moved to `terminalSessionRegistry.ts` with a default of **64**, an override via
`BIOROUTER_MAX_TERMINAL_SESSIONS` (non-positive or unparseable values fall back to the default
rather than disabling the rail), and a refusal message that names both the limit and the variable.
The per-dock pane cap moved to 32 so the UI is no longer the binding constraint at 8.

Ruled out: this has nothing to do with `DEFAULT_MAX_CONCURRENT_TOOLS = 8` in
`crates/biorouter/src/agents/tool_dispatch_limits.rs`. The terminal never touches the Rust agent —
`terminal:create` is an Electron IPC handler calling `node-pty` directly. The matching value is a
coincidence.

**Files.** [`ui/desktop/src/terminalSessionRegistry.ts`](../../../ui/desktop/src/terminalSessionRegistry.ts),
[`ui/desktop/src/main.ts`](../../../ui/desktop/src/main.ts),
[`ui/desktop/src/components/InAppTerminalDock.tsx`](../../../ui/desktop/src/components/InAppTerminalDock.tsx).

### TERM-01 — opening the terminal creates two panes, not one (minor, documented)

**Symptom.** One click on "Open in-app terminal" yields a dock with two pane tabs — `<dir>` and
`<dir> 2` — and two live shells. Reproduced on every dock opened during this sweep, though not
deterministically: one dock in one run opened with a single pane.

**Repro.** Open any chat tab's terminal in a dev build; count `[role="tab"]` in the dock.

**Root cause.** The auto-open effect in `InAppTerminalDock.tsx` guards on `panes.length === 0` read
from the render closure. Under React StrictMode — which `renderer.tsx` applies unconditionally —
the mount effect runs twice, and both runs observe `panes.length === 0` because neither has
committed yet, so `addPane` is called twice. `addPane` compounds this by being an impure updater:
it calls `window.crypto.randomUUID()` and `setActivePaneId` *inside* the `setPanes` updater
function, so a double-invoked updater mints two different ids.

**Why documented rather than fixed.** StrictMode's double effect invocation is development-only, so
a packaged build almost certainly opens one pane. The impure-updater pattern is a genuine latent
fragility and it doubles the rate at which the session cap is approached, but the fix is a behaviour
change to pane-creation semantics that deserves its own review, and it is not the defect the user
reported. Suggested fix: hoist `setActivePaneId` and the id mint out of the updater, and guard the
auto-open with a ref rather than the closure's `panes.length`.

**Screenshot.** `/tmp/br-shots-term/02-terminal-open.png`.

### TERM-04 — switching chat tabs moves keyboard focus into the terminal (major, documented)

**Symptom.** With a terminal open in a background chat tab, clicking that chat tab moves keyboard
focus out of wherever it was and into the terminal's input. A user who switches tabs and starts
typing a chat message types it into a shell instead; pressing Enter executes it.

**Repro (verified live).**

1. Open two chat tabs, each with a terminal open.
2. Click into the chat composer of tab B and confirm focus: `document.activeElement` is the
   composer's `<textarea>`.
3. Click chat tab A.
4. `document.activeElement` is now `textarea.xterm-helper-textarea`, inside the terminal dock.

**Root cause.** `fitAndFocus` in `TerminalPaneView` calls `term.focus()` (twice — once in a
`requestAnimationFrame`, once behind a 30 ms timer) whenever `open && active` becomes true. Making a
dock visible because the *chat tab* changed satisfies that condition just as much as the user
opening the terminal does, so the dock claims focus it was never given.

**Why documented rather than fixed.** The correct behaviour for a terminal-focused user switching
tabs is a genuine product question, and the surrounding focus semantics are shared with components
another agent was editing concurrently. The narrow case above is unambiguous — focus should not
move from the composer into a shell — but the fix needs to distinguish "the dock became visible" from
"the user asked for this terminal", which the current props do not express. Suggested approach:
auto-focus on a pane's *creation* and on pane-tab activation, not on the dock's `open` transition.

**Files.** [`ui/desktop/src/components/InAppTerminalDock.tsx`](../../../ui/desktop/src/components/InAppTerminalDock.tsx)
(`fitAndFocus`, and the effect that calls it).

## What passed, with evidence

**PTY reaping on normal close is clean.** Seven shells across three docks; closing every pane
brought the count to zero, with all docks destroyed:

```text
before: 7
round1 clicked 9 ; round2 clicked 2 ; round3 clicked 0
docks: 0   panes: 0
live zsh children: 0
```

**The ceiling does not decay across normal open/close cycles.** Creating sessions until refused,
disposing them all, and repeating restored the full budget every time — the leak is specific to the
reload path, not to ordinary close:

```text
round 3: created 8 → live zsh after dispose: 0
round 4: created 8 → live zsh after dispose: 0
```

**Load is handled without jank.** Two concurrent `while true; do date; sleep 0.2; done` loops plus a
200 000-line `yes` burst left the renderer at ~3.7% CPU and the main process at ~1.5%, with a DOM
round-trip of 1 ms. No dropped output, no frozen panes.

**Rapid churn does not leak.** Ten cycles of "add two panes, close two panes" left the shell count
exactly where it started (3 → 3), including cycles that closed panes running infinite loops.

## Gates

The registry fix is gated by `ui/desktop/src/terminalSessionRegistry.test.ts` (12 tests). Reverting
the two behaviours to their pre-fix form — the hardcoded cap of 8, and a `releaseOwner` that frees
nothing — fails five of them, verbatim:

```text
 ❯ src/terminalSessionRegistry.test.ts (12 tests | 5 failed)
   × defaults to a rail far above what a person opens by hand
   × names both the limit and the knob in the refusal
   × releases every session a window owns when its document is replaced
   × lets a window reopen its full budget after a reload — the ceiling does not decay
   × keeps releasing the rest when one shell throws on the way down

AssertionError: expected 8 to be greater than or equal to 32
AssertionError: expected +0 to be 3 // Object.is equality
AssertionError: expected 8 to be +0 // Object.is equality
```

With the fix in place all 12 pass, and `npx tsc --noEmit` is clean (exit 0).

### Live re-verification after the fix

The unit gate covers the registry's rules; these runs cover the Electron wiring, against a
rebuilt main bundle:

```text
cap raised          12 sessions created, no refusal        (was: refused at 9)
reload reaping      12 shells → document replaced → 0      (was: 12 orphans)
budget restored     12 more created after the reload       (was: 0 available)
hash navigation     #/settings #/extensions #/pair #/ → 12 shells throughout
UI at scale         3 docks × 5 panes = 15 live shells, 0 dead panes
normal close        15 → 0 shells
cwd unchanged       /tmp/br-term-{a,b,c} still exact per tab
console errors      0
```

The hash-navigation row is the one that matters most for safety: the app is a hash router, so
if the `isSameDocument` filter were wrong, moving between Settings and a chat would kill the
user's shells. It does not.

**Screenshot.** `/tmp/br-shots-term/05-fixed-15-terminals.png`.

## Console errors

`window.__errs` — a hook capturing `error`, `unhandledrejection`, `console.error` and
`console.warn` — recorded **zero** entries across the whole functional sweep, including three
simultaneous docks, nine panes, the session-cap refusals, and ten churn cycles. The dead panes from
the cap refusal are rendered as terminal text, not thrown, which is why they produce no console
noise — and also why they are easy to miss.

## What could not be tested

- **Theme-family switching with terminals open.** Another agent was actively editing the theme
  definitions, `main.css`, and the generated palettes throughout this session. Switching families
  would have been testing their in-flight edits, not the terminal. The per-family `terminalGround`
  invariant is unverified here and should be swept separately.
- **The process-pipe fallback backend.** `node-pty` loaded successfully on every run, so the
  `spawn`-based fallback was only read, never exercised. It has no resize support by construction
  (`resize: () => {}`), which is worth a look if it is ever the live path.
- **Windows and Linux.** macOS only.
- **Interactive TUIs and copy/paste.** Driving `top` and clipboard gestures through the Playwright
  harness was not reliable enough to produce trustworthy evidence; left for manual testing.

## Related documentation

- [Terminal UI stability](../subsystem-reviews-2026/terminal-ui-stability.md) — the earlier subsystem review of the same dock
- [User-requested work register](user-requested-work-register.md) — item 30 tracks the cap-and-leak report this sweep answered
- [Campaign final report](campaign-final-report.md) — the campaign these findings were folded into
- [Documentation style guide](../../contributing/documentation-style.md) — the house style this file follows
