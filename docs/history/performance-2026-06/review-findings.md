# BioRouter performance and responsiveness review

> **What this is.** A whole-app latency and resource review across 12 subsystems, synthesizing five cross-cutting themes — per-token streaming cost, blocking I/O on the async runtime, recompute-instead-of-cache, whole-object copies, and polling — into a tiered roadmap backed by `file:line` evidence.
> **Status:** Historical record — review conducted 2026-06-22 against v1.86.0. Its Tier 0 and part of its Tier 1 items were implemented the next day and are recorded in [the implementation log](implementation-log.md); the untouched Tier 2 and Tier 3 items were never scheduled here.
> **Audience:** maintainers working on BioRouter performance.

**Date:** 2026-06-22 · **Version reviewed:** v1.86.0 · **Scope:** whole app — Rust backend (agent loop, providers, MCP, sessions, server), Electron/React GUI, ratatui CLI/TUI, and the boundaries between them.

BioRouter was decomposed into 12 subsystems; each was reviewed independently for latency, redundant work, and wasted allocations with concrete `file:line` evidence. This document synthesizes those reviews, leading with the cross-cutting themes (where multiple independent reviews converged) and then giving the per-subsystem detail.

> **Warning.** Every `file:line` anchor below is pinned to the v1.86.0 tree and has not been re-verified since. The version line has moved on by several minor releases, and some of the findings were fixed in the days after this review. Use the anchors as a description of what was read on 2026-06-22, not as current line numbers.

**Identifier key.** Findings are cited elsewhere as `A1`, `D3`, `H2` and so on. The letter names the subsystem section under "Subsystem detail" and the number is the row within that section's table:

| ID | Subsystem |
|---|---|
| A | Core agent loop, context, conversation |
| B | Provider layer |
| C | Extension manager / MCP |
| D | Session persistence |
| E | Server / Axum |
| F | Built-in MCP servers |
| G | Electron main process and startup |
| H | React rendering and state |
| I | Frontend API client / fetching |
| J | Frontend build / bundle |
| K | CLI / TUI |
| L | Cross-boundary and scheduler |

## Executive summary

Biorouter is **not** architecturally slow — startup reuses agents/providers, tool catalogs are version-cached, MCP startup is already parallel, streaming is genuinely incremental, and SQLite is in WAL mode. The slowness that exists is concentrated in a few **hot paths that run per streamed token** and a set of **blocking-I/O-on-the-async-runtime** mistakes. Fixing roughly ten items — most of them trivial-to-moderate — would remove the bulk of the perceptible latency.

**The single dominant theme:** every layer does O(history) or O(message) work **per streamed token**. One assistant reply of *T* tokens in a session of *N* messages currently costs:

| Layer | Per-token cost | Evidence |
|---|---|---|
| Server (biorouterd) | 2 SQLite queries (incl. `COUNT(*)` that grows with N) | `routes/reply.rs:356` → `session_manager.rs:983,1006` |
| Transport | 1 JSON serialize + 1 SSE frame + 1 client JSON parse + 1 client re-stringify | `reply.rs:185`, `useChatStream.ts:128-156` |
| React | full message-array copy + whole-list re-render (no `memo`) | `useChatStream.ts:132-169`, `BioRouterMessage.tsx:37` |
| React markdown | full re-parse + Prism + KaTeX re-render of the streaming msg | `MarkdownContent.tsx:180-240` |
| TUI | full markdown re-parse of whole message + full scrollback clone + full-history width re-measure | `tui/app.rs:161-179`, `tui/mod.rs:751,750` |

Each is individually "fine for one token" and quadratic over a reply. The fix is the same idea everywhere: **coalesce/throttle to a frame budget and stop re-doing O(N) work that didn't change.**

## Cross-cutting themes (highest confidence — multiple reviews agreed)

### Theme 1 — The per-token streaming pipeline is the #1 latency source

Traced end to end:

1. **Server, per delta:** `routes/reply.rs:356` calls `get_token_state()` on *every* `AgentEvent::Message`, which runs `get_session(id,false)` = a `SELECT` on `sessions` **plus** `SELECT COUNT(*) FROM messages WHERE session_id=?` (`session_manager.rs:1006`). The `COUNT(*)` grows linearly with conversation length, so long sessions get slower mid-stream — and the token counts it reads were *already computed in-process by the agent* before the event was emitted (`agent.rs:1343`). Pure redundant disk work on the hottest path.
2. **Transport, per delta:** each partial-text delta is independently `serde_json::to_string`'d and pushed as its own SSE frame (`reply.rs:185`). The CLI already has a `stream_coalesce` module that batches deltas — the SSE path has no equivalent.
3. **Client, per delta:** `useChatStream.pushMessage` (`useChatStream.ts:132-169`) rebuilds the whole `messages` array (`[...slice(0,-1), updated]`) and re-stores the full growing message text → O(N·T) array work + O(T²) string growth, and calls `setMessages` **per token** with no batching. `sameContent` even `JSON.stringify`s messages to dedupe (`:128`).
4. **React render, per delta:** because `BioRouterMessage`/`UserMessage`/`ToolCallWithResponse` are **not** `React.memo`'d (`BioRouterMessage.tsx:37`), the new array reference re-renders *every* message in the list, including hundreds of finished ones — each re-running `getToolRequests`/`getToolResponses` scans and an O(N²) request→response map (`BioRouterMessage.tsx:87-112`).
5. **Markdown, per delta:** the streaming message re-runs `wrapHTMLInCodeBlock` inside a `useEffect`+`setState` (double render), then full `ReactMarkdown` + remark/rehype + **KaTeX** + **Prism** highlighting every token (`MarkdownContent.tsx:180-240`, `CodeBlock.tsx:58-139`).
6. **TUI, per delta:** doesn't use `stream_coalesce` at all; `stream_delta` (`tui/app.rs:161-179`) re-parses the *entire* accumulated message as Markdown every token (O(T²)), then `draw_history` clones the whole scrollback (`tui/mod.rs:751`) and re-measures every line's unicode width (`wrapped_count`, `:750`) every frame, with no FPS cap (`:391`).

**Fix (one coherent change across layers):**

- **Server:** carry the agent's already-computed `TokenState` *in* the event payload (remove the DB read); at minimum drop the `COUNT(*)` and cache last token-state in `AppState`, refreshing only on `Finish`.
- **Server:** coalesce partial-text deltas on a ~50–100 ms timer before emitting (reuse the `stream_coalesce` idea on the SSE path).
- **Client:** throttle `setMessages` to a `requestAnimationFrame`/30–60 ms boundary (keep `messagesRef` synchronous for correctness); stop `JSON.stringify` in the hot dedupe path.
- **React:** `React.memo` the three message components + stabilize props in `BaseChat` (`useCallback` `append`, `useMemo` `chat={{sessionId}}`); compute tool-call chains/response-map once in `ProgressiveMessageList`, not per message.
- **React markdown:** move `wrapHTMLInCodeBlock` into `useMemo`; render the *streaming* message as plain text and only switch to the full markdown/Prism/KaTeX pipeline once `!isStreaming`.
- **TUI:** add a dirty-flag + ~60 fps render clock so a burst of tokens coalesces into one redraw; render only the visible viewport slice instead of cloning+measuring the whole scrollback; show a cheap raw-text preview while streaming and run `md_lines` only per coalesced chunk.

This theme alone accounts for most of the "typing/streaming jank" in both GUI and TUI.

### Theme 2 — Blocking I/O on the tokio runtime

Synchronous syscalls on async worker threads stall *all* concurrent tasks. Found in many places:

- **`RequestLog` writes on every request AND every stream chunk** (`providers/utils.rs:473-562`, called in the streaming loop at `:207`): blocking `std::fs` open/write/rename interleaved between every token batch. **Highest-impact** (touches 100% of requests). → buffer in memory, flush via `spawn_blocking`, or send to a logging task over `mpsc`.
- **PDF/DOCX/XLSX parsers** run fully synchronous (`computercontroller/pdf_tool.rs`, `mod.rs:1036-1284`) with no `spawn_blocking` (contrast `mod.rs:949` which does it right).
- **`text_editor`** reads+rewrites whole files with `std::fs` on the async path (`text_editor.rs:784,806,997,1062`).
- **Scheduler** `fs::write` of the whole job list inside async cron callbacks (`scheduler.rs:171,339,431`).
- **Knowledge** git2 commits synchronous on async path (`knowledge/service.rs:~623`); computercontroller `fs::write`/`read_to_string` in async fns (`mod.rs:507,1343`).

→ wrap in `tokio::task::spawn_blocking` or switch to `tokio::fs`. Mostly trivial-to-moderate.

### Theme 3 — Recomputing instead of caching

Work redone every call/turn/frame that depends on inputs that didn't change:

- **Regexes recompiled per call:** `extension_manager.rs:359-384` (per HTTP header at setup), `rmcp_developer.rs:1690`, `knowledge/graph.rs:54`. → `LazyLock` statics. Trivial.
- **Tree-sitter `Query` recompiled per file × 3** (`analyze/parser.rs:211,285,365`). → cache `Arc<Query>` per (lang, kind). Moderate.
- **Knowledge BM25 index rebuilt from scratch per `kb_search`** (`knowledge/store.rs:176-227`) — re-walks the tree, clones every doc, builds a corpus, queries once, throws it away. → cache built engine, invalidate on write. Moderate.
- **Tool catalog deep-cloned per turn** (`extension_manager.rs:710-735`): the `Arc<Vec<Tool>>` cache is defeated by `.cloned().collect()`. → return `Arc::clone` on the unfiltered path. Moderate.
- **Whole conversation re-fixed per turn** (`conversation/mod.rs:164-200`): 7-pass normalization over the *entire* history every iteration, then a second full pass inside `inject_moim` (`moim.rs:43`). → only re-fix the suffix appended since last turn. Moderate.
- **Token counter cache thrown away** (`context_mgmt/mod.rs:184-199`): a fresh cache-less `TokenCounter` is allocated and dropped each call; eviction is random not LRU (`token_counter.rs:49-54`). → hold one shared counter on the agent/session. Moderate.
- **Settings re-read from disk ~20×/launch** (`utils/settings.ts:41-51`): `existsSync`+`readFileSync`+`JSON.parse` per call, several on the startup path. → module-level write-through cache. Trivial.
- **Extension config YAML re-parsed per helper call** (`config/extensions.rs:30-76`): 3 full re-parses for one enable/disable. Moderate.
- **Provider metadata list rebuilt per `/config/providers` request** (`config_management.rs:361`). → `OnceCell`. Moderate.
- **`McpAppCache::new()` rebuilt per `/agent/list_apps`** then delete-all+rewrite (`routes/agent.rs:1024,1049`) — the "cache" is invalidated every read. → hold in `AppState`. Moderate.

### Theme 4 — Whole-object copies where a reference or delta would do

- **Agent loop deep-clones the entire message history 2–3× per turn** (`agent.rs:1288,1137`, `reply_parts.rs:186`, `moim.rs:26`). → thread `Arc<[Message]>` through read-only paths. Moderate but high payoff in long sessions.
- **Per-message DB writes in a serial await loop at end of each turn** (`agent.rs:1781-1783`) + a full-session read-then-write per usage chunk (`reply_parts.rs:349-385`). → batch insert; track metrics incrementally. Moderate.
- **`replace_conversation` rewrites the whole history** on every `/reply` when `conversation_so_far` is sent (`routes/reply.rs:294-309`); also full rewrite on compaction (`session_manager.rs:1191`). (GUI doesn't send `conversation_so_far` today, so this mainly bites other clients.) → append delta. Moderate.
- **`UpdateConversation` SSE event re-sends the entire conversation** mid-stream (`useChatStream.ts:228`, the code's own TODO) and re-parses it on the main thread (`serverSentEvents.gen.ts:204`). → emit message-level patches. Major (protocol change).
- **Auto Visualiser inlines megabytes into every figure**: `mermaid.min.js` is **3.3 MB**, base64'd to ~4.4 MB, stored in session history and re-sent on every chat reopen (`autovisualiser/common.rs:126-273`). → default heavy libs to CDN/proxy-served assets (env flag `BIOROUTER_AUTOVIS_CDN=1` already exists). Moderate.

### Theme 5 — Polling where a push exists; chatty refetches

- **Schedules list full-refetch every 15 s** with no change detection, re-rendering all cards (`SchedulesView.tsx:235-245`).
- **Llama status** fixed-interval poll with no backoff/cap during multi-minute downloads (`LlamaServerInlineCard.tsx:97-117`).
- **`ScheduleDetailView` fetches the entire schedules list to find one** (`ScheduleDetailView.tsx:75-91`) — no single-schedule GET.
- **Session-rename disambiguation** polls `getSession` up to 4× + a full `listSessions` per session for the first 3 turns (`useChatStream.ts:339-399`).
- **`message-stream-finished`** triggers `getSessionExtensions` refetch in bottom-menu components on *every* reply (`BottomMenuExtensionSelection.tsx:41-86`).
- **No shared session-list cache:** multiple views independently `listSessions` the full list (with per-session `extension_data`) on every mount (`SessionListView.tsx`, `SessionsInsights.tsx`).

→ Move schedule/llama updates onto SSE (the infra already exists); add change-detection before `setState`; add a single-schedule route; emit a `session-renamed` push to kill the rename poll; only refetch extensions on `session-created`/explicit toggles; add a shared cached fetch (SWR/react-query already partially present).

## Subsystem detail

Each table rates a finding twice. **Impact** is the latency or resource cost of the problem. **Fix** ranks the size of the change required, on an uncalibrated `Trivial` → `Moderate` → `Major` scale (with hyphenated in-betweens such as `Trivial-Mod`); it is a relative ordering only, and this review does not define the terms further or map them to hours.

### A. Core agent loop, context, conversation (`crates/biorouter/src/agents`, `context_mgmt`, `conversation`)

| # | Finding | Location | Impact | Fix |
|---|---|---|---|---|
| A1 | Whole conversation cloned 2–3×/turn | agent.rs:1288,1137; reply_parts.rs:186; moim.rs:26 | High | Moderate |
| A2 | `fix_conversation` 7-pass over full history/turn, run twice | conversation/mod.rs:164-200; moim.rs:43 | High | Moderate |
| A3 | `check_if_compaction_needed` re-tokenizes history with cache-less counter | context_mgmt/mod.rs:184-199 | High | Trivial-Mod |
| A4 | Per-message serial DB writes at end of turn | agent.rs:1781-1783; reply_parts.rs:349-385 | High | Moderate |
| A5 | `response.clone()` to append one tool result | agent.rs:1517,482,1390 | Med | Trivial |
| A6 | Token cache eviction random, O(n) (not LRU) | token_counter.rs:49-54 | Med | Moderate |
| A7 | Tool inspectors serial; `RepetitionInspector` re-scans full history/turn | tool_inspection.rs:85-117; tool_monitor.rs:122-134 | Med | Moderate |
| A8 | `do_compact` re-formats/re-tokenizes whole history up to 5× on overflow | context_mgmt/mod.rs:282-330 | Med | Moderate |
| A9 | `filter_tool_responses` uses Vec::contains (O(n·k)) | context_mgmt/mod.rs:264-269 | Low-Med | Trivial |
| A10 | `effective_role` allocates String per compare | conversation/mod.rs:427-436 | Low | Trivial |

Already correct: tokenizer is a global `OnceCell`; `get_all_tools_cached` is version-checked; streaming uses `tokio::select!` (no spin); MOIM uses minute-granularity timestamps.

### B. Provider layer (`crates/biorouter/src/providers`)

Good news: `reqwest::Client` is built **once** per provider and reused (`api_client.rs:208`) — no per-request client bug. Streaming is incremental (`utils.rs:190-211`). GCP tokens cached until expiry. Retry has jitter and skips non-retryable classes.

| # | Finding | Location | Impact | Fix |
|---|---|---|---|---|
| B1 | `RequestLog` blocking file I/O per request **and per stream chunk** | utils.rs:473-562, used at :207 | High | Moderate |
| B2 | Full request payload cloned per retry attempt (+ redundant serialize for a debug log even when DEBUG off) | openai.rs:371,406; api_client.rs:348-351 | Med | Moderate/Triv |
| B3 | Error body parsed twice; Google success body parsed twice | api_client.rs:189; utils.rs:182,281 | Med-Low | Trivial |
| B4 | `BIOROUTER_PROVIDER_SKIP_BACKOFF` env read inside retry loop | retry.rs:178 | Low-Med | Trivial |
| B5 | `with_header` rebuilds whole client (re-reads TLS certs) — up to 3-4× at construction | api_client.rs:229-275; openai.rs:136-150 | Low | Moderate |
| B6 | `auto_detect` makes 2 sequential `/models` round-trips per candidate | auto_detect.rs:204,214; base.rs:464 | Low (onboarding) | Trivial |
| B7 | `LeadWorkerProvider` ~10 sequential mutex locks/completion | lead_worker.rs:109-119,343-392 | Low | Moderate |
| B8 | `unescape_json_values` clones whole JSON tree unconditionally | utils.rs:440-444 | Low | Trivial |

### C. Extension manager / MCP (`crates/biorouter/src/agents/extension_manager.rs`, `mcp_utils`, `subprocess`)

Good news: startup is parallel (`agent.rs:800 join_all`), `fetch_all_tools` fans out with per-extension 10 s timeout, dispatch drops the global lock before the tool-call await (different extensions run in parallel).

| # | Finding | Location | Impact | Fix |
|---|---|---|---|---|
| C1 | `substitute_env_vars` recompiles 2 regexes per call (+ clones header) | extension_manager.rs:359-384 | Med-High (HTTP ext setup) | Trivial |
| C2 | Tool dispatch: 3 lock acquisitions + O(N) `starts_with` scan per call | extension_manager.rs:857-864,1123,1157 | Med | Moderate |
| C3 | Per-turn deep clone of whole tool catalog (Arc cache defeated) | extension_manager.rs:710-735 | Med | Moderate |
| C4 | `get_ui_resources` lists resources serially; `collect_moim` serial per turn | extension_manager.rs:977-1009,1366-1405 | Med | Moderate |
| C5 | `get_extensions_map()` re-parses YAML per helper call (3×/op) | config/extensions.rs:30-76 | Low | Moderate |
| C6 | `dispatch_tool_call` clones args instead of moving | extension_manager.rs:1171 | Low | Trivial |

### D. Session persistence (`crates/biorouter/src/session`)

Good news: persistence is incremental (no per-turn full rewrite except compaction); WAL on; key indexes present; async sqlx.

| # | Finding | Location | Impact | Fix |
|---|---|---|---|---|
| D1 | Chat history search = full JSON table scan, no FTS (leading-wildcard LIKE) | chat_history_search.rs:124-172 | High (grows w/ history) | Major (FTS5) |
| D2 | N+1 `COUNT(*)` per matched session in search | chat_history_search.rs:222-237 | Med-High | Trivial (GROUP BY) |
| D3 | `synchronous` pragma unset → FULL fsync per commit | session_manager.rs:542-548 | Med | Trivial (NORMAL) |
| D4 | `add_message` = txn + 2 writes per message | session_manager.rs:1162-1189 | Med | Trivial-Mod |
| D5 | No `max_connections` cap → write-lock contention | session_manager.rs:548 | Med-Low | Trivial |
| D6 | `truncate`/`get_conversation` filter/order on unindexed columns | session_manager.rs:1411,1132 | Low | Trivial (composite index) |
| D7 | Session list JOINs+GROUPs all messages to count | session_manager.rs:1234-1264 | Low-Med | Moderate (denormalize count) |
| D8 | `maybe_update_name` deserializes whole conversation to count users | session_manager.rs:333-404 | Low | Moderate |

### E. Server / Axum (`crates/biorouter-server`)

Good news: SSE flushes immediately; agents reused per session (pre-warmed); no global lock across await; `restart_agent_internal` parallelized.

| # | Finding | Location | Impact | Fix |
|---|---|---|---|---|
| E1 | 2 SQLite queries per streamed event (token state) | reply.rs:356,362 → session_manager.rs:983 | High | Moderate |
| E2 | No HTTP compression anywhere (large config/session/tools JSON) | commands/agent.rs:56-61 | High (big endpoints) | Trivial (CompressionLayer, skip SSE) |
| E3 | `McpAppCache::new()` per `/agent/list_apps` + full rewrite | routes/agent.rs:1024,1049 | Med | Moderate |
| E4 | `get_providers()` rebuilt per config/provider request | config_management.rs:361,409,634 | Med | Moderate |
| E5 | `replace_conversation` full write at start of `/reply` | reply.rs:294-309 | Med | Moderate |
| E6 | Auth rate-limit takes global mutex on success path too | auth.rs:17-26,62-73 | Low | Trivial |
| E7 | Tunnel buffers full response, lossy UTF-8 of binary | tunnel/lapstone.rs:340,162,231 | Low (opt-in) | Moderate |

### F. Built-in MCP servers (`crates/biorouter-mcp`)

| # | Finding | Location | Impact | Fix |
|---|---|---|---|---|
| F1 | Auto Visualiser inlines 3.3 MB Mermaid (→4.4 MB base64) per figure | autovisualiser/common.rs:126-273 | High | Moderate (CDN/proxy default) |
| F2 | PDF/DOCX/XLSX parse synchronous on async runtime | pdf_tool.rs; computercontroller/mod.rs:1036-1284 | High | Moderate (spawn_blocking) |
| F3 | BM25 index rebuilt per `kb_search` | knowledge/store.rs:176-227 | High | Moderate (cache+invalidate) |
| F4 | Blocking `std::fs` in async tool fns | computercontroller/mod.rs:507,1343,710 | High (big payloads) | Trivial (tokio::fs) |
| F5 | tree-sitter `Query` recompiled per file ×3 | analyze/parser.rs:211,285,365 | High (dir analysis) | Moderate (cache Arc<Query>) |
| F6 | text_editor reads+rewrites whole file per edit | text_editor.rs:784-1094 | High (large files) | Moderate |
| F7 | Knowledge `list_pages`/`list_sources` re-walk + re-parse YAML per call | knowledge/store.rs:25-70; raw.rs:43-59 | Med | Moderate |
| F8 | git2 commits synchronous on async path | knowledge/service.rs:~623 | Med | Moderate |
| F9 | AnalysisCache small (100) + getter deep-clones the Arc | analyze/cache.rs:52-58; mod.rs:287 | Med | Trivial-Mod |
| F10 | Dir traversal re-walked per call | analyze/traversal.rs:103-142 | Med | Moderate (TTL cache) |
| F11 | Misc regexes per call; `git_context_block` 3 subprocesses/`get_info` | rmcp_developer.rs:1690,53-99; graph.rs:54 | Low | Trivial |

### G. Electron main process and startup (`ui/desktop/src/main.ts`, `biorouterd.ts`, `preload.ts`)

| # | Finding | Location | Impact | Fix |
|---|---|---|---|---|
| G1 | First paint blocks on backend health check — `loadURL` runs *after* `await checkServerStatus` | main.ts:666→838; biorouterd.ts:30-55 | High | Moderate (loadURL first, overlap) |
| G2 | Health check is 100 ms poll, not event-driven | biorouterd.ts:30-55 | High | Moderate (backoff / READY line on stdout) |
| G3 | `loadSettings()` uncached, ~20× per launch (sync fs) | utils/settings.ts:41-51 | Med | Trivial (cache) |
| G4 | `get-secret-key` re-reads settings for a process constant | main.ts:548,1374 | Med | Trivial |
| G5 | Renderer does 2 sequential awaited IPC RTTs pre-render | renderer.tsx:52,62 | Med | Trivial (Promise.all / use injected config) |
| G6 | `getVersion` uses synchronous IPC (`sendSync`) | preload.ts:361-363 | Med | Trivial |
| G7 | Dependency checks use `spawnSync` (blocks event loop on timeout) | dependencyChecker.ts:146,362,542 | Low-Med | Moderate (async spawn) |
| G8 | Logs full redacted env as pretty JSON per launch | biorouterd.ts:154-168 | Low | Trivial |

Already correct: preload is `invoke`-based; updater/dep/extension checks already deferred 2–8 s; stderr ring buffer bounded.

### H. React rendering and state (`ui/desktop/src/components`, `contexts`)

| # | Finding | Location | Impact | Fix |
|---|---|---|---|---|
| H1 | Message components not `React.memo` → whole list re-renders/token | BioRouterMessage.tsx:37; ToolCallWithResponse.tsx:149; UserMessage.tsx:17 | High | Moderate |
| H2 | Inline arrow/object props in BaseChat defeat memo | BaseChat.tsx:629,614,627; ProgressiveMessageList.tsx:222 | High | Trivial-Mod |
| H3 | Markdown re-parsed + Prism + KaTeX per token; effect causes double render | MarkdownContent.tsx:180-240; CodeBlock.tsx:58-139 | High | Moderate |
| H4 | O(N²) tool-chain/response-map recompute per token | ProgressiveMessageList.tsx:169; BioRouterMessage.tsx:87-112 | High | Moderate |
| H5 | No virtualization for long message/session lists | ProgressiveMessageList.tsx; SessionListView.tsx:764 | Med | Major (react-window) |
| H6 | `ConfigContext` value churns → all consumers re-render | ConfigContext.tsx:274-324 | Med | Moderate (split data/actions) |
| H7 | BaseChat reverses whole `messages` per token for dashboard preview | BaseChat.tsx:185-216 | Med | Trivial |
| H8 | `ChatInput` (1802 lines) not memoized, re-renders per token | ChatInput.tsx:128 | Med | Moderate |
| H9 | `SessionItem`/`SessionSkeleton` defined inside render body (memo useless) | SessionListView.tsx:536,704 | Low-Med | Trivial (hoist) |
| H10 | `ToolCallView` reads localStorage + adds window listeners per tool call | ToolCallWithResponse.tsx:332-347 | Low | Trivial-Mod |

### I. Frontend API client / fetching (`ui/desktop/src/hooks`, `api`)

| # | Finding | Location | Impact | Fix |
|---|---|---|---|---|
| I1 | O(n²) per-token message-array copy + per-token setState | useChatStream.ts:132-169,209 | High | Moderate (throttle to frame) |
| I2 | `UpdateConversation` re-sends/re-parses whole conversation | useChatStream.ts:228; serverSentEvents.gen.ts:204 | High | Major (delta protocol) |
| I3 | Schedules polled full-list every 15 s, no change detection | SchedulesView.tsx:235-245 | Med | Trivial-Mod |
| I4 | Llama status poll: no backoff/cap | LlamaServerInlineCard.tsx:97-117 | Med | Trivial |
| I5 | `ScheduleDetailView` fetches whole list to find one schedule | ScheduleDetailView.tsx:75-91 | Med | Moderate |
| I6 | Rename disambiguation: up to 4 `getSession` + full `listSessions`/session | useChatStream.ts:339-399 | Med | Moderate (server push) |
| I7 | `message-stream-finished` triggers extension refetch every reply | BottomMenuExtensionSelection.tsx:41-86 | Med | Trivial-Mod |
| I8 | No shared session-list cache across views | SessionListView/SessionsInsights | Low-Med | Moderate |
| I9 | `SessionsInsights` no AbortController on unmount | SessionsInsights.tsx:25-86 | Low | Trivial |

### J. Frontend build / bundle (`ui/desktop`, Vite + Forge)

Electron loads JS from local disk, so cost is parse/eval+memory, not download — but startup and memory still matter.

| # | Finding | Location | Impact | Fix |
|---|---|---|---|---|
| J1 | No route-level code splitting — all 14 routes eager | App.tsx:22-51 | High | Moderate (lazy + Suspense) |
| J2 | `react-syntax-highlighter` Prism bundles all 594 languages | MarkdownContent.tsx:8 | High | Moderate (prism-async-light + register ~12) |
| J3 | `react-icons` declared but unused — 83 MB latent footgun | package.json:80 | High (install/footgun) | Trivial (remove) |
| J4 | No `manualChunks`/vendor splitting | vite.renderer.config.mts | Med | Moderate |
| J5 | No bundle analyzer configured | — | Med | Trivial (visualizer) |
| J6 | KaTeX CSS+fonts eager on chat path | MarkdownContent.tsx:6-7 | Med | Moderate (lazy/gate on `$`) |
| J7 | `react-force-graph-2d`+`d3-force` eager via Knowledge route | App.tsx:43 | Med | Trivial once J1 done |
| J8 | One full `import {isEqual} from 'lodash'` | BottomMenuAlertPopover.tsx:3 | Low-Med | Trivial (lodash/isEqual) |
| J9 | Renderer config lacks `build.target:'esnext'` for Chromium-only | vite.renderer.config.mts | Low | Trivial |

Heaviest installed deps: react-icons 83 M (unused), @radix-ui/themes 9.1 M (one component), react-syntax-highlighter 8.7 M (eager), lodash 4.9 M, katex 4.4 M (eager), refractor 2.8 M (eager), react-force-graph-2d 1.6 M.

### K. CLI / TUI (`crates/biorouter-cli/src/session`)

Root cause: the TUI doesn't use `stream_coalesce` (it's wired only into the classic readline path) and has no render-rate cap.

| # | Finding | Location | Impact | Fix |
|---|---|---|---|---|
| K1 | Whole in-progress message re-parsed as Markdown per token (O(T²)) | tui/app.rs:161-179 ← tui/mod.rs:455 | High | Moderate |
| K2 | Unbounded redraw rate — one full draw per token | tui/mod.rs:391-394,151 | High | Moderate (dirty flag + FPS clock) |
| K3 | Whole scrollback re-wrapped (`wrapped_count`) every draw | tui/mod.rs:750-777 | High | Moderate (cache/incremental) |
| K4 | Full `scrollback.clone()` every draw | tui/mod.rs:751 | High | Trivial-Mod (viewport slice) |
| K5 | Unbounded scrollback kept fully laid out | tui/app.rs:84-86 | Med | Moderate (ring buffer) |
| K6 | Input handling shares loop with render+stream consume → keystroke lag | tui/mod.rs:391-484 | Med | Moderate (resolved by K1/K2) |
| K7 | Cursor position recomputed by re-wrapping input every draw | tui/mod.rs:954-977 | Med | Moderate (cache) |
| K8 | Completion list re-sorts full catalog per keystroke | tui/app.rs:210-243 | Low | Trivial |

### L. Cross-boundary and scheduler

Covered by Themes 1, 2, 5. Confirmed clean: CLI↔library is in-process (no JSON round-trip); Agent↔MCP tool dispatch is already concurrent (`agent.rs:1473`); `Config::global()` is a singleton (not re-read per call). Scheduler-specific: `persist_jobs` rewrites the whole job file up to 4× per firing with blocking `fs::write` (`scheduler.rs:161-173,255-293,577-608`).

## Recommended roadmap

The 20 items below carry no status column. To tell what was actually built from what was only proposed, see "What happened next" at the end of this document.

### Tier 0 — Trivial, high ROI (a day or two total)

1. **Server `synchronous=NORMAL`** (D3) + **`max_connections` cap** (D5) + **N+1→GROUP BY** in search (D2).
2. **HTTP `CompressionLayer`**, excluding the SSE route (E2).
3. **Cache `loadSettings()`** write-through (G3, G4) and **`Promise.all` the startup IPC** / read from injected config (G5, G6).
4. **Hoist regexes to `LazyLock`** (C1, F11) and **`react-icons` removal** + **`lodash/isEqual`** (J3, J8).
5. **`tokio::fs` for the easy blocking calls** (F4) and **drop the `COUNT(*)`** from the token-state path (E1 partial).

### Tier 1 — The streaming hot path (the big perceived-snappiness win)

6. **Throttle React `setMessages` to a frame** + **`React.memo` the 3 message components** + **stabilize BaseChat props** (I1, H1, H2).
7. **Server: carry `TokenState` in the event** (remove per-delta DB read) + **coalesce partial-text deltas** (E1, Theme 1).
8. **Markdown: `useMemo` the wrap; plain-text streaming, full pipeline only on finalize** (H3).
9. **Compute tool-chain/response-map once** (H4).
10. **TUI: dirty-flag + FPS clock; viewport-slice rendering; wire in `stream_coalesce`** (K1–K4).

### Tier 2 — Moderate refactors

11. **Agent loop:** stop deep-cloning + double-fixing the whole history per turn (A1, A2); shared token counter (A3, A6); batch DB writes (A4).
12. **`spawn_blocking` the heavy MCP parsers** + tree-sitter query cache + BM25 cache (F2, F5, F3, F6).
13. **`RequestLog` off the hot path** (B1).
14. **Electron: overlap renderer load with backend spawn; event-driven readiness** (G1, G2).
15. **Route-level code splitting + syntax-highlighter slimming + manualChunks/analyzer** (J1, J2, J4, J5).
16. **Auto Visualiser: default heavy libs to CDN/proxy** (F1).
17. **Cache `McpAppCache`/provider metadata in `AppState`** (E3, E4); **scheduler: coalesce persists, async I/O** (Theme 2/5).

### Tier 3 — Major (do when the cheaper wins are exhausted)

18. **SQLite FTS5** for chat history search (D1).
19. **Delta SSE protocol** to replace `UpdateConversation` whole-conversation sends (I2).
20. **List virtualization** for message/session lists (H5).

## What's already good (don't "fix")

- Providers reuse one `reqwest::Client`; streaming is incremental; GCP tokens cached; retry is sane.
- MCP startup parallel; tool catalog version-cached; tool dispatch concurrent across extensions; per-extension timeouts.
- Sessions append incrementally (no per-turn full rewrite); WAL on; async sqlx; key indexes present.
- Server reuses pre-warmed agents; SSE flushes immediately; no locks across await.
- Tokenizer is a global `OnceCell`; CLI↔library is in-process; `Config::global()` is a singleton.
- Preload is async/`invoke`-based; updater/dependency checks already deferred.

## What happened next

Nine fixes drawn from this review were implemented on 2026-06-23 and merged to `main`. [The implementation log](implementation-log.md) records them commit by commit, with behaviour-preservation evidence and before/after benchmarks, and it is the authoritative answer to "did finding X get fixed?" — this document deliberately keeps no status column of its own.

That log also names the findings deliberately **deferred**: H1/H2/H4, J1, J2, C3, B1, F1/F3/F5, K1/K2, A1/A2/A4, and I2.

Two days later a separate performance effort, driven by a comparison against the third-party jcode harness rather than by this review, took up several of the same themes from a different angle — see [the comparison analysis](jcode-comparison-analysis.md) and [its implementation report](jcode-borrows-implementation-report.md).

## Related documentation

- [Performance fixes: implementation log and benchmarks](implementation-log.md) — which of these findings shipped, when, and what the fix measured.
- [Performance and efficiency comparison: jcode vs BioRouter](jcode-comparison-analysis.md) — an independent second look at the same subsystems from an external benchmark.
- [Terminal UI stability review](../subsystem-reviews-2026/terminal-ui-stability.md) — a later review of the CLI/TUI subsystem covered in section K.
- [Desktop reliability defects](../subsystem-reviews-2026/desktop-reliability-defects.md) — a later review of the Electron and React surfaces covered in sections G–J.
- [System overview](../../architecture/system-overview.md) — the architecture the 12 subsystem boundaries were drawn from.
