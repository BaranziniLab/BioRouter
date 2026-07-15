# Diverge — comprehensive behavior checklist

A catalog of ~100 concrete user actions for the **Diverge** feature and the
behavior BioRouter must exhibit for each. Use it as a manual QA script and as
the spec the automated tests encode. Items marked **[T]** are covered by an
automated test; **[UI]** require driving the real app.

Core invariants (everything below is a consequence of these):

- **I1.** Diverge branches the conversation into a *new session* that inherits
  the full history; the **original session is never mutated**.
- **I2.** Diverge only ever *spins up a new chat surface*. It changes nothing
  else about the agent, the current window, or other conversations.
- **I3.** **Chat and Dashboard are isolated.** A diverge in the chat opens an
  Electron **window**; a diverge on the dashboard spawns an on-canvas **box**.
  Neither leaks into the other.
- **I4.** Closing a chat surface **never deletes a real conversation.** History
  is only removed for an empty throwaway spawn that was never used.

---

## A. Button presence & affordance

1. **[UI]** Finish an assistant reply in a chat → a **Diverge** action appears next to **Copy** on hover.
2. **[UI]** While the assistant is still streaming → Diverge is **not** shown.
3. **[UI]** On a message containing a tool call/result → Diverge is **not** shown (text-only messages only).
4. **[UI]** On a user message → no Diverge (only assistant messages).
5. **[T]** Button is disabled while a diverge is in flight (no double-trigger).
6. **[UI]** Hover tooltip wording differs in chat ("new window") vs dashboard ("new chat box").
7. **[UI]** Diverge button has an accessible label ("Diverge conversation into a new chat").
8. **[T]** Rapid triple-click triggers exactly **one** diverge.

## B. Diverge in a single chat window (Hub / Pair)

9. **[UI]** Diverge from a Pair window → a **second** Electron window opens.
10. **[UI]** The new window is **focused / in front**; the original stays where it was.
11. **[UI]** The new window is **offset** (~40px) from the original so both are visible.
12. **[UI]** The new window shows the **full prior history** of the original.
13. **[UI]** The original window is **unchanged** — same session id, same scroll, still interactive.
14. **[T]** In a chat view the code path calls `createChatWindow(..., newSessionId, 'pair')`, **not** dashboard spawn.
15. **[UI]** Continue typing in the new window → it appends to the **branch**, not the original.
16. **[UI]** Continue typing in the original window → unaffected by the branch.
17. **[UI]** Diverge twice from the same message → two independent new windows/sessions.
18. **[UI]** Close the new branch window → the original window and session remain intact.
19. **[UI]** Diverge from the Hub landing chat behaves the same as from Pair.
20. **[UI]** Model / mode / extensions in the new window match the branch's inherited config; the original's are untouched.

## C. Diverge via `/diverge` slash command (GUI)

21. **[UI]** Type `/diverge` in the chat input → it appears in the slash popover.
22. **[UI]** Select it (Enter) → inserts `/diverge`; Enter again → branches (does **not** send a chat message).
23. **[UI]** `/diverge` from a chat opens a new focused window (same as the button).
24. **[UI]** `/diverge` with leading/trailing spaces still triggers.
25. **[UI]** `/divergexyz` or `/diverge now` is treated as a normal message (no branch).
26. **[UI]** `/diverge` with no active session yet (empty Hub) is a no-op, input clears.
27. **[UI]** After `/diverge`, the input is cleared and the original conversation is unchanged.

## D. Diverge via CLI / TUI `/diverge`

28. **[T]** CLI parser maps `/diverge` → `InputResult::Diverge`; near-misses don't.
29. **[T]** `/diverge` is in the CLI completion registry.
30. **[T]** The deeplink is `biorouter://diverge?session_id=…&dir=…`, URL-encoded.
31. **[UI]** Running `/diverge` in the TUI prints "Diverged into a new window (session …)".
32. **[UI]** The TUI's own conversation is unchanged after `/diverge`.
33. **[UI]** The deeplink opens a **new, focused** desktop window with the branch (main.ts `diverge` handler).
34. **[UI]** If the desktop app can't be opened, the TUI still reports the created branch + the link.
35. **[T]** A branch session row is created in the DB with full history + lineage.
36. **[UI]** Classic CLI `/diverge` renders the success/"couldn't open" output.

## E. Diverge on the Dashboard canvas

37. **[UI]** Diverge from a canvas chat window → a **new box** appears on the canvas.
38. **[T]** On the canvas the code path calls `dashboard.spawnWindow({resumeSessionId})`, **not** `createChatWindow`.
39. **[UI]** The new box is focused; the original box stays exactly in place.
40. **[UI]** The new box shows the full inherited history.
41. **[UI]** The new box is positioned without overlapping existing boxes.
42. **[UI]** The original box keeps chatting independently of the branch.
43. **[UI]** No new **Electron window** opens for a canvas diverge.
44. **[UI]** Diverge several boxes → each is independent and closeable.

## F. Chat ⇄ Dashboard isolation (the core bug)

45. **[T]** Diverge in chat (provider present, **not** on canvas) → **no** dashboard window is spawned.
46. **[UI]** Diverge in chat, then open the Dashboard → the branch is **not** sitting there.
47. **[UI]** Spawn boxes on the dashboard, then go to chat → chat is normal, boxes untouched.
48. **[UI]** Switch chat ⇄ dashboard repeatedly → neither view gains/loses conversations.
49. **[UI]** The dashboard "+ / Spawn" button creates a **fresh** (non-diverged) chat.
50. **[UI]** A fresh spawned chat has no inherited history; a diverged box does.
51. **[UI]** Diverge on dashboard does not open or affect any standalone chat window.
52. **[UI]** Diverge in a standalone window does not add to the dashboard's persisted list.

## G. Closing surfaces & data preservation (the data-loss bug)

53. **[T]** Close a **diverged** dashboard box → `stopAgent` is called, `deleteSession` is **not**.
54. **[T]** Close a **resumed-from-history** box → history preserved (no delete).
55. **[T]** Close a **created-here, used** box → history preserved (stopAgent, no delete).
56. **[T]** Close a **created-here, empty** box → that throwaway session is deleted.
57. **[UI]** Close a diverged box, immediately switch to chat → **no** "cannot load" error.
58. **[UI]** Close a diverged box, reopen it from History → it still loads with full history.
59. **[UI]** Closing a standalone branch **window** never deletes its session.
60. **[UI]** Closing the *original* window/box never affects the branch, and vice-versa.
61. **[UI]** Rapidly close a box and navigate to chat 10× → never a load error.
62. **[T]** Diverged sessions are never even probed for emptiness on close.

## H. Clear-all

63. **[T]** `clearAll` removes all boxes from the canvas.
64. **[T]** `clearAll` preserves diverged/resumed conversations (stopAgent only, no delete).
65. **[UI]** After `clearAll`, those conversations are still in History.
66. **[UI]** After `clearAll`, the Spawn button still works (no zombie LRU lock).
67. **[UI]** `clearAll` then immediately Spawn → new fresh box appears.

## I. Naming & lineage

68. **[T]** Branch name = `"{parent} (branch 1)"`, `"(branch 2)"`, … (sibling-numbered).
69. **[T]** Diverging a branch flattens numbering (no `(branch 1) (branch 1)`).
70. **[T]** Placeholder parent name → branch name derived from the conversation.
71. **[T]** Branch records `diverged_from = parent id`.
72. **[T]** Branch token counts reset; the parent's are preserved.
73. **[UI]** History shows "⑂ branched from {parent}" on a branch; parent shows none.
74. **[T]** A custom name passed to diverge overrides the auto branch name.

## J. Edge cases & failure modes

75. **[T]** Diverge with no session id → error toast, nothing opens.
76. **[T]** Backend diverge fails → error toast, nothing opens, original intact.
77. **[T]** Diverge response missing session id → error toast, nothing opens.
78. **[T]** Nonexistent source session → backend 404.
79. **[T]** Invalid session id → backend 400.
80. **[T]** Name longer than 200 chars → backend 400.
81. **[UI]** Diverge an empty conversation → branch created (empty), original intact.
82. **[T]** Concurrent diverges from one session → unique branch ids, no collision.
83. **[UI]** Diverge while the agent is mid-stream is not offered (button hidden while streaming).
84. **[T]** LIKE-wildcard names (`100%_done`) don't break sibling counting.

## K. Persistence & reload

85. **[T]** `createdHere` persists across dashboard reload.
86. **[T]** A box whose session was deleted (404) is dropped on hydrate.
87. **[T]** A box is **kept** on hydrate if the backend errors transiently (no false drop).
88. **[UI]** Reload the app with diverged boxes on the canvas → they re-appear and load.
89. **[UI]** A branch window survives an app restart (session is in History).
90. **[T]** `isBusy` / preview are not persisted; `folded` is.

## L. "Changes nothing else" guarantees (I2)

91. **[UI]** Diverge does not change the current window's model, mode, or extensions.
92. **[UI]** Diverge does not interrupt a different conversation's in-flight stream.
93. **[UI]** Diverge does not alter scroll position or input contents of other surfaces.
94. **[UI]** Switching conversations in chat after a diverge behaves exactly as before.
95. **[UI]** Spawning/closing/folding other dashboard boxes is unaffected by a diverge.
96. **[UI]** Knowledge / workflow / scheduler views are unaffected by diverge.
97. **[UI]** Diverge works regardless of the active provider/model.
98. **[UI]** Diverge from a renamed session keeps the original's user-set name.
99. **[UI]** The original conversation can still be exported/diverged/edited after a diverge.
100. **[UI]** No extra biorouterd backends leak per diverge beyond the new window's own.

---

### Automated coverage map

| Area | Test file |
|------|-----------|
| Isolation (chat vs canvas) | `src/hooks/useDiverge.test.tsx`, `src/components/MessageDivergeLink.test.tsx` |
| Close/clear/hydrate lifecycle | `src/components/Dashboard/DashboardProvider.diverge.test.tsx` |
| Button affordance & failure modes | `src/components/MessageDivergeLink.test.tsx` |
| CLI parsing + deeplink | `crates/biorouter-cli` (`session::input::tests`, `session::tests`) |
| Backend naming/lineage/history | `crates/biorouter` (`session_manager::tests`), `crates/biorouter-server` (`diverge_tests`) |
