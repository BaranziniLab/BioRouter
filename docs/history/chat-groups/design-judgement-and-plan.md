# Chat groups: design judgement and reduced plan

> **What this is.** The adversarial judgement of three competing chat-groups designs
> (`lift-state`, `minimal-shell`, `reuse-dashboard`), the synthesis drawn from them, and
> the reduced plan that was actually authorised on 2026-07-16 — chat tabs in a single
> group, with the splitting machinery's state model landed but unrendered.
> **Status:** Historical record — written 2026-07-16, and its scope was **overtaken by
> the work that followed**. The chat-groups branch went on to ship both the tabs (step 9,
> "Stages 1–2 — tabs in one group") and the split with drag, drop zones and the global
> terminal dock (step 10, "Stage 3"), which this document explicitly deferred. For what
> was built, see [UI overhaul — execution status](../../design/ui-overhaul/execution-status.md).
> **Audience:** maintainers working on the BioRouter desktop chat UI.
> **Identifier key.** `lift-state`, `minimal-shell` and `reuse-dashboard` are the
> codenames of the three candidate designs judged here; the candidate documents
> themselves are not preserved under `docs/`, so this file is the only surviving record
> of their content. `R1`, `R4`, `R5`, `R7`, `R9`, `R12` are risk numbers, and `Stage 0`–
> `Stage 3` stage numbers, **from each candidate's own register** — they are not a shared
> scheme, so `R1` means the per-window `localStorage` key in `lift-state` but the
> `WebkitAppRegion` hazard in this document's own top-five list. `R7` is `minimal-shell`'s
> nested-`KnowledgeProvider` risk, resolved separately in
> [the nested `KnowledgeProvider` blocker](../../design/chat-groups/knowledge-provider-nesting-blocker.md).
> Items `1`–`12` are this plan's own ordered work list.

Three authors independently designed browser-style chat groups for the desktop app.
This document reads all three against each other, finds each one's fatal flaw, keeps the
parts that survive scrutiny, and specifies a smaller plan that could be built safely in
one day. Two constraints were treated as hard: the terminal dock is global, and the
preview panel follows the active group.

> **Note on citations.** Source references such as `BaseChat.tsx:1370` are pinned to the
> tree as it stood on 2026-07-16 and have since drifted. They already disagree inside this
> document — the same 52 px `WebkitAppRegion: 'drag'` header is cited as `BaseChat.tsx:1629`
> in one place and `BaseChat.tsx:1663` in another. Treat every line number as a pointer to
> a symbol, not an address.

> **Note on the Dashboard.** Much of the reasoning below weighs the Dashboard — its cost,
> its bugs, and whether to delete it. The Dashboard component tree was removed from the
> desktop app in July 2026; see [Dashboard mode — removal record](../dashboard-mode/README.md).
> Statements here about Dashboard code describe the app as it was, not as it is.

## The decision

**None of the three designs is safely buildable in full today. We ship a subset.** The
subset is: *chat tabs in a single group, behind a correct green suite, with the splitting
machinery's state model landed but unrendered.* Splitting, panel hoisting, and N-mounted
BaseChats do **not** ship today.

Rationale below, then the plan.

## Adversarial read of the three designs

### `lift-state` — fatal flaw: it ships §5(b) as a known regression and calls it "filed"

It deletes the `ArtifactViewer` from BaseChat, hoists it, then admits (§5) that keying the
panel `key={activeSessionId}` **silently drops the artifact tab stack on every group
switch**. It labels this "a real regression from today's single-chat behavior… bounded".
It is not bounded — today a user opens five figures and cycles them; after this change,
one click into another group nukes four of them. Then Risk 6 admits tab-switching **eats
composer draft text** and defers the fix to "Stage 2" of a 7-stage plan. Two accepted
regressions to the *existing single-chat path* violate the hard constraint outright.

Merits, and they are large: the reducer-is-pure-and-testable framing (§1) is the single
best structural call in any of the three; the `pendingInitialMessage`-is-not-persisted
rule kills a real re-send bug; the per-window `localStorage` key (Risk 1) is the only
design that catches the two-Electron-windows collision *and* insists it lands early; the
`firstLeaf` reserve prop with a mandated padding test (§5) is correct. Its Stage 0 is
exactly right.

### `minimal-shell` — fatal flaw: the preview panel lands in the middle of the window

§5 is admirably honest — "the panel is inside the active group's box, not pinned to the
window's right edge… That looks wrong and I won't pretend otherwise." That is a direct
violation of *"the preview follows the active group"* read as a window-level surface, and
it is a visible, shipping-quality defect the moment anyone splits. It also keeps **all
panes mounted always** (§4) to make its `artifactPanelEnabled` gating work, which loads
Risk 4 (N mounted BaseChats, "untested") as a *load-bearing requirement* rather than an
optimization. And R7 — nested `KnowledgeProvider` with the outer one still bound to the
same `sessionId`, two providers writing the same
`biorouter-knowledge-active-kb:${sessionId}` key — is flagged as "my least-confident move"
and left unresolved.

Merits: the **BaseChat seam list is the best engineering in the packet.** Five props,
~20 lines, `renderSessionTitle` threading the strip *through* the existing 52px row so
`renderSessionHeaderActions()` (`BaseChat.tsx:1370`, which closes over local state and
genuinely cannot move cheaply) never has to move. R12 — "two 52px rows are structurally
possible" — is a hazard only this design saw. Its `zoneFromRect` is the cleanest of the
three. Its "ship-if-cancelled line: Stage 2" is the correct instinct, and I am adopting it.

### `reuse-dashboard` — fatal flaw: Risk 9 is a data-loss bug wired into the most frequent new interaction

It harvests `createdHere` + `releaseDashboardSession` (`DashboardProvider.tsx:330-340`) so
closing a tab **deletes the session** when empty. Then it makes preview tabs — where
*every single Recents click churns a tab* — the primary browse gesture. It names this
itself: "browsing history silently deletes sessions… the highest-consequence bug in the
whole design." It proposes a rule ("a preview tab is never `createdHere`") but no test, and
the same design admits (Risk 12) it never read `useChatStream`'s mount path. Combining
unverified remount behaviour with automatic session deletion is how you lose a user's work.
Additionally it plans to **delete ~4,000 lines of Dashboard** — a feature that ships today
and works — as part of the same arc.

Merits: it is the only design that read `ChatWindow.tsx` end-to-end and confirmed the prior
art empirically. Its Stage 0 ("spike, throwaway, DO NOT SKIP") is the most honest scheduling
in the packet, and its **`WebkitAppRegion` risk (R5) is a real hazard the other two missed
entirely** — the 52px header is `drag` at `BaseChat.tsx:1629`, and a pointer-drag gesture on
a `no-drag` child inside a `drag` region has a bad history on macOS/Electron. Its
`workingDirRef` fix for `InAppTerminalDock.tsx:381` is the correct mechanism.

### The synthesis

Take `lift-state`'s pure reducer + per-window storage key + `firstLeaf` reserve test. Take
`minimal-shell`'s BaseChat seams and its ship-line discipline. Take `reuse-dashboard`'s
`workingDirRef` fix, its `WebkitAppRegion` warning, and its refusal to skip the spike.
Discard: all three's Stage-N splitting, both panel hoists, `⌥`-duplicate, the Dashboard
deletion, and `createdHere` session-deletion-on-close.

**What makes the split unshippable today** is not the layout tree — that's an afternoon. It
is that *every* design's split requires N mounted BaseChats, and **all three authors
independently admit they have not measured it** (`lift-state` risk 7, `minimal-shell` R4
"Untested", `reuse-dashboard` risk 1 "I cannot cost this from reading, and it may invalidate
the design"). Three independent readers converging on "unknown, possibly design-invalidating"
is a stop sign, not a risk line.

I verified the Stage-0 facts directly. They are real: `scroll-chat-to-bottom` is dispatched
with **no detail at all** from `MCPUIResourceRenderer.tsx:120` and `McpAppRenderer.tsx:145`;
the listener at `BaseChat.tsx:1187` has `[]` deps. `make-agent-from-chat` (`:1207`) is
likewise unfiltered. `session-diverged` (`:1216`) destructures
`{ newSessionId, shouldStartAgent, editedMessage }` and **never reads an origin field**
before calling `navigate()`. The template fix is sitting in `ChatInput.tsx:379-382` with a
comment explaining itself. These are live bugs in the shipping Dashboard today.

## Measured results, 2026-07-16

> **Do not re-litigate these from reading.** Both were measured in the running app after the
> adversarial read above was written; they retire two of the risks it raised.

### R1 — `WebkitAppRegion` breaks tab drag on macOS: PASS, the risk is closed

Measured with real OS-level input through CDP (`ui/desktop/scripts/debug/probe-dragregion.mjs`),
not synthetic DOM events — synthetic events bypass the OS hit test entirely and would have
reported a false PASS. A pointer drag on a `no-drag` child injected into the real 52px
`WebkitAppRegion: 'drag'` header (`BaseChat.tsx:1663`) delivered
`pointerdown: 1, pointermove: 32, pointerup: 1` to the DOM, and the window did **not** move
(bounds unchanged at x:95 y:33). The strip can live inside the header as designed; the
fallback (strip fully `no-drag`, drag region moved to the trailing space) is **not needed**.

### R4 — N-mounted BaseChats: AFFORDABLE, the split is not design-invalidating

Measured in the running app by mounting real chats on the Dashboard (which already mounts N
BaseChats today, so the experiment was free):

| mounted chats | heap | DOM nodes |
|---|---|---|
| 1 (on /pair)  | 59.7 MB | 540 |
| 3 (dashboard) | 68.6 MB | 605 |
| 6 (dashboard) | **66.2 MB** (stable over 6 consecutive samples) | 912 |

Heap at 6 chats is LOWER than at 3 — it is flat within GC noise, so nothing is retained per
chat at a scale that matters. The real, monotonic cost is **~74 DOM nodes and ~1 MB per
chat**. Every one of the three designs stopped here on a guess ("untested", "unknown, may
invalidate the design"); it cost ~2 minutes to answer.

Caveat, disclosed: dashboard windows are small and these sessions are freshly spawned (short
transcripts). A full-height group with a long transcript and tiktoken counting will cost
more. This retires R4 as a BLOCKER; it does not license an unbounded split. Re-measure
before allowing >4 groups.

### Superseded: the first R4 reading

An earlier verdict recorded here read **"R4 / N-mounted BaseChats: STILL UNMEASURED"** and
described the first Dashboard probe as void:

> An attempt to measure it via the Dashboard (which already mounts N) was **void** — the
> probe never mounted a single chat (`baseChats: 0`, `domNodes` flat at 254 across all
> samples), so its "affordable" verdict was noise from an empty page and has been discarded
> along with the probe. This must be measured in the configuration that actually matters —
> two real panes — **before the split is widened past 2 groups**.

That probe was deleted rather than banked, and the measurement above replaces it.

## Scope statement

> **Superseded.** This scope held on 2026-07-16 only. The branch subsequently shipped both
> "Stages 1–2 — tabs in one group" (step 9) and "Stage 3 — split, drag, drop zones, global
> dock" (step 10), so the deferrals listed here as "Explicitly NOT today" no longer describe
> the product. See [UI overhaul — execution status](../../design/ui-overhaul/execution-status.md).

**Ship today:** chat tabs in one group. Tab strip on `.br-tab`, preview tabs, running pulse,
close-with-successor, drag-to-reorder, overflow, deep links, sidebar, Home recents,
CLI-opened sessions — all working. The layout tree exists in the state model and is exercised
by reducer tests, but `ChatGroupsShell` renders **only** `layout.kind === 'leaf'` and throws
in dev on a branch node.

**Explicitly NOT today:** splitting, drop zones, cross-group move, ⌥-duplicate, hoisting
`ArtifactViewer`, hoisting the artifact tab reducer, N mounted BaseChats, Dashboard deletion,
`createdHere` deletion-on-close, keep-alive mounts.

**The two hard constraints are satisfied degenerately and honestly.** With one group: the
terminal dock is global (there is one), and the preview follows the active group (there is
one). Both are then *structurally* correct — the dock moves behind `TerminalDockContext` and
the panel is gated by `artifactPanelEnabled` — so the split lands later without re-litigating
either.

## State model — exact types

`ui/desktop/src/components/chatGroups/chatGroupsTypes.ts`

```ts
export type ChatTabId = string;    // 'tab-7'
export type ChatGroupId = string;  // 'grp-1'

export interface ChatTab {
  tabId: ChatTabId;          // stable across sessionId changes — this is why tabId ≠ sessionId
  sessionId: string;         // '' only transiently, before createSession resolves
  title: string;             // mirror for the strip; kept fresh by utils/sessionNameSync.ts
  userSetName: boolean;
  /** VS Code enablePreview. Italic label (.br-tab--preview, main.css:1003 — its first
   *  consumer). The next preview-open in this group REUSES this tab in place, keeping
   *  tabId. At most one per group; reducer invariant. */
  preview: boolean;
  /** Route-state cargo, consumed exactly once by BaseChat on mount. NEVER persisted —
   *  a queued message must not re-send after reload. */
  pendingInitialMessage?: string;
  pendingInitialAttachments?: UserAttachment[];
  workflowId?: string;
  cwd?: string;
}

export interface ChatGroup {
  groupId: ChatGroupId;
  tabs: ChatTab[];                  // array order IS strip order
  activeTabId: ChatTabId | null;    // null only when tabs === []
}

/** The tree ships today with depth 0. It exists now so Stage 2 is not a state migration. */
export type GroupLayout =
  | { kind: 'leaf'; groupId: ChatGroupId }
  | { kind: 'branch'; dir: 'row' | 'col'; children: GroupLayout[]; sizes: number[] };

export interface ChatGroupsState {
  version: 1;
  layout: GroupLayout;
  groups: Record<ChatGroupId, ChatGroup>;
  activeGroupId: ChatGroupId;       // ALWAYS names a live leaf
  seq: number;                      // deterministic ids → testable reducer
}
```

**Deliberately absent:** `createdHere`. We do not delete sessions on tab close, today or
ever, without a separate reviewed change. An orphaned empty session is cheap; a deleted one
is not. This drops `reuse-dashboard`'s Risk 9 to zero by construction rather than by rule.

### Reducer invariants

**Reducer** (`chatGroupsReducer.ts`, pure, zero React): `openTab`, `activateTab`, `pinTab`,
`closeTab`, `reorderTab`, `renameTab`, `setActiveGroup`. Invariants, each a test:

- `openTab({preview:true})` into a group with an existing preview tab **replaces in place**,
  same `tabId`.
- `openTab` for a `sessionId` already open **anywhere** → activate that tab + its group,
  never duplicate. (Generalizes `artifactSourceKey` dedupe, `ArtifactViewer.tsx:183-198`.)
- `closeTab` successor = `Math.min(closingIndex, remaining.length - 1)` — identical to
  `ArtifactViewer.tsx:213-218`, so the two surfaces cannot drift.
- Closing the last tab of the last group leaves an empty group (renders BaseChat's existing
  empty state, `suppressEmptyState={false}`). It does **not** navigate away.
- `activeGroupId` always names a live leaf.
- **Pin-on-run:** `openTab({preview:true})` never replaces a tab whose session is running. A
  live turn is committed-to. (This resolves `lift-state`'s open question #8 in the safe
  direction.)

### Storage

**Storage** — `chatGroupsStorage.ts`, cloned from `dashboardStorage.ts`'s versioned-envelope
shape. Key `biorouter.chatgroups.v1:${windowId}`, **per-window from day one** (`lift-state`
Risk 1). `createChatWindow` (`main.ts:894`) spawns a real second renderer on the same origin;
the Dashboard has this collision today and nobody noticed because nobody opens two. Tabs will
be used. `windowId` rides in via `appConfig`; a `beforeunload` sweep prunes dead keys, and a
stale key per crashed window is an accepted leak. Strip
`pendingInitialMessage`/`pendingInitialAttachments` on write. Drop `sessionId === ''` tabs on
load. `load()` returns `null` on any shape mismatch → cold-boot path.

### URL contract — one reader, one writer, never mutually recursive

- **In (command, once per param change):** `ChatGroupsProvider` holds `lastAppliedParamRef`.
  When `searchParams.get('resumeSessionId')` differs from it →
  `openTab({ sessionId, preview: location.state?.preview === true })`. Guarded additionally
  on `location.key` so one nav is consumed once.
- **Out (mirror of focus):** effect on `activeSessionId` →
  `if (id && id !== searchParams.get('resumeSessionId')) navigate('/pair?resumeSessionId=' + id, { replace: true })`.
  Same guard shape as the existing `App.tsx:174`.
- `App.tsx:172-179` is deleted; the provider becomes the only writer.
- **The URL encodes one session, by design.** It is a deep-link inbox and a focus mirror, not
  a description of the window. `AppSidebar.tsx:137`'s `currentSessionId` highlight keeps
  working with **zero sidebar edits**.

### Back button

With one group today, `replace: true` on tab activation means Back no longer walks session
history — it leaves `/pair`. This *is* a behaviour change (`minimal-shell` R5, correctly
flagged, no clean answer). Mitigation: today's `handleOpenChat` push-nav is preserved for
*cross-route* entries, and only in-strip tab clicks use `replace`. Back therefore still
returns you to where you came from *into* `/pair`, which is the case that actually matters.
Verified by hand in the smoke matrix.

## File-by-file work list, ordered, each item independently reviewable

Each item is a separate commit, separately green.

1. **`BaseChat.tsx` + `MCPUIResourceRenderer.tsx:120` + `McpApps/McpAppRenderer.tsx:145` —
   sessionId-filter the three window listeners.** Add `{ detail: { sessionId } }` to both
   `scroll-chat-to-bottom` dispatch sites (verified: today they dispatch bare
   `new CustomEvent('scroll-chat-to-bottom')`) and filter at `BaseChat.tsx:1187`. Filter
   `make-agent-from-chat` (`:1207`) — needs a `sessionId` at its dispatch site too. Filter
   `session-diverged` (`:1216`) on an **origin** sessionId; `chatStreamStore.tsx:836` already
   carries `targetSessionId`, the listener at `:1216-1244` just destructures past it and
   navigates. Copy `ChatInput.tsx:379-382` verbatim, comment and all. **Ships alone. Fixes
   live Dashboard bugs. Zero UI change. Merge this even if everything below is cancelled.**

2. **`BaseChat.tsx` — `allowWindowResize?: boolean` (default `true`).** Early-return in
   `ensureArtifactPanelFits` (`:652-661`). A session-scoped component must not resize the OS
   window when it isn't the only one. No behaviour change at N=1.

3. **`InAppTerminalDock.tsx` — `workingDir` to a ref.**
   `const workingDirRef = useRef(workingDir); workingDirRef.current = workingDir;`, read it
   inside the create-pane callback, drop it from the `:381` deps. Panes already capture cwd at
   creation (`:339`) and never re-read — this makes the existing contract explicit and stops a
   future global dock from blowing away live shells on group switch. Pure refactor, no
   behaviour change.

4. **`styles/main.css` — extend the existing `:921-1005` block.**
   `.br-tab[data-dragging='true'] { opacity: .35 }`, `.br-tab[data-dropbefore='true']::after`
   (insertion hairline), `.br-tab__dot` (coral pulse),
   `.br-tabstrip { flex-wrap: nowrap; overflow-x: auto }`,
   `.br-tab { flex: 0 1 auto; min-width: 88px }`. This finally gives rules to
   `data-dragging`/`data-dragover`, which `ArtifactViewer.tsx:673-674` writes into the DOM
   today with **no CSS reading them** (the drag visuals are Tailwind at the call site,
   `:678-679`). Do **not** add `.br-dropzone` yet — no dead CSS for unshipped features.

5. **`components/ui/tabReorder.ts` + `components/ui/useTabDragReorder.ts` — extract the
   gesture.** Lift the splice-move (`ArtifactViewer.tsx:202-211`) and the pointer gesture
   (`:380-430`, `:575-592`) verbatim: 5px promotion threshold, `elementFromPoint` + `closest`,
   `suppressTabClickRef` + `setTimeout(…, 0)` to swallow the synthetic click. Generalize
   `[data-artifact-tab-id]` → `[data-tab-id]`.

6. **`components/ui/DocumentTabs.tsx` + migrate `ArtifactViewer` onto it.** Presentational +
   gesture only: `items`/`activeId`/`onSelect`/`onClose`/`onReorder`/`size`/`endSlot`/`scrollable`.
   **Absorb neither reducer** — `ArtifactViewer` keeps its own (`:174-219`) and its Cmd+W /
   Ctrl+Tab policy (`:331-370`); those are domain, not tab. `role="tablist"` goes on the
   `.br-tabstrip` element itself, no inner div re-declaring `gap` (that inner div is exactly
   how `InAppTerminalDock.tsx:512` drifted to `gap-1` = 4px against the contract's 3px). Add
   roving tabindex here — both surfaces promise `role="tablist"` and neither delivers.
   `ArtifactViewer.test.tsx:799-809` reaches the styled node via `.closest('.br-tab')` and
   survives the restructure unchanged; that it survives *is the signal the seam is right*.

7. **Migrate `InAppTerminalDock.tsx:505-575` onto `DocumentTabs`.** Pass
   `onReorder={undefined}` — the dock's panes have no reorder action (`:428-429` is
   `panes`/`activePaneId`, no splice), and an affordance that does nothing is worse than none.
   `scrollable` + `endSlot={<NewPaneButton/>}`. Kills the `gap-1` drift and the two
   `data-active` spellings.

8. **`chatGroupsTypes.ts` + `chatGroupsReducer.ts` + `chatGroupsStorage.ts` — headless.**
   Nothing renders it. All tests are pure. **This is where the review value concentrates.**

9. **`contexts/TerminalDockContext.tsx` + `BaseChat.tsx` seam.** `const dock = useTerminalDock()`
   returns `null` outside a provider (the `ChatContext.tsx:74-77` pattern).
   `isTerminalDockOpen` resolves to `dock ?? local`; `{!dock && <InAppTerminalDock/>}` at
   `:1769`; the `[sessionId]` reset at `:632` becomes `if (!dock) setIsTerminalDockOpen(false)`.
   At N=1 with no provider mounted, byte-identical behaviour.

10. **`BaseChat.tsx` — `renderSessionTitle?: () => ReactNode` + `suppressTitlebarReserve?: boolean`
    + `artifactPanelEnabled?: boolean`.** At `:1640-1647`:
    `{renderSessionTitle ? renderSessionTitle() : <SessionNamePill/>}`, inside the existing
    `min-w-0 flex-1` div. **`renderSessionHeaderActions()` at `:1644` does not move.** This is
    the whole reason the strip goes *through* BaseChat rather than above it — it closes over
    `isTerminalDockOpen`/`reviewOpen`/`session` and cannot be hoisted cheaply. A comment at the
    seam says exactly that, because the day someone hoists the strip without it, you get a strip
    row *and* an actions row: two 52px bars.

11. **`components/chatGroups/ChatTabStrip.tsx`.** `.br-tabstrip` + `DocumentTabs` + close +
    pulse + overflow. Owns `getSessionTitlePadding` (moved from `BaseChat.tsx:592`, its only
    consumer) and takes `reserveTitlebar: boolean` computed by the shell as **`firstLeaf(layout)`**
    — a recursive `children[0]` walk, **never** an array index. In a root `col` split both
    children sit at x=0 but only the top one hits the traffic lights. It ships as a tree walk
    today, at depth 0, so the split cannot introduce it later as a bug.

12. **`contexts/ChatGroupsContext.tsx` + `components/chatGroups/ChatGroupsShell.tsx` +
    `ChatGroupPane.tsx` + `ChatTabHost.tsx`.** Shell renders `leaf` only; `branch` throws in dev.
    `PairRouteWrapper` shrinks to the URL adapter. `Pair.tsx` deleted (34-line pass-through).
    Sidebar recents gain `{preview:true}` / `onDoubleClick`→`{preview:false}`.

**Not touched:** `hooks/chatStreamStore.tsx` (already correctly keyed, `:862-874`),
`contexts/ChatContext.tsx` (see [The ChatContext singleton](#the-chatcontext-singleton--precisely)),
all of `components/Dashboard/`, `navigateWithViewTransition` (inert, ~20 call sites, churn with
no payoff — leave the vestigial name).

## The ChatContext singleton — precisely

**`ChatContext` is not the bug. Do not rewrite it.** It is prop-driven
(`ChatContext.tsx:21-33`), stores nothing, returns `null` outside a provider (`:74-77`), and
already carries `contextKey` for exactly this. `ChatWindow.tsx:47,195` mounts one per chat
today and it works. Rewriting churns seven consumers for nothing.

The singleton is `App.tsx:405`. **I demote it. I do not delete it today.**

```text
App.tsx:405     useState<ChatType> hubChat            ← SURVIVES, renamed
  App.tsx:611   <KnowledgeProvider sessionId={hubChat.sessionId||null}>   ← SURVIVES (fallback)
    App.tsx:624 <ChatProvider chat={hubChat} contextKey="hub">            ← SURVIVES
      AppLayout                          ← AppSidebar :137 and :177 UNCHANGED
        /pair → ChatGroupsProvider → ChatGroupsShell → ChatGroupPane
                  const [tabChat, setTabChat] = useState<ChatType>({...})   // ChatWindow.tsx:47
                  <ChatProvider chat={tabChat} setChat={setTabChat}
                                contextKey={`tab-${activeTab.tabId}`}>      // SHADOWS "hub"
                    <BaseChat key={activeTab.tabId} setChat={setTabChat} .../>
```

Four moves:

1. **`ChatTabHost` owns the chat state**, copied from `ChatWindow.tsx:47-53`. BaseChat's
   `setChat` points at the tab's setter. `App.tsx:405` stops being `/pair`'s identity.
2. **`hubChat` stays** because Hub composes a first message before a session exists and
   `/extensions` runs its own mini-chat (`App.tsx:636`) — neither is a `/pair` chat. Rename to
   `hubChat`/`setHubChat`; stop passing it to `PairRouteWrapper`. Deleting it is a separate,
   later change.
3. **The mirror effect** (from `minimal-shell` §3, the best idea in it): `ChatGroupPane` fires
   `onChatChange` for the active group, and the shell pipes it into `setHubChat`. So `hubChat`
   is always the focused chat. **`AppSidebar.tsx:177` (`document.title`) and `:137` (recents
   highlight) work with zero edits.** The "focused chat" concept the brief asked for, delivered
   without touching AppSidebar. At N=1 this is a tautology — which is exactly why it's safe to
   land today and already correct when N>1.
4. **`KnowledgeProvider` does NOT move today.** This is the one place I overrule all three
   designs. All three want it inside the per-chat subtree; `minimal-shell` R7 correctly
   identifies that this leaves the outer `App.tsx:611` provider bound to the *same* sessionId,
   so two providers mount against the same `biorouter-knowledge-active-kb:${sessionId}` keys
   (`KnowledgeContext.tsx:17-22`) and may fight — and `useKnowledge` **throws** outside a
   provider (`:246`), so any tree mistake is a white screen, not a degradation. **At N=1 the
   leak does not exist** (one chat, one KB, the singleton is correct by accident). Moving it
   buys nothing today and risks a white screen on `/knowledge`. It is the first item of the
   split PR, prefaced by reading `KnowledgeContext.tsx` end to end.

## Drag and drop

**Pointer events. No library.** Verified: no dnd-kit/react-dnd/sortable in `package.json`;
`@radix-ui/react-tabs` is imported by exactly one file (`components/ui/tabs.tsx`) and neither
tab surface uses it — Radix Tabs has no reordering anyway. HTML5 DnD is also wrong on the
merits: its `dragover` throttling is precisely what would make a live tint lag.

**Today's scope: reorder within one strip. That is all.**

`useTabDragReorder`, lifted from the already-debugged `ArtifactViewer.tsx:380-430`:

1. `onPointerDown` records origin only (`:575-582`). No drag yet.
2. Promote past **5px** (`:386-391`). Mirror into a ref *and* state — ref because the window
   listeners close over `[]` deps and must read fresh; state to drive the render.
3. `pointermove` → `document.elementFromPoint(x,y)?.closest('[data-tab-id]')`. Hit-test, not
   per-tab enter/leave — it survives scroll and a moving strip.
4. `pointerup` → the splice-move from `tabReorder.ts`, then `setTimeout(…, 0)` clearing
   `suppressTabClickRef` to swallow the synthetic click (`:409-413`). Cleanup on
   `pointercancel` and `blur`.

**The dragged tab stays in flow at `opacity: .35`. No cursor-following ghost.** `.br-tab`'s
divider is adjacent-sibling based (`main.css:966-975`); pulling the tab out of the DOM
re-flows every divider mid-drag. This is a CSS-contract constraint, not a taste call.

**The `WebkitAppRegion` hazard is item 0 of the day.** The 52px header is `drag` at
`BaseChat.tsx:1629`; tabs must be `no-drag`. A pointer-drag on a `no-drag` child inside a
`drag` region has misbehaved on macOS/Electron before. `reuse-dashboard` was the only design
to see this. **Prototype it in the real app before writing `ChatTabStrip`** — a 20-minute
check that can force the strip to `no-drag` entirely, with the drag region only in the
trailing empty space. Discovering this at item 11 costs the day.

**Not today:** drop zones, `zoneFromRect`, the live accent tint, cross-group move, the ghost,
⌥-duplicate. ⌥-duplicate is cut permanently until proven: two tabs on one sessionId → two
BaseChats sharing one memoized controller (`chatStreamStore.tsx:867-874`) → two composers that
can both submit into one turn.

## What we do not build today

- **Splitting.** No `branch` rendering, no splitters, no drop zones, no overlay. The types
  exist; the renderer throws on a branch.
- **Hoisting `ArtifactViewer`.** It stays in BaseChat, gated by `artifactPanelEnabled` (always
  `true` today). No lifted reducer → **no lost artifact tab stacks**. This is the regression
  `lift-state` accepted and we do not.
- **N mounted BaseChats.** One group, one mounted BaseChat. Every "untested" risk in all three
  designs stays dormant.
- **Deleting `App.tsx:405`, moving `KnowledgeProvider`, deleting the Dashboard.**
- **`createdHere` / delete-session-on-close.** Never, without its own review.
- **Keep-alive mounts / draft preservation.** Tab switch remounts BaseChat and loses composer
  draft text — **but today's session switch loses it identically** (`<Pair key={sessionId}>`,
  `App.tsx:201`), so this is *not a regression*. It is a promise tabs make and don't yet keep.
  Documented, deferred, honest.
- **Cmd+W arbitration.** With one group, `ArtifactViewer.tsx:335-345`'s capture-phase Cmd+W
  keeps winning when the panel is open — exactly as today. Chat tabs get **no** Cmd+W binding.
  An ambiguous global shortcut is worse than a missing one.

## Test strategy

### Unit tests (vitest, jsdom) — must be green to merge

| File | Asserts |
|---|---|
| `chatGroupsReducer.test.ts` | Every state-model invariant. Preview replace-in-place keeps `tabId`. Preview never replaces a **running** tab. Dedupe activates rather than duplicates. Close successor `Math.min(i, n-1)`. Last-tab-of-last-group → empty group, no navigation. `activeGroupId` always a live leaf. Deterministic ids from `seq`. |
| `chatGroupsStorage.test.ts` | Round-trip. `pendingInitialMessage` **absent** after save. `sessionId:''` tabs pruned on load. Garbage/wrong-version → `null`. Two `windowId`s → two disjoint keys. |
| `tabReorder.test.ts` | Splice-move, not swap. Same-index no-op. Out-of-range no-op. |
| `useTabDragReorder.test.ts` | 4px does not promote; 6px does. Post-drag click suppressed. `pointercancel` cleans up. |
| `ChatTabStrip.test.tsx` | `.br-tab` contract via `.closest('.br-tab')`. `data-active='true'` on exactly one. `.br-tab--preview` italic on preview tabs — **its first consumer ever**. Pulse dot present iff `useRunningChats()` has the id. |
| `ChatTabStrip.titlebar.test.tsx` | **The reserve test the spec demands.** macOS + sidebar collapsed → first strip's computed `paddingLeft` === 172 (`getTitlebarControlReserve(true)` = 100+64+8, verified). Non-mac → no reserve. **Asserts against a `firstLeaf(layout)` tree walk with a synthetic depth-1 `col` tree**, even though we render depth 0 — so the split cannot regress it later. A reserve that fails silently is worse than no reserve. |
| `chatGroupsUrlSync.test.tsx` | **Fixed point.** `activateTab` → exactly one `navigate`, no re-dispatch. Same param twice → one `openTab`. Provider is the only writer. |
| `BaseChat.listeners.test.tsx` | Two BaseChats mounted; `session-diverged` for A does **not** navigate B; `scroll-chat-to-bottom` for A does not scroll B; `make-agent-from-chat` for A opens one modal. |
| existing `ArtifactViewer.test.tsx`, `InAppTerminalDock.test.tsx` | **Unchanged, still green.** Both reach the styled node via `.closest('.br-tab')`. If they need edits, the `DocumentTabs` seam is wrong. |

### Verification by driving the real app — jsdom cannot prove these

Run with `BIOROUTER_NO_HMR=1` (any save under `ui/desktop/src/` full-reloads and destroys the
session under test, which fails in ways that look like app bugs) + `just agent-browser-ui`. Use
the `debug-app` skill.

1. **`WebkitAppRegion`** — drag a tab inside the `drag` header region on macOS. Do this
   **first, before writing the strip.**
2. **Tab drag reorder** — jsdom has no `elementFromPoint` geometry and no real pointer capture.
   The 5px threshold, click suppression, and `pointercancel` all need a real cursor.
3. **The 172px reserve** — jsdom computes no layout. Screenshot: sidebar collapsed, macOS, tabs
   must clear the traffic lights. Then expand the sidebar and confirm it releases.
4. **Overflow: shrink → scroll.** `.br-tab` caps at `max-width:190px` with no min and no
   `flex-shrink` today — it does not shrink, it overflows. Needs a real browser at three window
   widths. Note the reserve steals width from group 1 only, so it overflows first at identical
   tab counts; that's correct and will read as a bug in review.
5. **Prism/Tailwind token collision** — `code [class~='token'] { display: inline }`
   (`main.css`) is unlayered and jsdom applies no Tailwind. Sweep the artifact panel across
   md/csv/json/yaml/R/py/sql after the `DocumentTabs` migration, per CLAUDE.md.
6. **The smoke matrix, by hand, every row:** deep link `/pair?resumeSessionId=X` cold → refresh
   → sidebar recents single-click (italic, reuses) → double-click (pins) → Home composer submit
   with `initialMessage` → sidebar new-chat → **CLI-opened session** → `createChatWindow` new
   window (two windows, two disjoint layouts, no clobber) → `/dashboard` still works untouched
   → Back button from `/pair`.
7. **Terminal dock** — open, run a long shell, switch tabs, confirm the pane is not recreated
   (item 3's ref fix).

**Ship gate:** every unit test green, plus rows 1–7 observed. Not "tests pass" — *observed*.

## Top five risks

**R1 — `WebkitAppRegion` breaks tab drag on macOS.** The 52px header is `drag`
(`BaseChat.tsx:1629`); a pointer-drag on a `no-drag` child inside it has a bad Electron
history. *Mitigation:* prototype it as the literal first action of the day, before any strip
code. Fallback: strip goes fully `no-drag`, drag region moves to the trailing empty space after
the last tab. Cheap either way — **if discovered at item 11 it costs the day.**
(Subsequently measured and closed — see [Measured results](#measured-results-2026-07-16).)

**R2 — The URL adapter ping-pongs into an infinite render.** Today's single-id guard
(`App.tsx:173`) does not generalize; read and write become mutually recursive if either fires
on the wrong dep. All three designs flag this; `reuse-dashboard` calls it "the most likely
infinite-render bug." *Mitigation:* `lastAppliedParamRef` gates in; `replace:true` + `!==`
gates out; `chatGroupsUrlSync.test.tsx` asserts the fixed point mechanically before the shell
is wired. Budget real time.

**R3 — Two Electron windows clobber `biorouter.chatgroups.v1`.** `createChatWindow`
(`main.ts:894`) spawns a real second renderer on the same origin. The Dashboard has this bug
*today* with `biorouter.dashboard.v2`, unnoticed only because nobody opens two dashboards —
tabs will be used daily. *Mitigation:* per-window key from item 8, `windowId` via `appConfig`,
`beforeunload` sweep. Accepted residue: one stale key per crashed window.

**R4 — The `DocumentTabs` extraction regresses the artifact panel.** `ArtifactViewer`'s drag is
subtle, already-debugged code, and its `role="tab"` sits on the inner `<button>` while
`.br-tab` is the outer `<div>`. *Mitigation:* migrate `ArtifactViewer` **first** (item 6),
against its existing ~800-line test file, **unmodified**. Those tests reach the styled node via
`.closest('.br-tab')`; if they need editing, the seam is wrong and we stop. Absorb neither
reducer. Dock migrates second, separately.

**R5 — Tab switch eats the composer draft, and tabs *promise* cheap switching.**
`<BaseChat key={activeTab.tabId}>` unmounts the outgoing session; messages survive via the
registry (`chatStreamStore.tsx:862-874`) but unsent draft text in ChatInput does not.
*Mitigation:* **this is not a regression** — today's session switch (`App.tsx:201`) loses it
identically, so we ship no worse than current. Documented in the PR as a known gap with the fix
named (lift draft state out of ChatInput, keyed by sessionId — cheaper and safer than
keep-alive mounts, which would multiply R1/R3 and drag in the whole untested N-mount question).
First item of the follow-up.

**Honourable mention (not top 5, will bite someone):** `ChatStreamController`'s ctor calls
`subscribeSessionNameChanges` (`chatStreamStore.tsx:232-247`) and never unsubscribes;
controllers are never evicted. Bounded today by "you touch one session at a time." Tabs make
touching 40 sessions a minute's work — **this feature worsens a pre-existing leak without
causing it.** File an issue; do not fix it in this PR.

## Ship line

**Items 1–7 are pure wins with zero dependency on the rest and should merge even if tabs are
cancelled** — they fix live Dashboard bugs, close the `data-dragging`/`data-dragover` CSS gap,
kill the `gap-1` drift, and give the dock the roving tabindex both surfaces have been promising
via `role="tablist"` and never delivering.

**Items 8–12 deliver "chat has tabs" as a coherent, shippable feature with no dormant
multi-mount risk.** Everything genuinely dangerous — N mounted BaseChats, the
`KnowledgeProvider` move, the panel hoist, split geometry — is behind a line we do not cross
today.

## Related documentation

- [UI overhaul — execution status](../../design/ui-overhaul/execution-status.md) — the status
  record for the branch this plan fed into; it shows that tabs *and* the split both shipped,
  which supersedes this document's scope statement.
- [Nested `KnowledgeProvider`: the chat-groups nesting blocker](../../design/chat-groups/knowledge-provider-nesting-blocker.md)
  — the spike that resolved `minimal-shell`'s R7, the one risk this plan deferred rather than
  answered.
- [Dashboard mode — removal record](../dashboard-mode/README.md) — what happened to the
  Dashboard whose cost, bugs and possible deletion are weighed throughout this judgement.
