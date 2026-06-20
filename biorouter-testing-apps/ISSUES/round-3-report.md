# BioRouter QA — Round 3 Issues Report (apps 11–15)

Apps 11–15 built/verified on the **improved CLI** (rounds 1–2 live: `file_path`
alias + deeper 429 retry), spanning the round-3 source improvements (git context,
verify hook, `--resume` fallback, readable paths, quantified turn-limit) and the
move of the QA suite into the BioRouter repo.

## Outcome

| # | App | Lang | Tests (independently verified) | Turns | Note |
|---|-----|------|-------------------------------|-------|------|
| 11 | seq-alignment | Python | 110 pass | build+fix | affine-gap KeyError fixed |
| 12 | fasta/fastq-toolkit | Rust | 68 pass | 1-shot | clean |
| 13 | phylo-tree-builder | Python | 156 pass | 1-shot | clean-checkout green out of box |
| 14 | variant-caller | Python | 124 pass | 1-shot (after keychain blip) | clean |
| 15 | kmer-counter | **C++** | **82/82 first try** | **1-shot** | **first clean C++ — no fix turn** |

All five green. **Round 3 is the strongest batch so far** (4 of 5 one-shot).

## Findings

**Positive trend — C++ verification discipline improved (notable).** Apps 3, 6, 9
(rounds 1–2) all shipped broken cmake / red tests and needed 4–5 fix turns. App 15
(round 3) built clean and passed 82/82 on the **first try**, with **7 logical
commits**. Likely contributors: (a) the **git-context** improvement (commit policy
visibly took — 7 commits vs the earlier 1), (b) the spec's explicit "keep
CMakeLists in sync and RUN cmake yourself" emphasis. Not yet conclusive (n=1 clean
C++), but the direction is right and worth continuing to watch.

**New gotcha — keychain/keyring transient failure (dev-workflow).** Apps 14 & 15
first failed instantly with `Configuration value not found: XIAOMI_MIMO_API_KEY`:
macOS locks the keychain after inactivity, and a mid-loop `cargo build` (ad-hoc
signature) can invalidate the "Always Allow" grant; a headless read then aborts
the build at turn 0 with no prompt to answer. Recovered on its own once the
keychain was accessible; re-running succeeded. Recommendations: re-sign with the
stable Developer ID after rebuilds (`just sign-dev-binaries debug`), and/or set
`XIAOMI_MIMO_API_KEY` via env for long unattended runs. (Logged in FAILURE_LOG.)

**Reproducibility improving.** Every Python app this round (11/13/14) passed
`pytest` from a **clean venv with no editable install** — the round-1/2 src-layout
breakage did not recur, consistent with the git/reproducibility nudges.

## Improvements this round
Already shipped as the round-3 source batch (see `IMPROVEMENTS.md`): git Plan A
(context) + Plan B (verify/checkpoint Stop hook) + FINAL_REPORT §4 items
(`--resume` fallback, readable paths, quantified turn-limit). No new code change
required this checkpoint — instead, **observing whether the shipped changes move
the metrics**, and the C++ one-shot at app 15 is the first evidence they do.
