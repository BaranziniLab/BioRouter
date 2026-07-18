> ## STATUS — 2026-07-16
>
> **Verdict accepted: GO-WITH-CHANGES. The prerequisite fix is NOT yet made.**
>
> I attempted the fix (resolve the field a writer is not changing from
> localStorage rather than from its own React closure, in BOTH `setActiveKbId`
> and `setHiddenKbIds` — the bug is symmetric, which this spike did not note)
> and **reverted it**. The change was principled and typechecked and the 14
> existing knowledge tests passed with it, but I could not get a regression
> test to demonstrate it: the provider's server-hydrate effect
> (`KnowledgeContext.tsx:187-209`) overwrites BOTH localStorage keys from the
> response and kept racing the state the test seeds. "Correct by inspection"
> is not good enough for a path that POSTs to the server.
>
> Two concrete traps for whoever picks this up — both cost me time:
>   * The storage keys are `knowledge_active_kb:<id>` / `knowledge_hidden_kbs:<id>`
>     (`:14-15`). This spike and the plan both cite
>     `biorouter-knowledge-active-kb:<id>`, which **does not exist**.
>   * The hydrate reads `res.data.active_kb`, NOT `kb_id`. A mock returning
>     `kb_id` hydrates to null and silently CLEARS the key, which looks exactly
>     like the bug under test. Make `getActive` never settle to isolate the
>     write path.
>
> **This is a prerequisite for Stage 3 (nested providers), not for Stage 1-2
> (tabs, one group), which never mounts two providers.** It must land with a
> green, mutation-killed test before anything nests KnowledgeProvider.

The `BaseChat.tsx` diff contains zero references to my spike or to Knowledge — it's Stage 0 work (`isEventForSession`, the broadcast-event session filter), written **concurrently by another agent/session** at 21:12 while I was running tests. It is not mine, so I deliberately did **not** revert it — doing so would have destroyed someone else's in-flight work. My spike file is deleted; my footprint is zero.

---

# R7 Spike: Nested `KnowledgeProvider` — Verdict: **GO-WITH-CHANGES**

**The plan has the path wrong.** It cites `ui/desktop/src/contexts/KnowledgeContext.tsx`; the file is at **`ui/desktop/src/components/knowledge/KnowledgeContext.tsx`**. Line numbers cited in R7 are all correct.

## 1. What the provider owns / fetches

State: `bases` (`:53`), `loading` (`:54`), `activeKbId` (`:57-59`, lazy-init from localStorage), `hiddenKbIds` (`:60-73`), `graphRefreshRef` (`:74`).

Two independent fetches per provider:
- `refresh()` → `listBases` in an effect with `[refresh]` deps (`:130-141`, `:151-153`). `refresh` is `useCallback(…, [])`, so **once per mount**.
- `getActive` in the hydrate effect (`:169-213`), deps `[hiddenStorageKey, sessionId, storageKey]` → **once per mount, re-runs on sessionId change**.

**N providers ⇒ N `/knowledge/bases` fetches. No caching, no dedup, no shared module-level cache.** Measured: 2 providers → `listBases = 2`, `getActive = 2`. `KnowledgeView.tsx:22-27` confirms the intent ("only fetches the base list once at app start") — that comment silently becomes false under nesting. This is a cost concern, not a correctness one.

## 2. localStorage — **the plan's key name is wrong; the fight is real**

Actual keys (`:14-23`): **`knowledge_active_kb:<sessionId>`** and `knowledge_hidden_kbs:<sessionId>` — not `biorouter-knowledge-active-kb:<sessionId>`. No `biorouter-` prefix, underscores not hyphens.

Both providers **read** (`:57-59`, `:170`) and both **write** (`:80-82` in `syncSelection`, `:200-202` in hydrate). Definitively:

- **No loop.** Neither provider listens for `storage` events, and the hydrate effect (`:169-213`) does not depend on `activeKbId`. A write never re-triggers a read. No infinite cycle.
- **No mount-time fight.** Both hydrate from the same server `getActive` and write the same value. Idempotent, converges.
- **Yes, a clobber — and it needs no user action to reach the dangerous state.**

The exact chain, **proven** (see §Experiment):

1. Pane provider `setActiveKbId('kb2')` → `syncSelection` (`:76-95`) writes localStorage **and** POSTs `setActive`.
2. The app-level provider **never hears about it** — no storage listener, no re-fetch. Its `activeKbId` stays `kb1`. **Silently stale.**
3. **The kill shot is `setHiddenKbIds` (`:104-110`): `syncSelection(activeKbId, nextIds)` — it writes `activeKbId` too.** So *any* hidden-KB change in the stale provider writes its stale `activeKbId` over localStorage **and the server**.
4. Worse, `:161-167` calls `setHiddenKbIds` **automatically** from an effect when `bases` changes and hidden ids contain a stale entry. **No user action required.** `:155-159` is a second automatic path (`setActiveKbId(null)` on prune).

The pane's in-memory state stays `kb2`, so the UI shows `kb2` while server + localStorage say `kb1` — the user's KB choice silently reverts on next mount, and **the agent is already running against the wrong KB** (`session_id` is sent at `:87`, so this is server-side session state, not just cosmetics).

## 3. `useKnowledge` outside a provider

Throws (`:246`). **Blast radius: white screen.** No error boundary wraps `KnowledgeProvider` at `App.tsx:611`; it sits inside `<Routes>`, so a tree mistake unmounts the whole router. R7's characterization is correct.

## 4. Consumers — which KB does each want?

| Consumer | Wants |
|---|---|
| `BottomMenuKnowledgeSelection.tsx:24` | **per-pane** — the chat's own KB chip |
| `IngestPanel.tsx:27` | **app-level** — `/knowledge` route |
| `KBSelectorPalette.tsx:29` | **app-level** — and it calls `toggleKbHidden`, i.e. **it is the clobber trigger** |
| `KBSelectorTrigger.tsx:14` | app-level |
| `KnowledgeView.tsx:20` | app-level |
| `ChangeLogDrawer.tsx:35` | app-level |
| `KnowledgeGraphPanel.tsx:24` | app-level |
| `useKnowledgeBases.ts:8` | ambiguous — used by both `KBSelectorPalette` and `KnowledgeGraphPanel` (app-level) |

**Exactly one consumer wants the per-pane KB.** Seven want app-level. That asymmetry drives the recommendation.

## 5. Alternatives

- **(a) One app-level provider, re-pointed at the active group's sessionId.** Kills the clobber by construction (one writer). But `sessionId` change re-runs `:169-213` → every group switch re-fetches `getActive` and rewrites state; and a *background* group's `BottomMenuKnowledgeSelection` would display the *active* group's KB. **Wrong semantics** — the chip lies about which KB its own chat uses.
- **(b) `sessionId={null}` to the app-level provider while `/pair` is mounted.** R7's own hunch. Removes key collision (`:18` falls back to the unsuffixed `knowledge_active_kb`), but `/knowledge` then edits *global* KB state while chats edit per-session state — two divergent meanings of "active KB", and it couples `App.tsx` to route identity. Also `getActive` with no `session_id` (`:190`) returns different server state, so `/knowledge` shows a KB unrelated to any chat. **Rejected.**
- **(c) Nest as designed.** Correct semantics — the one consumer that wants per-pane gets per-pane, seven get app-level, React's nearest-provider does exactly the right thing. **The only defect is the shared-key write path, which is a bug in `syncSelection`, not in nesting.**

## Recommendation: **(c), plus a mandatory prerequisite fix**

Nesting is semantically right and the alternatives are worse. But **(c) is only safe once `setHiddenKbIds` stops writing `activeKbId`.** Concretely, before Stage 3:

1. **Split the writes.** `syncSelection` (`:76-95`) conflates active-KB and hidden-KB writes; `setHiddenKbIds` (`:107`) must not carry `activeKbId`. This is the single change that makes two providers on one key non-interfering — and it is worth doing on its own merits *today*, independent of chat groups.
2. **Prefer a `storage` listener or lift `activeKbId` to a shared store** so the stale-read window closes. Without this, §2's divergence (pane says kb2, `/knowledge` says kb1) remains as a *display* inconsistency even after the clobber is fixed.
3. **Wrap the pane provider in an error boundary** — §3's white-screen radius is unacceptable for a per-pane provider mounted N times.
4. **Dedup `listBases`** (module-level cache or lift `bases` to one app-level fetch) before allowing >2 groups; N panes = N identical fetches on every mount.

Items 1 and 3 are blocking. 2 and 4 are strongly advised.

## The experiment

Throwaway vitest file, mounted a nested pair with `sessionId="S1"` against a mocked `../../api` with simulated server state. **Deleted.** Three experiments, all passed — i.e. every predicted failure reproduced:

```
[R7] listBases fetches for 2 providers = 2
[R7] getActive  fetches for 2 providers = 2
[R7] after pane setActiveKbId(kb2):
       pane state = kb2   app state = kb1   localStorage = kb2   server = kb2
[R7] after app-level toggleKbHidden(kb1):
       setActive POSTs = [{"kb_id":"kb1","hidden_kbs":["kb1"]}]
       server active = kb1    localStorage = kb1    pane state = kb2
```

Line 3 proves **staleness**: no propagation to the outer provider. Line 5 proves the **clobber**: a hidden-KB toggle in the app-level provider POSTed `kb_id: "kb1"` — dragging its stale active KB along — reverting the server and localStorage to `kb1` while the pane still renders `kb2`. **Divergent, silent, and it reaches the server.**

Caveat, disclosed: `Object.keys(localStorage)` printed `["store"]` — an artifact of the `MemoryStorage` class in `src/test/setup.ts:29-30` (private `store` Map), not a finding. `getItem`/`setItem` are faithful, so experiments 2 and 3 stand.

## Verdict: **GO-WITH-CHANGES**

R7's instinct was right to flag this and wrong about the remedy — the fix is **not** `sessionId={null}` (alternative (b), which trades a clobber for two incompatible meanings of "active KB"). **Nesting is sound; the shared-key write path is not.** The clobber is a live latent bug in `syncSelection` that nesting *exposes* rather than *creates* — which is why fixing it belongs in Stage 0 alongside the `isEventForSession` work already landing, not in Stage 3. De-risked: R7 can come off the critical path once `setHiddenKbIds` is decoupled from `activeKbId`.