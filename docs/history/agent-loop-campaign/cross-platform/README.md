# Cross-platform work in the agent-loop fix campaign

This folder is the archived cross-platform ("xplat") arm of the 2026-07 agent-loop fix
campaign. **It happened, and it shipped:** an audit of the campaign's own diff found that
its security, process and checkpoint work was POSIX-only in substance, three remediating
designs were written, and the resulting Wave 3 cluster merged Gate 3 GREEN on 2026-07-13.
The code it produced is on `main` today —
`crates/biorouter/src/security/policy/{target,pwsh,cmd_shell}.rs`, the four
`baseline.*.policy.yaml` rule sets, the `biorouter-sandbox` crate's
`shell_sandbox/` backends, `scripts/cross-env.sh` and `.github/workflows/rust.yml`.
It still matters for two reasons: three lower-ranked `GAP` findings were never remediated,
and Windows containment is the weakest part of the sandbox story by the campaign's own
account.

Read this folder to learn *why* a rule, sandbox tier or CI recipe is shaped the way it is,
and what BioRouter's agent loop does — and does not — enforce on Windows and Linux. Do not
read it for the current plan of record on sandboxing: BR-69 remains partly unbuilt, so its
design stayed live in
[`docs/agent-loop/designs/linux-and-windows-sandboxing.md`](../../../agent-loop/designs/linux-and-windows-sandboxing.md)
rather than being archived here. The rest of the campaign — its wave conventions, merge log
and outcome report — is in [the parent campaign folder](../README.md).

> **Identifier key.** Two schemes run through every file. **BR-NN** is a proposal from the
> agent-loop review's master list, defined in
> [improvement proposals](../../agent-loop-review/improvement-proposals.md) for `BR-1`…`BR-67`
> and in [the platform parity audit](platform-parity-audit.md) below for `BR-68`, `BR-69` and
> `BR-70`. **GAP-N** is a per-platform degradation finding; that same audit coined and
> numbered them, and is the canonical key other campaign documents cite without redefining.

## Documents

| Document | What it covers |
|---|---|
| [Platform parity audit](platform-parity-audit.md) | A per-file Windows/Linux/macOS audit of the campaign's diff, classifying each feature OK/GAP/BREAK, ranking eight GAP findings by user impact, and specifying the commands that would verify each platform. It coined the `GAP-N` identifiers and defined BR-68/69/70. Historical record — the top four findings were remediated in Wave 3, while GAP-6, GAP-7 and GAP-8 appear to remain open, so the tail still has reference value. |
| [Cross-platform command safety (BR-68)](command-safety.md) | The design that made the catastrophic denylist and the command policy engine work off POSIX: platform × dialect applicability on every rule, PowerShell alias and abbreviation normalization, dialect-aware tokenizers, and per-platform baseline rule sets. Historical record — shipped in full as commit `651acff0` in Wave 3; now the rationale record for shipped code, not a live plan. |
| [Cross-platform CI verification gate (BR-70)](ci-gate.md) | The design for the project's first Rust CI: one shared cross-compile recipe sourced by the release pipeline, the `Justfile` and CI alike, plus drift and glibc-floor guards, so the Windows and Linux `cfg` surface is compiled on every pull request. Historical record — implemented as commit `ab721780` and verified in the Wave 3 pass. |
| [macOS Seatbelt sandbox for the shell tool (BR-64)](macos-seatbelt-sandbox.md) | BioRouter's first kernel-enforced containment of the developer shell tool — a Seatbelt profile with injected writable roots and outbound network denied, kept deliberately separate from the approval policy. Superseded — Slice 1 shipped as `crates/biorouter-sandbox/src/seatbelt.rs`, but BR-69 replaced its forward-looking phasing wholesale. Read it for the profile design and the two-axis model only. |
| [Cross-platform cluster verification report](parity-verification-report.md) | The gate record for the xplat cluster: the four proposals it contains, the regression the verifier had to fix before the cluster could pass, and the evidence collected for each gate step. Historical record — a one-time pass dated 2026-07-13, verdict GREEN, with the caveat that BR-69's Linux and Windows sandbox arms were type-checked rather than built. |

## Related documentation

- [OS-level tool sandboxing on Linux and Windows (BR-69)](../../../agent-loop/designs/linux-and-windows-sandboxing.md) — the fourth design of this cluster, still a live plan of record because real Windows containment was never built, and therefore filed with the current agent-loop designs rather than here.
- [Agent-loop fix campaign](../README.md) — the parent record: wave conventions, the regression-gate rule, the dated merge log, and the outcome report this cluster closed into.
- [Agent-loop review](../../agent-loop-review/README.md) — the point-in-time diagnosis where the `BR-NN` improvement proposals cited throughout this folder are defined and indexed.
- [Security documentation](../../../security/README.md) — the user- and admin-facing view of the permission modes and managed policy tier these designs enforce.
