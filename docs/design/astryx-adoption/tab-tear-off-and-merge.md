# Tab tear-off and merge

> **What this is.** The specification for generalising the chat-tab drag from a window-local gesture into a window-manager gesture: dragging a tab out of the app creates a new window carrying that session, and dragging it onto another window's tab strip merges it back in at the drop position.
> **Status:** Proposed — investigated and specified, not implemented. Partly blocked on the contested files named in §8 and §9 landing first.
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

### D1 — A tab with a turn in flight cannot be torn off or merged

**Decision.** While `runningSessionIds` contains a tab's `sessionId`, the cross-window half of the gesture is refused: the drag still works inside the window (reorder, pane move, split), but leaving the window's content rect produces no tear-off preview and `pointerup` outside returns the tab to its slot. The strip already renders a coral pulse on a running tab (`main.css:1568`, `.br-tab__dot`), so the "this one is busy" affordance exists and needs no new chrome; the refusal is explained by a one-line toast on the first attempt per session.

**Reasoning.** §2 shows the alternatives are worse. Killing and restarting the turn discards work and doubles token spend. Letting it move and freeze is a silent failure that looks like data loss. Holding the source window alive invisibly until the turn ends is magic the user cannot see or predict.

**Rejected: making this permanent.** It is not a property of window management, it is a property of `/reply` having exactly one subscriber. The unlock is small and nameable — see D9 and Phase 5.

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
> **Phase 3 must wire `focus`/`show` → `raise()`**, or the rule silently degrades
> to registration order — which is wrong the first time a user clicks between
> windows.
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

### D9 — What would unblock D1, stated precisely so it is not rediscovered

A tab with a live turn becomes movable the moment a turn's event stream has more than one possible subscriber. The smallest shape that does it:

- a `GET /sessions/{session_id}/live` SSE route that attaches to the running turn's broadcast and replays nothing (the transcript is already fetchable);
- `/reply`'s `tx` becomes a `broadcast::Sender` fan-out, and the cancel-on-hangup at `reply.rs:257` fires only when the **last** subscriber leaves, not the first;
- `ChatStreamController` gains "detach without abort", so the source renderer can hand the turn over.

That is a backend feature with its own correctness surface (who owns cancel, what a late subscriber sees, how the turn lock interacts). It is named here so the block in D1 is understood as provisional, and it is explicitly **not** in this plan's scope.

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
| Drag, outside | point outside content rect, running ⇒ **stop here** (D1) | ghost clamps to edge, `data-detach='true'` | — | resolve screen point |
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
| Tab has a turn in flight | cross-window half refused; local drag unaffected | D1 |
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

### Phase 1 — main-process geometry (**uncontested, independently landable**)

New `ui/desktop/src/windowDrag.ts`: the strip-band registry, `resolveDropTarget(screenPoint, registry, sourceWindowId)` returning the `CrossWindowPhase` discriminant, and `tornOffWindowBounds(screenPoint, grabOffset, sourceBounds, workArea)` generalising `branchWindowBounds`. Pure functions over plain rectangles, with the Electron calls confined to a thin shim so the logic is testable.

New `ui/desktop/src/windowDrag.test.ts`: overlapping windows, z-order, a point in a band versus a body, a point on a second display, a de-registered window, clamping at every work-area edge.

This is the largest single piece of the feature and it touches nothing anyone else is editing.

### Phase 2 — renderer gesture logic (**uncontested**)

New `ui/desktop/src/components/chatGroups/tabTearOff.ts`: `isOutsideViewport(point, viewport)`, `payloadFromTab(tab)`, `insertionIndexFromStrip(clientX, tabRects)` — the target-side index computation, pure and testable, mirroring the discipline of `dropZones.ts`.

Extend `ui/desktop/src/components/chatGroups/useTabDragReorder.ts` — **not on the contested list**, so this can land now. Additions follow the pattern the hook already uses for `onDropToGroup` (`:19`): optional callbacks whose absence preserves today's behaviour exactly.

- `setPointerCapture`/`releasePointerCapture` (D2)
- optional `onCrossWindow?(phase, screenPoint)` and `onCrossWindowCommit?(phase, screenPoint)`
- optional `isTabLocked?(tabId): boolean` — the D1 running guard, injected rather than assumed, so the hook stays free of chat-stream knowledge
- an `Escape` listener wired to the existing `finish` with a cancel flag (D8)

New tests alongside the existing `dropZones.test.ts` for the pure parts; the gesture itself is browser-verified per Phase 0.

New `ui/desktop/src/components/chatGroups/ChatDropOverlay.tsx` variant — **also uncontested**: the `data-detach` prop on `ChatTabGhost`. The CSS it needs is contested and lands in Phase 4.

### Phase 3 — wiring (**contested — must wait**)

| File | Change | Contested |
|---|---|---|
| `ui/desktop/src/main.ts` | register `tab-drag:register-bands`, `tab-drag:move`, `tab-drag:commit`, `create-torn-off-chat-window`; window `closed`/`hide` cleanup | yes |
| `ui/desktop/src/preload.ts` | three send methods; the receive side needs nothing (the generic `on` at `:483` already covers it) | yes |
| `ui/desktop/src/components/chatGroups/ChatGroupsShell.tsx` | pass `isTabLocked` from `groups.runningSessionIds`; own the cross-window callbacks; subscribe to the remote-preview channel | yes |
| `ui/desktop/src/components/chatGroups/ChatTabStrip.tsx` | **likely zero edits.** The running set already reaches the shell, and the caret already renders from `dragOverTabId`. Confirm before scheduling any | yes |

The preload additions are append-only at the end of one object literal, so the merge surface is a few lines even though the file is contested.

**Obligations Phase 3 inherits.** Each was found while building Phases 1 and 2 and is deliberately *not* implemented there, because each needs a party those phases do not have. A Phase 3 that skips them compiles and appears to work.

1. **Re-check the lock at commit, not only at move.** `isTabLocked` is sampled on every `pointermove` while the pointer is outside — so a turn *ending* mid-drag correctly unlocks the tab. But a turn *starting* in the milliseconds after the final move still commits as a detach. The shell → main commit path is the only place that can close that, and it must.
2. **Enforce D5 at commit.** Tearing out a window's only tab is a no-op, and nothing in Phases 1 or 2 enforces it — the hook does not know how many tabs the window has, and the resolver does not either. Same commit path, same reason.
3. **Wire `focus`/`show` → `raise()`** on the band registry, or z-order degrades to registration order (see D3 above).
4. **Own the ghost clamp.** D7 says the ghost is clamped to the source window's content rect. That needs the ghost's rendered size, which the hook does not have — it computes `clientX − grabOffset` and nothing more. It belongs to whoever renders the ghost, or to CSS in Phase 4. Today an outside ghost simply flies past the frame and Electron clips it.
5. **Decide whether Escape is swallowed.** The hook cancels the drag on Escape but does not `preventDefault` or stop propagation, so the keypress also reaches anything else listening — a modal, the composer. Swallowing it needs `stopImmediatePropagation` plus an assumption about listener ordering, which Phase 2 declined to bake in silently.

**A testing note that will cost someone an hour otherwise.** This repo's jsdom has no `document.elementFromPoint` *at all* — not "returns null", missing. The drag hook's move handler therefore throws on the first *promoted* move, which is why no pre-existing test in `components/chatGroups/` promotes a drag. Stub it to `null` in `beforeAll`. And note that pointer capture does not exist in jsdom either, so the capture *target* — which must be the element that received the `pointerdown`, never the tab wrapper, because capture retargets the compatibility mouse events and would otherwise fire `click` on the wrapper and break selecting a tab — can only be asserted, never exercised.

### Phase 4 — visuals (**partly contested**)

`ui/desktop/src/styles/main.css` — contested — gains one block beside the existing drag rules (`:1504-1600`): `.br-tab-ghost[data-detach='true']`, its transition, and the reduced-motion entry. Roughly eight lines, no new tokens.

Phase 4b, optional and separate: reconsider the ghost window rejected in D7.

### Phase 5 — optional unlock

Implement D9 and lift the D1 block. Backend work in `crates/biorouter-server/src/routes/reply.rs` plus `chatStreamStore.tsx`. Independent of everything above and should be specified on its own before it is started.

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

The row click loses the session's name. `useChatGroupsUrlSync` reads a `title` hint from route state (`:103-124`) precisely so a tab opens already named instead of flashing the "New Session" placeholder until `BaseChat` finishes loading; `AppSidebar.handleOpenChat` (`:194-203`) passes it, and `SessionsView.handleSelectSession` does not. The list row is the one caller that also holds `session.user_set_name`, which the sidebar cannot supply and which the URL sync already accepts (`:123`).

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
