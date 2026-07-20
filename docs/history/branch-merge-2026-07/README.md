# Branch and pull-request merge campaign, July 2026

This folder holds the execution record of the July 2026 branch and pull-request merge campaign for `BaranziniLab/biorouter`. It happened, and it finished: nine pull requests were examined and all nine merged, the campaign completed on 2026-07-13, and execution evidence was captured through 2026-07-14. Three of the nine carried live feature work — #12 (Apps SDK v2), #13 (usage reporting) and #11 (the agent-loop overhaul) — and were integrated in that order so the high-overlap branch landed last. The other six were already on `main` at the audit snapshot and are recorded as the historical baseline. Everything described here shipped; the folder is kept for the record and for provenance, not as current guidance.

Come here when you are auditing what merged and why — the decision register (`D1`–`D10`), the conflict resolutions, the CI qualification evidence, or the commit-level inventory and merge anchors that let you reconstruct the repository's state at the end of the campaign. Do not come here for current behaviour of anything the campaign merged: [`docs/apps-sdk/`](../../apps-sdk/README.md) owns the live Apps SDK contract, and [`docs/agent-loop/`](../../agent-loop/README.md) owns the agent loop that PR #11 rebuilt. One detail in the record has already been overtaken by the tree: the four post-merge items it leaves open (process-tree hardening, D6 browser coverage, release-tag provenance, and GitHub issue #14) reflect the position as of 2026-07-14, not today's.

## Documents

| Document | What it covers |
|---|---|
| [Merge execution plan and record](merge-execution-plan.md) | The decisions taken, the conflicts resolved, the qualification evidence gathered, and the commit-level inventory of everything that landed in the campaign. |

## Related documentation

- [Agent-loop fix campaign](../agent-loop-campaign/README.md) — the 70-item `BR-`numbered campaign whose commits became PR #11, the largest branch merged here.
- [Apps SDK RFC, June 2026](../apps-sdk-rfc-2026-06/README.md) — the RFC behind PR #12, for why the merged SDK design looks the way it does.
- [Historical records](../README.md) — the rest of BioRouter's archive, and which of it has since been superseded or removed.
- [Releases](../../releases/README.md) — how the merged work reaches users, including the release notes that follow this campaign.
