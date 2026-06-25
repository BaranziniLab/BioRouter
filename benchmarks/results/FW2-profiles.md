# FW2 — Cargo build profiles (strip release + release-dist + quick)

**Change:** root `Cargo.toml` now defines:
- `[profile.release]` → `strip = true` (every `cargo build --release`, the
  Justfile, and the whole `scripts/release.sh` pipeline get smaller binaries —
  no path/pipeline changes, no runtime change).
- `[profile.release-dist]` → inherits release + thin LTO + 16 codegen-units
  (opt-in max-optimized distribution profile).
- `[profile.quick]` → opt-level 1 + 256 codegen-units + incremental (opt-in fast
  compile for iterating on optimized builds).

Plus CLAUDE.md documents the three profiles (there was no `release-lto`
reference to "fix" — the analysis doc overstated that — so this adds accurate
profile docs instead).

## Binary size (release, measured)
| binary | baseline | FW2 (strip) | Δ vs baseline | Δ vs unstripped (FW1) |
|---|---:|---:|---:|---:|
| biorouterd | 123.7 MB | **107.6 MB** | **−13.0%** | −17.4% |
| biorouter  | 137.8 MB | **120.4 MB** | **−12.6%** | −12.9% |

(Confirmed independently by `strip`-ing a copy of the pre-strip binary: identical
result, since cargo's `strip=true` just runs the platform strip on the linked
binary.)

## Idle RSS / startup
- idle RSS: ~23.5 MB — unchanged vs FW1 (strip is file-size only, not RAM).
- startup: 125–140 ms steady; one 3283 ms outlier on the *first* exec of the
  freshly-written binary (macOS page-in + signature validation on first run), not
  a regression — subsequent runs are normal.

## Not built here
`release-dist` (thin LTO) was not compiled for this benchmark — LTO adds ~20 min
of link time and is opt-in. Expected to shave a further ~5–10% size + give a
small runtime gain on top of strip. Wire `just release-binary`/`scripts/release.sh`
to it only after a macOS notarized-packaging smoke test (flagged in CLAUDE.md).

## Verdict
**Clear, safe win.** −13% on both shipped binaries with zero runtime cost and no
release-pipeline rewiring (the default `release` profile is what the pipeline
already uses). `release-dist`/`quick` add opt-in headroom.
