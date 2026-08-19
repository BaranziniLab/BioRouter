# Tab tear-off and merge

> **What this is.** The specification for generalising the chat-tab drag from a window-local gesture into a window-manager gesture: dragging a tab out of the app creates a new window carrying that session, and dragging it onto another window's tab strip merges it back in at the drop position.
> **Status:** Current — Phases 0–5 implemented (§8); Phase 4's stylesheet rule and §9's session-list menu item are the remainder, both waiting on files another campaign holds. **D1 is superseded** — see §3.1 before adding any restriction on moving a running tab.
> **Audience:** developers implementing this; maintainers reviewing the decisions in §3 and §7.

Today a chat tab can be dragged only *within* one window: reorder it inside its strip, or carry it to another pane of a split and back. Dragging it past the window's edge does nothing, and a session that lives in its own window can never be dragged back into a strip. Browsers have had both since 2010, which is precisely why their absence reads as breakage rather than as a missing feature.

This document establishes what the current implementation actually does (§1–2), answers the questions on which the feature is won or lost (§3), and specifies the state model, gesture, edge cases and phased plan (§4–8). §9 covers the same generalisation seen from the chat-history list: a second, "Open in new tab" item in each row's launch menu. Every decision that the request did not settle is marked **Decision** with its reasoning; none is buried in prose.

## 1. What exists today

All line references are to this worktree at the time of writing.

**The gesture.** One `useTabDragReorder` instance is created by the shell (`ChatGroupsShell.tsx:121`) and handed to every strip through `ChatTabDragProvider`, because a cross-pane drag has to tint a pane the source strip does not render. A strip mounted bare falls back to a private reorder-only instance (`ChatTabStrip.tsx:67-69`).

The hook (`useTabDragReorder.ts`) is pointer-events, not HTML5 drag-and-drop:

- `beginDrag` (`:212`) records origin and grab offset on `pointerdown`. No drag yet.
- Window-level `pointermove` (`:107`) promotes past a 5px threshold (`:4`, `:112-117`), then positions the ghost and hit-tests.
- The hit test is `document.elementFromPoint` + `closest()` (`:147-150`), never per-tab enter/leave, so a scrolling or resizing strip cannot make it stale.
- Routing (`:156-166`): a hit on the **source** strip is a reorder; a hit on **another** strip is a move into that group; anything else falls through to `dropTargetAtPoint` for the split zones.
- `finish` on `pointerup`/`pointercancel` (`:174`) commits and clears, then suppresses the synthetic click via a `setTimeout(…, 0)` guard (`:185-189`).

There is **no `setPointerCapture`** anywhere in this path; the hook relies on window-level listeners, as `ChatGroupSplitter.tsx:82-84` does. The only `setPointerCapture` in the codebase is the artifact panel's resize handle (`BaseChat.tsx:1269`).

**Geometry.** `dropZones.ts` is the one pure, unit-tested piece: `zoneFromRect` (`:39`) picks the single deepest edge incursion so corners are unambiguous and stable; `dropTargetAtPoint` (`:85`) hit-tests `[data-chat-group-id]`; `groupBodyRect` (`:104`) subtracts the strip height from the top so the `top` band is reachable.

**The data model** (`chatGroupsTypes.ts`):

| Type | Shape | Note |
|---|---|---|
| `ChatTab` | `{ tabId, sessionId, title, userSetName, pendingInitialMessage?, pendingInitialAttachments?, workflowId?, cwd? }` | `tabId ≠ sessionId` by design (`:6-10`) so a tab survives its session bind |
| `ChatGroup` | `{ groupId, tabs[], activeTabId }` | array order **is** strip order (`:37`) |
| `GroupLayout` | `leaf \| branch{dir,children,sizes}` | recursive tree |
| `ChatGroupsState` | `{ version, layout, groups, activeGroupId, seq }` | `activeGroupId` always names a live leaf |

The whole model is **window-local**: there is no window in it. `collapseEmptyGroup` (`chatGroupsReducer.ts:362`) drops a leaf that lost its last tab *unless it is the last leaf* — one empty group is a valid, intended state, and `useEmptyPairRedirect` sends `/pair` Home when the layout is empty.

**Persistence** is already per-window and already knows why. `chatGroupsStorage.ts:8-33`: the key is `biorouter.chatgroups.v1:<windowId>`, where the window id is minted into `sessionStorage` — per-`BrowserWindow` by construction, surviving reload. The header comment names `createChatWindow` as the reason and cites a real bug where a shared key was clobbered.

**Window creation** (`main.ts`). `createChat` (`:993`) is the one constructor. It seeds a window by URL: `resumeSessionId` and `resumeSessionTitle` become hash query params (`:1318-1329`), the renderer's `useChatGroupsUrlSync` consumes them exactly once (its `lastAppliedParamRef`/`selfWriteRef` fixed point) and dispatches `openTab`. `ChatWindowOptions` (`:983`) already carries `initialBounds`, `show`, `manageWindowState`, `resumeSessionTitle`. `branchWindowBounds` (`:1446`) offsets a new window from an anchor and clamps it into `screen.getDisplayMatching(...).workArea`. `openDivergedChatWindow` (`:1467`) is the proven consumer.

**A session can already be moved between windows by another route.** `SessionListView.tsx:642` — "Open in new window" calls `createChatWindow(undefined, session.working_dir, undefined, session.id, 'pair')`. It is a *copy*, not a move: the source window keeps its tab. The same seed path backs Diverge (`useDiverge.ts:71`) and the `biorouter://diverge` deeplink. So the seeding half of tear-off is not new work — it exists, ships, and is exercised daily.

**IPC.** `preload.ts` exposes `createChatWindow` (`:414`), `createDivergedChatWindow` (`:431`), `closeWindow` (`:551`), and a **generic disposer-returning receiver** `on(channel, cb)` (`:483-495`). There is no generic `send`/`invoke`, so the renderer→main direction needs named methods; the main→renderer direction needs none.

**Electron 39.8.10.** `screen.getCursorScreenPoint`, `screen.dipToScreenPoint`, `screen.screenToDipPoint`, `screen.getDisplayNearestPoint` are all present (`node_modules/electron/electron.d.ts:11581, 11601, 11609, 11623`).

## 2. Where the agent connection actually lives

This is the finding that shapes everything else, so it gets its own section.

**The daemon is shared across windows.** `main.ts:1011-1034` — BR-54 Slice A. One `biorouterd` serves every window by default (`BIOROUTER_SHARED_DAEMON=0` reverts). The daemon is a session-keyed singleton and its agents are cached per session id. So the **agent** is not per-window, and neither is the session.

**The output stream is per-window, and only one window can hold it.** In the renderer, `ChatStreamRegistry` is a module-level singleton (`chatStreamStore.tsx:1408`) — one per renderer process, therefore one per window. It keys `ChatStreamController`s by session id (`:1334-1341`). A turn's `/reply` SSE response is consumed by a `fetch` held open by that controller's `AbortController` (`:968`, `:1003`).

**Dropping that socket cancels the turn on the server.** `crates/biorouter-server/src/routes/reply.rs:257-260`:

```rust
if tx.send(format!("data: {}\n\n", json)).await.is_err() {
    tracing::info!("client hung up");
    cancel_token.cancel();
}
```

The `/agent/cancel` doc comment (`reply.rs:944-948`) confirms this was the *only* cancellation mechanism before that route existed.

**There is no way for a second client to attach to a turn in flight.** A `/reply` carrying the same `turn_id` is rejected with `409` and `duplicate: true` and returns **no stream** (`reply.rs:442-473`); a different turn on the same session is rejected by the per-session turn lock. Every `/agent/*` route is request/response (`agent.rs:1262-1273`). `/active_work` reports what is running but streams nothing.

**Therefore, a naive tear-off of a running tab has exactly two outcomes, both bad:**

1. The source window survives (it had other tabs). Its renderer keeps the SSE socket, so the turn runs on — invisibly, in a window that no longer shows that chat. The new window paints the transcript as of the tear and then freezes. Nothing is lost, but the user watches a live conversation stop dead.
2. The source window closes (it had only that tab). The renderer dies, `tx.send` fails, the cancellation token trips, and **the turn is killed mid-flight** — tool calls abandoned, tokens spent, no output.

This is the single most important finding in this document, and it is the reason for Decision D1.

## 3. The hard parts, decided

### D1 — ~~A tab with a turn in flight cannot be torn off or merged~~ **SUPERSEDED 2026-08-06**

> **Superseded by D9, which shipped.** D1 said its own expiry condition out loud —
> *"it is not a property of window management, it is a property of `/reply` having
> exactly one subscriber"* — and that property is gone. `crates/biorouter-server/src/turn_stream.rs`
> merged (`d7cb1fe5`) and all three halves of D9 were verified before Phase 3 was
> written; the evidence is in **§3.1** below. Nothing in the implementation
> refuses a running tab: there is no `isTabLocked`, no refusal toast, and no
> "stop here" row in the §5 table. **Do not re-add a lock** — the codebase no
> longer has the defect it was defending against, and a lock would now be a
> restriction with no cause.
>
> The original decision is kept below, struck through, because §2 is still the
> correct description of what a single-subscriber turn stream would have done and
> is the reason the backend work happened at all.

~~**Decision.** While `runningSessionIds` contains a tab's `sessionId`, the cross-window half of the gesture is refused: the drag still works inside the window (reorder, pane move, split), but leaving the window's content rect produces no tear-off preview and `pointerup` outside returns the tab to its slot. The strip already renders a coral pulse on a running tab (`main.css:1568`, `.br-tab__dot`), so the "this one is busy" affordance exists and needs no new chrome; the refusal is explained by a one-line toast on the first attempt per session.~~

~~**Reasoning.** §2 shows the alternatives are worse. Killing and restarting the turn discards work and doubles token spend. Letting it move and freeze is a silent failure that looks like data loss. Holding the source window alive invisibly until the turn ends is magic the user cannot see or predict.~~

~~**Rejected: making this permanent.** It is not a property of window management, it is a property of `/reply` having exactly one subscriber. The unlock is small and nameable — see D9 and Phase 5.~~

### 3.1 — The three measurements that retired D1

Verified against the tree at `d7cb1fe5`, before any Phase 3 code was written. "The
mechanism exists" is not "the renderer uses it", so each half was checked
separately and each is cited.

| D9 required | What is true now | Where |
|---|---|---|
| cancel-on-hangup fires when the **last** subscriber leaves, not the first | **Better than required: a departing subscriber never cancels at all.** The old `tx.send(…).is_err() ⇒ cancel_token.cancel()` is gone; the drain logs *"a turn stream observer disconnected; the turn continues"* and returns. The only audience-driven cancel is a separate orphan reaper that trips after `DEFAULT_ORPHAN_TIMEOUT` = **300s** with **zero** observers, and it makes the decision and the cancel under one lock so an `attach()` racing it either wins outright or reads an already-cancelled turn. | `routes/reply.rs:1019-1022`, `:1529-1532`; `turn_stream.rs:175`, `:556-588` |
| `ChatStreamController` gains "detach without abort" | `stopObserving()` refuses to abort a controller that is **driving** — *"'the tab closed' is not a reason to do that (BR-62b: the server keeps running either way)"* — and the registry never disposes controllers, so a tab's removal tears down no socket. Attach-before-detach is legal on the server side too: two simultaneous observers of one turn are pinned by test. | `hooks/chatStreamStore.tsx:1679-1692`, `:2342-2430`; `reply.rs::two_simultaneous_observers_receive_identical_frames` |
| the renderer attaches via the stream rather than owning the response body | `attachToTurn(turnId)` re-POSTs the turn's own id and the server answers **200 + SSE** with the replay backlog from `from_seq` then the live tail, where it used to answer 409-and-no-stream. `resumeActiveTurn` calls it, and `noteActiveTurn` fires it automatically off `/agent/resume`'s `active_turn` on **every session load** — so a torn-off window rejoins a running turn without anyone asking it to. | `chatStreamStore.tsx:1724-1806`, `:1820-1870` |

**What this means for the gesture.** A tab whose turn is in flight tears off and
merges like any other. The source window drops its socket; the turn does not
notice; the new window loads the session, sees `active_turn`, attaches from the
sequence it has already painted, and the transcript continues. The 300s orphan
window is the budget for that hop, and a window boot is ~4.6s.

**One consequence to keep in view.** The orphan reaper is the *only* thing that
now ends an unwatched turn, so a tear-off that fails to produce a window — the
user quits mid-gesture, the seed URL 404s — leaves a turn running for five
minutes. That is the intended trade (a live turn is worth more than five minutes
of tokens) and it is not new to this feature; it is the behaviour of every reload
since `turn_stream.rs` landed.

### D2 — The mechanism is pointer capture in the source window plus a main-process resolver

**Decision.** Three parts:

1. `beginDrag` calls `setPointerCapture(event.pointerId)` on the tab element, and `finish` releases it. Today the hook relies on implicit capture; making it explicit is what the Pointer Events spec guarantees will keep delivering `pointermove`/`pointerup` to the capturing element regardless of hit-test position.
2. On each move, the source renderer decides *locally* whether the point is inside its own content rect (`0 ≤ clientX ≤ innerWidth`, likewise Y). Inside ⇒ today's code path, untouched. Outside ⇒ the cross-window path.
3. Outside, the renderer sends `event.screenX/screenY` to the main process, which is the only party that knows where every window sits.

**Reasoning.** HTML5 `dragstart`/`drop` do not cross Electron window boundaries and their payload cannot carry live state. Pointer events plus capture keep the whole gesture in one renderer, which is also what makes the "drag back in" case free (D6). Main-process resolution is not a preference — `BrowserWindow.getBounds()` has no renderer equivalent.

**Verification owed.** That capture survives the cursor crossing over *another window of the same app* is asserted from the spec, not measured. Phase 0 exists to measure it with real OS input; see §8.

### D3 — Hit-testing is main-process geometry, never a renderer-side test in the target

**Decision.** Each chat window registers, on mount and on resize, the viewport-relative rectangle of its tab-strip band. Main converts that to screen coordinates with `win.getContentBounds()` and stores it. Given a screen point from the source window, main answers:

- inside another registered window's **strip band** ⇒ `{ kind: 'merge', windowId }`
- inside another registered window's rest ⇒ `{ kind: 'detach' }` — see the correction below
- anywhere else, including over our own artifact/launcher/app windows and over other applications ⇒ `{ kind: 'detach' }`

> **Corrected during Phase 1 (implemented, `821108c2`).** This decision originally
> gave a drop over another chat window's *body* its own `{ kind: 'none' }`, which
> contradicts §4 — whose `CrossWindowPhase` union has only `local | detach |
> merge` — and contradicts §5, whose release table gives a body drop the same
> outcome as a desktop drop. §4 and §5 agree with each other and with Chrome, so
> they win: a body drop is `detach`, and nothing consumes the distinction. The
> door §7 wanted to leave open for cross-window *pane* drops is reopened by
> adding the arm back when something needs it, not by carrying an unused state.

> **Two limits found while implementing this, neither of them fixable in the
> renderer.**
>
> **Z-order has no source in Electron.** "The topmost wins" is right, but Electron
> exposes no z-order query and `getAllWindows()` order is not documented as one.
> The registry therefore carries an explicit `stackOrder`, raised on `focus`.
> ~~**Phase 3 must wire `focus`/`show` → `raise()`**~~ — **done**, along with
> `restore`/`hide`/`minimize`/`closed`; see obligation 3 in §8. Without it the
> rule silently degrades to registration order, which is wrong the first time a
> user clicks between windows.
>
> **Only chat windows register, so only chat windows can occlude.** A launcher,
> artifact, or app window of ours sitting *above* a chat window's strip does not
> hide that strip from the resolver, so a drop there resolves as `merge` rather
> than `detach`. There is no Electron API to fix this with. The consequence is a
> merge the user did not aim at — recoverable, not destructive. It means §6's
> "dropped on our own artifact/launcher window ⇒ treated as desktop" row holds
> only while that window is not overlapping a registered strip.

The *insertion index* inside the target strip is computed by the **target renderer**, which is the only party that knows its own tab rects. Main forwards the screen point; the target converts it to client coordinates and paints its own `[data-dropbefore]` caret.

**Reasoning.** The seam is natural: main owns "which window", the renderer owns "where in my strip". It also means the source renderer never names another window — it sends a point, main resolves it — so a compromised renderer cannot address a sibling window by id.

**A consequence worth stating plainly.** While the button is held, the OS delivers the mouse to the source window. The target window receives **no pointer events at all**. Its caret is therefore driven entirely by IPC from main, and the target window must **not** be raised or focused during preview — raising it would steal the drag.

### D4 — Coordinate space is DIP, normalised in main

**Decision.** The renderer sends raw `screenX/screenY`. Main normalises with `screen.screenToDipPoint()` before comparing against `getBounds()`/`getContentBounds()`, and picks the destination display with `screen.getDisplayNearestPoint()`.

**Reasoning.** On macOS the two spaces agree and the conversion is a no-op. On Windows with per-monitor DPI they diverge, and a tab dropped on a second monitor would otherwise land on the wrong display or off-screen. The conversion costs nothing and the failure it prevents is invisible on the development platform — exactly the class of bug this repo keeps finding late.

### D5 — Tearing out a window's only tab is a no-op

**Decision.** If the source window would be left with zero tabs across all its groups, the tear-off does not happen: the tab returns to its slot, no window is created, none is closed.

**Reasoning.** Destroying a window and building an identical one to hold the same session is expensive here in a way it is not in a browser — a new renderer boots, extensions reload per session (`chatStreamStore.tsx:570-578` measures this at ~4.6s), and the user watches a window disappear and reappear. The request "put this chat in its own window" is already satisfied: it *is* in its own window, and moving that window is what the titlebar is for. Chrome resolves the same case the same way.

**Consequence.** A tear-off never closes a window. Only a **merge** can, and it must: see D6.

### D6 — Merge that empties the source window closes it; a drag back onto its own window is the existing local path

**Decision (a).** When a merge removes the source window's last tab, the source window closes after the target confirms the insert. Order matters: the source dispatches its removal only on the target's acknowledgement, so a failed insert cannot lose the tab.

**Decision (b).** Dragging back onto the window the tab came from needs no code. The cross-window state is derived per `pointermove` and never latched; the moment the pointer re-enters the source window's content rect, the existing `elementFromPoint` path resolves it as an ordinary reorder or pane move. Re-entering after having been outside cancels any detach preview.

### D7 — The visual for "this will become a new window"

The existing choreography is: ghost lifts under the cursor with `--ease-spring` (`main.css:1544`), source tab dims to 35% in flow (`:1520`), landing half tints with a dashed accent outline and **no transition**, because the tint must track the cursor exactly (`:1614-1631`), and the insertion point is a static 2px accent hairline (`:1557`).

**Decision.** It extends, with no new vocabulary and one new state:

- **Outside the window, the ghost cannot follow.** Electron cannot paint outside a window's frame. The ghost is clamped to the source window's content rect and gains `data-detach='true'`.
- **The detach state reads as a window, not a tab**: rotation returns from `2deg` to `0`, and it takes the same dashed accent outline the drop zones already use (`outline: 2px dashed color-mix(in srgb, var(--accent-bar) 55%, transparent)`). A flat, outlined, shadowed rectangle detached from the strip is a window; a tilted one riding the strip is a tab.
- **The source tab keeps its 35% dim.** Removal is already communicated; changing it would say something new that is not true until the drop.
- **The target window paints the ordinary `[data-dropbefore]` caret.** Merging into a strip is the same act as inserting into a strip, so it is the same hairline.
- **Transition, not animation.** The ghost-lift is mount-only for a reason (`main.css:1541-1543`); detach is a state change on a mounted element, so it is `transition: transform var(--motion-fast) var(--ease-spring), outline-color var(--motion-fast) var(--ease-out)`. **Never on `left`/`top`** — those are written per `pointermove` in JS and must not lag the cursor. That prohibition is already stated at `main.css:1533` and applies unchanged.
- **Reduced motion.** `main.css:1592-1597` already zeroes the ghost's animation; the transition must be zeroed in the same block, leaving the outline change instant. The state is carried by colour and geometry, not by movement, so nothing is lost.

**Rejected: a real cross-desktop ghost window.** A transparent, always-on-top, borderless `BrowserWindow` tracking the cursor is what Chrome does natively and is the only way to draw outside the app. Rejected for v1 — it creates and destroys a real window per drag, interacts badly with macOS `vibrancy` and traffic-light positioning, risks stealing focus mid-gesture, and buys legibility the clamped detach state already provides. Reconsider as polish once the gesture ships (Phase 4b).

### D8 — Escape cancels

**Decision.** While a drag is promoted, `Escape` runs `finish` with a cancel flag: no reorder, no move, no tear-off, no merge; ghost and previews clear, including the remote one. Today `Escape` does nothing and only `pointercancel` aborts.

**Reasoning.** A gesture that can create and destroy windows needs a way out that does not require finding a safe place to release.

### D9 — What unblocked D1 — **DONE, shipped in `d7cb1fe5`**

> **Landed.** This was written as a named future; it is now the code. See §3.1 for
> the three-part verification and the file/line citations. The shape below is
> what was asked for; what shipped exceeds it (a replayable, sequence-numbered
> frame log with an orphan reaper, rather than a bare broadcast fan-out).

A tab with a live turn becomes movable the moment a turn's event stream has more than one possible subscriber. The smallest shape that does it:

- a `GET /sessions/{session_id}/live` SSE route that attaches to the running turn's broadcast and replays nothing (the transcript is already fetchable);
- `/reply`'s `tx` becomes a `broadcast::Sender` fan-out, and the cancel-on-hangup at `reply.rs:257` fires only when the **last** subscriber leaves, not the first;
- `ChatStreamController` gains "detach without abort", so the source renderer can hand the turn over.

That is a backend feature with its own correctness surface (who owns cancel, what a late subscriber sees, how the turn lock interacts). It was named here so the block in D1 would be understood as provisional. It was then built — on its own branch, with its own tests — and merged before Phase 3 began, which is why Phase 3 shipped with no lock in it.

## 4. The state model

Nothing in `ChatGroupsState` changes. The feature adds a second, ephemeral model that lives only for the duration of a gesture and is never persisted.

**In the source renderer** — extends the existing `TabDragReorder` return:

```ts
type CrossWindowPhase =
  | { kind: 'local' }                              // pointer inside our content rect
  | { kind: 'detach' }                             // over desktop / a non-chat window
  | { kind: 'merge'; targetWindowId: number };     // over another chat window's strip
```

Derived on every `pointermove`, never latched, cleared by `finish`. `{ kind: 'local' }` is exactly today's behaviour.

**In the main process** — one registry, keyed by `BrowserWindow.id`:

```ts
interface ChatWindowStrip {
  windowId: number;
  /** Viewport-relative, as reported by the renderer. */
  band: { x: number; y: number; width: number; height: number };
}
```

Written by an IPC message from each chat window on mount, on resize, and when the layout tree changes (a split gives a window more than one strip; the registry holds a list). Entries are removed on `closed`.

**The payload that crosses a window boundary** is the serialisable subset of a `ChatTab`, and deliberately excludes the transient cargo that `chatGroupsStorage.stripTransient` already refuses to persist:

```ts
interface TabMovePayload {
  sessionId: string;
  title: string;
  userSetName: boolean;
  cwd?: string;
  workflowId?: string;
}
```

`tabId` does not travel: it is a window-local identity (`chatGroupsTypes.ts:6-10`) and the receiving window mints its own. `pendingInitialMessage`/`pendingInitialAttachments` do not travel — a queued message that re-sends in a second window is the same data bug the storage layer already guards against.

## 5. The gesture, stage by stage

| Stage | Condition | Source window | Target window | Main |
|---|---|---|---|---|
| Press | `pointerdown` on a tab | record origin, grab offset; `setPointerCapture` | — | — |
| Promote | moved > 5px | ghost lifts (`--ease-spring`), source tab → 35% | — | — |
| Drag, inside | point inside content rect | today's behaviour: reorder caret, pane tint, split zones | — | — |
| Drag, outside | point outside content rect (a running turn is **not** a barrier — D1 superseded, §3.1) | ghost clamps to edge, `data-detach='true'` | — | resolve screen point |
| Drag, over desktop | `{ kind: 'detach' }` | detach ghost held | — | — |
| Drag, over a strip | `{ kind: 'merge' }` | detach ghost held | `[data-dropbefore]` caret at the resolved index; **not raised, not focused** | forwards point to target |
| Release, local | inside | commit reorder/move as today | — | — |
| Release, detach | outside, no target | remove tab; if it was the last tab, **no-op** (D5) | — | create window at the drop point, seeded by URL |
| Release, merge | over a strip | remove tab **on the target's ack**; close the window if now empty (D6) | insert at the caret index, activate it, focus the window | route the commit |
| Cancel | `Escape`, `pointercancel` | everything clears | caret clears | preview cleared |

The new window's bounds: size copied from the source window, origin `(screenX − grabOffsetX, screenY − grabOffsetY)` so the tab lands under the cursor, clamped into `screen.getDisplayNearestPoint(dropPoint).workArea` by the same rule `branchWindowBounds` (`main.ts:1446-1465`) already implements. It is seeded through the existing, proven path — `createChat(..., resumeSessionId, 'pair', ..., { initialBounds, show: false, resumeSessionTitle })` — then shown, focused and raised, exactly as `openDivergedChatWindow` does.

## 6. Edge cases, each with a decided answer

| Case | Answer | Where it is decided |
|---|---|---|
| Tab has a turn in flight | **moves like any other tab.** The source drops its socket, the turn does not notice, the destination window rejoins it from `/agent/resume` | D1 superseded, §3.1 |
| Source window would be emptied by a tear-off | no-op; tab returns | D5 |
| Source window is emptied by a merge | source window closes, after the target acks | D6a |
| Dropped back on its own window | ordinary local path, no special case | D6b |
| Dropped on our own artifact / launcher / app window | treated as desktop ⇒ new window | D3 |
| Dropped on another application's window | treated as desktop ⇒ new window (browser behaviour) | D3 |
| Dropped on a second display | new window on that display via `getDisplayNearestPoint` | D4 |
| Target window is minimised or hidden | not a merge target; its band is de-registered on `hide`/`minimize` | §4 registry |
| Target window closes mid-drag | its registry entry goes on `closed`; the next move resolves to `detach` | §4 registry |
| Source window closes mid-drag | the whole gesture dies with the renderer; main drops the pending drag on `closed` | §4 registry |
| Tab is dragged into a split pane of *another* window | v1 merges into that window's **strip only**; pane zones are not offered cross-window | §7 scope |
| Same session already open in the target window | the target's reducer dedupes by `sessionId` (existing `openTab` behaviour); the caret still shows, the insert activates the existing tab | existing reducer |
| Two drags at once (two pointers) | main keys the pending drag by source window id and refuses a second; the hook already tracks a single `gestureRef` | §4 registry |
| Persistence across the move | free: each window's `chatGroupsStorage` key is minted per `BrowserWindow` from `sessionStorage` | `chatGroupsStorage.ts:8-33` |
| Terminal attached to the torn tab | **does not travel.** Terminals are keyed by `tabId` in the source window's `TerminalDockContext`; `tabId` is window-local. The tab's removal fires the existing `retain` sweep (`ChatGroupsShell.tsx:253-262`) and its pty is disposed. Stated so it is not discovered as a bug | D-terminal, below |

**Decision (terminals).** A per-tab terminal is not carried across the tear-off; the new window's tab opens without one. Carrying it would mean handing a live pty between renderers, which the current dock cannot do (`main.ts:3470` creates ptys per `webContents`). The cost is a shell the user reopens; the alternative is a cross-window pty transfer, which is a larger feature than the one being asked for. Say it in the release note.

## 7. Scope boundaries

In scope: tear-off to a new window; merge into another window's strip at a position; the cancel and edge cases above.

Out of scope, deliberately:

- **Cross-window pane drops.** Dragging onto another window's *body* to split it. The split zones are a within-window language and offering them across windows multiplies the preview surface for a gesture nobody asked for. `{ kind: 'none' }` over a window body keeps the door open.
- **Cross-desktop ghost window** (D7, rejected for v1).
- **Live-turn portability** (D1/D9).
- **Merging whole windows** (drag a titlebar onto another window).

## 8. Phases, and the files each touches

Phase 0 is a gate, not a task: it decides whether the mechanism in D2 is sound before anything is built on it.

### Phase 0 — measure the capture, with real OS input

Two BioRouter windows, a `pointerdown` on a tab in window A, the cursor dragged across window B and onto empty desktop. Record: does A keep receiving `pointermove`? Does it receive `pointerup` released over B? Over the desktop? Does B receive *anything*? Are `screenX/screenY` consistent with `getBounds()` across displays?

**This must use real OS input, not CDP `Input.dispatchMouseEvent`** — injected events are delivered straight to a renderer and therefore cannot answer a question about OS-level mouse capture. The repo has precedent for measuring exactly this way (the `WebkitAppRegion` finding recorded in `ChatTabStrip.tsx:255-259`). jsdom is useless here on four counts: no `elementFromPoint`, zeroed `getBoundingClientRect`, no pointer capture, no windows.

Files: none. Output: a result recorded in this section.

#### Result — measured 2026-08-06. **Capture holds. The mechanism in D2 is sound.**

Measured against the shipped **1.88.6** app (identical version to the repo), two windows on one 2560×1080 display, with **real OS-level input**: a compiled Swift driver posting `CGEvent`s at `.cghidEventTap`, which enter below the window server and are routed exactly as a physical mouse's are. CDP was used only to instrument the pages and read the results back — never to generate the input, which is what would have made the measurement circular.

| Question | Answer |
|---|---|
| Does the source keep receiving moves once the cursor leaves its frame? | **Yes.** 21–26 of ~55 moves per run were delivered with `inside: false`, tracked to the far edge of the display (x=2300). |
| Does the source receive the release outside its frame? | **Yes**, every run, over empty desktop and over another window alike. |
| `lostpointercapture` during the gesture? | **Never**, across every run. |
| Does the target window receive anything? | **No.** With the target fully un-occluded and the cursor dragged directly across its strip for 46 outside-moves and released on it, the target logged **zero** events. |
| Are `screenX/screenY` consistent with window origins? | **Yes**, exactly; client coordinates round-tripped through `screenX − window.screenX` to the pixel. |

Five repeat runs, alternating the exit edge (right and left): source `down=1, up=1, upOutside=1, lost=0` in 5/5.

**One anomaly, recorded rather than smoothed over.** When the drag exited through a 40px band where the target window extended *past* the source's edge, the target logged exactly **one** buttoned move (3 of 3 right-exits; 0 of 2 left-exits, where no such band existed). It did not recur in the un-occluded test, so it reads as a boundary artifact at the moment the cursor crosses the source's frame. It changes nothing: one sparse event cannot drive a hit test, so **D3 stands** — and its stronger claim, that the target receives *nothing*, is confirmed for the case that matters.

**Two things learned that the spec did not anticipate:**

1. **D5 is already enforced by the app, at a level below the renderer.** With only ONE tab in a window, pressing that tab does not start a drag at all — the press is claimed by the `-webkit-app-region: drag` strip and **moves the window**. The `no-drag` on `.br-tab` only takes effect once there is something to reorder. So "tearing out a window's only tab is a no-op" is not a rule Phase 3 must add; it is a rule Phase 3 must avoid *breaking*. Verified both ways: with one tab the window moved by exactly the drag vector; with two tabs the window did not move and the tabs reordered.
2. **The gesture is fully drivable from an agent shell after all**, which the previous revision of this section said it was not. The harness plus the Swift driver are reproducible; `ui/desktop/scripts/tab-tearoff-phase0.js` is the instrument, and the driver is ~40 lines of `CGEventPost`. Phase 3 and 4 can therefore be verified the same way instead of by hand.

### Phase 1 — main-process geometry (**uncontested, independently landable**)

New `ui/desktop/src/windowDrag.ts`: the strip-band registry, `resolveDropTarget(screenPoint, registry, sourceWindowId)` returning the `CrossWindowPhase` discriminant, and `tornOffWindowBounds(screenPoint, grabOffset, sourceBounds, workArea)` generalising `branchWindowBounds`. Pure functions over plain rectangles, with the Electron calls confined to a thin shim so the logic is testable.

New `ui/desktop/src/windowDrag.test.ts`: overlapping windows, z-order, a point in a band versus a body, a point on a second display, a de-registered window, clamping at every work-area edge.

This is the largest single piece of the feature and it touches nothing anyone else is editing.

### Phase 2 — renderer gesture logic (**uncontested**)

New `ui/desktop/src/components/chatGroups/tabTearOff.ts`: `isOutsideViewport(point, viewport)`, `payloadFromTab(tab)`, `insertionIndexFromStrip(clientX, tabRects)` — the target-side index computation, pure and testable, mirroring the discipline of `dropZones.ts`.

Extend `ui/desktop/src/components/chatGroups/useTabDragReorder.ts` — **not on the contested list**, so this can land now. Additions follow the pattern the hook already uses for `onDropToGroup` (`:19`): optional callbacks whose absence preserves today's behaviour exactly.

- `setPointerCapture`/`releasePointerCapture` (D2)
- optional `onCrossWindow?(phase, screenPoint)` and `onCrossWindowCommit?(phase, screenPoint)`
- ~~optional `isTabLocked?(tabId): boolean` — the D1 running guard~~ **removed in Phase 3.** It was landed in Phase 2 as an injection point precisely so the policy could change at the call site without touching the hook; D1 was then superseded outright (§3.1), so the injection point was deleted rather than left dangling. A dead hook argument named for a lock is an invitation to re-implement the lock.
- an `Escape` listener wired to the existing `finish` with a cancel flag (D8)

New tests alongside the existing `dropZones.test.ts` for the pure parts; the gesture itself is browser-verified per Phase 0.

New `ui/desktop/src/components/chatGroups/ChatDropOverlay.tsx` variant — **also uncontested**: the `data-detach` prop on `ChatTabGhost`. The CSS it needs is contested and lands in Phase 4.

### Phase 3 — wiring (**contested — must wait**)

| File | Change | Contested |
|---|---|---|
| `ui/desktop/src/main.ts` | register `tab-drag:register-bands`, `tab-drag:move`, `tab-drag:commit`, `create-torn-off-chat-window`; window `closed`/`hide` cleanup | yes |
| `ui/desktop/src/preload.ts` | three send methods; the receive side needs nothing (the generic `on` at `:483` already covers it) | yes |
| `ui/desktop/src/components/chatGroups/ChatGroupsShell.tsx` | own the cross-window callbacks; report strip bands; subscribe to the remote-preview and merge channels | yes |
| `ui/desktop/src/components/chatGroups/ChatTabStrip.tsx` | **one prop.** `remoteDropBeforeTabId` — the caret in a MERGE target is driven by IPC, not by this window's own drag state, so `dragOverTabId` cannot supply it (the target window receives no pointer events at all; §3, Phase 0). The rendering is the same `[data-dropbefore]` hairline | yes |

The preload additions are append-only at the end of one object literal, so the merge surface is a few lines even though the file is contested.

**Obligations Phase 3 inherits.** Each was found while building Phases 1 and 2 and is deliberately *not* implemented there, because each needs a party those phases do not have. A Phase 3 that skips them compiles and appears to work.

1. ~~**Re-check the lock at commit, not only at move.**~~ **Void.** There is no lock to re-check — D1 was superseded before Phase 3 was written (§3.1). This obligation existed only to close a race between a turn *starting* mid-drag and the commit; with a running tab freely movable, the race has no consequence.
2. **Enforce D5 at commit.** Tearing out a window's only tab is a no-op, and nothing in Phases 1 or 2 enforces it — the hook does not know how many tabs the window has, and the resolver does not either. **Discharged in the shell:** `commitCrossWindow` counts the window's tabs across every group and returns `noop` without calling main when the count is 1.
   ⚠ **It is also enforced one level below the renderer, and that path is the one that actually fires.** Phase 0 measured it: with a single tab, `-webkit-app-region: drag` on the strip claims the press and the OS **moves the window** — no `pointerdown` reaches React, so no drag begins. The renderer check is therefore a backstop for the case where that does not hold (a future non-drag strip, a platform without app regions), not the primary rule. **Do not add `no-drag` to the strip wrapper to "fix" a single tab not dragging.** That is the feature.
3. **Wire `focus`/`show` → `raise()`** on the band registry, or z-order degrades to registration order (see D3 above). **Discharged in `main.ts`:** `focus`, `show` and `restore` raise; `hide` and `minimize` set `hidden`; `closed` removes. `move`/`resize` need no listener because every `tab-drag:move` refreshes each entry's `contentBounds` from the live `BrowserWindow` before resolving — a window dragged to a new position mid-gesture is therefore never stale, which no amount of renderer-side re-registration could have achieved (a window MOVE fires no renderer resize).
4. **Own the ghost clamp.** D7 says the ghost is clamped to the source window's content rect. That needs the ghost's rendered size, which the hook does not have — it computes `clientX − grabOffset` and nothing more. **Discharged as measurement in the shell, CSS reported to the token steward:** `clampGhostToViewport` (in `tabTearOffBridge.ts`) is a pure function over the ghost's measured box and the viewport, and `ChatTabGhost` reports its own rendered size through a ref. Its output is the `left`/`top` the ghost is already positioned with, so it needs no new stylesheet rule; the one rule main.css still owes is `.br-tab-ghost[data-detach='true']` (Phase 4).
5. **Decide whether Escape is swallowed.** **Decided: NO.** The cancel listener does not `preventDefault`, does not `stopPropagation`, and does not `stopImmediatePropagation`. Three reasons, in order of weight:
   - **A drag is not a modal.** Escape during a tab drag has exactly one plausible second meaning — close the dialog or menu that is also open — and cancelling the drag does not make that meaning wrong. Both should happen.
   - **Swallowing needs an ordering assumption the hook cannot honour.** `stopImmediatePropagation` only wins over listeners registered *after* it on the same node. The hook registers on `window` at mount; a dialog registered later would still fire, and one registered earlier would be silently killed. The behaviour would then depend on component mount order, which changes with every layout.
   - **The failure modes are asymmetric.** A swallowed Escape leaves a modal stuck open with no visible cause. A doubled Escape closes a menu the user was probably done with. The first is a bug report; the second is not.

**A testing note that will cost someone an hour otherwise.** This repo's jsdom has no `document.elementFromPoint` *at all* — not "returns null", missing. The drag hook's move handler therefore throws on the first *promoted* move, which is why no pre-existing test in `components/chatGroups/` promotes a drag. Stub it to `null` in `beforeAll`. And note that pointer capture does not exist in jsdom either, so the capture *target* — which must be the element that received the `pointerdown`, never the tab wrapper, because capture retargets the compatibility mouse events and would otherwise fire `click` on the wrapper and break selecting a tab — can only be asserted, never exercised.

### Phase 4 — visuals (**partly contested**)

`ui/desktop/src/styles/main.css` — contested — gains one block beside the existing drag rules (`:1504-1600`): `.br-tab-ghost[data-detach='true']`, its transition, and the reduced-motion entry. Roughly eight lines, no new tokens.

Phase 4b, optional and separate: reconsider the ghost window rejected in D7.

### Phase 5 — ~~optional unlock~~ **done, and it landed first**

Implement D9 and lift the D1 block. Backend work in `crates/biorouter-server/src/routes/reply.rs` plus `chatStreamStore.tsx`. Independent of everything above and should be specified on its own before it is started.

> **It was, and it overtook the plan.** `turn_stream.rs` merged at `d7cb1fe5` as its
> own branch with its own spec and tests, ahead of Phase 3 rather than after it.
> The order in the recommendation below is therefore historical: what actually
> happened was 0 → 1, 2 → **5** → 3, and Phase 3 was smaller for it, because the
> lock it would otherwise have had to thread through four files was deleted
> instead of wired.

**Recommended order:** Phase 0 → Phase 1 and 2 in parallel (both uncontested) → wait for the contested branch → Phases 3 and 4 together, which is when the feature first becomes visible.

## 9. "Open in new tab" in the session list

The same generalisation seen from the list rather than from the tab strip. Each row in chat history carries a launch dropdown offering exactly one item, "Open in new window"; it should offer two, with "Open in new tab" **first**, because a tab in the current window is the lighter and far more common act.

### The behaviour already exists — the menu item is a name for it

This is the finding that shrinks the work to almost nothing. Clicking the row already opens the session as a tab in the current window, by exactly the path a menu item would use:

`SessionListView.tsx:682-684` `handleCardClick` → `onSelectSession(session.id)` → `SessionsView.tsx:38-46` `handleSelectSession` → `setView('pair', { resumeSessionId })` → `navigationUtils.ts:74-80`, which puts the id in the **query string** (with a comment explaining that a state-only navigation silently drops it) → `useChatGroupsUrlSync`'s IN effect → `openTab`, which dedupes by `sessionId` anywhere in the window and merely activates an already-open tab (`chatGroupsReducer.ts:277-288`).

So nothing about tab-opening needs building. What is missing is **discoverability**: the gesture is unnamed, and it sits beside a menu that names its heavier sibling. A one-item dropdown is a button wearing a costume; the second item is what makes it a menu.

**Decision.** Add one `DropdownMenuItem` above the existing one, calling the **existing** `onSelectSession` prop. No new prop, no change to `SessionsView.tsx`, no change to the callback's signature — about eight lines in one file.

**Icon.** `MessageSquare`, which is the glyph the tab strip already uses for a tab (`ChatTabStrip.tsx:297`) and for the drag ghost. The window item keeps `NewWindow`. One glyph, one meaning — the same rule the strip states for its running dot.

### The refinement worth taking, and why it is separable

The row click loses the session's name. `useChatGroupsUrlSync` reads a `title` hint from route state (`:103-124`) precisely so a tab opens already named instead of flashing the "New chat" placeholder until `BaseChat` finishes loading; `AppSidebar.handleOpenChat` passes both the summary title and `user_set_name`, while `SessionsView.handleSelectSession` does not. The URL sync accepts both fields and the loaded session still corrects them authoritatively.

**Decision.** Ship the plain menu item first, since it needs no signature change. Widening `onSelectSession` to `(sessionId, hint?: { title, userSetName })` is a separate, optional follow-up that removes the name flash for this surface — worth doing, not worth blocking on.

### Contested, and the row-budget tension

`SessionListView.tsx` and `SessionsView.tsx` are both on `.contested-files.txt`, so **none of this can land yet** and none of it is independently landable — unlike the tear-off, there is no uncontested module to extract, because the logic is a single call to a prop that already exists. The change is small enough that its merge surface is a few lines inside one `DropdownMenuContent`.

**Reported, not resolved:** the row already shows four visible actions (launch, edit, export, delete) at `h-7 w-7`, with the destructive delete inline. The Astryx row specification allows at most three visible actions at 32px and puts destructive actions behind a `…` overflow only. The row is therefore over budget *before* this change, and this change does not add to the visible count — it adds a second item to an existing menu. Restructuring the row belongs to the later views phase; it is recorded here so that phase inherits the observation rather than rediscovering it.

## 10. Verification

| Gate | Command | Covers |
|---|---|---|
| Types | `npx tsc --noEmit` | all phases |
| Unit | `npx vitest run src/windowDrag.test.ts src/components/chatGroups/` | Phases 1, 2 |
| Lint | `npx eslint "src/windowDrag*.ts" "src/components/chatGroups/**/*.{ts,tsx}" --max-warnings 0` | all |
| Real input | manual, two windows, per Phase 0 | the gesture, the capture, the previews |
| Multi-display | manual, second monitor at a different scale factor | D4 |

jsdom cannot verify the gesture, the previews, the clamping, or the capture. Do not let a passing unit suite read as a working feature; the hook's own header already says as much (`useTabDragReorder.ts:72-76`).

## Related documentation

- [Astryx adoption](README.md) — the folder index; this specification is the window-management section of that work.
- [Astryx UI adoption — comprehensive interface revision](astryx-ui-adoption-design.md) — the design of record whose motion and token vocabulary this specification extends rather than replaces.
- [Biorouter Design System](../../../design.md) — the yield ladder (`D-32`) and the drop-zone language that §5 and D7 build on.
- [Launching the dev GUI](../../desktop-ui/launching-the-dev-gui.md) — required reading before attempting the Phase 0 measurement from an agent shell.
