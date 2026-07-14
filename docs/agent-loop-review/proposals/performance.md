# BioRouter Agentic Loop — Improvement Proposals

**Lens: Performance & efficiency** (latency, token efficiency, prompt caching, parallel tool
execution, compaction efficiency, cheaper-model delegation, streaming, startup time, redundant
context elimination)

Grounded in the internal reviews (`internal/core-loop.md`, `context-injection.md`,
`state-awareness.md`, `long-running.md`, `server-flow.md`, `guardrails-permissions.md`), the
comparison chapters (`compare/context.md`), and the two in-repo performance studies
(`docs/performance-review-2026-06-22.md`, `docs/jcode-comparison-perf-analysis.md`).

This is an exhaustive list (quick wins → ambitious redesigns), not a curated shortlist.

---

## Prompt caching & token efficiency

### P-1: Lock the tool list to stop busting the provider prompt cache
- Problem: The tool catalog is re-derived every turn and rebuilt mid-turn whenever an extension is enabled (`agent.rs:1712-1715`, `agent.rs:2039-2042`), and async MCP registration silently invalidates the entire tool-prefix cache. Anthropic `cache_control: ephemeral` is applied correctly but the tool prefix keeps changing, so cache-capable providers pay a recurring token/$ cost (`jcode-comparison-perf-analysis.md:141-153`).
- Proposal: Lock the tool list after first assembly; allow exactly one rebuild when MCP tools first appear, gated by a cheap `CacheTracker` that hashes the tool-prefix and refuses silent rebuilds. Sort tools by name (already partly done) and freeze until an explicit toolset change.
- Inspired by: jcode (`cache_tracker.rs`, tool-list locking).
- Affected code: `crates/biorouter/src/agents/tool_execution.rs` (`prepare_tools_and_prompt`), `agents/extension_manager.rs` (`get_all_tools_cached`), `providers/formats/anthropic.rs`.
- Impact: High — removes a recurring cache-miss cost on the hottest path for Anthropic/OpenAI-style caching.
- Effort: M
- Risk: A stale locked tool list could omit a just-enabled extension's tools for one turn if the invalidation gate is wrong.

### P-2: Deep-clone elimination on the cached tool catalog
- Problem: The `Arc<Vec<Tool>>` cache is defeated by `.cloned().collect()` — the tool catalog is deep-cloned per turn (`extension_manager.rs:710-735`, perf review A/Theme 3).
- Proposal: Return `Arc::clone` on the unfiltered path instead of cloning the vector; only materialize a new Vec when filtering actually removes tools.
- Inspired by: novel (internal perf review).
- Affected code: `crates/biorouter/src/agents/extension_manager.rs:710-735`.
- Impact: Medium — removes a per-turn allocation proportional to tool count.
- Effort: S
- Risk: Shared `Arc` must stay immutable; a mutation path would need copy-on-write.

### P-3: Introduce a total context budget with ranking/truncation
- Problem: There is no aggregate size/token accounting anywhere. Hints, extension instructions, inlined skill bodies, and MOIM are concatenated with only a 128 KB per-file *parse* cap (`import_files.rs:53`; `context-injection.md` gap #1). A large `AGENTS.md`, chatty MCP server, or several `@import`s can silently blow the window.
- Proposal: Add a token-budgeted assembler that measures each injected block, ranks by relevance/recency, and truncates or drops lowest-priority blocks to fit a configurable budget. Mirror OpenHands' pinned-head + minimum-progress condenser and Codex's `project_doc_max_bytes` cap.
- Inspired by: OpenHands (`LLMSummarizingCondenser`), Codex CLI (`project_doc_max_bytes`).
- Affected code: `crates/biorouter/src/agents/prompt_manager.rs`, `hints/load_hints.rs`, `agents/moim.rs`.
- Impact: High — bounds worst-case token spend and prevents silent overflow-induced compaction thrash.
- Effort: M
- Risk: Over-aggressive truncation could drop guidance the model needed.

### P-4: Dedup MOIM — remove the prior `<info-msg>` before inserting the new one
- Problem: `inject_moim` runs every loop iteration (`agent.rs:1596`) with no removal of the prior block, so a long multi-tool turn accumulates repeated near-identical `<info-msg>` blocks and the model sees several timestamps (`context-injection.md` gap #3).
- Proposal: Before inserting a fresh MOIM, strip the previous agent-only MOIM block from the conversation tail so exactly one is present.
- Inspired by: novel (internal review implication #5).
- Affected code: `crates/biorouter/src/agents/moim.rs`.
- Impact: Medium — cuts redundant tokens per multi-tool turn and removes contradictory timestamps.
- Effort: S
- Risk: Must not remove a MOIM that was already merged into a user turn by `fix_conversation`.

### P-5: Refresh the system-prompt clock per turn (or drop it)
- Problem: `current_date_timestamp` is frozen at agent construction (`agent.rs:253`, `prompt_manager.rs:186`) and never refreshed, so `{{current_date_time}}` goes stale in long sessions; it is also UTC-hour granularity in the system prompt vs Local-minute in MOIM — two clocks at once (`context-injection.md` gap #2).
- Proposal: Recompute the date per turn at hour granularity (cache-stable within the hour), or drop the system-prompt date entirely and rely on MOIM's timestamp — removing the contradiction and one source of cache churn.
- Inspired by: Codex CLI (`current_time.rs` context fragments).
- Affected code: `crates/biorouter/src/agents/prompt_manager.rs`, `agent.rs:253`.
- Impact: Low-medium — correctness + minor cache stability.
- Effort: S
- Risk: Per-second refresh would bust the cache; must keep hour granularity.

### P-6: Cap and cache eager skill-body inlining
- Problem: `skill_resource_context` calls `loadSkill` and inlines the entire skill body into a synthetic user message every turn the ref appears, with no size cap and no caching across turns (`agent.rs:506-535`; `context-injection.md` gap #7).
- Proposal: Cache the loaded skill body per session, inject it once, and cap its size against the P-3 context budget; on repeat turns inject a short pointer rather than the full body.
- Inspired by: novel.
- Affected code: `crates/biorouter/src/agents/agent.rs:506-535`, `resource_refs.rs`.
- Impact: Medium — removes repeated multi-KB injections on skill-heavy sessions.
- Effort: M
- Risk: A skill body edited mid-session would be stale until cache invalidation.

### P-7: Token-aware large-response handling with inline preview + handle
- Problem: `process_tool_response` uses a 200,000-**character** (~50k token) threshold applied per content item, so several items just under threshold still blow the context; the remediation dumps to a temp file outside the session working dir and gives no preview, no line count, no token-aware truncation (`core-loop.md` gap #4, `large_response_handler.rs`).
- Proposal: Switch to a token-aware, aggregate threshold; inline a bounded head/tail preview plus a line-count summary and a searchable handle that resolves inside the session working dir.
- Inspired by: Claude Code / Codex (bounded preview + handle).
- Affected code: `crates/biorouter/src/agents/large_response_handler.rs`.
- Impact: High — directly reduces tokens from oversized tool results, the biggest single-turn blowup source.
- Effort: M
- Risk: Truncating a result the model needed in full; mitigate with the searchable handle.

### P-8: Externalize large tool results from `content_json`
- Problem: Large tool responses are serialized whole into `messages.content_json` (`session_manager.rs:1857`); message load deserializes all of it eagerly, bloating session DBs and slowing every load (`state-awareness.md` gap #9).
- Proposal: Store payloads over a threshold in a side blob table (or file) referenced by handle; load lazily only when the model requests them.
- Inspired by: novel.
- Affected code: `crates/biorouter/src/session/session_manager.rs`, `agents/large_response_handler.rs`.
- Impact: Medium — faster session load, smaller DBs, less RAM on `get_conversation`.
- Effort: L
- Risk: Migration + backward compatibility for existing sessions.

---

## Compaction efficiency

### P-9: Move auto-compaction off the user-visible critical path
- Problem: Compaction is a synchronous LLM round-trip inside `reply()` — the user waits for a summarization call, and `do_compact` can retry up to 5× with progressive tool-response removal, all blocking (`jcode-comparison-perf-analysis.md:157-163`, perf review A8).
- Proposal: Trigger background compaction at ~80% budget via `tokio::spawn`, swap the compacted history in on a later turn; keep only a synchronous no-LLM hard-drop at ~95% as a floor.
- Inspired by: jcode (`compaction.rs:860-1027`, background compaction + 0.95 hard-drop).
- Affected code: `crates/biorouter/src/context_mgmt/mod.rs`, `agents/agent.rs:1432,1478`.
- Impact: High — removes a multi-second stall from the user's turn.
- Effort: L
- Risk: Racing a background swap against a live turn needs careful history-version handling.

### P-10: Progressive context-overflow fallback instead of a 2-attempt cliff
- Problem: After two failed compactions the agent simply stops with a "still exceeded" notice (`agent.rs:1967-1976`; `core-loop.md` gap #6). A single very long tool result can wedge a session.
- Proposal: Add graduated fallbacks — drop oldest turns, summarize more aggressively, externalize the largest tool result (P-7/P-8), or transparently route the turn to a larger-context model — before giving up.
- Inspired by: novel (state-of-the-art progressive degradation).
- Affected code: `crates/biorouter/src/agents/agent.rs:1964-2013`, `context_mgmt/mod.rs`.
- Impact: Medium-high — recovers sessions that currently dead-end.
- Effort: M
- Risk: Dropping turns can lose needed context; make it explicit to the model.

### P-11: Use a char estimate or `spawn_blocking` for the compaction-trigger token count
- Problem: `check_if_compaction_needed` re-tokenizes the whole history with a cache-less `TokenCounter` allocated and dropped each call (`context_mgmt/mod.rs:184-199`, perf review A3); the real tiktoken BPE runs synchronously on the async runtime.
- Proposal: Hold one shared `TokenCounter` on the agent/session (P-13), and for the trigger check use a fast char/heuristic estimate or run the exact encode in `spawn_blocking`.
- Inspired by: jcode (trusts provider-observed tokens; char estimate for trigger).
- Affected code: `crates/biorouter/src/context_mgmt/mod.rs:184-199,282-330`.
- Impact: High — removes a full-history synchronous encode from every turn's start.
- Effort: S-M
- Risk: A heuristic estimate can mis-trigger; keep the exact count for the actual compaction.

### P-12: Only re-fix the conversation suffix, not the whole history, per turn
- Problem: `fix_conversation` runs a 7-pass normalization over the *entire* history every turn, and `inject_moim` runs a second full pass (`conversation/mod.rs:164-200`, `moim.rs:43`; perf review A2). This is O(history) per turn plus O(history) per token in some paths.
- Proposal: Cache the normalized prefix and only re-fix the suffix appended since last turn; the prefix is already invariant-valid.
- Inspired by: jcode (lazily-rebuilt provider-message cache).
- Affected code: `crates/biorouter/src/conversation/mod.rs:164-221`, `agents/moim.rs`.
- Impact: High — turns a per-turn O(N) cost into O(delta) in long sessions.
- Effort: M
- Risk: A subtle invariant bug if the prefix is mutated by a later normalization pass.

---

## Streaming pipeline latency

### P-13: Carry the agent's already-computed `TokenState` in the SSE event
- Problem: The server calls `get_token_state()` on *every* `AgentEvent::Message`, running a `SELECT` on `sessions` plus a `COUNT(*) FROM messages` that grows linearly with conversation length — and the token counts were already computed in-process before the event was emitted (`reply.rs:356,363` → `session_manager.rs:1006`; perf review Theme 1). Pure redundant disk work on the hottest path.
- Proposal: Attach the agent's computed `TokenState` to the event payload; drop the per-event DB read entirely, or at minimum drop `COUNT(*)` and cache last token-state in `AppState`, refreshing only on `Finish`.
- Inspired by: jcode (trusts provider-observed tokens).
- Affected code: `crates/biorouter-server/src/routes/reply.rs:158-181,356-472`, `crates/biorouter/src/agents/agent.rs`.
- Impact: High — removes 2 SQLite queries per streamed token, one of them growing with history.
- Effort: M
- Risk: Event payload schema change; keep a fallback path for older clients.

### P-14: Coalesce partial-text SSE deltas on a frame timer
- Problem: Each partial-text delta is independently `serde_json::to_string`'d and pushed as its own SSE frame (`reply.rs:185`); the CLI already has a `stream_coalesce` module but the SSE path has no equivalent (perf review Theme 1).
- Proposal: Buffer deltas on a ~50-100 ms timer and emit one coalesced frame, reusing the `stream_coalesce` idea on the SSE path.
- Inspired by: jcode (drain ≤32 events → 1 frame); BioRouter's own CLI `stream_coalesce`.
- Affected code: `crates/biorouter-server/src/routes/reply.rs:183-199,345-406`.
- Impact: High — fewer JSON serializes, SSE frames, and client-side parses per reply.
- Effort: M
- Risk: Adds up to ~100 ms of perceived typing latency; tune the budget.

### P-15: Emit message-level patches instead of re-sending the whole conversation
- Problem: The `UpdateConversation` SSE event re-sends the entire conversation mid-stream (its own TODO at `useChatStream.ts:228`), which the client re-parses on the main thread (perf review Theme 4). `HistoryReplaced` after compaction ships the full history.
- Proposal: Define a message-id/patch protocol so `UpdateConversation`/`HistoryReplaced` send only changed messages; client applies patches by id.
- Inspired by: novel (server-authoritative patch protocol).
- Affected code: `crates/biorouter-server/src/routes/reply.rs`, `ui/desktop/src/hooks/useChatStream.ts`, `chatStreamStore.tsx`, generated SSE client.
- Impact: High — eliminates a full-conversation resend + reparse on every compaction/history event.
- Effort: L
- Risk: Protocol change touching both sides; needs stable per-message ids (P-16).

### P-16: Stable per-message ids instead of positional `msg_<session>_<idx>`
- Problem: Synthetic ids are positional, so any history rewrite (compaction/edit) renumbers messages, making stable references (UI anchors, patch protocol) fragile (`state-awareness.md` gap #10).
- Proposal: Persist a stable message id column; derive synthetic positional ids only for legacy rows.
- Inspired by: novel.
- Affected code: `crates/biorouter/src/session/session_manager.rs:1810-1841`.
- Impact: Medium — enables P-15 and reliable UI diffing.
- Effort: M
- Risk: Schema migration; dual-read for old sessions.

### P-17: Move `RequestLog` file I/O off the async runtime
- Problem: `RequestLog` does blocking `std::fs` open/write/rename on **every request and every stream chunk** (`providers/utils.rs:473-562`, called at `:207`), interleaved between token batches — stalls all concurrent tasks (perf review Theme 2, B1). Highest-impact blocking-I/O finding (touches 100% of requests).
- Proposal: Buffer in memory and flush via `spawn_blocking`, or send to a dedicated logging task over an `mpsc` channel.
- Inspired by: novel (internal perf review).
- Affected code: `crates/biorouter/src/providers/utils.rs:473-562,207`.
- Impact: High — unblocks the runtime during streaming.
- Effort: M
- Risk: Buffered logs could be lost on crash; flush on shutdown.

### P-18: Wrap remaining blocking file/git I/O in `spawn_blocking`/`tokio::fs`
- Problem: Synchronous syscalls on async workers in `text_editor` (whole-file read+rewrite, `text_editor.rs:784,806,997,1062`), PDF/DOCX/XLSX parsers (`computercontroller/pdf_tool.rs`, `mod.rs:1036-1284`), scheduler `fs::write` (`scheduler.rs:171,339,431`), and knowledge git2 commits (`service.rs:~623`) stall the runtime (perf review Theme 2).
- Proposal: Wrap each in `tokio::task::spawn_blocking` or switch to `tokio::fs` (mirror the already-correct `mod.rs:949`).
- Inspired by: novel (internal perf review).
- Affected code: as listed above.
- Impact: Medium — smoother concurrency under multi-session load.
- Effort: S-M (many small edits)
- Risk: Low; mechanical.

### P-19: Render streaming markdown as plain text until the message finishes
- Problem: The GUI re-runs the full `ReactMarkdown` + remark/rehype + KaTeX + Prism pipeline on the streaming message **every token** (`MarkdownContent.tsx:180-240`), and the TUI re-parses the entire accumulated message as Markdown every token (`tui/app.rs:161-179`), both O(T²) (perf review Theme 1).
- Proposal: Render a cheap plain-text preview while `isStreaming`, switch to the full markdown/Prism/KaTeX pipeline once finished; add a dirty-flag + ~60fps clock in the TUI so token bursts coalesce into one redraw.
- Inspired by: jcode (five-layer content cache; `needs_redraw` gate).
- Affected code: `ui/desktop/src/components/MarkdownContent.tsx`, `CodeBlock.tsx`, `crates/biorouter-cli/src/tui/app.rs`, `tui/mod.rs`.
- Impact: High (perceived) — removes streaming jank in both GUI and TUI.
- Effort: M
- Risk: Preview-to-final swap flicker; keep layout stable.

### P-20: `React.memo` message components + compute tool-chain maps once
- Problem: `BioRouterMessage`/`UserMessage`/`ToolCallWithResponse` are not memoized, so each new array reference re-renders every message including hundreds of finished ones, each re-running an O(N²) request→response map (`BioRouterMessage.tsx:37,87-112`; perf review Theme 1).
- Proposal: `React.memo` the three components, stabilize props with `useCallback`/`useMemo` in `BaseChat`, and compute tool-call chains/response-maps once in `ProgressiveMessageList`.
- Inspired by: novel (internal perf review).
- Affected code: `ui/desktop/src/components/BioRouterMessage.tsx`, `BaseChat.tsx`, `ProgressiveMessageList`.
- Impact: High (GUI) — turns O(N) per-token re-render into O(1).
- Effort: M
- Risk: Memo comparators must be correct or updates are dropped.

### P-21: Throttle client `setMessages` and drop `JSON.stringify` from the hot dedupe
- Problem: `pushMessage` rebuilds the whole `messages` array and calls `setMessages` per token, and `sameContent` `JSON.stringify`s messages to dedupe (`useChatStream.ts:128-169`) — O(N·T) array work + O(T²) string growth per reply (perf review Theme 1).
- Proposal: Throttle `setMessages` to a `requestAnimationFrame`/30-60 ms boundary (keep `messagesRef` synchronous for correctness) and replace the `JSON.stringify` dedupe with an id/length check.
- Inspired by: novel (internal perf review).
- Affected code: `ui/desktop/src/hooks/useChatStream.ts:128-169`.
- Impact: High (GUI) — removes quadratic client-side work.
- Effort: S-M
- Risk: Throttling can delay final-token render; flush on `Finish`.

---

## Startup time

### P-22: Don't block the first frame / first turn on full MCP boot
- Problem: The CLI drains the entire extension `JoinSet` before entering the TUI, so first frame waits on the slowest MCP handshake (`builder.rs:578,285`); the GUI runs `loadURL` only after `/status` polls ready (`main.ts:665→837`); and `/status` itself is gated behind `Scheduler::new()` + `load_jobs_from_storage()` + `soul::install()` all awaited before `TcpListener::bind` (`commands/agent.rs:44,68`) (`jcode-comparison-perf-analysis.md` §B).
- Proposal: Render the first frame and accept input while MCP registration and scheduler/soul init happen in the background; make the MCP pool lazy (`OnceCell` on first tool use); bind the listener before the heavy init.
- Inspired by: jcode ("do NOT block the first turn on MCP connection").
- Affected code: `crates/biorouter-cli/src/session/builder.rs`, `ui/desktop/src/main.ts`, `crates/biorouter-server/src/commands/agent.rs`.
- Impact: High (perceived) — the dominant startup latency.
- Effort: M
- Risk: A tool called before its MCP server is ready must queue or error gracefully.

### P-23: Cache `settings.json` reads at module scope
- Problem: `utils/settings.ts` does `existsSync`+`readFileSync`+`JSON.parse` per call, invoked ~20× per launch including on the startup path (`utils/settings.ts:41-51`; perf review Theme 3).
- Proposal: Add a module-level write-through cache; read once, invalidate on write.
- Inspired by: novel.
- Affected code: `ui/desktop/src/utils/settings.ts`.
- Impact: Low-medium — shaves redundant disk I/O off startup.
- Effort: S
- Risk: Stale cache if settings change out-of-band; write-through mitigates.

### P-24: Add Cargo profiles + feature-gate heavy deps
- Problem: The workspace has **no `[profile.*]` section for optimization** beyond release/strip and compiles ~988 crates unconditionally (23 AWS, 15 tree-sitter, 7 boa-JS-engine, all PDF/DOCX) (`jcode-comparison-perf-analysis.md` executive summary #2). This bloats binary size and cold compile.
- Proposal: Feature-gate heavy/optional dependencies (AWS Bedrock, tree-sitter langs, JS engine, document parsers) behind Cargo features off by default; ship a lean default binary.
- Inspired by: jcode (feature-gated `jemalloc`, minimal default deps).
- Affected code: `Cargo.toml` (workspace + crate manifests).
- Impact: High (binary size + compile time; smaller RSS from fewer static init).
- Effort: M
- Risk: Feature-flag matrix can break less-common provider paths; needs CI coverage.

### P-25: Add a tuned jemalloc allocator to `biorouterd`/CLI
- Problem: BioRouter uses the system allocator everywhere (no `#[global_allocator]`); combined with per-turn full-transcript reload + double-clone, glibc per-thread arenas retain freed pages, causing RSS creep on long-running `biorouterd` (`jcode-comparison-perf-analysis.md` §C).
- Proposal: Wire `tikv-jemallocator` as `#[global_allocator]` with decay tuning (`dirty_decay_ms:1000,muzzy_decay_ms:1000,narenas:4`), opt-in behind a `jemalloc` feature; on non-jemalloc Linux, `mallopt(M_ARENA_MAX,4)`.
- Inspired by: jcode (reclaimed ~1.4 GB RSS in their testing).
- Affected code: `crates/biorouter-server/src/main.rs`, `crates/biorouter-cli/src/main.rs`, `Cargo.toml`.
- Impact: High (RAM on long-running daemon).
- Effort: S
- Risk: jemalloc build issues on some targets; keep it feature-gated.

---

## Resource sharing / redundant context elimination

### P-26: Share MCP servers across agents/sessions (SharedMcpPool)
- Problem: Each `Agent` builds its own `ExtensionManager` and spawns MCP child processes per agent (`agent.rs:236`, `extension_manager.rs:236,250-252`); up to 100 live agents × M stdio/uvx servers (each 40-150 MB), with no shared pool (`jcode-comparison-perf-analysis.md` §A). This is the dominant RAM multiplier.
- Proposal: Introduce an `Arc<SharedMcpPool>` keyed by extension config so N sessions share M server processes instead of N×M; sessions attach to shared clients rather than spawning their own.
- Inspired by: jcode (`mcp/pool.rs`).
- Affected code: `crates/biorouter/src/agents/extension_manager.rs`, `execution/manager.rs`.
- Impact: Very high (RAM) — collapses the largest process/memory multiplier.
- Effort: L
- Risk: Shared MCP state across sessions (working dir, per-session env) needs careful isolation.

### P-27: One `biorouterd` shared across Electron windows
- Problem: `startBiorouterd` spawns a fresh daemon (new port, tokio runtime, `AgentManager`, SQLite pool, MCP trees) for *every* window (`biorouterd.ts:115,172`, `main.ts:612,683`), even though the server is already session-keyed and singleton (`execution/manager.rs:19-24`) (`jcode-comparison-perf-analysis.md` §A.1).
- Proposal: Start one daemon and connect all windows to it; only the Electron spawn path assumes per-window.
- Inspired by: jcode (one process owns all sessions).
- Affected code: `ui/desktop/src/biorouterd.ts`, `ui/desktop/src/main.ts`.
- Impact: Very high (RAM) — N windows → 1 backend.
- Effort: L
- Risk: Window lifecycle/ownership (which window kills the daemon) needs rework.

### P-28: Share a static `reqwest::Client` across provider instances
- Problem: A fresh provider (and its own `reqwest::Client`) is created on each session restore (`agent.rs:1978`) and each subagent spawn (`subagent_tool.rs:414`), each paying ~10 ms TLS+pool init and holding a separate connection pool (`jcode-comparison-perf-analysis.md` §A.3). (Note: within one provider instance the client is already reused, `api_client.rs:208`.)
- Proposal: Hold one shared/static `reqwest::Client` (or a small pool keyed by TLS config) reused across provider instances; providers borrow it rather than build their own.
- Inspired by: jcode (one HTTP pool per machine).
- Affected code: `crates/biorouter/src/providers/api_client.rs:208-227`, `factory.rs`.
- Impact: Medium — removes repeated TLS/pool warmups and connection duplication.
- Effort: M
- Risk: Providers with distinct proxy/TLS settings need separate clients; key the pool accordingly.

### P-29: Reduce whole-conversation deep clones per turn
- Problem: The agent loop deep-clones the entire message history 2-3× per turn (`agent.rs:1288,1137`, `reply_parts.rs:186`, `moim.rs:26`), and the reply route double-clones (`reply.rs:274,298`) (perf review Theme 4, A1).
- Proposal: Thread `Arc<[Message]>` through read-only paths; only clone when mutating.
- Inspired by: jcode (Arc-shared transcript).
- Affected code: `crates/biorouter/src/agents/agent.rs`, `reply_parts.rs`, `moim.rs`, `crates/biorouter-server/src/routes/reply.rs`.
- Impact: High (long sessions) — removes O(history) allocation per turn.
- Effort: M
- Risk: Ownership refactor; borrow-checker friction.

### P-30: Batch per-message DB writes and incremental usage tracking
- Problem: Messages are written in a serial `await` loop at the end of each turn (`agent.rs:1781-1783`) and a full-session read-then-write runs per usage chunk (`reply_parts.rs:349-385`) (perf review A4).
- Proposal: Batch-insert the turn's messages in one transaction; track usage metrics incrementally against the `token_events` side-table instead of read-modify-write on the sessions row.
- Inspired by: novel.
- Affected code: `crates/biorouter/src/agents/agent.rs:1781-1783`, `reply_parts.rs:349-385`, `session/session_manager.rs`.
- Impact: Medium-high — fewer DB round-trips at turn end.
- Effort: M
- Risk: Transaction failure handling; partial writes on crash.

### P-31: Default Auto Visualiser heavy libs to CDN/proxy assets
- Problem: `mermaid.min.js` (3.3 MB) is base64'd to ~4.4 MB, stored in session history, and re-sent on every chat reopen (`autovisualiser/common.rs:126-273`; perf review Theme 4). The `BIOROUTER_AUTOVIS_CDN=1` flag already exists (the desktop app sets it), but standalone figures still inline.
- Proposal: Make CDN/proxy-served assets the default for persisted figures and store only a small reference; keep inline as an offline opt-in. Already partly done — extend to the persisted/reloaded path.
- Inspired by: novel (internal review); existing `BIOROUTER_AUTOVIS_CDN`.
- Affected code: `crates/biorouter-mcp/src/autovisualiser/common.rs`.
- Impact: Medium — cuts megabytes from session history and reopen bandwidth.
- Effort: M
- Risk: Offline reopen breaks if assets aren't inlined; guard with the offline flag.

---

## Parallel tool execution & tool-loop efficiency

### P-32: Bound tool parallelism with a concurrency cap
- Problem: `select_all` over all approved tool futures (`agent.rs:1792`) has no concurrency cap and no cross-tool isolation, so many write-side tool calls in one message run all at once (`core-loop.md` gap #8).
- Proposal: Add a configurable semaphore over dispatched tool futures (e.g. default 8, like subagents); optionally serialize write-side tools targeting the same resource.
- Inspired by: novel; mirrors the subagent `SUBAGENT_SEMAPHORE` pattern.
- Affected code: `crates/biorouter/src/agents/agent.rs:708-745,1792`.
- Impact: Medium — avoids thundering-herd on disk/network and ordering hazards.
- Effort: M
- Risk: Too-low a cap slows legitimately-parallel read tools.

### P-33: Per-tool timeout + partial/streamed tool output
- Problem: Every tool call inherits the extension's coarse 300 s timeout (`mcp_client.rs:369`, `config/extensions.rs:11`); there is no per-tool budget, no adaptive timeout, and a single slow tool blocks the turn from advancing until the stream drains (`core-loop.md` gap #3).
- Proposal: Add a per-tool timeout override and surface a "this tool is taking a while" partial signal so the turn can proceed or the model can decide to background/kill it.
- Inspired by: Claude Code / Codex (per-tool timeouts, partial tool output).
- Affected code: `crates/biorouter/src/agents/agent.rs`, `agents/mcp_client.rs`, `config/extensions.rs`.
- Impact: Medium — bounds worst-case turn latency from a single slow tool.
- Effort: M
- Risk: Cancelling a mid-flight tool must clean up its side effects.

### P-34: Loop-level retry with backoff for transient streaming/provider errors
- Problem: A mid-stream decode error or any non-context `ProviderError` ends the turn with a "please retry" string (`agent.rs:2020-2028`); the streaming path is not wrapped in `ProviderRetry` (`anthropic.rs:273`) (`core-loop.md` gap #5). Transient blips surface to the user and force a full context re-send.
- Proposal: Wrap the streaming path in bounded retry-with-jitter for transient classes (5xx, connection reset, decode) before surfacing an error.
- Inspired by: the existing non-streaming `with_retry` (`anthropic.rs:228`); jcode (mid-stream rollback).
- Affected code: `crates/biorouter/src/providers/anthropic.rs:273-313`, `providers/retry.rs`, `agents/agent.rs`.
- Impact: Medium — avoids full-turn re-runs on transient failures.
- Effort: M
- Risk: Retrying a partially-streamed turn must not duplicate emitted content; needs a resume/rollback point.

### P-35: General loop-guard / circuit-breaker in the main agent loop
- Problem: Repeated identical failing tool calls are only caught inside the optional goal loop (`goal.rs:303-311`); outside `/goal`, an agent can retry the same failing action indefinitely (`state-awareness.md` gap #8). The `RepetitionInspector` exists (max 3) but re-scans the full history per turn (perf review A7).
- Proposal: Add a lightweight, always-on same-tool+same-args failure counter (incremental, not full-history rescan) that escalates to the model after K identical failures.
- Inspired by: novel; extends `RepetitionInspector`.
- Affected code: `crates/biorouter/src/agents/tool_inspection.rs`, `tool_monitor.rs`.
- Impact: Medium — caps wasted tokens/latency on stuck loops.
- Effort: S-M
- Risk: False positives on legitimately-repeated calls (polling); key on failure, not just repetition.

### P-36: Make `RepetitionInspector` incremental instead of full-history rescan
- Problem: `RepetitionInspector` re-scans the full history every turn (`tool_inspection.rs:85-117`, `tool_monitor.rs:122-134`; perf review A7), and inspectors run serially.
- Proposal: Maintain a rolling window / counter of recent tool signatures updated incrementally; run independent inspectors concurrently.
- Inspired by: novel.
- Affected code: `crates/biorouter/src/agents/tool_inspection.rs`, `tool_monitor.rs`.
- Impact: Low-medium — removes an O(history) per-turn scan.
- Effort: S-M
- Risk: Window sizing; must still catch the loop it's meant to.

---

## Cheaper-model delegation

### P-37: Route auxiliary LLM calls (compaction, rename, judges) to a cheaper model
- Problem: Compaction summarization, session rename (`maybe_rename_session`, `agent.rs:2258`), the goal judge (`goal.rs`), and the (currently dead) permission judge all use the session's primary model — an expensive model doing cheap classification/summarization work.
- Proposal: Add a configurable "utility model" (cheap/fast) used for compaction summaries, rename, and judge calls; fall back to the primary model if unset.
- Inspired by: multi-model agents (Claude Code sub-model for titles); Codex utility calls.
- Affected code: `crates/biorouter/src/context_mgmt/mod.rs`, `agents/agent.rs` (rename), `agents/goal.rs`, `providers/factory.rs`.
- Impact: Medium — cuts $/latency on frequent auxiliary calls.
- Effort: M
- Risk: A too-weak utility model produces poor summaries/titles; keep it configurable.

### P-38: Per-model prompt variants to right-size guidance and tokens
- Problem: One fixed `system.md` serves 43+ providers of wildly varying capability; the only per-model transform is the toolshim JSON rewrite (`reply_parts.rs:160-166`) (`context-injection.md` gap #10, `compare/context.md`). Strong models get verbose guidance they don't need (wasted tokens); weak models get too little.
- Proposal: A provider/model-keyed prompt-variant table with a default fallback (Codex pattern), letting weaker models get more scaffolding and stronger models a leaner prompt; retain the contract test per variant.
- Inspired by: Codex CLI (per-model prompt files).
- Affected code: `crates/biorouter/src/agents/prompt_manager.rs`, `prompts/`.
- Impact: Medium — token savings on strong models + better behavior on weak ones.
- Effort: M-L
- Risk: Variant sprawl / maintenance burden; keep variants minimal.

### P-39: Cheaper-model or heuristic pre-pass for read-only permission classification
- Problem: The LLM permission judge is dead code and `readonly_tools`/`regular_tools` sets are always empty, so every non-user-configured tool requires approval in Approve/SmartApprove (`guardrails-permissions.md` gaps #1, #2). Beyond over-prompting, the intended cheap read-only auto-approve never fires.
- Proposal: Populate the read-only sets from extension `read_only_hint` annotations (a free, no-LLM signal) so read-only tools auto-approve; reserve any LLM judge for genuinely ambiguous cases and run it on a cheap model.
- Inspired by: existing `read_only_hint` annotations; Codex read-only classification.
- Affected code: `crates/biorouter/src/permission/permission_inspector.rs`, `agent.rs:348-351`, `permission_judge.rs`.
- Impact: Medium — fewer human prompts and no LLM call for the common case.
- Effort: M
- Risk: Mis-annotated "read-only" tools that actually write; validate hints.

### P-40: Async subagent handles instead of fully-blocking subagents
- Problem: The parent `subagent` tool call blocks until the child finishes (`subagent_tool.rs:341-349`); parallelism only comes from issuing many blocking calls in one message, all of which must complete before the turn advances (`long-running.md` gap #4).
- Proposal: Add a "spawn subagent → get handle → poll/await later" mode so a long subagent doesn't stall the parent turn; surface a `subagent_status`/`subagent_result` tool pair.
- Inspired by: jcode (background compaction/sidecar pattern); async task handles.
- Affected code: `crates/biorouter/src/agents/subagent_tool.rs`, `subagent_handler.rs`.
- Impact: Medium — unblocks parent turns during long delegated work.
- Effort: L
- Risk: Lifecycle/cleanup of detached subagents; result delivery ordering.

---

## Redundant refetch / recompute elimination (supporting)

### P-41: Compile regexes and cache tree-sitter queries once
- Problem: Regexes are recompiled per call (`extension_manager.rs:359-384` per HTTP header, `rmcp_developer.rs:1690`, `knowledge/graph.rs:54`) and tree-sitter `Query`s recompiled per file × 3 (`analyze/parser.rs:211,285,365`) (perf review Theme 3).
- Proposal: Move regexes to `LazyLock` statics; cache `Arc<Query>` per (lang, kind).
- Inspired by: novel.
- Affected code: as listed above.
- Impact: Low-medium — removes per-call compilation on hot analysis/extension paths.
- Effort: S-M
- Risk: Low.

### P-42: Cache the Knowledge BM25 index instead of rebuilding per search
- Problem: The BM25 index is rebuilt from scratch on every `kb_search` — re-walks the tree, clones every doc, builds a corpus, queries once, throws it away (`knowledge/store.rs:176-227`; perf review Theme 3).
- Proposal: Build the engine once and cache it; invalidate on KB write.
- Inspired by: novel.
- Affected code: `crates/biorouter-mcp/src/knowledge/store.rs:176-227`.
- Impact: Medium (KB-heavy sessions) — turns per-search O(corpus) rebuild into O(query).
- Effort: M
- Risk: Cache invalidation correctness on concurrent writes.

### P-43: Move schedule/llama status and session refetches onto SSE push + change-detection
- Problem: The GUI polls: schedules list every 15 s with no change detection (`SchedulesView.tsx:235-245`), llama status at a fixed interval during multi-minute downloads (`LlamaServerInlineCard.tsx:97-117`), session-rename disambiguation up to 4× `getSession` + a full `listSessions` for the first 3 turns (`useChatStream.ts:339-399`), and refetches extensions on *every* reply (`BottomMenuExtensionSelection.tsx:41-86`) (perf review Theme 5).
- Proposal: Push schedule/llama updates over SSE (infra exists); add change-detection before `setState`; emit a `session-renamed` push to kill the rename poll; refetch extensions only on `session-created`/explicit toggle; add a shared cached session-list fetch.
- Inspired by: novel (internal perf review).
- Affected code: `ui/desktop/src/components/SchedulesView.tsx`, `LlamaServerInlineCard.tsx`, `hooks/useChatStream.ts`, `BottomMenuExtensionSelection.tsx`.
- Impact: Medium — removes chatty background refetches and redundant re-renders.
- Effort: M
- Risk: SSE reconnection handling; missed updates if a push is dropped.

### P-44: Cache provider metadata and app cache in `AppState`
- Problem: Provider metadata is rebuilt per `/config/providers` request (`config_management.rs:361`) and `McpAppCache::new()` is rebuilt per `/agent/list_apps` then delete-all+rewrite (`routes/agent.rs:1024,1049`) — the "cache" is invalidated every read (perf review Theme 3).
- Proposal: Hold both in `AppState` behind a `OnceCell`/`RwLock`; invalidate only on actual change.
- Inspired by: novel.
- Affected code: `crates/biorouter-server/src/routes/config_management.rs`, `routes/agent.rs:1024,1049`, `state.rs`.
- Impact: Low-medium — removes rebuilds on read-only endpoints.
- Effort: S-M
- Risk: Staleness after config edits; wire invalidation to the write paths.

---

## Server-flow efficiency & correctness-adjacent

### P-45: Per-session turn lock/queue to prevent concurrent-turn waste
- Problem: There is no server-side single-turn-per-session guard; two `/reply` calls for one session share one `Arc<Agent>`, one `confirmation_rx`, and one `soft_interrupts` vec, interleaving turns (`server-flow.md` gap #1). Serialization is only client-side. Beyond correctness, a raced/duplicate turn doubles token spend.
- Proposal: Hold a per-session turn lock/queue server-side; a second `/reply` waits or is rejected with a clear status.
- Inspired by: state-of-the-art agents (per-session turn lock).
- Affected code: `crates/biorouter-server/src/routes/reply.rs:257`, `crates/biorouter/src/execution/manager.rs`.
- Impact: High (correctness + wasted-token avoidance).
- Effort: M
- Risk: Deadlock/queue starvation if a turn hangs; needs a TTL.

### P-46: Idempotency / resume token on `/reply`
- Problem: With `sseMaxRetryAttempts: 1`, an SSE reconnect re-POSTs and starts a *second* turn (re-appending the user message) rather than resuming (`server-flow.md` gap #7). Wasted duplicate turn + token spend.
- Proposal: Attach a client-generated turn/idempotency id; the server resumes or dedupes an in-flight turn instead of starting a new one.
- Inspired by: novel (idempotent request keys).
- Affected code: `crates/biorouter-server/src/routes/reply.rs`, `ui/desktop/src/hooks/chatStreamStore.tsx`.
- Impact: Medium — avoids accidental duplicate turns on flaky networks.
- Effort: M
- Risk: Resume semantics for a partially-streamed turn are subtle.

### P-47: TTL on pending tool-confirmation waits
- Problem: `confirmation_rx.recv().await` has no timeout and is not in a `select!` with the cancel token, so a lost/duplicate confirmation blocks the turn **forever** until the client disconnects (`server-flow.md` gap #2, #3). A blocked turn holds an agent + resources indefinitely.
- Proposal: `select!` the confirmation wait against the cancel token and a TTL; on expiry, emit a "prompt expired" tool result and continue.
- Inspired by: novel.
- Affected code: `crates/biorouter/src/agents/tool_execution.rs:171-172`.
- Impact: Medium — frees stuck turns and their held resources.
- Effort: S
- Risk: A slow-but-legitimate human approval could be cut off; pick a generous TTL.

### P-48: Reap orphaned background shell jobs and reconcile stuck scheduled jobs
- Problem: Background shell jobs set `kill_on_drop(false)` with no PID-file/parent-death reaping, so a daemon crash orphans process groups forever (`long-running.md` gap #1); and `load_jobs_from_storage` never resets `currently_running`, so a job mid-run at crash is permanently skipped (gap #2). Orphans consume CPU/RAM indefinitely.
- Proposal: Reuse the llama.cpp sidecar's `run/<ppid>.pid` reaping pattern for background jobs; add a one-line reconcile on scheduler load (force `currently_running=false`).
- Inspired by: existing `llamacpp_sidecar.rs:833-936` reaping.
- Affected code: `crates/biorouter-mcp/src/developer/background.rs`, `crates/biorouter/src/scheduler.rs:512-548`.
- Impact: Medium — reclaims leaked processes/resources after crashes.
- Effort: M
- Risk: Killing a still-wanted orphan from a different live parent; key reaping on dead ppids only.

---

## Summary of highest-leverage items

- **P-1** Tool-list locking + cache tracker (prompt-cache stability, recurring $/latency).
- **P-13/P-14** Kill per-token DB reads and coalesce SSE deltas (the #1 streaming-latency source).
- **P-9/P-11** Background compaction + cheap trigger token-count (remove multi-second turn stalls).
- **P-26/P-27** SharedMcpPool + one daemon per app (the dominant RAM multipliers).
- **P-22** Don't block first frame/turn on MCP boot (dominant perceived startup latency).
- **P-7** Token-aware large-response handling (biggest single-turn token blowup).
- **P-37/P-38** Cheaper utility model + per-model prompt variants (token/$ efficiency).
- **P-12/P-29** Incremental conversation re-fix + Arc-shared history (remove per-turn O(N) work).
