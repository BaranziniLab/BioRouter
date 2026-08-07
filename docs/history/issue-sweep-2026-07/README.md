# Issue sweep 2026-07

> **What this is.** The record of the July 2026 campaign that closed every open GitHub issue in one
> pass — the plan of record, the in-app visual verification, and the concurrency stress test run
> against the result.
> **Status:** Finished. Every issue in the range was fixed and closed except workspace control
> ([#30](https://github.com/BaranziniLab/biorouter/issues/30)), which was plan-only at the time and
> landed later as its own campaign.
> **Audience:** Anyone tracing why a fix landed, or looking for the harness and method used to verify
> a broad sweep.

This folder was written during the sweep and indexed on 2026-08-03, which is why it may be referenced
from commit messages before it appears here.

## What is in here

| Document | What it holds |
|---|---|
| [Plan of record](plan.md) | The execution plan for closing issues #18–#43: validated root causes, batch composition, ordering, worktree strategy, test gates and review gates. |
| [GUI vision pass](gui-vision-pass.md) | The in-app visual verification of the sweep's UI-visible fixes, driven over CDP against a dev GUI built from merged `main`, with a sandboxed `XDG_CONFIG_HOME`. |
| [Parallel stress test](stress-test.md) | The post-sweep concurrency test: parallel headless `biorouter run` fleets on both UCSF Versa providers, deliberately maximising shared-session-store contention with the desktop GUI. |
| `stress-harness.py` | The harness that drove the stress test. Kept beside its report so the numbers can be reproduced. |

## Two things worth carrying forward

**The visual pass found what the test suite could not.** jsdom applies no Tailwind, so a whole class
of defect — a token collision that shreds a layout while every assertion passes — is invisible to the
unit suite. Driving the real GUI over CDP is what caught those, and it is why later campaigns budget
for a vision pass rather than treating a green suite as done.

**The stress harness leaked its workers.** A later session found 24 orphaned busy-loops from this
harness still running after four hours, at a load average of 500. If you re-run it, check for
orphans afterwards by looking for processes whose parent is `1` with high `%CPU`.

## Related documentation

- [Historical records index](../README.md) — the archive this folder belongs to.
- [Workspace control defect notes](../workspace-control/README.md) — the campaign that finished #30,
  the one issue this sweep deliberately left as a plan.
- [Documentation organization](../../organization.md) — where a document goes, and why finished work
  lives here rather than at the top level.
