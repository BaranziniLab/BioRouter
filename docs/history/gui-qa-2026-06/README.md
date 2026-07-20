# GUI QA session, June 2026

This folder holds the two records produced by a single desktop-GUI QA session that ran on
**2026-06-24/25** against dev build **1.86.1**. It happened: the session drove the real
Electron app over the Chrome DevTools Protocol, verified roughly 40 rows live, and fixed
eight defects — session auto-naming (three separate causes), the Expand window's blocked
scripts, two CLI status-line errors, and the human-in-the-loop permission card. Those
fixes shipped. Everything here is a **historical record**, kept for provenance rather than
as current guidance: the current release is 1.88.3, neither document is a checklist to
re-run, and at least one row (`G3`, diverge on the Dashboard canvas) covers a feature that
has since been removed from the product entirely.

Come here to find out *why* a piece of the desktop UI is shaped the way it is — the root
cause behind a session-naming fix, why artifact windows get their own Content Security
Policy, what was and was not exercised in the week's 133 commits. Do not come here for how
the app behaves today. For that, leave for [`docs/desktop-ui/`](../../desktop-ui/README.md),
which holds the live expectations for the desktop app; for the Agent Drafter and brsdk
surfaces these documents defer to, see [`docs/apps-sdk/`](../../apps-sdk/README.md) and
[`docs/agent-drafter/`](../../agent-drafter/README.md). The sibling folders under
[`docs/history/`](../README.md) archive other completed campaigns; this one covers only
these two June 2026 days.

> **Note on identifiers.** Both files key their items by a section letter plus a number
> (`A1`, `K6`), and the two schemes are **independent** — a letter means different things
> in each file. Where one document cites the other's key, it says so explicitly.

## Documents

| Document | What it covers |
|---|---|
| [GUI debug session issue tracker](debug-session-issue-tracker.md) | The item-by-item tracker for the session: every UI edit, bug and feature test requested on 2026-06-24/25, the status each reached, and the files changed for each fix. Groups A and B completed, group C completed except `C3`, group D still in progress when the session closed. |
| [Week-of-2026-06-24 commit GUI regression pass](week-commit-regression-pass.md) | The companion regression matrix over the 133 commits of the eight days ending 2026-06-24, driven through the Electron app via agent-browser on CDP port 9222. Found and fixed one regression (`B5`, session auto-naming), verified about 40 rows live, and left the rest not run. |

Read the tracker for fixes and their root causes; read the regression pass for coverage.
The two were written side by side during the same two days and cross-reference each other.

## Related documentation

- [Historical records index](../README.md) — the archive this folder sits in, and how to
  read a `Status:` line to check any document's standing.
- [Dashboard mode removal record](../dashboard-mode/README.md) — why row `G3` of the
  regression pass no longer describes a shippable feature.
- [Agent-browser debugging](../../desktop-ui/agent-browser-debugging.md) — the CDP-attach
  workflow used to drive every GUI row recorded here.
- [Diverge behaviour checklist](../../desktop-ui/diverge-behavior-checklist.md) — the
  current expectations for the diverge feature these documents tested as `G1`.
