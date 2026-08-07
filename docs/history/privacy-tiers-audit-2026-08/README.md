# Privacy tiers audit — August 2026

This folder holds the record of the six-surface privacy audit run against branch
`feat/privacy-tiers` on **2026-08-06**, and its complete synthesis, re-run on **2026-08-07**. The
audit happened and finished; the campaign it feeds — issue #56, the privacy-tier boundary between a
chat's classification and the model bound to it — was still in flight on that branch when the
synthesis was written, so this folder records a review, not a shipped feature. For what the boundary
is *supposed* to do, and for its current state, go to [`docs/security/privacy-tiers.md`](../../security/privacy-tiers.md)
and its execution plan; this folder answers only "what did the audit find, and what was true of the
tree when it was checked."

The audit ran as six parallel agents over six surfaces: data-flow paths into a model's context;
subagent spawn and the Workspace Control extension; extension calls and the four gates; session-store
writes and every path out of the store; the desktop renderer; and the innocent-mistake question —
how a well-meaning researcher leaks PHI while trying to do their job. **The first synthesis received
four of the six payloads**: the renderer audit was truncated mid-finding, and the renderer gap list
and the innocent-mistake ranking never arrived. That report said so in its own opening paragraph and
is incomplete by its own account. The document here is the re-run with all six, and it re-checks
every finding against the tree at commit `bbcfdb06` rather than merging the reports as written —
which mattered, because twenty-one commits landed in between and sixteen findings had already been
closed — among them every one of the earlier synthesis's top four, and the single item it said it
would block a release on.

> **Identifier scheme.** The synthesis uses `C-nn` for a finding closed since the audit ran, `O-nn`
> for one still open, and `N-nn` for one produced by a fix rather than found by the audit. Gate
> letters, `DR-nn` decision records and `§n` section numbers belong to the campaign's own scheme and
> are defined in [`docs/security/privacy-tiers.md`](../../security/privacy-tiers.md).

## Documents

| Document | What it covers |
| --- | --- |
| [Six-surface privacy audit — the complete synthesis](six-surface-audit-synthesis.md) | The full cross-surface report: the innocent-mistake ranking the operator asked for (twelve paths, ordered by likelihood × harm), the desktop-renderer gap list folded in, the sixteen findings already closed with the mechanism that closed each, the thirteen still open in the daemon and ten in the renderer, three new findings produced by the fixes themselves, and seven decisions only the operator can settle. |

## What the audit concluded

The machinery is sound and the gaps are entry points, not predicates. The classification ratchet is
monotone in SQL rather than in its callers; the four extension gates share one prefix resolver and
one predicate rather than four spellings; the capability is sampled once and carried; the spawn path
refuses in both directions above any approval machinery. Where the audit found problems, they were
overwhelmingly of two shapes: **a guard with no caller**, and **a second door built next to the one
that was locked**. Both shapes recur in the fixes — see `N-02`, where the fix for an ungated export
route introduced a second export decision that only one of two doors calls.

The single largest remaining path is not a defect at all. R11(ii) rules that anything not published
on the BAAM marketplace is Public, so PHI arriving through an ordinary file read, a lab-built MCP
server or an internal portal never classifies the chat, and the tier never ratchets. That ruling,
not any of the open findings, is the answer to the question the audit was commissioned to ask.

## Related documentation

- [History archive index](../README.md) — the full archive this campaign sits in.
- [`docs/security/README.md`](../../security/README.md) — the living security folder, including the
  privacy-tiers design, its execution plan and decision records, and the user-facing PHI guidance.
- [`docs/history/workspace-control/README.md`](../workspace-control/README.md) — the BR-71 workspace
  control record; four of this audit's open findings are about those tools.
