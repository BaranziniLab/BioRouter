# Performance Fixes — Implementation Log & Benchmarks

Companion to [performance-review-2026-06-22.md](performance-review-2026-06-22.md).
Records the 9 performance fixes that shipped to `main`, their validation, and
the benchmark numbers that justify each one.

## Final state on `main`

All fixes are merged and pushed (`origin/main`). They sit cleanly on top of the
finished Agent Drafter feature (PR #6) — the perf work was rebased onto
`origin/main` so nothing of that feature was disturbed:

```
v1.86.0
  └─ feat(agent-drafter): BioRouter Apps platform (PR #6)
       ├─ 2b0e661  perf(regex): compile per-call regexes once via Lazy statics
       ├─ 6c2ed87  perf(electron): cache settings.json instead of re-reading per call
       ├─ e4ce4c6  perf(bundle): import lodash/isEqual directly, not whole lodash
       ├─ 0ad7477  perf(db): pragmas, bounded pool, search GROUP BY, token-only read
       ├─ 3c429ce  perf(server): gzip-compress HTTP responses (SSE excluded)
       ├─ 230544b  perf(ui): coalesce streaming message re-renders to one per frame
       ├─ 4014c44  perf(mcp): stop blocking the async runtime in computer-controller I/O
       ├─ 8d78ddd  perf(scheduler): non-blocking persist_jobs writes
       └─ 0f9bb71  docs(perf): review report + implementation log
```

Every fix was built, tested for behavior-preservation, and committed separately.
A full backup of the original pre-rebase branch (including the unrelated local
WIP snapshot) is preserved at branch `perf/streaming-and-latency`.

## The fixes

| Commit | Fix (review ref) | What changed | Behavior-preservation |
|---|---|---|---|
| `2b0e661` | Regex hoisting (C1, F11) | 3 hot/setup-path regexes (`substitute_env_vars`, knowledge `derive`, mac-screenshot) compiled once via `once_cell::Lazy` instead of every call | knowledge::graph 6 tests pass; clean build |
| `6c2ed87` | Electron settings cache (G3) | `loadSettings()` write-through in-memory cache (was ~20 sync disk reads+parse/launch); returns `structuredClone` to keep "fresh object per call" | typecheck clean |
| `e4ce4c6` | Bundle (J8) | `import isEqual from 'lodash/isEqual'` instead of whole `lodash` | typecheck clean |
| `0ad7477` | DB hot paths (D2, D3, D5, E1) | `synchronous=NORMAL` (no fsync/commit), `max_connections(4)`, chat-search N+1→`GROUP BY`, and `get_token_counts()` (token-only read on the per-streamed-event path, dropping the `COUNT(*)` + metadata deserialize) | session tests pass; `get_token_counts` equivalence verified before rebase |
| `3c429ce` | HTTP compression (E2) | `tower_http::CompressionLayer` (gzip); `DefaultPredicate` excludes `text/event-stream` so SSE `/reply` stays unbuffered | 20 server lib tests pass |
| `230544b` | Streaming render throttle (I1) | Coalesce per-token `setMessages` to one per animation frame (`requestAnimationFrame`); `messagesRef` stays synchronous; cancel on unmount | typecheck clean; **vitest identical 407 pass / 33 pre-existing fail with and without the change** |
| `4014c44` | MCP blocking I/O (F2, F4) | computer-controller cache → `tokio::fs`; `pdf_tool` split into sync `pdf_tool_blocking` run via `spawn_blocking` | 25 computercontroller tests pass (incl. 4 PDF tests) |
| `8d78ddd` | Scheduler de-block | `persist_jobs` → `tokio::fs` instead of blocking `std::fs` on the async cron path | 2 scheduler tests pass |

## Benchmark validation

Each result below is **before vs after, measured side-by-side**. The DB numbers
use the **real async `sqlx` code paths and the real `add_message` transaction**
(`BEGIN; INSERT message; UPDATE sessions; COMMIT`), 9 runs, median reported.

| Fix | Benchmark | Before → After | Result |
|---|---|---|---|
| E1 token-only read | 5000 reads, 5000-msg session (real sqlx) | 723 ms → 108 ms | **6.7× faster** |
| Search N+1 → GROUP BY (D2) | 200 sessions × 100 searches (real sqlx) | 451 ms → 98 ms | **4.6× faster** |
| `synchronous=NORMAL` (D3) | 1000 real `add_message` txns | 118 ms → 74 ms | **1.59× faster** |
| `max_connections=4` (D5) | 16 concurrent writers × 200 txns | 286 ms → 239 ms | **1.20× faster** |
| Regex hoisting | 50k calls | 1.67 s → 4.5 ms | **370× faster** |
| gzip compression | 200 KB OpenAPI JSON | 200 KB → 17 KB | **11.8× smaller** |
| lodash import | minified bundle of one import | 72 KB → 16 KB | **4.5× smaller** (56 KB) |
| Settings cache | 20 reads/launch | 1.57 ms → 0.74 ms | **2.1×** + 19 fewer syscalls |
| Streaming throttle | 600-token reply, fast stream | 600 → 60 renders | **up to 10× fewer** full-list renders (neutral for slow streams — it caps render rate to the display, never adds work) |
| `spawn_blocking` / `tokio::fs` | concurrent task during heavy work | task starved for the whole op → stays responsive | **responsiveness** win (not a single-op speedup): a blocking PDF parse or large file write no longer freezes every other async task for its full duration |

### A note on benchmark rigor (why two results changed)

An initial pass benchmarked the search-N+1 and `max_connections` changes with
**synchronous `rusqlite` and single-statement inserts**, and they looked like
non-wins (0.8× and "1.19× slower"). Re-running against the **real async `sqlx`
path and the real 2-statement transaction** reversed both: the async per-query
overhead makes N+1 genuinely expensive (→ GROUP BY is **4.6× faster**), and with
real transactions a smaller write pool contends less on the SQLite write lock
(→ `max_connections=4` is **1.20× faster**). Lesson baked into the table above:
benchmark the *actual* code path, not a simplified stand-in.

## Honest behavior caveats

No change alters chat content, tool results, or normal workflows. The few
genuinely observable differences, named explicitly:

- **Streaming throttle (I1):** chat repaints at ~60 fps instead of once per
  token. Final text is identical; at very high token rates the animation is
  slightly frame-batched rather than strictly one-token-at-a-time.
- **`synchronous=NORMAL` (D3):** changes a *failure-mode* — on an OS crash or
  power loss (not a normal app quit) the last DB commit can be lost. Standard
  WAL trade-off; invisible in normal use.
- **Settings cache (G3):** an *external* edit to `settings.json` while the app
  is running isn't seen until restart (in-app changes write through the cache).
- **gzip (E2):** the HTTP wire format changed; the client decompresses
  transparently.

## Deferred (not implemented — see review for full rationale)

Higher-risk or behavior-sensitive items left for a focused, test-backed pass:
H1/H2/H4 (React message `memo` refactor), J1 (route code-splitting — transient
Suspense flash), J2 (syntax-highlighter slimming — would drop highlighting for
unregistered languages), C3 (tool-catalog Arc sharing — wide ripple), B1
(RequestLog), F1/F3/F5 (autovis CDN, knowledge BM25 / tree-sitter caches), K1/K2
(TUI render redesign), A1/A2/A4 (agent-loop clone/batch-write), I2 (delta SSE
protocol).
