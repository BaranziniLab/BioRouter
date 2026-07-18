# Cross-platform work in the agent-loop fix campaign

This folder holds the cross-platform ("xplat") arm of the agent-loop fix campaign: the
audit that found the agent loop's security, process and checkpoint work was POSIX-only in
substance, the three designs that remediated it (command safety, OS-level sandboxing, and
a CI gate that actually compiles the Windows and Linux `cfg` arms), and the verification
report that gated the resulting cluster. Come here when you need to know what BioRouter's
agent loop does — and does not — enforce on Windows and Linux, or why a rule, sandbox tier
or CI recipe is shaped the way it is.

Two identifier schemes run through every file. **BR-NN** is a proposal from the agent-loop
review's master list, defined in
[improvement proposals](../../history/agent-loop-review/improvement-proposals.md).
**GAP-N** is a per-platform degradation finding coined by
[the platform parity audit](platform-parity-audit.md) in this folder, which is the
canonical key for those numbers. The boundary with neighbouring folders: the sibling
designs folder holds the campaign's platform-agnostic subsystem designs — the
[command policy engine](../designs/command-policy-engine.md) and the
[macOS Seatbelt sandbox](../designs/macos-seatbelt-sandbox.md) these documents extend — and
[the campaign history folder](../../history/agent-loop-campaign/README.md) holds the
campaign's wave reports and outcome record. If you want the *original* macOS-only sandbox
or policy-engine design rather than its cross-platform generalization, you are in the
wrong folder.

## Documents

| Document | What it covers |
|---|---|
| [Platform parity audit](platform-parity-audit.md) | A per-file Windows/Linux/macOS audit of the campaign's diff, classifying each feature OK/GAP/BREAK and ranking eight GAP findings by user impact; it coined the `GAP-N` identifiers. Historical record — the top four findings were remediated in Wave 3, while GAP-6, GAP-7 and GAP-8 appear to remain open. |
| [Cross-platform command safety (BR-68)](command-safety.md) | The design that made the catastrophic denylist and command policy engine work off POSIX: platform × dialect applicability on every rule, PowerShell alias normalization, dialect-aware tokenizers, and per-platform baseline rule sets. Historical record — shipped in full in Wave 3; now the rationale record for shipped code, not a live plan. |
| [OS-level tool sandboxing on Linux and Windows (BR-69)](linux-and-windows-sandboxing.md) | The design generalizing the macOS-only Seatbelt sandbox into one `ShellSandbox` trait with three backends — Landlock + seccomp on Linux with a bubblewrap fallback, and an honest "no containment" tier on Windows — plus the capability reporting that tells a user which tier they got. Current and partly implemented: Slice 3 (real Windows containment) and CI enforcement remain planned. |
| [Cross-platform CI verification gate (BR-70)](ci-gate.md) | The design for the project's first Rust CI: one shared cross-compile recipe sourced by the release pipeline, `Justfile` and CI alike, plus drift and glibc-floor guards, so the Windows and Linux `cfg` surface compiles on every pull request. Historical record — implemented and verified in Wave 3. |
| [Cross-platform cluster verification report](parity-verification-report.md) | The gate record for the xplat cluster: the four proposals it contains, the regression the verifier had to fix, and the evidence for each gate step. Historical record — a one-time pass dated 2026-07-13; verdict GREEN, with the caveat that BR-69's Linux and Windows sandbox arms were type-checked, not built. |

## Related documentation

- [macOS Seatbelt sandbox design](../designs/macos-seatbelt-sandbox.md) — the BR-64
  macOS-only sandbox that BR-69 generalizes into a three-backend trait.
- [Agent-loop review](../../history/agent-loop-review/README.md) — where the `BR-NN`
  improvement proposals cited throughout this folder are defined and indexed.
- [Agent-loop fix campaign](../../history/agent-loop-campaign/README.md) — the wave
  reports and outcome record for the campaign this cluster was part of.
- [Security documentation](../../security/README.md) — the user-facing view of permission
  modes and the managed policy tier these designs enforce.
