# BioRouter QA — Round 4 Issues Report (apps 16–20)

The bioinformatics batch (apps 11–20) closes here. Apps 16–20 add the **R**
toolchain and stress-test the loop's resilience to environmental disruptions.

## Outcome

| # | App | Lang | Tests (independently verified) | Note |
|---|-----|------|-------------------------------|------|
| 16 | gene-expression | **R** | 67 pass | first R app; idiomatic package; 1 fix turn |
| 17 | protein-structure | Python | 1775 LOC, **no tests** | code complete; agent never produced a test suite (2 turns) — partial |
| 18 | blast-lite | Rust | 60 pass | seed-extend BLAST; 1 integration fix turn |
| 19 | genome-assembly | Python | 70 pass | OLC + de Bruijn; clean after binary-rebuild |
| 20 | motif-finder | Python | 97 pass | Gibbs/MEME-lite; 1 CLI fix turn |

4 of 5 fully green; app 17 the lone partial. Cumulative: **~21 apps attempted,
~1,930 passing tests across Rust / Python / C++ / R.**

## Findings

**R is well-supported (positive, important — validates the analyzer addition).**
App 16: MiMo produced a correct R *package* (DESCRIPTION / NAMESPACE / R/ modules /
tests/testthat), and **ran `Rscript`/testthat ~94×** during the build — the same
self-verification discipline it shows for cargo/pytest. Only 2 testthat cases were
off (filtering threshold, a statistics calc), fixed in one turn → 67 green. Good
news given R was newly added to the `analyze` tool.

**Resilience to environmental disruption (positive).** Two infra failures hit
mid-batch and the loop recovered both:
- *Keychain/keyring* (apps 14, 15 first attempt): macOS locked the keychain / a
  rebuild's ad-hoc signature invalidated the "Always Allow" grant → headless read
  failed at turn 0. Recovered by re-running once accessible.
- *CLI binary deleted* (apps 19, 20 first attempt): `target/debug/biorouter`
  vanished mid-loop (concurrent `cargo clean`/build in the shared workspace) →
  empty logs, 0 files. Recovered by rebuild + re-sign + re-run.
  → **Recommendation for long unattended runs: pin a stable *installed* CLI and set
  `XIAOMI_MIMO_API_KEY` via env, rather than driving a dev-target symlink that
  shared workspace activity can clean/relink.**

**Premature stream stop (reliability).** App 17 ended mid-sentence ("Now let me
create the core PDB parser module:") with rc=0 and no error — a clean-looking
truncation indistinguishable from completion. Resumable, but reinforces the C2
"no done-vs-stopped signal" gap.

**Interactive fix didn't always converge (app 17).** Two explicit "write the
pytest suite" turns produced only `tests/__init__.py`, never real tests. A rare
miss for the otherwise-reliable precise-failure→repair pattern — accepted as a
documented partial rather than burning more turns.

## Improvements
No new source change this checkpoint — the round-3 batch (git context + verify
hook + `--resume` fallback + readable paths + quantified turn-limit) is doing its
job: C1 confirmed live in real output (`path: ~/…/project/src/...`), Python apps
pass clean-checkout pytest consistently, and the first clean C++ one-shot (app 15)
plus diligent R verification (app 16) suggest the git/reproducibility nudges land.
The standing higher-effort item — a deterministic C++/cmake build-verify the agent
is *forced* through — remains the best next investment (the verify-and-checkpoint
Stop hook already provides an opt-in version).
