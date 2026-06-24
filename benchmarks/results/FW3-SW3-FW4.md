# FW3 / SW3 / FW4 — behavior, latency & safety changes

These are **not** size/RAM changes, so the macro harness only confirms
**no regression** (binaries +0.1 MB from the new code, idle RSS ~24 MB,
startup 122–180 ms; the one 5942 ms run is the cold first-exec page-in artifact
of a freshly-written binary). Each change is verified structurally + by targeted
tests; the runtime gains are conditional on load that needs a live provider.

## FW3a — `spawn_blocking` the cold-path token scan
`context_mgmt::check_if_compaction_needed` ran a synchronous tiktoken BPE pass
over the whole history on a tokio worker when `session.total_tokens` was `None`
(first turn / usage-less providers). Now offloaded to `spawn_blocking`.
- **Verification:** structural — the CPU-bound loop is moved off the async
  runtime; compiles + macro bench (which boots biorouterd) shows no regression.
- **Gain:** removes a multi-ms-to-second runtime-worker stall on the cold path
  under concurrency. Happy path unchanged (it uses provider-reported tokens).

## FW3b / SW3 — HTTP client hardening (api_client.rs)
One shared `tune_client_builder` now applies to every `ApiClient`:
`connect_timeout(15s)`, `read_timeout(300s)`, `tcp_keepalive(30s)`,
`pool_idle_timeout(90s)`, and the overall timeout default raised 600s→1800s.
- **Verification:** structural — all `ApiClient`s built via `with_timeout` /
  `rebuild_client` get the tuning; compiles. (h2 keepalive intentionally omitted:
  biorouter's reqwest has `default-features=false` without the `http2` feature, so
  those methods don't compile and h2 isn't used — connection reuse is via HTTP/1.1
  keep-alive, which `pool_idle_timeout`+`tcp_keepalive` cover.)
- **Gains:** (1) `read_timeout` aborts a *stalled* SSE stream/body in 300s instead
  of waiting the old 600s whole-request cap; (2) raising the overall cap to 1800s
  stops a *healthy* long generation being killed at 600s; (3) `connect_timeout`
  fails fast on a black-holed connect so a retry can open a fresh connection;
  (4) `pool_idle_timeout`+`tcp_keepalive` reuse warm connections across turns.
  Numeric verification needs a mock stalling server + repeated requests (live).

## FW4 — resource-aware scheduler + subagent fork-bomb guard
- **Subagent caps** (`subagent_tool.rs`): a global `Semaphore`
  (`BIOROUTER_SUBAGENT_MAX_CONCURRENT`, default 8) throttles concurrent
  subagents; an in-flight ceiling (`BIOROUTER_SUBAGENT_MAX_INFLIGHT`, default 64)
  refuses outright once too many are queued+running. Previously unbounded — the
  model is told it can spawn many subagents in parallel and a subagent can spawn
  more. `inflight_subagent_count()` added for tests/introspection.
- **Scheduler pause-when-active** (`scheduler.rs` + `reply.rs`): an interactive
  reply holds an `InteractiveTurnGuard`; while any are active `claim_run_slot`
  defers scheduled jobs (env `BIOROUTER_SCHEDULER_PAUSE_ON_ACTIVE`, default on).
- **Scheduler 429 backoff** (`retry.rs` + `scheduler.rs`): a 429 from any request
  records a `RATE_LIMITED_UNTIL` window (≥30s); `claim_run_slot` defers scheduled
  jobs while inside it, so background work doesn't pile onto a throttled provider.
  Deferral does NOT consume the job's run count — the cron fires again next tick.
- **Verification:** structural + unit tests of the mechanisms (rate-limit
  signal, interactive counter, in-flight ceiling) — see crate tests. Gains are
  safety/responsiveness (no fork-bomb; background work yields to the user).
