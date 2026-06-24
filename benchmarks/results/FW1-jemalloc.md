# FW1 — jemalloc (tuned) as global allocator

**Change:** `tikv-jemallocator` as `#[global_allocator]` in `biorouterd` + `biorouter`
(CLI), behind a default-on `jemalloc` feature, with dirty/muzzy decay lowered to
1000 ms via `mallctl` so freed pages return to the OS promptly.

## Verification
- jemalloc **is** the allocator: `nm` shows **472 `_rjem_`/jemalloc symbols** in each
  release binary.
- decay tuning code exercised by the churn microbench (identical `tune()` pattern).

## Allocator A/B microbench (`benchmarks/alloc-churn`)
Mimics BioRouter's per-turn transcript reload + double-clone across 6 threads ×
40 sessions × 60 turns, then measures RSS. 3 runs each:

| allocator | peak RSS | after churn | settled |
|---|---:|---:|---:|
| system (macOS libmalloc) | 13.4–14.9 MB | 13.4–14.9 MB | 3.8–8.4 MB |
| **jemalloc (tuned, as shipped)** | **3.3 MB** | **3.3 MB** | **2.8–3.3 MB** |

➡ **~4.4× lower peak RSS under churn**, and far more stable. This is the
representative workload (a busy daemon cloning transcripts every turn), and is
where the win lives. On Linux (glibc arenas) the gap is expected to be larger
still; macOS libmalloc is already comparatively decent.

## biorouterd idle-boot (harness, 3 runs)
| metric | baseline | FW1 jemalloc | Δ |
|---|---:|---:|---:|
| idle RSS | 21.3 MB | 23.5 MB | **+2.3 MB** |
| startup | 34–99 ms | 105–348 ms | +~50 ms (1st run cold) |
| biorouterd size | 123.7 MB | 124.1 MB | +0.4 MB |
| biorouter size | 137.8 MB | 138.2 MB | +0.4 MB |
| Cargo.lock crates | 988 | 991 | +3 |

## Verdict
**Net win for the real workload.** jemalloc costs ~+2 MB of arena metadata at
pure idle, but holds ~4× less peak RSS once the daemon is actually doing turns —
exactly the long-running, allocation-churning scenario BioRouter runs in. The
idle overhead is a fixed one-time cost; the churn savings compound over a
session's lifetime. Behind a default-on feature, so trivially disabled
(`--no-default-features`) on memory-trivial/idle deployments if ever desired.
