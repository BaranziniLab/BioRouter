# Multi-KB verification record

> **What this is.** The evidence behind [issue #45](https://github.com/BaranziniLab/biorouter/issues/45) —
> what was tested, what was found, and what was decided — for the change that made a
> session's knowledge bases a set with an explicit primary.
> **Status:** Current.
> **Audience:** anyone auditing the multi-KB change, or debugging knowledge-base selection.

The design and the task-by-task plan live in
[multi-kb-implementation-plan.md](multi-kb-implementation-plan.md). This page records
verification: the review that found sixteen defects the implementation had not, the GUI
passes that reproduced two of them in a running app, and the one design conflict that
went to the operator.

## What shipped

A session's **visible set** of knowledge bases *is* its working set — there is no separate
"active" collection. Alongside it sits an explicit optional **primary**: the target for
KB-less writes, the base a single-base read resolves to, and the subject of the Knowledge
view. Two invariants hold everywhere:

- The primary is always a member of the resulting set.
- The primary is never **invented** for a scope that has none. No auto-promote on create,
  no promotion of a sole visible base, no promotion from a pointer at a deleted base.

The second invariant is narrower than it first sounds, and the distinction cost a round
trip to settle — see [The one design conflict](#the-one-design-conflict) below.

Storage keys are unchanged (`.active-kb`, `.active-kb-sessions/<sha256-digest>`), so
today's stored value *is* the primary and the legacy read is the migration. The wire adds
`primary_kb` and `kb_ids` and keeps `active_kb` as a deprecated mirror.

## Adversarial review found what the implementation did not

The implementation landed 25 commits with every gate green. An adversarial review by
Codex (GPT-5.4) then found **sixteen defects across six of seven areas**, all real. That
gap is the point of recording this: a green suite meant the code did what its author
believed, not that the belief was right.

Only **session isolation** passed clean — no equivalent of the deleted process-global
cache remained, per-session SHA-256 paths were sound, and writers were serialized by the
knowledge-root lock.

The defects that mattered:

| Area | Defect |
| --- | --- |
| Never-invented | `repair_primary_unlocked` treated "base was deleted" identically to "base was hidden" and promoted a replacement. An upgrade whose `.active-kb` named a deleted base read as no-primary, then silently acquired one on the next unrelated edit. |
| Never-invented | The workflow path promoted a sole visible base when `default` was absent; the CLI auto-promoted a newly created base. |
| Atomicity | `set_selection` wrote the hidden set **before** validating the primary, so a rejected request left the stored pointer outside the resulting set — and the hole was reachable over HTTP, where an error response concealed a successful mutation. |
| Coherence | `selection()` performed three separately-unlocked reads and could return a primary absent from its own `kb_ids`. |
| Concurrency | Four callers did read-modify-write outside the root lock. Measured: four threads each hiding a distinct base lost at least one hide in **40 of 40** rounds. |
| Correctness | Worker app grants rested on a comment claiming workers share the main session. They do not — `build_worker` always mints its own — and the regression test fabricated a shared session, so it never exercised production topology. |
| Renderer | Five sync defects, including a prune effect that erased the hidden set whenever a base-list fetch failed. |

Each fix is its own commit. The service layer gained a private tri-state
`StoredPrimary { Inherit | Pinned | NoPrimary }` — an absent session file means inherit
the machine preference, an absent machine file resolves to Soul, a bare id means pinned,
and a **blank** file means explicitly no primary — because the previous
two-state encoding could not distinguish "never chose" from "chose nothing", and clearing
a session's primary therefore handed it the machine default straight back.

Three root-locked membership operations (`hide_kb`, `include_kb`, `set_visible_kbs`) were
added so the racy read-modify-write shape is no longer expressible by a caller.

## Verified in a running app, twice

Static review is not enough for a feature whose whole surface is a selection UI. Two
passes drove the real Electron app over CDP against a sandboxed config.

**First pass** (commit `f181ab13`) — 4/4 core behaviours passed, and it independently
**reproduced two of the review's findings live**, which upgraded them from analysis to
observed defects:

- A filtered bulk toggle applied one of three changes, because the callback derived every
  update from one captured value. As a side effect it silently moved the user's write
  target, since the racing writes momentarily orphaned the primary and the daemon
  correctly repaired.
- A forced-failure POST left the UI claiming a base was excluded while the daemon still
  had it in the chat.

**Final pass** (commit `4173a311`) — every check passed, both defects gone, and the
operator's ruling confirmed in both directions.

| Check | Result |
| --- | --- |
| Two-state row, one switch per row, no third toggle | Verified at 4, 4, and 5 rows, and while the new strip is showing |
| Exactly one primary badge, and it moves | Always exactly one, across six transitions |
| Hiding the primary promotes — pinned **and** inherited | Byte-identical payloads |
| Hiding **all** bases | Clears rather than invents |
| Primary at a deleted base | Cleared, with four other bases still in the set |
| Chat chip tracks the set | 4 → 1 after a bulk hide, matching `kb_ids` |
| Trigger overflow count at 0 primary | `No primary knowledge base +3` / `+4` — the off-by-one stays fixed |
| Ingest on Versa GPT-5.5 | 16 pages, 146 links; **every** `LLM_REQUEST` in the daemon log was `gpt-5.5-2026-04-24`, zero Anthropic-shaped requests |

## The one design conflict

The integrator found that the same gesture produced different outcomes depending on
invisible state: a chat that had **pinned** its primary and then hid it was promoted to
another member, while a chat that had merely **inherited** its primary and hid it was left
with none. Same visible starting state, same click, two results — and the inheriting case
is the common one, since most chats never pin.

Two documents genuinely disagreed. The implementation contract said the primary is never
invented; the plan-of-record said hiding promotes and deleting clears. The integrator
declined to pick a side and escalated.

**The operator ruled: always promote.** `repair_decision` now resolves the *effective*
pointer through the session-to-machine fallback and decides against that, writing the
result at the scope's own file. Both cases now agree.

The two rules were never actually in conflict; the wording was. They are now stated apart:

- **Never invent a primary for a scope that has none** — no auto-promote on create, no
  sole-visible-base promotion, no promotion from a dangling pointer.
- **A primary the user already had, whose base they just removed from the chat, moves** to
  another member.

One consequence needed its own fix. `delete_base` writes a durable explicit-no-primary
override into every session that pointed at the deleted base — correct, because a chat
must not be silently handed the machine pointer after a destructive act. But that made the
state a **one-way door**: nothing exposed `PrimaryUpdate::Inherit`, so such a chat could
never follow the machine default again. The escape hatch now exists at all three layers —
`inherit_primary` on the HTTP body, `knowledge active --session <id> --inherit` on the
CLI, and a "Follow the default" strip above the palette in the desktop app.

That strip is deliberately **not** a third control on the KB row. The row is two-state by
decision, and its rejected alternative was precisely a per-row primary control; a per-row
inherit would have reopened it. A test pins the row's switch count while the strip shows.

## Deliberately not fixed

- **Cross-session addressing** — any authenticated caller can read or overwrite any
  session's selection by supplying its id. Filed as
  [#47](https://github.com/BaranziniLab/biorouter/issues/47) rather than fixed here: the
  caller-supplied `session_id` predates this change, auth is a single daemon-wide secret
  with no principal to bind a session to, and `POST /reply` already accepts any
  `session_id` and runs a tool-enabled turn in it — strictly more exposure. A real fix is
  a breaking auth migration across the desktop client, CLI, exported apps and the
  generated client.
- **`primary_source` on the response** — a chat that pinned the same base the default
  names is indistinguishable from one that inherits it, so the "follow the default" offer
  does not appear there. Inheriting is a no-op in that state; the offer appears as soon as
  the default moves. Closing it properly needs a new response field.
- **`--session` for every CLI knowledge command** — only `knowledge active` is
  session-aware. A `--kb`-less CLI ingest still names the machine-wide primary.

## Reproducing the verification

```bash
cargo test -p biorouter-mcp --lib knowledge::                  # 188
cargo test -p biorouter-server --test knowledge_routes         # 38
cargo test -p biorouter-cli                                    # 253
cargo test -p biorouter --lib knowledge:: --lib agents::knowledge_tool
cd ui/desktop && npm run test:run                              # 1650
```

`cargo test --workspace` aborts on a pre-existing environmental failure —
`providers::test_anthropic_provider` calls the live Anthropic API and fails on billing.
Use `--no-fail-fast`; that one failure is expected and touches no knowledge code.
`SessionListView.test.tsx` fails in the full frontend run and also fails on a clean tree.

The GUI passes used a sandboxed `XDG_CONFIG_HOME` and the procedure in
[launching the dev GUI](../desktop-ui/launching-the-dev-gui.md) — which gained a fifth
failure mode as a result of the first pass. One caveat: the launcher sandboxes
`XDG_CONFIG_HOME` but not `XDG_DATA_HOME`, so a dev daemon shares the real
`sessions.db`. Sandbox both if a pass must leave no trace.

## Related documentation

- [Multi-KB implementation plan](multi-kb-implementation-plan.md) — the design decisions and the 24 tasks
- [Knowledge base README](README.md) — the feature as a whole
- [Launching the dev GUI](../desktop-ui/launching-the-dev-gui.md) — the procedure both passes used
