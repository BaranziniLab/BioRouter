# Privacy tiers — implementation plan

> **What this is.** The task-by-task execution plan for the privacy-tier capability system
> designed in [`privacy-tiers.md`](privacy-tiers.md) ([issue #56](https://github.com/BaranziniLab/biorouter/issues/56)):
> forty-eight tasks in seven phases — forty numbered, plus **4b** (resolve every test filter against a
> real `cargo --list`), **10A, 10B, 10C and 10D**, the knowledge-base tier the operator ruled on
> after the first adversarial review, and **14A, 14B and 14C**, the read-deny sandbox the operator
> ruled on after the fifth — each with a Files table, a failing test, complete
> implementation code, a run step, a gate that fails a plausible wrong implementation, and one commit.
> **Status:** Proposed — ready to execute. The design's rulings are settled (see
> [Decisions of record](#decisions-of-record)); the costs the operator knowingly accepted are in
> [Accepted risks](#accepted-risks); **eighteen** questions remain open (see
> [Open questions](#open-questions)) — the design's eleven minus the one the fifth-round ruling
> closed (question 3), plus eight this plan surfaced — and none of them blocks Phase 0–3.
> **Audience:** the engineer or agent implementing issue #56, and the reviewer of its PRs.

> **For agentic workers:** follow the subagent-driven-development or executing-plans skill and
> work task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Work in the worktree
> `/Users/wgu/Desktop/BioRouter-privacy` on branch `feat/privacy-tiers` (forked from `main` at
> `9558c346`). **Never implement on `main`.**

> **Revision note (second adversarial round).** An adversarial verifier read the first version of
> this plan and returned "another round". Every finding it raised is fixed here: the knowledge-base
> laundering path is now Tasks 10A–10C under an explicit operator ruling (§[Accepted risks](#accepted-risks));
> the `floor` caller-audit test was rewritten (it could not pass at any point, and two later phase
> gates ran it); six gates whose expected count would have failed a *correct* implementation were
> recomputed or replaced with anchored patterns; one vacuous gate (`awk 'NR==247'`) was re-anchored
> to a symbol; the contrast total agrees in both places that quote it (now **288** — see the third
> round's note below); and the hand-traced type
> errors S1–S9 are corrected. Two further defects this pass found on its own are also fixed: the
> `privacy::` test count in Task 7's gate was 4 where the code produces 5, and
> `POST /knowledge/bases/{id}/ingest-conversation` is a **third** fully-open cross-session read that
> the first version's Gate G did not cover. See
> [Which test filters are validated, and which are not](#which-test-filters-are-validated-and-which-are-not).

> **Revision note (third round — the gates were RUN).** A verifier executed every runnable gate
> command in this plan against the worktree and compared the real output with each stated
> expectation. It found roughly eighteen broken or vacuous gates, and this revision fixes all of
> them. The four classes, because the classes matter more than the list:
>
> - **Gates that fail a correct implementation.** A counted `fn tier(&self)` gate satisfiable by
>   deleting an override (Task 5); an `awk` whose START matched two functions and concatenated them
>   (Task 15's `list_prompts`); an `awk` whose END pattern occurred nowhere in the file, so the range
>   ran to EOF (Task 10); a `-v` filter that did not match the negated form of the assertion it meant
>   to exclude (Task 30); two `cargo test -p biorouter-mcp --test mcp_integration_test` invocations
>   naming a target that lives in `crates/biorouter/tests/`, which cargo hard-errors on (Tasks 20,
>   40); and a contrast total of **294** that is unreachable — three of its eighteen new assertions
>   fail AA and the same task forbids the theme edit that would fix them (Tasks 26, 32; now **288**,
>   with the deferred pair recorded as [Open question 16](#open-questions)).
> - **Gates that pass vacuously**, which is worse, because they are reported as verification:
>   `grep -c '"409"'` (already 6 before any #56 code); `grep -rn "PrivacyInspector"` in three places
>   (a name this plan invented — 0 today and 0 under every wrong implementation, so green both ways;
>   now an enumeration of the real `impl ToolInspector for` set); `grep -c "sessionId"` (already 17);
>   Task 33's registry-fixture loop, whose script has **no argv handling** and whose fixture
>   directory does not exist, so any error — a bad flag, an ENOENT, a `ReferenceError` — satisfied
>   it; and Task 37's `npx vitest run registry -t "…"`, which matches three unrelated suites, filters
>   every test out, and prints green having run **zero** of the four tests it exists to protect.
> - **A whole task on a false premise.** Task 2 claimed the daemon-secret leak in the stdio MCP spawn
>   was live. It was closed in `b249a203` + `8e7407fe`, **both ancestors of this plan's own
>   verification anchor**, and a passing test already covers it. The task is rewritten as a
>   pin-and-correct; Task 3's gate, which inherited the same false baseline, is rewritten with it.
> - **A pre-count table wrong in the direction that hides a no-op.** §[Which test filters are
>   validated](#which-test-filters-are-validated-and-which-are-not) said `routes::agent` and
>   `routes::session` had no test module. They have **8** and **20** tests respectively, so a worker
>   told to expect zero reads `8 passed` as "my tests landed" when none did.
>
> Two rules were applied throughout and are worth stating once. **A named `cargo test` filter is
> never gated on "PASS"** — libtest prints `0 passed` and exits 0 when a filter matches nothing, so
> every such line now asserts the printed count. And **an `awk` range gate asserts its own
> non-emptiness first**, because `grep -c` over no output is `0`, which reads identically to a
> correct absence.

> **Revision note (fifth round — the channel that needs no tool call).** A reviewer read the
> worktree against `main` and returned one CRITICAL: **a public-capability model never has to defeat
> CP1–CP5, because it can read the private material straight off the filesystem.** `developer__shell`
> runs an arbitrary command, the shell is explicitly not jailed by the file tools' base
> (`rmcp_developer.rs:1950`), the OS sandbox that could confine it defaults to **Off**
> (`shell_sandbox/mod.rs:244`), `computercontroller__automation_script` executes a model-written
> script, SecretGuard's floor covers credentials but not the session database, knowledge or memory
> roots (`secret_guard.rs:28`) and its scan is lexical and existence-gated (`:265`), and `sessions.db`
> carries a **contentful** FTS mirror of every message by design (`session_manager.rs:29`). The design
> named this channel at `privacy-tiers.md:692` and this plan never closed it.
>
> **The operator ruled**, and the ruling is implemented rather than re-argued: a mandatory read-deny
> sandbox for public-capability sessions, **on by default** (DR-14, Tasks 14A–14C), plus a single
> master toggle that disables the whole feature (DR-15, Task 30 rewritten). The sandbox hides four
> directories and nothing else, so private sessions and ordinary work are untouched; where a platform
> cannot express the exclusion the fail direction is **closed** and the tool is refused. What the
> ruling did not settle and this revision decides: the toggle's key, scope and default; that badges
> stay visible and say "enforcement off" rather than disappearing; that the ratchet stops with
> everything else, with the permanent consequence written out as AR-7; the four roots, resolved from
> real symbols across **two** directories (`data_dir` for the session store, `config_dir` for the
> other three); and that Landlock cannot express a read-deny at all, so on Linux the control needs
> `bubblewrap` (Open question 17).

> **Revision note (sixth round — three surfaces, and six gates that could not fail).** The same
> reviewer returned three HIGH findings and a table of six gates that pass a plausible wrong
> implementation. All nine are fixed here.
>
> The three surfaces are each a *right value read in the wrong place*, which is why no count-based
> gate saw any of them. **`kb_get_active` takes no arguments and returned the whole selection** —
> every visible base id, plus the primary pointer — so one call enumerated what `kb_list_bases`
> omits four functions away; the plan had exempted it on a premise (*"the caller already knows the
> id"*) that is false for a no-argument tool, and its completeness test named it as exempt, which is
> to say blessed it. **CP3's mid-turn call site** (`routes/apps.rs:3847`) sits after `turn_agent` is
> bound at `:3541`, so sending all three call sites to `agent` attributes a **worker's** KB access
> to the main agent — wrong in both directions, and it compiles because both agents are in scope.
> **CP5's capability read** was placed at `capability_report`'s existing position, two lines above
> `configure_main_provider`, so it reads the provider the session held *before* the app's own model
> was bound. Fixes: filter the pointer tools' **view** (never the store — `repair_decision` writes
> `next_ids.first()`, so filtering the store would re-point the user's primary as a side effect of a
> read); read `turn_agent` at the one site that has one; and move the report below the bind. Design
> amended once, as B4.1, because a `null` pointer is user-visible.
>
> The six gates share one failure: **each could be satisfied without the behaviour being right.**
> Task 4b's deferral was keyed on a filter *name*, so `-p biorouter-mcp --lib privacy::refusal` — a
> filter that will never name a test — was reported DEFER; it is now keyed on the `(package, filter)`
> pair, with every deferred row evidenced. Tasks 10B/11 forbade only the *trusting* literal, so
> hardcoding `Public`/`false` under-ratcheted and passed; both directions are now forbidden and each
> production caller has a behavioural private/public matrix. Task 10C's completeness test *was* its
> exemption list; the exemptions now carry the test that pins them, and a universal
> "volunteers nothing" property covers the twentieth. Task 10D's metadata detector excluded
> `src/knowledge/`, i.e. the module the leak was in; it is two sweeps now, with `.selection(` in the
> pattern, plus a register of every metadata-returning **tool**. Task 10A required only that the tier
> not travel — leaving export-private → import-public copying a whole private base in two permitted
> calls; closed with a raise-only provenance marker and a model-export destination inside deny root
> #2, with AR-8 for what remains. And Task 12's race ran 200 unconstrained spawns on a
> `current_thread` runtime under a conditional assertion that was **false for a correct
> implementation**; it is now two forced interleavings behind `#[cfg(test)]` seams, plus a
> multi-threaded fuzz loop that must prove it raced.

> **Revision note (fourth round — the barrier's edges, and the first real `cargo` run).** A verifier
> re-derived the four choke points independently and could not break them on content: it read
> `rmcp-macros-0.14.0/src/tool_handler.rs` and confirmed the hand-written `call_tool` body is
> verbatim, that `ToolCallContext::new` is `pub`, and that all four named surfaces are genuinely
> covered (it also found `KbToolDispatch` has **seven** tools, not five — `kb_classify_source` and
> `kb_list_pages` too, both still covered). **The architecture is settled and is not re-opened here.**
> What broke was around the edges: two surfaces hand a public model the base ids CP1 then refuses,
> and three ordering/packaging defects left nine consecutive commits failing `cargo test`. One fix
> per blocker, each checked for the mirror defect the way the verifier checked:
>
> | # | Blocker | Fixed by | Mirror check |
> |---|---|---|---|
> | **B1** | `list_platform_catalog` (`agent_drafter/mod.rs:2626`) serialises `{id, name}` for **every** base, and `validate::check_*` renders the same list into three rejection strings — an enumeration oracle needing no valid input. Neither of Task 10C's new-surface detectors can see it: both key on `store::`/service **content** calls and this goes through `list_bases` | **New [Task 10D](#task-10d-the-metadata-surface--cp5-because-a-barrier-that-names-what-it-refused-has-not-refused-it)** — CP5 at `Catalog::discover`, a measured 6-production-caller choke point, plus a metadata new-surface detector (two sweeps: 27 hits / 18 production outside `knowledge/`, 22 / 5 inside) | Swept every `list_bases`/`session_kb_ids` caller outside `knowledge/` by hand — 20, all classified. Found the **third** instance, `resolve_target_kb` (below) |
> | **B2** | `gated_kb_id`'s deliberate fall-through lets `kb_id_or_primary` (`server.rs:323-341`) answer with `"Pass kb_id explicitly (one of: default, omop)"`, built from a list filtered on `hidden` only — while the same task asserts `kb_list_bases` returns `["default"]` | **Task 10C** gains a third filter and two tests | The mirror is `resolve_target_kb` (`knowledge_tool.rs:149`), the same shape in `biorouter`, which Task 10C cannot reach. **Fixed in Task 11**, with its own test |
> | **B3** | `crates/biorouter-mcp/tests/knowledge_macros_e2e.rs` constructs `IngestArgs`/`QueryArgs` and was in no Files table, no `git add` and no run step; every `cargo test -p biorouter-mcp` here is `--lib` | **Task 10B** adds the file, a `--test` line and `cargo check --workspace --all-targets`; **O13** states the rule | Swept `crates/*/tests/` for every changed type: that file is the only out-of-lib constructor of the three macro `Args`, and Task 10D's `Catalog::discover` has two more (`catalog_write_boundary.rs`, `testdrive_corpus_relint.rs`) — both now listed |
> | **B4** | 10B made `IngestArgs.caller_is_private` required, which makes `conversation_ingest.rs:205` a compile error with nothing to pass; the field was reserved for Task 11. The only compiling answer was a hardcoded `false`, which reproduces §10A ⚠(3) verbatim — *"every per-file gate reported green"* | **`caller_capability` moves to Task 10B**, together with all three callers; Task 11 keeps only the guard, and its Step 2 now expects FAIL rather than COMPILE ERROR | Checked the inverse: 10B's gate now also greps for a hardcoded `ProviderTier::Private` / `caller_is_private: true`, which is the way to compile while disabling the ratchet |
> | **B5** | `awk KB_RATCHETING_TOOLS \| grep -c '"kb_'` expects **3** and measures **1**: the const is 94 characters, rustfmt keeps it on one line, and `grep -c` counts lines | **`grep -o … \| wc -l`** in both 10B and 10C | The sibling `KB_ID_GATED_TOOLS` gate happens to measure 14 only because rustfmt explodes *that* array — same gate, opposite side of a formatter's line-wrap. Both converted |
>
> **Also in this round: the first `cargo` this plan has ever run.** Every previous pass ended with
> *"nothing has been compiled or run"*, and the last verifier named an actual `cargo test -- --list`
> after Task 4 as the single thing that would most change its confidence. That is now
> **[Task 4b](#task-4b-resolve-every-test-filter-against-a-real-cargo---list-docs-only)**, a short
> docs-only task placed immediately after Task 4, and its Step 1 was executed while writing this
> revision — against `main` at `89c1f026`, whose only difference from the plan's anchor is six
> developer-only files. Four of the five packages listed clean and the measured counts are pasted
> into the task. Two of the plan's own numbers were wrong and are corrected there;
> `agents::chatrecall_extension` and `session::chat_history_search` really are **0**, as claimed, and
> `routes::agent` = 8 / `routes::session` = 20 are confirmed exactly. What remains unrun is stated in
> the task rather than implied: seven modules this plan creates cannot be listed until they exist,
> and Tasks 20 and 40 re-run the audit with a shrinking and then an empty deferred set.

**Goal.** Two lattices, one column pair, five gates plus four the design did not name. A session's
**capability** (what it may *do*) is the least-privileged model bound to it; its **classification**
(how sensitive its *contents* are) is the most sensitive thing it has touched, ratcheted
permanently in SQL. A public model must never reach a private session — not once, not read-only,
not indirectly.

**This plan produces no design decisions.** Where it departs from `privacy-tiers.md` it says so and
gives the measurement that forced the departure; there are six such departures and they are
collected in [Departures from the design](#departures-from-the-design).

---

## Read this before you chase a line number

**Every Rust anchor in `privacy-tiers.md` is stale.** The design was verified against `main` at
`708390d8`; this worktree forked at `9558c346`. The three files the design cites most moved by
+150 to +720 lines. Every anchor in *this* document was re-verified by reading the code in this
worktree at `9558c346` on 2026-07-28, and every Files table below says so. The design itself now
carries a banner saying its anchors are historical; Task 1 Step 2 replaces its §20 anchor list with
a pointer to the drift table below.

**Confirmed still valid at `main` = `89c1f026`** (2026-07-28, one day after the fork). `main` moved
four commits past the fork point, and `git diff --stat 9558c346 89c1f026` touches exactly six files,
all of them in the Developer MCP server: `crates/biorouter-mcp/src/developer/rmcp_developer.rs`,
`crates/biorouter-mcp/src/secret_guard.rs`, and four new `crates/biorouter-mcp/tests/developer_*.rs`
files (issues #64, #67, #68 — the file-tool jail). **No file this plan anchors into changed**:
`crates/biorouter/src/**`, `crates/biorouter-server/src/**`, `crates/biorouter-mcp/src/knowledge/**`,
`crates/biorouter-mcp/src/memory/**`, `crates/biorouter-mcp/src/developer/shell.rs`,
`crates/biorouter-sandbox/src/**`, `ui/desktop/**` and `landing/**` are byte-identical between the
two commits. Re-run that `git diff --stat` before starting; if it now names a file this plan
anchors into, re-verify that file's anchors before trusting them.

**The named SYMBOL is the anchor, never the line number.** Knowing the size of the drift is what
keeps a near miss from reading as a hit — the expensive failures in BR-71's history were all near
misses (`extension_manager.rs:999` is `get_extension_configs`, one method past the
`is_extension_enabled` at `:987` the task meant).

| Symbol | `privacy-tiers.md` says | **This worktree at `9558c346`** |
|---|---|---|
| `Agent::update_provider` | `agent.rs:4936-4956` | `crates/biorouter/src/agents/agent.rs:5655` |
| `Agent::restore_provider_from_session` | `:4960-4986` | `:5679` |
| `Agent::provider()` | `:2017-2022` | `:2511` |
| `Agent::handle_denied_tools` | `:1961-2000` | `:2455` |
| `Agent::reply` | "top of" (unnumbered) | `:3258` (**and the top is the wrong seam — see Task 13**) |
| `Agent::dispatch_tool_call` | (unnumbered) | `:2624` |
| `ExtensionManager::add_extension` | `extension_manager.rs:532` | `:674` |
| `ExtensionManager::add_client` | `:737` | `:879` |
| `ExtensionManager::add_inprocess_server` | `:759` | `:901` |
| `ExtensionManager::filter_tools` | `:877-902` | `:1027` |
| `ExtensionManager::get_all_tools_cached` | `:904-933` | `:1054` |
| `ExtensionManager::get_client_for_tool` | `:1033-1040` | `:1183` |
| `ExtensionManager::read_resource_tool` | `:1043` | `:1193` |
| `ExtensionManager::read_resource` | `:1116` | `:1266` |
| `ExtensionManager::get_ui_resources` | `:1153` | `:1303` |
| `ExtensionManager::list_resources` | `:1226` | `:1376` |
| `ExtensionManager::dispatch_tool_call` | `:1288` | `:1438` |
| SecretGuard choke-point comment | `:1351` | `:1499-1502` |
| `ExtensionManager::list_prompts_from_extension` | `:1428` | `:1578` |
| `ExtensionManager::list_prompts` | `:1458` | `:1608` |
| `ExtensionManager::get_prompt` | `:1505` | `:1655` |
| `Frontend` variant refusal | `:691-693` | `:833-836` |
| `copy_session` | `session_manager.rs:4138-4168` | `:4710` |
| `diverge_session` | `:4204-4265` | `:4776` (builder at `:4824-4836`) |
| `import_session` | `:4096-4135` | `:4668` (builder at `:4683-4700`) |
| `list_sessions` type filter | `:3537` | `SessionManager::list_sessions` at `:1403`-adjacent; storage `list_session_summaries` at `:4090` |
| `add_update!` emission | `:2876-2880` | `:3126-3132` |
| `COALESCE` accumulation precedent | `:2852-2854` | `:3104-3124` |
| `sessions` DDL tail | ends `branch_point_msg_uid` | ends `incarnation INTEGER NOT NULL DEFAULT 0` at `:2096`, **no trailing comma**; `diverged_from TEXT,` at `:2093` still holds as the insertion anchor |
| `memory` global injection | `memory/mod.rs:207-247` | **claim is false** — see [Design correction 3](#task-1-correct-the-design-against-the-tree-docs-only) |
| `build_shell_command` | `developer/shell.rs:337-359` | **does not exist** — the function is `configure_shell_command` at `:330-378` |

Anchors that **check out unchanged**: `Extension` struct's six fields (`extension_manager.rs:56-72`),
`code_execution` inner dispatch (`code_execution_extension.rs:1815`), `POST /agent/call_tool`
(`routes/agent.rs:1140`, dispatch at `:1162`), `provider_class` (`apps.rs:2089`, enum `:2061`,
`LOCAL_PROVIDERS` `:2068`, `INSTITUTIONAL_PROVIDERS` `:2074`, test table `:6447`), both
`chat_history_search.rs` joins (`:135`, `:211`) and both `LIMIT ?` pushes (`:150`, `:244`),
`chatrecall_extension.rs` (`handle_chatrecall` `:78`, LOAD `:90-159`, `get_session` `:92`, header
`format!` `:113`), `CURRENT_SCHEMA_VERSION = 16` (`session_manager.rs:29`),
`RegistryExtension` (`registry.ts:8-19`), `build-registry.mjs`'s `data-license` idiom (`:102`),
`baam.html`'s five render functions (`:3804`, `:3838`, `:3864`, `:3909`, `:3941`),
`shared.css:413-415`, `BrxtInstallModal.tsx:152-161`.

---

## Non-negotiable orderings

BR-71 names five; this plan has thirteen, and each one has a failure mode behind it.

**O1 — The types precede the column, and the column precedes every gate.**
Nothing can consult a tier that does not exist. Task 4 (types) → Task 6 (columns) → any gate.
Writing the column as a bare `String` and retrofitting the enum later produces two readers with
two parse rules, and the fail-closed one is whichever was written second.

**O2 — The generated private-extension const precedes `classify_extension`, which precedes
`Extension.tier`, which precedes Gates C, E and F.**
There is no network path to the registry from Rust (`main.ts:2832` is the only fetch, and it is
Electron). An extension gate written before the const has nothing to read and will be written
against `config.yaml`, which is user-writable and contradicts R11(i).

**O3 — Gate A ships in the same commit as the typed 409 and the `throwOnError` fix.**
Verified live: `ModelAndProviderContext.tsx:282-290` calls `updateAgentProvider` **without**
`throwOnError`, while `setConfigProvider` at `:294-300` has it. A Gate A refusal is therefore
discarded, execution continues to `setConfigProvider`, `setCurrentProvider`/`setCurrentModel` fire,
and a green success toast claims the switch worked (`:307-310`) — while the session is still bound
to the private model. Gate A alone is worse than no Gate A: the user believes they are on a public
model and are not.

**O4 — Gate A precedes Gate B.**
Gate B's ratchet is permanent. If it runs while the bind is still unchecked, every session ratchets
and then accepts an arbitrary provider on the next bind, manufacturing the residual state
(`classification = Private, capability = Public`) in bulk — which Gate B then refuses, one chat at
a time, on a machine measured at 57% private.

**O5 — The ratchet fires on the first TURN and on a permitted private-extension dispatch, never on
the bind.** Settled ruling. Ratcheting at bind time privatises a chat on a mis-click and *still*
misses `POST /agent/call_tool` (`routes/agent.rs:1140-1176`), which dispatches straight into the
extension manager without touching the reply path.

**O6 — Gate E lives in `filter_tools` and nowhere upstream of it.**
`get_all_tools_cached` (`extension_manager.rs:1054`) is guarded by `tools_cache_version`, bumped
only by `invalidate_tools_cache_and_bump_version` from `add_client`, `add_inprocess_server`,
`remove_extension` and one other site. `update_provider` never bumps it. Filtering in
`get_all_tools_cached` or `fetch_all_tools` (`:1090`) freezes one model's allowed set across a
mid-session model swap.

**O7 — Gate C sits in `ExtensionManager::dispatch_tool_call`, not in `Agent::dispatch_tool_call`,
and not as a `ToolInspector`.**
Four production paths converge on the extension-manager function and only one of them is the agent
loop: `agent.rs:2772`, `routes/agent.rs:1162`, `code_execution_extension.rs:1815`, and
`Agent::call_prefetch_tool` (`agent.rs:1618`, which runs **before** the turn). An inspector-shaped
control is invisible to three of the four. Proven, not assumed: `grep -rn "\.call_tool(" --include='*.rs' crates/`
returns exactly one production hit, `extension_manager.rs:1562`, inside `dispatch_tool_call`'s
spawned future.

**O8 — The two fully-open cross-session reads ship as the first gates.**
`chatrecall` LOAD (`chatrecall_extension.rs:90-159`) has no filter of any kind — not even SEARCH's
`exclude_session_id` — and `platform__ingest_conversation` (`knowledge_tool.rs:24-86`) takes a
caller-supplied `session_ids` array, loads each with `get_session(sid, true)`, and writes the full
transcripts into a machine-wide knowledge base. The design says LOAD is "the only fully-open
cross-session read in the product today"; that is false, and the second one is worse because its
sink is readable by every other session. A **third** copy of the second, discovered in the second
review round, is `POST /knowledge/bases/{id}/ingest-conversation` (`routes/knowledge.rs:1187-1258`);
Task 11 closes all three at once by guarding the function they share (departure D8).

⚠ **The second read's gate cannot ship before its sink has a tier**, so O12's three tasks sit between
the two: the phase opens with Task 10 (LOAD), then Tasks 10A–10D (the sink), then Task 11 (ingest).
That is not a weakening of O8. Closing the *read* while leaving the *sink* an unclassified
machine-wide tree fixes the narrower half and leaves the laundering path open — which is exactly what
the first version of this plan did, and what the operator's ruling reverses. Task 11's own second
test asserts the ratchet, so it is unwritable before Task 10B exists.

**O9 — `sessions.parent_session_id` precedes any lineage rule.**
It does not exist on `main`: `grep -rn "parent_session_id" --include='*.rs' crates/` returns only
in-memory uses (`subagent_task_config.rs:18`, `subagent_handle.rs:79`,
`subagent_handler.rs:66/158/286`). Every `L ∈ {self, child, other}` cell of design §7 is
unimplementable without it. `diverged_from` is branch lineage, not spawn lineage, and is not a
substitute.

**O10 — The migration number is not load-bearing, and must not become load-bearing.**
`main` is at 16. `/Users/wgu/Desktop/BioRouter-br71` (`feat/br71-workspace-control`, `ea15a4de`)
already has `CURRENT_SCHEMA_VERSION = 17` with a **written, working** `17 => ALTER TABLE sessions
ADD COLUMN parent_session_id TEXT`. Whoever merges second silently re-uses a number, and a database
that already ran the other branch's 17 skips the second feature's arm entirely — the exact incident
`run_migrations`' own comment at `:2344-2348` records for v11-v14. Task 6 therefore ships a
**shape-guarded numbered arm plus an unconditional `ensure_privacy_schema`**, following the
`ensure_session_incarnation_schema` precedent (`:2782-2789`, called from `reconcile_loop_schema`
`:2354`, itself called at `:2349` *after* the version loop). With that, merge order is free in both
directions.

**O11 — The whole of `landing/` (Phase 5) is independent and may ship on any cadence.**
Enforcement runs off the compiled-in const, so the website blocks nothing. It is sequenced last
because its `--check` gate needs the generated Rust file to exist, not because anything waits on it.

**O12 — The knowledge-base tier store precedes the KB ratchet, which precedes the KB read barrier,
which precedes the metadata scope, and all four precede Gate G.**
Task 10A (store + caller-capability channel + migration) → Task 10B (the ratchet on every write) →
Task 10C (the read barrier) → **Task 10D (the metadata surface)** → Task 11 (Gate G). 10D is last of
the four because it consumes both halves: it reads `tier::is_private` (10A) through the meta channel
10B installs, and it is only meaningful once 10C refuses the content — a catalog that omits a base
whose pages are still readable protects nothing. It is *before* Task 11 because Task 11 closes the
third instance of 10D's own defect (`resolve_target_kb`'s id list) and should be written with 10D's
ruling already on the page. Reversing 10B and 10C ships a barrier that refuses
nothing, because on a freshly-migrated machine **every** KB is public until a private session writes
to one — so a read gate landing first is green everywhere and proves nothing, and its own tests have
to fabricate a tier the tree cannot yet produce. And Task 11's second test asserts the ratchet, so it
cannot be written before 10B exists. Nothing here depends on `sessions.privacy_tier`: a KB's tier
lives in its own machine-local store (`<knowledge-root>/.kb-tiers`), because
`crates/biorouter-mcp` **cannot depend on `crates/biorouter`** — the dependency runs the other way
(`extension_manager.rs:1512` uses `biorouter_mcp::secret_guard`), which is the same constraint that
made the knowledge macros take a `Box<dyn Completer>` instead of a `Provider`.

⚠ **The ordering also carries the plumbing.** 10A–10C hang off the *same* four choke points
(Task 10A's ⚠, CP1–CP4). 10B is what installs the caller's capability at each seam — a hand-written
`KnowledgeServer::call_tool`, a required `caller_is_private` on the three macro `Args`, a parameter
on `handle_kb_frame` and on `stage_full_payload` — and 10C is then literally one `if` at each. Doing
10C first means writing all of that plumbing inside the barrier task, where a reviewer cannot tell
the signature churn from the control. 10D adds a **fifth** choke point of its own
(`Catalog::discover`) rather than a check at any of the four, because the surface it closes returns
metadata and never touches base content — the reason both of 10C's new-surface detectors are blind
to it.

**O13 — Every task's commit leaves `cargo test` green, and where it cannot, the task says what red
to expect.**
Not a style rule: the previous draft left **nine consecutive commits** (10B through 19) failing
`cargo test`, because 10B changed three struct signatures and the only file outside `crates/*/src/`
that constructs them — `crates/biorouter-mcp/tests/knowledge_macros_e2e.rs` — was in no Files table,
no `git add` and no run step, and every `cargo test -p biorouter-mcp` in the plan is `--lib`. Nine red
commits is not a cosmetic cost: a worker at commit six cannot tell a genuine break from the expected
state, which is the condition under which people stop reading failures. Three rules follow, and Tasks
10B, 10C, 10D and 11 all carry them:
1. A task that changes a `pub` or `pub(crate)` signature runs **`cargo check --workspace --all-targets`**
   in its Step 4 and again in Step 6 before `git commit`. `--lib` does not compile `crates/*/tests/`.
2. Every out-of-lib constructor of a changed type is a **row in the Files table**, a line in the
   `git add`, and a `--test <name>` line in Step 4.
3. A field a later task consumes is declared by the **earlier** task, together with a value for every
   caller. "Task N adds this field" in a Files table, for a field task N−1 makes required, is a task
   that cannot compile — see Task 10B's ⚠ on `conversation_ingest.rs:205`.

**O14 — Every read-deny is emitted AFTER the allow it subtracts from, and every capability check
runs BEFORE the early return it would otherwise sit behind.**
Four places, one shape, and all four failures are silent. In SBPL the last matching rule wins, so a
`(deny file-read* …)` emitted before `BASE_POLICY`'s `(allow file-read*)` (`seatbelt.rs:35`) or
before the writable-roots block produces a profile that compiles, runs, reports `Full`, and denies
nothing. In bubblewrap the later filesystem option wins for an overlapping path, so a `--tmpfs` that
precedes the writable `--bind`s is overridden whenever a deny root sits inside a writable root — and
three of DR-14's four roots sit under `$HOME`, which is routinely the working directory. In
`shell_sandbox_wrap` the DR-14 arm must precede the `mode == SandboxMode::Off` early return
(`developer/shell.rs:168-170`), or the whole control is dead for every user who never set
`BIOROUTER_SHELL_SANDBOX` — which is every user. And in `resolve_path_jailed` it must precede the
`if jail_relaxed { return Ok(resolved) }` at `rmcp_developer.rs:2084-2087`, because relaxed **is**
Auto mode, the mode agents run in. Each of the four is gated by an ordering assertion in Task 14A or
14B rather than by a test that could pass either way.

**A fifth and a sixth, in the same shape, found in the round that added Task 10D.** A capability read
must also run **after** the bind that decides it. `configure_agent` (`routes/apps.rs`) computes
`capability_report(cfg)` at `:1257` and binds the manifest's own provider at `:1259`, so the natural
place to add the read — where the report already is — reads the provider the session held *before*
the app's model was applied: global-private/manifest-public hands a public model the private catalog,
global-public/manifest-private strips a private model of its own bases. The call moves below
`configure_main_provider`. And `configure_worker_agent` `:1553-1561` has the ordering right and the
check missing: it binds the worker's provider and then grants `cfg.knowledge_base` with no report at
all. Both are gated by `awk` ordering assertions in Task 10D Step 5 — a count of capability reads
cannot see either, because in both defects the read is present and correct and merely early.

---

## Departures from the design

Nine, each forced by a measurement.

| # | Design says | This plan does | Why |
|---|---|---|---|
| D1 | §11.1/§19: the chatrecall LOAD guard is "five lines, ship it first, ahead of everything else in this design" | Ships it as the **first gate**, after the tier model | The guard compares the caller's capability with the target's classification. Neither exists before Phase 1. "First" is honoured within the gates. |
| D2 | §9.3 B1: "put the carry-over on `create_session` itself, parameterised" | Introduces one `create_derived_session` helper that the three copy paths share | `grep -rn --include='*.rs' "\.create_session(" crates/` returns **104** call sites. Parameterising a 3-arg function with 104 callers to fix three of them is a worse trade than collapsing the three hand-rolled builders into one. Task 22 keeps the design's enumeration test. |
| D3 | §5.1: `Classification` as the stored enum name | `SessionClassification` | `crates/biorouter/src/security/classification_client.rs` already defines `ClassificationClient` / `ClassificationRequest` / `ClassificationResponse` (an unrelated HuggingFace text classifier) in the same crate. |
| D4 | §14.1: Private pill = `--background-muted` fill + `--text-standard` label; Public pill = 1 px `--border-subtle` hairline + `--text-subtle` label | Private = `bg-background-muted text-text-default`; Public = `bg-background-muted text-text-muted` | `--text-standard` **does not exist** (`grep -rn "var(--text-standard)" ui/desktop/src` → 0; the only textual hit is a comment in `search.css:2` saying so). And no border token in the system reaches 3:1 on a pill's real ground: measured with the repo's own `ui/desktop/scripts/lib/theme-tokens.mjs`, `--border-subtle` vs `--background-muted` is **1.00–1.24** across all six family×mode scopes (parchment:dark is exactly 1.00 — identical colours). An outline pill is not expressible here. Full measurements in Task 26. |
| D5 | §15.1: "added by the same `ALTER TABLE sessions ADD COLUMN` arm BR-71 Task 1 uses" | Shape-guarded arm 17 **plus** an unconditional `ensure_privacy_schema` | O10. |
| D6 | §18.4: the prompt-hook provider check is "v1 emits a load-time warning; the hard skip is v1.1" | Hard refusal in v1, in the same task as the CLI plan-mode refusal | The Stop hook's payload is `crate::agents::goal::transcript_tail(&conversation)` (`agent.rs:5495-5496`) — a real transcript excerpt — shipped to an arbitrary endpoint resolved by `HooksManager::resolve_prompt_provider` (`hooks/mod.rs:690`) and sent by `run_prompt_hook` (`hooks/prompt_runner.rs:57`). It is structurally identical to P6 and carries the same content. |
| D7 | §9.3 B4: "Ratchet a KB's classification on ingest … **or** state plainly that KBs are a designed public sink" | Ratchets (operator ruling), and enforces the read side at **five choke points**, not at an enumeration of tool call sites | The design says "a public-capability session may not read a private KB" without naming where that is enforced, and the obvious answer — one check per `kb_*` tool — does not survive measurement. It misses four whole surfaces (`agent_drafter::export_app`, `routes/apps.rs::run_kb_read`, that route's `ingest` arm, and the `KbToolDispatch` sub-agent tool set), and **nine of the nineteen `kb_*` tools take no `RequestContext`** so they cannot learn the caller's capability at all. The barrier therefore sits at `<KnowledgeServer as ServerHandler>::call_tool` (which receives the `RequestContext` for every tool), the three sub-agent macro entries, `handle_kb_frame`, and `stage_full_payload`. A **fifth**, `Catalog::discover` (Task 10D), covers the surface the other four cannot see by construction: a base's **id and name**, which `list_platform_catalog` hands to any model with no arguments at all. Full derivation, with the measurements and what it costs, in Task 10A's ⚠ "where the barrier goes" and its coverage table. |
| D9 | §9.3 A2: "Add `**/sessions.db*` and the Biorouter data directory" to `DEFAULT_SECRET_PATTERNS` | Does **not** touch `DEFAULT_SECRET_PATTERNS`; hides four roots with a **capability-conditional path policy at the dispatch choke point (Layer A), backed by an OS sandbox for spawned children (Layer B)** (DR-14, Tasks 14A–14D) | Two measurements. `DEFAULT_SECRET_PATTERNS` (`secret_guard.rs:33-45`) is **unconditional** — it is an always-on floor applied to every session, so adding the data directory there would hide the user's own knowledge base and chat history from a **private** session too, which no requirement asks for and AR-1 already shows is expensive. And it would not close the read anyway: the design says so itself ("this raises the cost, it does not close the read"), because `candidate_is_denied` (`:278-292`) is lexical and existence-gated, so `sqlite3 "$(printf '%s' ~/.local/share/biorouter/sessions/sessions.db)"` walks past it. What replaces it is a **capability-conditional** policy evaluated at the same choke point `find_denied_path` already runs at, sharing that scan's argument walker but not its verdict — so it is scoped to public sessions, is not existence-gated (`memory/` does not exist on a fresh install and must still be denied), and is a barrier rather than a cost increase. The kernel deny is the **second** layer and covers the one thing an in-process check cannot: a child process the daemon has already handed the command to, where `printf`-style runtime path construction happens in a shell the daemon never sees. |
| D8 | §9.3 B4 and the first version of this plan put the cross-session ingest guard in `Agent::handle_ingest_conversation` | Puts it in `biorouter::knowledge::conversation_ingest::ingest_conversation` as a **required** `caller_capability` argument | Measured: `grep -rn "conversation_ingest::ingest_conversation\|ingest_conversation(" --include='*.rs' crates/` returns **three** production callers, not one — `agents/knowledge_tool.rs:61` (the platform tool), `biorouter-server/src/routes/knowledge.rs:1233` (`POST /knowledge/bases/{id}/ingest-conversation`, whose `session_ids` array at `:1192-1212` is caller-supplied and loaded with `get_session(sid, true)` at `:1203`) and `biorouter-cli/src/commands/knowledge.rs:571`. A guard in the platform tool leaves the HTTP route — reachable with nothing but the secret key — as an unguarded copy of the same primitive. A required parameter makes all three a compile error. |

---

## File structure

New files this plan creates, in the order they appear:

```
crates/biorouter/src/privacy/mod.rs                    Task 4  — the two enums + floor()
crates/biorouter/src/providers/tier_tests.rs           Task 5  — `#[cfg(test)] mod tier_tests;`
crates/biorouter/src/privacy/registry_private.rs       Task 8  — @generated from landing/
crates/biorouter/src/privacy/extensions.rs             Task 8  — classify_extension(name)
crates/biorouter-mcp/src/knowledge/tier.rs             Task 10A — the KB tier store + migration
crates/biorouter-sandbox/src/private_data.rs           Task 14B — the meta key, PrivateDataPolicy, the refusal
crates/biorouter-sandbox/tests/read_deny.rs            Task 14A — Layer B, the live kernel-enforcement proof
crates/biorouter-mcp/src/private_roots.rs              Task 14B — the five entries, the ONE resolver
crates/biorouter/src/privacy/path_policy.rs            Task 14B — Layer A, the barrier's verdict
crates/biorouter/src/privacy/private_roots.rs          Task 14B — a re-export of the resolver, plus its tests
crates/biorouter/src/privacy/refusal.rs                Task 12 — PrivacyRefusal; Tasks 13/14/23 add to it
crates/biorouter/src/privacy/alt_provider.rs           Task 19 — assert_alt_provider_allowed
crates/biorouter/src/privacy/visibility.rs             Task 21 — the §7 matrix as one predicate
crates/biorouter/src/privacy/declassify.rs             Task 29 — UserConfirmation + declassify()
ui/desktop/src/components/ui/PrivacyBadge.tsx          Task 26
ui/desktop/src/components/ui/DangerousConfirmDialog.tsx Task 29
ui/desktop/src/components/sessions/DeclassifySessionDialog.tsx Task 29
ui/desktop/src/components/settings/privacy/PrivacyPanel.tsx Task 30
ui/desktop/src/components/privacy/FirstRunPrivacyNotice.tsx Task 38
landing/scripts/check-docs-privacy.mjs                 Task 36
docs/security/privacy-tiers-migration.md               Task 38
```

⚠ Two of these moved after the first adversarial round and the old placement is a **compile error**,
not a style preference. `privacy/refusal.rs` is created by **Task 12**, not Task 14: Task 12's own
implementation returns `PrivacyRefusal::PublicModelOnPrivateSession`, Task 13's calls
`privacy::refusal::turn_refusal(row)`, and both run *before* Task 14. Each later task adds to the
module and says which item it adds. `privacy/visibility.rs` is Task **21**, not 24 — Task 21 is the
task that creates it. `privacy/alt_provider.rs` (Task 19) and the three UI files were missing from
this list entirely.

---

## Accepted risks

The costs below were put to the operator, and the operator accepted them. They are recorded here in
plain language because each one is a way a *correct* implementation of this plan still loses
something a user might expect to have. **Do not treat any of them as a bug report**; each is a
ruling. What is *not* on this list is a defect.

### AR-1 — A knowledge base that one private session touched becomes unreadable from every public chat, including the user's own ordinary work

The ruling (Tasks 10A–10C): **a knowledge base takes the tier of the most sensitive session that has
ingested into it, and a public-capability session may not read a private KB.** The alternative the
design offered — declare KBs a designed public sink and warn at ingest — was rejected.

The cost, stated plainly:

- A knowledge base is **machine-wide** (`knowledge_root()` = `in_config_dir("knowledge")`,
  `knowledge/paths.rs:43-45`) and there is exactly one default base on most installs. The moment a
  single Versa-backed chat writes one page into it, that base is private — and every subsequent chat
  on a commercial model gets a refusal from `kb_search`, `kb_read_page`, `kb_list_pages`,
  `kb_get_graph`, `kb_list_history`, `kb_search_raw_sources` and `kb_export`, from the `ingest` /
  `query` / `lint` macros the GUI Knowledge view runs, from any **BioRouter app** that declared that
  base as a `br.kb` source, and from `export_app`'s payload — **including for material that had
  nothing to do with the private work**. The KB does not un-ratchet. There is no per-page tier and
  there will not be one in v1: pages are markdown in a git tree, and per-page classification is a
  storage redesign.
- **A published app stops working, for its users, when a base it reads is ratcheted by someone
  else's chat.** The app's manifest grant (`resolve_kb_grant`, `routes/apps.rs:2268`) still permits
  it; the privacy barrier does not. The app surfaces the refusal string in its `kb_result` error
  frame rather than failing silently, but it is a working app that stops answering for a reason the
  app author did not cause and cannot fix.
- **The Knowledge view itself keeps working.** `GET /knowledge/bases/{id}/page`, `/pages`, `/graph`,
  `/history`, `/preview`, `/export` are not gated (Task 10C's second ⚠): the user reading their own
  notes is not a model. So the base is not *lost* — it is unreachable to models on a public
  capability, and readable by hand.
- The repair is the same one every other private surface offers — switch the chat to a private model
  — and it is discoverable, because the refusal string names it. It is still a real loss of
  ergonomics for a user whose default model is commercial and whose knowledge base has one private
  page in it.
- **There is no declassification path for a KB in v1.** Sessions get one (Task 29, user-only, graded,
  audited). A KB does not, and the CLI escape hatch (Task 31) does not cover KBs either. A user who
  ratchets their only knowledge base by accident has no in-product exit short of `kb_export` →
  `kb_import` into a fresh id from a public chat, which Task 10A explicitly does **not** launder
  (an import stamps the importing session's tier and cannot lower an existing one). Follow-up, and
  [Open question 15](#open-questions) records it.

### AR-2 — Every knowledge base that exists today starts **public** at migration, even if a private session fed it

`.kb-tiers` does not exist before Task 10A, and the tree keeps no record of which session wrote which
page — the git author is `Biorouter`, not a session id. So the migration writes `public` for every
existing base. This is the same fail-**open** direction DR-10 mandates for the session backfill and
for the same reason: a fail-closed migration would privatise the user's only knowledge base on first
launch, with no declassification path (AR-1) to get it back.

The residual: **a KB that a private OMOP session ingested into last week is readable by a public
model after the upgrade, and stays readable until the next private ingest ratchets it.** There is no
content scan and there will not be one. The first-run notice (Task 38) says this in one sentence.

### AR-3 — `memory`'s **local** store is not gated, and a private session's note reaches every session opened in that directory

Found by Task 1's Correction 3 and left open. `compose_instructions`
(`crates/biorouter-mcp/src/memory/mod.rs:277`) inlines local memories **in full** at `:310-322` under
`LOCAL_SECTION_HEADER`, into the system prompt of every session whose working directory holds that
`.biorouter/memory`. Task 19 refuses the **global** write from a private-capability session; it does
nothing about the local one, and Gate F2 (Task 18) cannot help because it filters by *extension*
tier and `memory` is Public.

Why it is not closed in v1: the design's own §9.3 B3 offers two fixes, and the one that would cover
this ("classify memory entries and filter `retrieve_all` by the session's capability tier at init")
needs provenance on each stored memory. The on-disk format is a `# {tags}` line followed by bare
lines (`memory/mod.rs:387-388`, read back at `:414-418`, keyed by the **tag string**, not the
category), and `compose_instructions` runs once at `MemoryServer::new` (`:108`) rather than per turn —
so a capability-aware filter there would also freeze across a mid-session model swap, which is
exactly the O6 hazard Gate E exists to avoid. Task 19 ships the cheap half instead: a private-capability
local write says out loud, in the tool result, that the note will be readable by any session opened
in that directory. [Open question 14](#open-questions) carries the real fix.

### AR-4 — `medcp` stays callable by a public model

Unchanged from DR-11. Listed here so the accepted-cost list is complete rather than split across two
tables.

### AR-5 — The existence of a private knowledge base is still inferable

Tasks 10C, 10D and 11 stop a public model from being *handed* the id or name of a private base:
`kb_list_bases` omits it, the Agent Drafter catalog omits it, and neither the no-primary nor the
no-target error enumerates it. **None of that stops a public model from asking about one id at a
time and learning the answer.** Two paths remain open, both by decision:

- `kb_create_base("omop")` on an existing base bails with `kb 'omop' already exists at
  <path>` (`service.rs:451`) — an existence answer *and* a filesystem path. The tool is deliberately
  outside `KB_ID_GATED_TOOLS` (Task 10C's ⚠) because gating it is what banned knowledge-base creation
  for public sessions in an earlier draft.
- `resolve_target_kb`'s `knowledge base '{id}' does not exist` (`knowledge_tool.rs:141`) answers the
  same question for a supplied id.

**This is DR-7 applied consistently, not an oversight.** The operator ruled side channels —
existence, counts, timing — out of scope for `chatrecall`: *"Only content must not cross."* The same
rule here would be incoherent if it were applied differently, so it is not. What *is* in scope, and
what Tasks 10C/10D/11 close, is the plan's own countervailing rule from one test over: **a knowledge
base's id and name are user-authored content** — *"a KB name is user-authored and routinely names a
cohort or a study"* — so volunteering the whole list is a content crossing, while answering one
guess is a side channel.

The residual cost, plainly: a determined public-capability model that already knows or can guess an
id can confirm the base exists, and can learn its on-disk path. It cannot read a page, a snippet, a
graph, a history entry or an export from it (CP1–CP4), and it cannot obtain the id from Biorouter in
the first place (CP5 and the two error lists). Closing the last inch needs constant-shape responses
on `kb_create_base`, which DR-7 declines and which would cost the user a truthful error on the
overwhelmingly common non-adversarial case.

### AR-6 — On a host that cannot express the read-deny a public session loses the shell, and two costs come with the sandbox itself

DR-14 fails **closed**. Three consequences were put to the operator and accepted.

**(1) Windows loses the five tools that spawn a child process — `developer__shell` and its
background jobs, `computercontroller__automation_script`, `computer_control`, and
`compute__compute_run`/`compute_python` — for every public-capability chat.** There is no
unprivileged, general-purpose way to hide a directory from an arbitrary command on Windows —
`shell_sandbox/windows.rs:1-51` works through the five candidates and why each fails — so the refusal
fires for the common configuration (a commercial model on a Windows laptop). The same applies to a
Linux host without `bubblewrap`, or with unprivileged user namespaces disabled, though there
the refusal names a third fix (`apt install bubblewrap`) that actually works. macOS is unaffected:
Seatbelt ships with the OS and expresses the deny directly.

⚠ **This cost is smaller than the first two rounds of this plan said, and the reason is
[the two-layer structure](#dr-14-is-two-layers-and-the-os-sandbox-is-the-second-one).** Earlier
drafts made the OS sandbox *the* mechanism, so an unsupported platform meant refusing **every** tool
that resolves a caller-supplied path — `text_editor`, `analyze`, `image_processor`, `cache`,
`xlsx_tool`, `pdf_tool`, `docx_tool`, and every knowledge / memory / drafter tool. That is most of
the product, and it was the single largest cost in this feature. It is not necessary: Layer A is a
check inside the daemon's own dispatch path and needs no kernel support at all, so on Windows it
holds exactly as it does on macOS. **What is refused on an unsupported platform is the five tools
whose reads happen in a child process the daemon cannot see into — and nothing else.**

The escape hatches are the two the refusal names — switch the chat to a private model, or turn
privacy tiers off entirely (DR-15) — and the second is a large part of why the operator asked for a
master toggle at the same time as the sandbox. The refused tools are **not** hidden from the model's
tool list: hiding them makes a model invent workarounds, while a deterministic refusal that
forecloses the retry makes it stop.

**(2) A public-capability session cannot use the `biorouter` CLI, or any other tool, to read its own
chat history from the shell.** `biorouter session list` reads the session store, which is deny root
#1. This is correct — the CLI is a general-purpose reader of exactly the material the sandbox
hides — but it will read as a bug the first time someone hits it, so the refusal names the four
directories rather than saying "permission denied".

**(3) A public-capability session cannot reach an Agent Drafter app's files from the shell.** The
drafter's own tools (`create_app`, `configure_app`, `update_app`, `build_app`) are unaffected — they
run in-process and do not resolve caller-supplied paths into the app tree — but `cd
~/.config/biorouter/agent_drafter/my-app && cat src/main.ts` is refused. The root is on the list for
two reasons and only one of them is `.vault/`: it is also the only place on disk an **app id** can
be read, and an app id is enough to drive that app's agent over the unauthenticated
`GET /apps/{id}/agent` socket (Task 14C (c)). Narrowing the deny to `.vault/` would reopen that.
[Open question 19](#open-questions) records the narrowing.

**The residual DR-14 does not close:** a public-capability model that *already knows* an app id — it
was told one, or it read one before the sandbox existed — can still fetch `GET /apps/{id}/`, read the
per-app socket token out of the served page, and drive that app. Closing it needs the app socket
authenticated by something the page has and a shell does not, which is a change to how apps are
served. [Open question 18](#open-questions).

### AR-7 — While the tiers are off, nothing is recorded, and turning them back on does not reclassify the gap

DR-15's master toggle stops the classification ratchet along with the gates, because the operator's
ruling is that with it off *"nothing will be impacted"* — and a ratchet that keeps firing is an
impact, just a deferred one. The consequence is one-way and permanent:

- A session that ran on a private model, called `ucsfomopagent`, and pulled a cohort into its
  transcript **while the toggle was off** is stamped `public`, because DR-4's two triggers never
  fired. Turning the toggle back on does not re-examine it: there is no content scan, no provenance
  on the messages, and `privacy_tier` is monotone — the reconcile can only raise a value that a
  trigger wrote, and no trigger wrote one.
- So after the toggle goes back on, that session is readable by `chatrecall` from a public-model
  chat, and its provider may be swapped to a public one with no 409. It looks, to every gate, like
  an ordinary public conversation.
- The same holds for knowledge bases: an ingest performed while the toggle was off does not raise
  the base's tier, and AR-1 records that there is no KB declassification path to correct it in
  either direction.

The alternative — keep ratcheting while the guardrails are off — was considered and rejected: it
would silently privatise sessions a user believes are unprotected, and the first they would learn of
it is a refusal weeks later when they turn the feature back on. Between "the toggle means what it
says" and "the toggle secretly keeps a ledger", the operator's ruling picks the first.

**The mitigation is disclosure, not mechanism.** Task 30's confirmation dialog says this in one
sentence before the switch flips, and the persistent strip repeats it while the toggle is off. A
user who accepts that sentence has accepted AR-7.

### AR-8 — A private model with a shell can still carry a knowledge base out of the deny root

Task 10A decision (2) closes the archive-laundering path in the two directions that matter: an
imported base takes `max(archive marker, importer)` so a `.brkb` can only ever over-classify itself,
and a **model's** export of a private base is written into `<knowledge-root>/exports/`, inside DR-14
deny root #2, where a public-capability session cannot read it.

What is left is the *private* side. A private-capability model holds the shell (DR-14 denies reads
to public capability only), so it can `cp` its own base's archive — or the base's markdown, or
`sessions.db` — anywhere it likes, and a public chat opened afterwards may read whatever it left
outside the four roots.

**This is not a hole this design can close, and pretending otherwise would be the more dangerous
claim.** The tier system's promise is about what a **public-capability model** can reach. A
private-capability model is, by construction, trusted with the private material — it is being *sent*
that material on every turn. Constraining what it may then write would mean a write-side sandbox
(an egress control), which is a different feature with a different threat model, and one this
design's §9 does not attempt for any of the four roots.

The user-facing consequence, stated plainly: **a private chat can be told to copy your notes
somewhere a public chat can read them, and it will.** The controls that bear on it are the ones
already in the product — approval mode for shell commands, `.biorouterignore`, and the fact that a
private chat is running the user's own institutional model on the user's own machine.

### AR-9 — Layer A is check-then-use, so a concurrently running shell can still race one in-process reader

[Layer A](#dr-14-is-two-layers-and-the-os-sandbox-is-the-second-one) refuses a tool call whose
arguments name a path inside a deny root. It decides that by resolving the path *at check time* —
lexically, then by canonicalizing the deepest existing ancestor — and the tool then opens the file a
few microseconds later. Between those two moments a **concurrently running** process can replace a
workspace symlink so the daemon's open follows it into a deny root.

Two things bound this, and both were measured.

- **The link cannot be planted from a sandboxed child on a supported platform.** Creating a symlink
  or hardlink whose source is inside a deny root requires reading the source path's metadata, and
  the kernel refuses that: macOS `ln: …: Operation not permitted, rc=1` under a
  `(deny file-read* (subpath …))`. So the attacker needs an *unsandboxed* writer, which on a
  supported platform means another tool call — and that tool call's own arguments go through the
  same barrier.
- **A pre-planted hardlink defeats the deny outright, on both platforms.** Seatbelt matches paths,
  not inodes, and `--tmpfs` hides a path, not an inode. Measured: macOS reads a pre-existing hardlink
  to a denied file (`SECRET-KB-CONTENT`, exit 0); Linux does too under a `--tmpfs` deny
  (`SECRET-KB-CONTENT, rc=0`). Nothing short of an inode-aware policy closes that, and neither
  platform offers one to an unprivileged process.

**Stated in the operator's terms:** the read-deny is a barrier against a public model *asking* for
private material, not against a public model that has already arranged, through some earlier
unsandboxed write, for a private file to be reachable under a second name. Closing it means
`openat2(RESOLVE_BENEATH)` (Linux 5.6+, no macOS equivalent) or an inode policy, and neither is
portable. What this plan does instead is make the window as small as the language allows: the check
is placed in the same synchronous block as the open wherever the reader is under our control
(Task 14D), and never across an `.await`.

### AR-10 — On Linux, a deny root that does not exist when a job starts stays visible to that job for its whole life

`--tmpfs` on a destination that does not exist **aborts bubblewrap** — measured,
`bwrap: Can't mkdir …/memory: Read-only file system`, exit 1, even when the parent directory exists.
So `wrap_bubblewrap` must skip absent roots or no sandboxed command runs at all. That skip is a
necessity, not a tolerance, and it creates a race an earlier draft of this plan waved away with *"a
root that does not exist holds nothing to read"*. That sentence is false, measured:

```
-- wrapper built with NO deny for $ABSENT (absent roots are skipped) --
   background process creates the root at t=2s; sandboxed job reads it at t=4s
LATE-MEMORY-SECRET
exit=0
```

**And it is live on this machine right now**, at the plan's own paths:

```
/Users/wgu/.local/share/biorouter/sessions     EXISTS
/Users/wgu/.config/biorouter/knowledge         EXISTS
/Users/wgu/.config/biorouter/memory            ABSENT   <-- bwrap would skip this root
/Users/wgu/.config/biorouter/agent_drafter     EXISTS
```

`memory/` is created lazily on first write (`global_memory_dir()` → `in_config_dir("memory")`,
`memory/mod.rs:82-84`), so on a fresh install the memory root is absent until the first
`remember_memory`, and a long-running public background job started before that moment can read it
afterwards.

**Two mitigations, and neither is a closure.** Task 14B creates the four roots at startup if they do
not exist, which shrinks the window to "a root deleted and recreated mid-session"; and Layer A does
not have this race at all — it re-resolves the roots on every tool call, so the in-process channel is
closed for an absent root from the instant it appears. **The residual is exactly one channel on one
platform: a Linux shell that was already running when the root appeared.** macOS does not share it —
measured, an SBPL deny of a path that does not yet exist still applies once another process creates
it (`Operation not permitted`), because SBPL is a path-pattern match. That asymmetry is why the
`is_dir()` skip must stay inside `wrap_bubblewrap` and must not be hoisted into a shared helper.

### AR-11 — The daemon's own API secret is recoverable, so the second door is held by Layer A and not by the environment strip

Issue #57's `strip_daemon_private_env` removes `BIOROUTER_SERVER__SECRET_KEY` from every child
BioRouter spawns, and it does that correctly (Task 14C's audit). The design then concluded that a
sandboxed child "knows where the daemon is and has no way to authenticate to it". **That conclusion
does not survive measurement and is withdrawn.**

- **macOS.** A child recovers its *parent's* environment with `ps -Ewww -p $PPID`. Re-measured for
  this round: a shell started with `SECRETCANARY=parent_canary_777` printed the canary from its own
  child. The protection people assume — SIP withholding process environments — applies only to
  Apple **platform** binaries: `/bin/sleep` withholds, a locally compiled binary does not, and
  neither does the shipped, notarized, hardened-runtime `BioRouter.app/…/biorouterd`
  (`flags=0x10000(runtime)`, `TeamIdentifier=F3YYBXAFJ8`), measured with a canary. It is not `ps`'s
  setuid bit either: a plain non-setuid `sysctl(CTL_KERN, KERN_PROCARGS2, pid)` reader recovers it,
  and it still does under `(allow default)`, under `(allow default)(deny file-read* (subpath …))`,
  and under a deny-by-default profile — `sysctl-read` is not gated by any profile that could be
  constructed.
- **Linux.** The daemon's own `/proc/self/environ` is readable **in-process**, by any tool that
  reads a caller-supplied path — `computercontroller__cache view /proc/self/environ` returns it.

**So the honest statement, in the words the design must now use: the daemon's API secret is not
defensible against a tool running inside the daemon, and no sandbox this feature installs changes
that.** What that costs, and what still holds:

- It costs the second-order argument. A public-capability session that recovers the secret can call
  `GET /sessions/{id}/export` and read a transcript over HTTP. Nothing in DR-14 stops it, and the
  plan no longer claims otherwise.
- **What still holds is the biggest route by far, and it holds because of Layer A.**
  `POST /agent/call_tool` (`routes/agent.rs:1140`, registered `:1268`) executes *any* tool of *any*
  extension with no capability check, no permission prompt and no approval frame — measured live,
  `computercontroller__cache view /etc/hosts` returned the file. It dispatches through
  `agent.extension_manager.dispatch_tool_call` (`routes/agent.rs:1160-1163`), which **is** Layer A's
  choke point, so the secret buys a caller nothing there that it does not already have in the chat:
  Gate C refuses a private extension and Layer A refuses a deny-root path, identically. A design that
  had put the barrier in `Agent::dispatch_tool_call` instead would have lost this entirely.
- The remaining exposure is the set of HTTP routes that return private content **without** going
  through a tool call — the transcript routes, the `/knowledge/*` read routes, `GET /apps/{id}/export`
  — plus `GET /diagnostics/{id}`, which is the widest of them (a zip of `session.json`, recent
  `logs/*.jsonl`, and a verbatim copy of `config.yaml`). Authenticating those against a caller that
  is on the same machine and can read the daemon's memory is not something a header comparison can
  do. [Open question 20](#open-questions) carries it.

---

## Which test filters are validated, and which are not

The adversarial verifiers could not run a single `cargo test` filter, and named this "the single
biggest hole in my own coverage", because BR-71's most expensive defect was *a filter that names a
nested module by the wrong path, prints `0 passed`, and exits 0*. This section closes as much of that
hole as is closable by reading; **[Task 4b](#task-4b-resolve-every-test-filter-against-a-real-cargo---list-docs-only)
closes the rest by running it**, and its Step 3 carries the measured pre-count of all 30 filters that
resolve today. Where this section and Task 4b disagree, **Task 4b wins** — it is the measurement.

**How each filter was checked.** For every `cargo test` line in this plan, the module path it implies
was resolved against the tree: for an existing module, that the file exists at the path the filter
spells **and** that it contains a `#[cfg(test)] mod tests`; for a module this plan creates, that the
task's Files table puts the file where the filter's path implies.

**Two filters name a module that has no test module today, so they print `0 passed` and exit 0 until
the task that owns them lands.** This is not a defect in the filter — it is the reason each of those
tasks must state a *pre-count of zero* and assert the exact post-count:

| Filter | Module today | Task |
|---|---|---|
| `cargo test -p biorouter --lib agents::chatrecall_extension` | `chatrecall_extension.rs` has **no** `#[cfg(test)]` at all (verified: 0 hits; **confirmed 0 by Task 4b's `--list`**) | 10, 17 |
| `cargo test -p biorouter --lib session::chat_history_search` | `chat_history_search.rs` has **no** `#[cfg(test)]` at all (verified: 0 hits; **confirmed 0 by Task 4b's `--list`**) | 17 |

⚠ **The other two filters this plan leans on are NOT zero today, and a previous version of this
section said they were.** That error runs in the direction that hides a no-op: a worker told to
expect `0 passed` reads `8 passed` as "my tests landed" when in fact none of them did. Both
modules already have **two** `#[cfg(test)]` blocks each, neither of them named `tests`, so a filter
on the module path picks them up:

| Filter | Module today — **measured at `9558c346`, confirmed by Task 4b's `--list`** | Task |
|---|---|---|
| `cargo test -p biorouter-server --lib routes::agent` | **8 tests**, in `mod working_dir_lock_tests` (`routes/agent.rs:1279`, 4 tests) and `mod knowledge_selection_tests` (`:1380`, 4 tests) | 12, 14 |
| `cargo test -p biorouter-server --lib routes::session` | **20 tests**, in `mod diverge_tests` (`routes/session.rs:1038`, 11 tests) and `mod edit_message_tests` (`:1417`, 9 tests) | 22, 29 |

⚠ **And a third, which the hand search missed: `agents::agent` spans three test modules**, not one —
`agents::agent::tests` (14), `agents::agent::rewrite_basis_tests` (2), `agents::agent::stall_seam_tests`
(5), total **21**. Task 4b found it by listing. The general lesson is stated below and is worth
repeating here: **do not assume `mod tests` is the only shape, and do not trust a hand search to have
found every module that isn't.**

So Tasks 12, 14, 22 and 29 must record the **pre-count** with the same command before Step 3 and
assert `post == pre + N`, exactly as Task 2 and Task 6 already do — never "expect a non-zero count",
which those two filters satisfy before a line of #56 exists.

Thirteen of the `crates/biorouter-server/src/routes/*.rs` files carry at least one `#[cfg(test)]`
block; `apps.rs` and `config_management.rs` (the two this plan filters on besides the four above)
both have one. Note also that the two non-zero modules above are the concrete instance of failure
mode (a) named at the end of this section — **a test module nested under a name its file does not
advertise**. Neither is called `tests`; both are reached by the module-path filter anyway. Do not
assume `mod tests` is the only shape.

**Two syntax rules, both verified analytically.**
`cargo test --lib A B` is a hard error (`unexpected argument 'B' found`) — cargo takes exactly one
`TESTNAME` positional, so multiple filters must go after `--`, where libtest ORs them. And a libtest
filter that matches nothing prints `0 passed` and **exits 0**, which is why every gate in this plan
asserts a *count*, never an exit code.

⚠ **`cargo test -p <pkg> --lib <MODULE> -- name1 name2` does not do what it looks like.** Cargo passes
its own `TESTNAME` positional to libtest as *another* OR'd filter, so the module runs in full and the
names after `--` add nothing. Task 6 Step 2 carried this shape and has been corrected to drop the
positional. If you want exactly N named tests, pass **only** names after `--`.

**What remains unvalidated, and why — and the task that closes it.** Nothing in this section was
*executed* when it was written. The filters were resolved statically against file paths and
`mod tests` presence, which catches the BR-71 defect class (a path that resolves to nothing) but not
two others: (a) a test that exists under a *different* nesting than its file suggests — e.g. a helper
`mod` inside `mod tests` — and (b) an expected pass-count that is right for the module today and
wrong after another task adds tests to the same module. Every gate in this plan that quotes a pass
count is therefore paired with either a named-test filter or a pre/post delta the task records itself.

**(a) is closed by [Task 4b](#task-4b-resolve-every-test-filter-against-a-real-cargo---list-docs-only)**,
which runs `cargo test -p <pkg> --lib -- --list` for all five packages this plan filters on and
resolves every one of its **42** `(package, filter)` pairs against the real listing. It is placed
immediately after Task 4 because Task 4 is the first commit that produces a `privacy::` module, and
it is docs-only. What it cannot close is the **nine** modules later tasks create
(`privacy::{extensions,refusal,alt_provider,visibility,declassify,private_roots}`,
`providers::tier_tests`, `knowledge::tier`, and `private_data` in the **sixth** package,
`biorouter-sandbox` — which no earlier revision of this plan filtered on at all, so Task 4b's
`--list` sweep must add it); those are a named deferred set that Task 20's and Task 40's gates re-run
with a shrinking and then an empty list. Task 14A also introduces a **second integration binary** in
that package, `--test read_deny`, alongside the existing `--test sandbox`; a `--test` filter naming a
binary that does not exist is a cargo hard error rather than a silent zero, so it is self-checking. Two further facts Task 4b's design turns on, both easy to get
wrong: this plan spells its filters in **two** forms (`--lib <FILTER>`, 34 occurrences, and
`--lib -- <NAME> <NAME>`, 7 — an audit of only the first misses `privacy::refusal` and
`privacy::alt_provider` entirely), and a libtest filter is a **substring** match, not a prefix, so
`privacy` matches more than `privacy::…` and the pre-counts must be measured rather than reasoned.

---

# Phase 0 — the design's own errors, and the one leak that needs no tier

Three tasks. Nothing here depends on the tier model, and Task 1 is a prerequisite for reviewing
every later task honestly: the design carries one symbol that has never existed and one section
that describes code deleted before this branch was cut.

### Task 1: Correct the design against the tree (docs only)

`privacy-tiers.md` is the specification every later task is reviewed against. Three of its claims
are false in this worktree and a reviewer who checks them will — correctly — conclude the plan was
written from the design rather than from the code. A fourth section, §9.3 B4, forces a choice the
design refuses to defer, and the operator has since ruled on it; the ruling belongs in the design,
not only in this plan.

**Files:**

| Action | Path | Anchor (re-verified at `9558c346`) |
|---|---|---|
| Modify | `docs/security/privacy-tiers.md` | §2.3 item 2 (`:88-91`), §9.3 A1 (`:676-706`), §9.3 B3 (`:729-738`), §9.3 B4 (`:740-749`), §11.4 table (`:1015-1022`), §15.1 (`:1498-1504`), §16 table (`:1601-1609`), §20 anchor list (`:1825-1853`) |

⚠ The **historical-anchors banner** on the design's header block already landed with this plan's
revision commit — do not add a second one. Task 1 still owns §20's anchor list. The banner is 16
lines, so **every anchor into `privacy-tiers.md` shifted by +16** when it landed; the row above is
the post-banner set, re-verified on 2026-07-28. (Anchors into *code* are unaffected — the banner is
in a docs file.)

- [ ] **Step 1: Verify each correction against the tree**

```bash
cd /Users/wgu/Desktop/BioRouter-privacy
# 1. build_shell_command does not exist; configure_shell_command does.
grep -rn "build_shell_command" crates/ ; echo "expect: no output"
grep -n "fn configure_shell_command" crates/biorouter-mcp/src/developer/shell.rs
# expect: 330

# 2. The shell half of A1 already shipped (issue #57).
grep -n "strip_daemon_private_env" crates/biorouter-mcp/src/developer/shell.rs \
                                   crates/biorouter-mcp/src/developer/background.rs \
                                   crates/biorouter-sandbox/src/environment.rs
# expect: shell.rs:368 (one call), background.rs x5, environment.rs (the helper)

# 3. The stdio MCP spawn is still open.
grep -n "envs(all_envs)" crates/biorouter/src/agents/extension_manager.rs
# expect: one hit around :749, with NO strip_daemon_private_env anywhere near it
grep -c "env_clear" crates/biorouter/src/agents/extension_manager.rs ; echo "expect: 0"

# 4. memory: global memories are index-only since #58.
grep -n "GLOBAL_INDEX_HEADER" crates/biorouter-mcp/src/memory/mod.rs
# expect: :89 (the const) and :~302 (its use in compose_instructions)

# 5. ingest_conversation is a second fully-open cross-session read — and there are
#    THREE callers of the shared function, not one.
grep -n "session_ids" crates/biorouter/src/agents/knowledge_tool.rs | head -3
# expect: :32-41, the caller-supplied array
grep -rn "conversation_ingest::ingest_conversation\|ingest_conversation(" --include='*.rs' crates/ \
  | grep -v "^crates/biorouter/src/knowledge/conversation_ingest.rs"
# expect: knowledge_tool.rs:61 (the platform tool), routes/knowledge.rs:1233 (the HTTP
# SSE route, whose session_ids array at :1192-1212 is caller-supplied), and
# biorouter-cli/src/commands/knowledge.rs:571 — plus the two declaration lines.

# 6. The KB read side really does bypass the visible set (design B4, Correction 6).
grep -n "kb_root(self.service.root()" crates/biorouter-mcp/src/knowledge/server.rs
# expect: kb_search :591, kb_search_raw_sources :619, and the mutating tools
sed -n '308,311p' crates/biorouter-mcp/src/knowledge/server.rs
# expect: the doc comment "An explicit `kb_id` always wins and is never filtered
# against the session's set — that is how a hidden base (Soul) stays reachable."

# 7. Re-measure the day-one counts on the live DB (aggregate only, no message text).
sqlite3 ~/.local/share/biorouter/sessions/sessions.db "
  SELECT session_type,
         SUM(provider_name IN ('versa_azure','versa_bedrock','llamacpp','ollama')) AS would_private,
         SUM(provider_name IS NOT NULL AND provider_name NOT IN ('versa_azure','versa_bedrock','llamacpp','ollama')) AS public_named,
         SUM(provider_name IS NULL) AS null_provider,
         COUNT(*) AS total
  FROM sessions GROUP BY session_type;"
```

- [ ] **Step 2: Apply the seven corrections**

**Correction 1 — §2.3 item 2 and §19 item 1: LOAD is not the only fully-open cross-session read.**
Replace "This is the only fully-open cross-session read in the product today." with:

> This is **one of two** fully-open cross-session reads in the product today. The other is
> `platform__ingest_conversation` (`crates/biorouter/src/agents/knowledge_tool.rs:24-86`), which
> takes a caller-supplied `session_ids` array (`:32-41`), loads each session's full conversation
> with `get_session(sid, true)` (`:49`) and ingests it into a knowledge base — with no lineage,
> ownership or tier check. It is dispatched at `agent.rs:2660`, *before* the extension-manager
> fall-through at `:2769`, so Gate C never sees it; it is not an MCP tool, so Gate E cannot hide it;
> and it never touches `chat_history_search.rs`, so Gate D never sees it. It is advertised
> unconditionally (`agent.rs:3126-3131`, whose own comment reads "The conversation-ingestion tool
> is always available on the platform extension") and its description tells the model outright to
> "Pass `session_ids` to ingest specific (or multiple) sessions instead"
> (`agents/platform_tools.rs:63-65`). Because a knowledge base is a machine-wide tree any session
> may name (§9.3 B4), this is a one-call private→public laundering primitive, and it belongs at or
> above LOAD in §19's order.

**Correction 2 — §9.3 A1: half of it shipped, and one symbol never existed.**
Replace the `build_shell_command (crates/biorouter-mcp/src/developer/shell.rs:337-359)` citation
with `configure_shell_command (crates/biorouter-mcp/src/developer/shell.rs:330-378)`, and record:

> **Fix (1) is half done.** Issue #57 landed the daemon-credential scrub on the shell path:
> `configure_shell_command` now ends with `strip_daemon_private_env(&mut command_builder);`
> (`shell.rs:368`, with the comment at `:367` — "Last, so nothing set above can re-admit a daemon
> credential (issue #57)"), and `developer/background.rs` calls it at `:431`, `:680`, `:766`,
> `:802` and `:847`. The helper is `crates/biorouter-sandbox/src/environment.rs:54-79`, keyed on
> `is_daemon_private_env_key` (`:36-50`), and its own test asserts `BIOROUTER_SERVER__SECRET_KEY`
> and `BIOROUTER_ACP_WS_TOKEN` are stripped while `BIOROUTER_PORT` survives (`:94-103`).
> **The stdio MCP extension spawn is still open**: `extension_manager.rs:748-750` does
> `command.args(args).envs(all_envs)` with no `strip_daemon_private_env` and no `env_clear`
> (`grep -c env_clear` over that file returns 0). The remaining work is one line calling the
> existing helper — not a new `.env_remove`.

**Correction 3 — §9.3 B3 describes code that no longer exists.** Replace the paragraph with:

> **B3 (was critical; now a narrower channel) — `memory`'s global store.** Issue #58 already
> landed in this branch's base. `MemoryServer::new` (`crates/biorouter-mcp/src/memory/mod.rs:108`)
> calls `compose_instructions` (`:277`), and global memories are now **index-only**:
> `retrieve_all(true)` (`:278`) contributes only sorted **category names** under
> `GLOBAL_INDEX_HEADER` (`:89-94`), emitted at `:302-307`; bodies and tags are deliberately
> excluded. The doc comment at `:245-273` enumerates what #58 left for #56. Two residuals remain:
> (a) **local** memories are still inlined in full (`:310-322`), so a sensitive note saved locally
> reaches the system prompt of every session opened in that directory; and (b) issue #63 (OPEN) —
> `retrieve_memories(category="*", is_global=true)` (`:542`) returns the whole machine-wide store
> as a **tool call on a public built-in**, so Gate C (both ends public) and Gate E (the tool is
> legitimately listed) both miss it, and Auto mode auto-approves it. The v1 fix is therefore not a
> `retrieve_all` filter: it is to refuse `memory__remember_memory { is_global: true }` from a
> **private-capability** session, which needs no storage change and is the exact mirror of Gate C.

**Correction 4 — §11.4 gains the row it is missing.** Add to the table:

| Field | Verdict | Why |
|---|---|---|
| `ChatRecallResult.last_activity` (`chat_history_search.rs:14`, rendered at `chatrecall_extension.rs:219`) | **CONTENT-adjacent — withheld** | it is `max` over *matched* message timestamps (`:347-351`), so it dates the private message containing the search term, not the session. Under §11.4's own rule ("anything derived from a message body is content") it is message-derived. Moot once rows are filtered in SQL, but a reviewer checking the table for completeness must not find a hole. |

**Correction 5 — §15.1 migration numbering.** Replace with the O10 text above.

**Correction 6 — §9.3 B4's forced choice has been ruled on. Record the ruling.** Replace the closing
sentence ("Ratchet a KB's classification on ingest … or state plainly that KBs are a designed public
sink and warn at ingest. 'Follow-on' is too weak…") with:

> **Ruled (operator, second review round): ratchet.** A knowledge base takes the tier of the most
> sensitive session that has ingested into it, and a public-capability session may not read a private
> KB. The read side is enforced at the seven entry points that accept an explicit `kb_id` and
> therefore bypass the visible-set logic — `kb_search` (`knowledge/server.rs:590-592`),
> `kb_search_raw_sources` (`:618-619`), `kb_export` (`:743`), and the four that route through
> `kb_id_or_primary`, whose doc comment states the bypass outright ("An explicit `kb_id` always wins
> and is never filtered against the session's set", `:308-311`): `kb_list_pages` (`:379`),
> `kb_read_page` (`:396`), `kb_get_graph` (`:482`) and `kb_list_history` (`:497`). The tier lives in
> a machine-local sidecar beside `.active-kb` and `.hidden-kbs`, not in `manifest.yaml`, because the
> manifest travels inside the `.brkb` archive and an imported tier would be attacker-supplied.
> Existing knowledge bases migrate **public** (fail-open, DR-10), and there is no KB
> declassification path in v1. Both costs are accepted and recorded in the execution plan's
> [Accepted risks](privacy-tiers-execution-plan.md#accepted-risks) (AR-1, AR-2).

**Correction 7 — §16 counts, re-measured.** Replace the table with Step 1's output and add:

> Re-measured on 2026-07-28, one day after the design was written. The **NULL-provider** bucket for
> `user` sessions moved from 29 to **343** — an order of magnitude — so the fail-open residual is
> far larger than first reported. Of those 343, **175 have messages**: 175 real conversations of
> unknown provenance that backfill **public**. Separately, History shows fewer rows than the raw
> counts imply, because `list_sessions_by_types` uses `INNER JOIN messages m ON s.id = m.session_id`
> (`session_manager.rs:4066`), so empty sessions never appear. The number the first-run notice must
> quote is user+scheduled **with at least one message**. Re-measure at implementation time; this
> moved in a day.

Finally, replace §20's anchor list with a pointer to this plan's drift table, and add one line:
"Every anchor below was verified at `708390d8` and has since moved; see
[the execution plan's drift table](privacy-tiers-execution-plan.md#read-this-before-you-chase-a-line-number)."
The header banner that says the same thing to a reader who never reaches §20 already landed with this
plan's revision commit — leave it alone.

- [ ] **Step 3: Gate — the design no longer asserts anything the tree contradicts**

```bash
cd /Users/wgu/Desktop/BioRouter-privacy
# The vanished symbol is gone from the design and the real one is present.
grep -c "build_shell_command" docs/security/privacy-tiers.md ; echo "expect: 0"
grep -c "configure_shell_command" docs/security/privacy-tiers.md ; echo "expect: 1"
# The false uniqueness claim is gone.
grep -c "only fully-open cross-session read" docs/security/privacy-tiers.md ; echo "expect: 0"
# The stale memory mechanism is gone and the real one is named.
grep -c "appended to the server instructions" docs/security/privacy-tiers.md ; echo "expect: 0"
grep -c "GLOBAL_INDEX_HEADER" docs/security/privacy-tiers.md ; echo "expect: 1"
# The second open read is named.
grep -c "platform__ingest_conversation" docs/security/privacy-tiers.md ; echo "expect: >= 1"
# B4's either/or is resolved, not restated. ⚠ Match a phrase that is ON ONE LINE
# in the source: the sentence "…or state plainly that KBs are a designed public
# sink…" is hard-wrapped mid-phrase at `:747-748`, so grepping the whole sentence
# returns 0 both before AND after the edit — a gate that passes vacuously, which
# is the exact defect this revision exists to remove. Measured today: 1.
grep -c '"Follow-on" is too weak' docs/security/privacy-tiers.md ; echo "expect: 0 (it is 1 today)"
grep -c "Ruled (operator, second review round): ratchet" docs/security/privacy-tiers.md ; echo "expect: 1"
# The two anchor warnings are both present: the header banner (which a reader hits
# first) and the §20 pointer (which a reader who goes looking for the list hits).
head -20 docs/security/privacy-tiers.md | grep -c "Every line anchor in this document is historical"
echo "expect: 1 — the banner, in the header block, before §1"
grep -c "the execution plan's drift table" docs/security/privacy-tiers.md
echo "expect: 2 — the banner (already landed) and §20's replacement text (this task)"
```

**What this catches.** A worker who "corrects the design" by adding a footnote rather than deleting
the false claim: the zero-counts fail. Grepping only for the *presence* of the new text would pass
that wrong implementation, which is why every check here is a zero-count paired with a positive one.

- [ ] **Step 4: Commit**

```bash
git add docs/security/privacy-tiers.md
git commit -m "docs(security): correct privacy-tiers against the tree (#56)"
```

---

### Task 2: §9.3 A1 is already closed — correct the design, and pin the half nothing asserts

⚠ **The first version of this task was built on a false premise and prescribed a redundant fix.** It
claimed the stdio MCP extension spawn still leaks `BIOROUTER_SERVER__SECRET_KEY`, citing
`extension_manager.rs:749`'s `command.args(args).envs(all_envs)` with "no `strip_daemon_private_env`
anywhere near it". That observation is literally true and materially wrong: it greps the wrong stack
frame. Measured at `9558c346`:

- the stdio spawn hands its `Command` to `child_process_client` (`:752`),
- `child_process_client` (`:402`) calls `prepare_child_environment` (`:413`),
- `prepare_child_environment` (`:367`) ends at `:399` with
  `biorouter_mcp::developer::shell::strip_daemon_private_env(command)` — deliberately last, with a
  comment saying so, so that neither the block above it nor the extension's own declared `envs` can
  leave a credential behind,
- and `strip_daemon_private_env` (`biorouter-sandbox/src/environment.rs:54`) removes both the
  **inherited** keys (`env::vars_os()`) and the ones **explicitly set on the command**
  (`command.as_std().get_envs()`), via `doomed_env_keys` `:81-88`.

It landed in `b249a203` ("the daemon's auth secret no longer reaches tool processes") and
`8e7407fe` ("centralize the daemon-secret strip at the sandbox boundary"), **both ancestors of this
plan's own verification anchor `9558c346`** — confirm with
`git merge-base --is-ancestor b249a203 9558c346`. A passing test already covers it:
`daemon_secret_never_reaches_an_extension_child` (`extension_manager.rs:3169`), which re-invokes the
test binary with the secret exported and spawns a real child through the real
`prepare_child_environment`. `grep -c "BIOROUTER_SERVER__SECRET_KEY" extension_manager.rs` returns
**2 today**, and both hits are inside that test — which is why the first version's gate
(`expect: 2 — both in Step 1's test`) would have failed a *correct* implementation of its own
Step 1: the new test would have made it 4.

So this task ships **no** new scrub. It ships the three things that are genuinely missing.

1. **The design still says the leak is live** and prescribes `.env_remove(…)` as fix (1) of three.
   A reviewer checking §9.3 A1 against the tree finds the fix already there and concludes the plan
   was written against a different tree. Correct it, and record which of A1's three fixes remain
   open — (2) *stop carrying the secret in the environment at all* and (3) *bind declassify to a
   one-shot capability token* are both still open, and (3) is Task 29's business.
2. **Nothing pins the explicit/declared half at this layer.** The existing probe declares
   `CLINICAL_RECORDS_TOKEN` and `EXTENSION_MODE`; neither is in BioRouter's namespace, so the run
   never exercises `doomed_env_keys`' `.chain(explicit)`. This matters because
   `merge_environments` (`:471-510`) will *fetch* a declared `env_keys` entry out of the config or
   the OS keyring and put it on the command — so a `.brxt` manifest saying
   `env_keys: ["BIOROUTER_SERVER__SECRET_KEY"]` is a real, cheap attempt. Only
   `developer/shell.rs:643-655` pins that direction, one layer over.
3. **Nothing pins the structural invariant** that the extension spawn path *reaches*
   `prepare_child_environment` at all. Both spawns route through `child_process_client` today, and
   `TokioChildProcess::builder(` occurs exactly once in the whole tree (`:415`, two lines after the
   call). A future third spawn that builds its own transport would leak with no test failing.

**Files:**

| Action | Path | Anchor (re-verified at `9558c346`) |
|---|---|---|
| Modify | `docs/security/privacy-tiers.md` | §9.3 A1 at `:676-707`: the `printenv`/`curl` repro `:679-681`, the `build_shell_command` claim `:687-688` (Task 1 Correction 1 already replaces the *symbol*; this task replaces the *status*), the stdio claim `:689-690`, and the "Three fixes, all needed" list `:701-707` |
| Modify | `crates/biorouter/src/agents/extension_manager.rs` | `leak_probe_prints_extension_child_env` `:3080-3100` (the `declared` map at `:3084-3095`); `daemon_secret_never_reaches_an_extension_child` `:3167-3181`; `run_extension_leak_probe` `:3134-3165` |
| Reference | `crates/biorouter/src/agents/extension_manager.rs` | `prepare_child_environment` `:367-400` (strip at `:399`); `child_process_client` `:402-413`; the two spawns `:748` (stdio) and `:814` (inline-python), both handing off at `:752`/`:822` |
| Reference | `crates/biorouter-sandbox/src/environment.rs` | `is_daemon_private_env_key` `:36-50`; `strip_daemon_private_env` `:54-67`; `doomed_env_keys` `:81-88`; its own test `:94-113` |
| Reference | `crates/biorouter-mcp/src/developer/shell.rs` | `:643-655` — the only place the *explicit* direction is pinned today, and the shape to copy |

- [ ] **Step 1: Write the test**

⚠ **This one starts green, and that is stated rather than hidden.** Every other task in this plan
opens with a red test; here the behaviour already works and the test is a **pin**. Step 2 is
therefore a *mutation* check — the same shape Task 7 Step 2 uses to give an empty `EXPECTED` meaning.
Do not "fix" a passing Step 1 by weakening it.

Extend the child half of the existing probe so it declares a daemon-private key of its own, exactly
as a hostile `.brxt` manifest would (`extension_manager.rs:3084`):

```rust
        // What `merge_environments` hands the spawn path for an extension that
        // declares its own credentials — including, since #56, one it is not
        // entitled to. A manifest may name any key in `env_keys`, and
        // merge_environments will resolve it out of the config or the OS
        // keyring and set it on the Command. `strip_daemon_private_env` covers
        // the explicitly-set case as well as the inherited one
        // (`doomed_env_keys` chains `env::vars_os()` with the command's own
        // envs); this is the assertion that says so at THIS layer rather than
        // only at developer/shell.rs:643.
        let declared = HashMap::from([
            (
                "CLINICAL_RECORDS_TOKEN".to_string(),
                "declared-credential-ok".to_string(),
            ),
            (
                "EXTENSION_MODE".to_string(),
                "declared-plain-ok".to_string(),
            ),
            (
                "BIOROUTER_SERVER__SECRET_KEY".to_string(),
                "declared-daemon-secret-9f2c".to_string(),
            ),
            (
                "BIOROUTER_ACP_WS_TOKEN".to_string(),
                "declared-acp-token-9f2c".to_string(),
            ),
        ]);
```

and add the assertion to the existing test (`:3169`), beside the two it already makes:

```rust
        // A manifest that ASKS for the daemon's key does not get it either. The
        // inherited path is covered by CANARY above; this is the explicit path,
        // and it is the one a malicious extension author controls.
        assert!(
            !child_env.contains("declared-daemon-secret-9f2c")
                && !child_env.contains("declared-acp-token-9f2c"),
            "an extension declared a daemon-private key in its own envs and received it:\n{child_env}"
        );
```

- [ ] **Step 2: Run it, then break the strip and watch it fail**

```bash
cargo test -p biorouter --lib \
  agents::extension_manager::tests::daemon_secret_never_reaches_an_extension_child \
  | grep "test result:" ; echo "expect: 1 passed; 0 failed"
```

Then comment out `biorouter_mcp::developer::shell::strip_daemon_private_env(command);` at
`extension_manager.rs:399`, re-run the same command, and confirm **`0 passed; 1 failed`** with the
new assertion in the failure message — not the pre-existing CANARY one, which would mean the new
assertion is unreachable. **Restore the line.** A pin that cannot fail is a comment.

⚠ Assert the printed **count**, not the exit code: a libtest filter that matches nothing prints
`0 passed` and exits 0 (see
[Which test filters are validated, and which are not](#which-test-filters-are-validated-and-which-are-not)).

- [ ] **Step 3: Correct the design**

In `docs/security/privacy-tiers.md` §9.3 A1, replace the two "verified" sentences that assert the
leak is live with the measurement above, and rewrite the fix list. Keep the finding — the *reasoning*
about why an environment-carried secret defeats Gates B and D is correct and load-bearing for Task 29
— but state its status:

> **Closed for the tool-process paths (2026-07).** `strip_daemon_private_env`
> (`crates/biorouter-sandbox/src/environment.rs:54`) removes BioRouter's daemon-private variables
> from every child spawned on an agent's behalf, both the inherited copies and any the extension's
> own manifest explicitly declares. It is invoked last inside `prepare_child_environment`
> (`extension_manager.rs:399`), which every stdio and inline-python extension spawn reaches through
> `child_process_client`, and inside `configure_shell_command` (`developer/shell.rs:368`), which is
> the Developer server's `shell`. Landed in `b249a203` and `8e7407fe` (issue #57). Fix (1) below is
> therefore **done**, and pinned by `daemon_secret_never_reaches_an_extension_child`
> (`extension_manager.rs:3169`).
>
> Fixes (2) and (3) remain open. (2) — stop carrying the secret in the environment at all — is
> unaddressed and is the reason this finding is not simply deleted: the strip is a filter, and a
> filter is only as good as its key list. (3) — bind declassification to a one-shot capability token
> rather than to `X-Secret-Key` — is [Open question 13](privacy-tiers-execution-plan.md#open-questions)
> and is why Task 29's R9 property is "only a human *through the GUI*", not "only a human".

- [ ] **Step 4: Run**

```bash
cargo test -p biorouter --lib agents::extension_manager 2>&1 | grep "test result:"
cargo test -p biorouter-sandbox --lib environment 2>&1 | grep "test result:"
cargo test -p biorouter-mcp --lib developer::shell 2>&1 | grep "test result:"
```

Record the `agents::extension_manager` pre-count **before** Step 1 with the identical command; the
post-count must be exactly `pre + 0` — this task adds assertions to an existing test, not a new one.
A `pre + 1` means the assertion was written as a separate `#[test]` that does not re-invoke the
probe, and therefore does not exercise the spawn path at all.

- [ ] **Step 5: Gate**

```bash
cd /Users/wgu/Desktop/BioRouter-privacy
# The strip is where it must be: last in prepare_child_environment, on the one
# path both spawns take. Anchored on the ENCLOSING FUNCTION — a file-wide count
# would be satisfied by a call anywhere, including inside a test.
awk '/fn prepare_child_environment/,/^}/' crates/biorouter/src/agents/extension_manager.rs \
  | grep -c "strip_daemon_private_env" ; echo "expect: 1"
# STRUCTURAL: there is exactly one place a child transport is built, and the
# strip runs immediately before it. This is the gate that survives a future
# third spawn path — the only way this leak can come back.
grep -c "TokioChildProcess::builder(" crates/biorouter/src/agents/extension_manager.rs
echo "expect: 1 — a second one is a spawn that may never have seen prepare_child_environment"
awk '/async fn child_process_client/,/^}/' crates/biorouter/src/agents/extension_manager.rs \
  | grep -n "prepare_child_environment\|TokioChildProcess::builder(" | head -2
echo "expect: prepare_child_environment on the SMALLER line number"
# The key list is not duplicated: nothing outside biorouter-sandbox names a
# daemon-private key in PRODUCTION code. PRINT the hits with line numbers and
# compare them against the `#[cfg(test)]` boundary rather than counting: a bare
# count is exactly the fragile shape that made the first version of this gate
# wrong. Measured today: 2 hits, at :3148 (run_extension_leak_probe's .env) and
# :3178 (the CANARY assertion), both far below the tests boundary at :1832.
grep -n "BIOROUTER_SERVER__SECRET_KEY\|BIOROUTER_ACP_WS_TOKEN" \
  crates/biorouter/src/agents/extension_manager.rs
grep -n "#\[cfg(test)\]" crates/biorouter/src/agents/extension_manager.rs | tail -1
echo "expect: 4 hits after this task (the 2 above, plus the 2 new keys in the declared map),"
echo "  and EVERY hit's line number must be GREATER than the #[cfg(test)] line printed last."
echo "  The two new assertions match on the VALUES (declared-daemon-secret-9f2c,"
echo "  declared-acp-token-9f2c), not the key names, so they add no hits here."
echo "  A hit ABOVE the boundary is a re-implemented key list in production code."
# And no second scrubber was invented. `scrub_daemon_env` was the first version
# of this task's own proposal; it must not exist, and neither must a hand-rolled
# env_remove of a DAEMON-PRIVATE key.
# ⚠ Match the two daemon-private prefixes exactly, not `"BIOROUTER`: there is a
# legitimate pre-existing `.env_remove("BIOROUTERD_BIN")` at
# crates/biorouter-mcp/src/agent_drafter/render.rs:1596 (an exported app's
# launcher path, not a credential), and a broader pattern reads red on a correct
# implementation. Measured with the pattern below: 0 today.
grep -rnE 'fn scrub_daemon_env|\.env_remove\("BIOROUTER_(SERVER__|ACP_)' --include='*.rs' crates/
echo "expect: no output — the one predicate lives in biorouter-sandbox and is shared"
# The design no longer asserts a closed leak is open.
grep -c "Closed for the tool-process paths" docs/security/privacy-tiers.md ; echo "expect: 1"
grep -c "build_shell_command" docs/security/privacy-tiers.md ; echo "expect: 0 (Task 1 removed it)"
```

**What this catches.** Three things, none of which is "the leak". (1) A worker who takes the first
version of this task at face value and adds `scrub_daemon_env` at the *narrower* layer: the tree
then strips twice, at two layers, with two key lists — and the second one drifts. The
`fn scrub_daemon_env` zero-count forbids it by name. (2) A worker who "corrects the design" by
deleting §9.3 A1 outright, losing fixes (2) and (3) — which are open, and (3) of which is the reason
R9 is scoped to the GUI. The positive count on the new status paragraph is what keeps the finding
alive. (3) The real future regression: a third extension spawn that builds its own
`TokioChildProcess` and never reaches `prepare_child_environment`. Nothing in the tree catches that
today, and no behavioural test can — the leaking path would not exist yet. The two structural greps
are the whole reason this task is not simply deleted.

- [ ] **Step 6: Commit**

```bash
git add crates/biorouter/src/agents/extension_manager.rs docs/security/privacy-tiers.md
git commit -m "test(extensions): pin the declared-key half of the daemon-secret strip, and correct A1's status (#56, #57)"
```

---

### Task 3: Phase 0 gate

- [ ] **Step 1: Backend suite**

```bash
cargo test --workspace --no-fail-fast 2>&1 | tail -20
```

Expected: no failure beyond this machine's recorded baseline (`providers::test_anthropic_provider`
calls the live Anthropic API and fails on billing; the frontend `SessionListView.test.tsx` isolation
flake also fails on a clean tree — **verify that claim on a clean checkout before dismissing it**,
rather than assuming it).

- [ ] **Step 2: Lints**

```bash
cargo fmt --check && ./scripts/clippy-lint.sh
```

- [ ] **Step 3: The credential is genuinely off the child's environment (manual, once)**

⚠ **"Before this task it is 1" was false**, and the whole point of running this by hand is to see
that for yourself. `strip_daemon_private_env` landed in `b249a203`/`8e7407fe`, both ancestors of the
fork point, so the count is **0 before Task 2 and 0 after it** — a bare zero-count here proves
nothing at all. The check below is therefore a **paired** one: the daemon's key must be absent *and*
the extension's own declared credential must be present, on the same probe run. Only the pair
distinguishes "the strip works" from "the extension never started, or `env_clear()` took everything".

```bash
just debug-server &      # BIOROUTER_SERVER__SECRET_KEY=test, port 3000
# Add a trivial stdio extension whose command is `printenv` and whose manifest
# declares BOTH an ordinary credential and a daemon-private one:
#   envs:     { SPOKEAGENT_PASSCODE: "extension-private-ok" }
#   env_keys: [ "BIOROUTER_SERVER__SECRET_KEY" ]
ENV=$(curl -s -X POST http://127.0.0.1:3000/agent/call_tool -H 'X-Secret-Key: test' \
  -H 'Content-Type: application/json' \
  -d '{"session_id":"'"$SID"'","name":"envprobe__printenv","arguments":{}}')
echo "$ENV" | grep -c "BIOROUTER_SERVER__SECRET_KEY" ; echo "expect: 0 — inherited AND declared"
echo "$ENV" | grep -c "BIOROUTER_ACP_WS_TOKEN"        ; echo "expect: 0"
echo "$ENV" | grep -c "extension-private-ok"          ; echo "expect: 1 — the child really ran,"
echo "  and the strip did not take the extension's own credential with it. Without this"
echo "  positive half, an extension that failed to spawn scores a perfect 0/0 and reads green."
echo "$ENV" | grep -c "BIOROUTER_PORT"                ; echo "expect: 1 — deliberately preserved"
```

- [ ] **Step 4: Commit (no code; record the gate in the PR description)**

---

# Phase 1 — the tier model

Six tasks. After this phase the tree can *say* what tier something is, and nothing yet acts on it.
That separation is deliberate: every gate in Phase 2 is then one branch over an already-tested
lookup.

### Task 4: `ProviderTier`, `SessionClassification`, and the one crossing

**Files:**

| Action | Path | Anchor (re-verified at `9558c346`) |
|---|---|---|
| Create | `crates/biorouter/src/privacy/mod.rs` | new |
| Modify | `crates/biorouter/src/lib.rs` | the `pub mod` list — `grep -n "^pub mod" crates/biorouter/src/lib.rs` |

Named `SessionClassification`, not `Classification`: `crates/biorouter/src/security/classification_client.rs`
already defines `ClassificationClient` / `ClassificationRequest` / `ClassificationResponse` in this
crate (D3).

- [ ] **Step 1: Write the failing test**

In the new module's own `#[cfg(test)] mod tests`:

```rust
#[test]
fn capability_is_a_least_and_classification_is_a_max() {
    use ProviderTier::{Private, Public};
    // CAPABILITY: least privileged wins. A private lead with a public worker
    // has public reach, because the transcript already goes to the worker.
    assert_eq!(ProviderTier::least(Private, Public), Public);
    assert_eq!(ProviderTier::least(Private, Private), Private);
    assert_eq!(ProviderTier::least(Public, Public), Public);

    // CLASSIFICATION: most sensitive wins, and it is Ord so `max` is spellable.
    assert!(SessionClassification::Private > SessionClassification::Public);
    assert_eq!(
        SessionClassification::Public.max(SessionClassification::Private),
        SessionClassification::Private
    );

    // The ONE crossing.
    assert_eq!(floor(Private), SessionClassification::Private);
    assert_eq!(floor(Public), SessionClassification::Public);
}

#[test]
fn an_unparseable_or_absent_classification_reads_private() {
    assert_eq!(SessionClassification::from_stored("private"), SessionClassification::Private);
    assert_eq!(SessionClassification::from_stored("public"), SessionClassification::Public);
    // Fail closed, loudly: a bug in a projection paints every session Private —
    // immediately visible, immediately fixed, safe meanwhile.
    assert_eq!(SessionClassification::from_stored("PUBLIC"), SessionClassification::Public);
    assert_eq!(SessionClassification::from_stored("nonsense"), SessionClassification::Private);
    assert_eq!(SessionClassification::from_stored(""), SessionClassification::Private);
}
```

- [ ] **Step 2: Run it**

```bash
cargo test -p biorouter --lib privacy::tests
```

Expected: **COMPILE ERROR** — `unresolved module privacy`.

- [ ] **Step 3: Implement**

```rust
//! Privacy tiers (issue #56). Two lattices over two different domains.
//!
//! * [`ProviderTier`] is CAPABILITY — what a session may *do*. It reduces with
//!   [`ProviderTier::least`] over the components of the provider bound right
//!   now. It is a pure function of live state and is never stored.
//! * [`SessionClassification`] is CLASSIFICATION — how sensitive a session's
//!   *contents* are. It reduces with `max` over events in time and is stored in
//!   `sessions.privacy_tier`, where it is a permanent ratchet.
//!
//! They do not interconvert. There is exactly one crossing, [`floor`], and a
//! repo-grep test in `Task 7` asserts its caller count.
//!
//! Invariant, proven by induction in the design (§4): for any sequence of legal
//! binds, `capability(S) >= classification(S)`. The bind admits `P` only when
//! `tier(P) >= classification(S)`; the ratchet then sets
//! `classification := max(old, floor(tier(P))) <= floor(tier(P))`.

use serde::{Deserialize, Serialize};

/// CAPABILITY — the least-privileged model currently bound to a session.
///
/// Deliberately **not** `Ord`: `max` over this type is always a bug. A mixed
/// lead/worker composite is `least(lead, worker)`, so a private lead with a
/// public worker has **public** reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ProviderTier {
    Public,
    Private,
}

impl ProviderTier {
    /// The capability reduction. Public is less privileged, so it wins.
    pub fn least(a: Self, b: Self) -> Self {
        match (a, b) {
            (Self::Private, Self::Private) => Self::Private,
            _ => Self::Public,
        }
    }

    pub fn is_private(self) -> bool {
        matches!(self, Self::Private)
    }
}

impl Default for ProviderTier {
    /// Fail-**safe**, not fail-open: Public is the *less* privileged tier, so a
    /// provider module that forgets `tier()` gets less reach, never more.
    fn default() -> Self {
        Self::Public
    }
}

/// CLASSIFICATION — the most sensitive thing a session has ever touched.
///
/// `Ord` is derived and `Public < Private`, so `max` is the accumulation and is
/// spellable. Monotone in time; the storage layer refuses to lower it (see
/// `SessionUpdateBuilder`'s `CASE WHEN` emission).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, utoipa::ToSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum SessionClassification {
    Public,
    Private,
}

impl SessionClassification {
    pub const PUBLIC_SQL: &'static str = "public";
    pub const PRIVATE_SQL: &'static str = "private";

    /// Named constructor for `#[serde(default = "…")]`, which takes a **path to
    /// a function**, not a variant. Task 6's `Session::privacy_tier` field uses
    /// `#[serde(default = "SessionClassification::public")]`; without this the
    /// struct does not compile (`expected function, found variant`).
    ///
    /// Serde's default is the *deserialization* fallback for a JSON document
    /// with no `privacy_tier` — an exported/imported session file, not a
    /// database row. Public is right here and Private is right for the DB read,
    /// and they differ on purpose: Task 22's `import_session` never trusts the
    /// deserialized value as authority to be public (it raises to Private and
    /// only ever raises), while `from_stored` below fails closed because a
    /// missing *column* is a projection bug rather than an absent field.
    pub fn public() -> Self {
        Self::Public
    }

    pub fn as_sql(self) -> &'static str {
        match self {
            Self::Public => Self::PUBLIC_SQL,
            Self::Private => Self::PRIVATE_SQL,
        }
    }

    /// Parse a stored value. **Fails closed**, deliberately breaking the
    /// tree's `try_get(..).ok().flatten()` convention for optional columns
    /// (`session_manager.rs:1971-1977`): an unrecognised or absent value is a
    /// bug in a projection, and `branch_point_msg_uid`'s absence from
    /// `list_sessions_by_types` is the live proof that a projection does get
    /// missed. Private paints every row with a badge the user will report on
    /// day one, and is safe until they do.
    pub fn from_stored(raw: &str) -> Self {
        match raw {
            Self::PUBLIC_SQL => Self::Public,
            Self::PRIVATE_SQL => Self::Private,
            other => {
                tracing::error!(
                    value = other,
                    "unrecognised sessions.privacy_tier; reading Private (fail-closed)"
                );
                Self::Private
            }
        }
    }

    pub fn is_private(self) -> bool {
        matches!(self, Self::Private)
    }
}

/// The ONE crossing between the two lattices: the classification floor a turn
/// run under `tier` establishes. `pub(crate)` on purpose — a repo-grep test
/// asserts the caller count, so a third crossing cannot appear unnoticed.
pub(crate) fn floor(tier: ProviderTier) -> SessionClassification {
    match tier {
        ProviderTier::Private => SessionClassification::Private,
        ProviderTier::Public => SessionClassification::Public,
    }
}

/// A session may bind `incoming` only when the provider is at least as private
/// as the session's contents. This is Gate A's predicate, extracted so it can
/// be unit-tested without a database.
pub fn bind_allowed(incoming: ProviderTier, target: SessionClassification) -> bool {
    match target {
        SessionClassification::Public => true,
        SessionClassification::Private => incoming.is_private(),
    }
}

/// Gate D / the §7 matrix's VIS rule: a caller sees a target only when the
/// target's classification does not exceed the caller's capability.
pub fn visible_to(caller: ProviderTier, target: SessionClassification) -> bool {
    bind_allowed(caller, target)
}
```

Register the module in `crates/biorouter/src/lib.rs`: `pub mod privacy;`.

- [ ] **Step 4: Run**

```bash
cargo test -p biorouter --lib privacy
```

Expected: **PASS**, 2 tests **at this point in the step order**. Step 5 adds a third in the same
`mod tests`, so **this task ends at 3**, and Task 7 — which adds two more to the same module — ends
at 5. Every later gate that quotes a `privacy::` count derives from that ladder; do not read "2" as
the task's final number.

- [ ] **Step 5: Gate**

```bash
# `least` is not `max`: the type must not be Ord.
grep -c "PartialOrd" crates/biorouter/src/privacy/mod.rs ; echo "expect: 1 (SessionClassification only)"
# There is no From/Into between the two lattices.
grep -c "impl From<ProviderTier> for SessionClassification" crates/biorouter/src/privacy/mod.rs ; echo "expect: 0"
grep -c "impl From<SessionClassification> for ProviderTier" crates/biorouter/src/privacy/mod.rs ; echo "expect: 0"
```

Plus a behavioural gate no grep can substitute for — add it now, it costs one test:

```rust
#[test]
fn deriving_ord_on_provider_tier_would_be_caught_here() {
    // If someone adds `PartialOrd` to ProviderTier, `least` becomes
    // interchangeable with `min` and a reviewer stops noticing which is meant.
    // This is the semantic assertion: the two lattices disagree on the same
    // pair, which is the entire point of having two of them.
    let cap = ProviderTier::least(ProviderTier::Private, ProviderTier::Public);
    let cls = SessionClassification::Private.max(SessionClassification::Public);
    assert_eq!(cap, ProviderTier::Public);
    assert_eq!(cls, SessionClassification::Private);
}
```

**What this catches.** The single-enum simplification — one `Ord` type used for both — which
compiles, passes any test written against one lattice, and silently makes a private-lead composite
badge Private (the exact inverse of R2, because `LeadWorkerProvider::get_name()` returns the lead's
name, verified at `providers/lead_worker.rs:332-334`). A grep for `enum ProviderTier` would not
catch it; the paired assertion above does.

- [ ] **Step 6: Commit**

```bash
git add crates/biorouter/src/privacy/mod.rs crates/biorouter/src/lib.rs
git commit -m "feat(privacy): add ProviderTier and SessionClassification, the two lattices (#56)"
```

---

### Task 4b: Resolve every test filter against a real `cargo --list` (docs only)

**This is the task that closes the plan's largest unclosed risk, and it costs one command.**

Four adversarial passes have now read this plan and every one of them ended with the same sentence:
*nothing has been compiled or run.* No `cargo`, no `vitest`, no OpenAPI regeneration. The
consequence is named in
[Which test filters are validated](#which-test-filters-are-validated-and-which-are-not): every
`cargo test` filter here was resolved **statically** — file exists, `#[cfg(test)]` present — which
catches a path that resolves to nothing but **cannot** catch *a test nested differently from the way
its file suggests*. That is BR-71's single most expensive defect (a filter that prints `0 passed` and
exits 0), and it is unruled-out for every filter in this plan, **including the ones this plan marks
green**. The last verifier named an actual `cargo test -- --list` after Task 4 as the one thing that
would most change its confidence.

Task 4 is the first commit that produces a `privacy::` module, so this is the earliest point at which
that command can be run. It is docs-only: it changes this file and nothing else.

⚠ **Do this task even if — especially if — the filters look fine.** The two errors this catches are
both silent: a filter that matches nothing is green, and a filter that matches *more* than intended
is also green. §[Which test filters are validated](#which-test-filters-are-validated-and-which-are-not)
already records two modules (`routes::agent`, `routes::session`) whose test blocks are **not** named
`tests` and which a module-path filter picks up anyway — that is the shape, found by hand, and there
is no reason to think the hand-search found all of them.

**Files:**

| Action | Path | Anchor |
|---|---|---|
| Modify | `docs/security/privacy-tiers-execution-plan.md` | §[Which test filters are validated](#which-test-filters-are-validated-and-which-are-not) `:400-462`, which gains the measured table; and any task whose filter the listing contradicts |

- [ ] **Step 1: List the real module paths, all four packages**

```bash
cd /Users/wgu/Desktop/BioRouter-privacy
mkdir -p /tmp/56-filters
# Five packages, because this plan filters on five. `biorouter-sandbox` is easy to
# forget — it appears exactly once (Task 2's `--lib environment`).
for p in biorouter biorouter-server biorouter-mcp biorouter-cli biorouter-sandbox; do
  cargo test -p "$p" --lib -- --list 2>/dev/null \
    | sed -n 's/: test$//p' | sort > "/tmp/56-filters/$p.txt"
  echo "$p: $(wc -l < /tmp/56-filters/$p.txt) tests"
done
# The privacy module as the tree actually nests it, verbatim:
grep '^privacy' /tmp/56-filters/biorouter.txt
```

⚠ **libtest filters are SUBSTRING matches, not prefix matches** (there is no `--exact` anywhere in
this plan). `privacy` therefore matches `privacy::tests::…` *and* anything else whose full path
contains the word. That is not a defect to fix — it is how every gate in this plan will actually
behave — so the resolution check below matches the same way, and the number it prints is the number
the task's `pre + N` arithmetic must be built on.

- [ ] **Step 2: Paste the output into the plan**

Replace the placeholder below — in this task, in this file — with the literal `grep '^privacy'`
output. Three tests exist at this point (Task 4 Step 1 wrote two, Step 5 added a third), so three
lines are expected; **paste what the command printed, not what this sentence predicts.**

```text
PASTE HERE (Task 4b Step 1 output, run after Task 4 lands):
privacy::tests::…
privacy::tests::…
privacy::tests::…
```

⚠ **Everything below in Step 3 was already measured**, against `main` at `89c1f026` on 2026-07-29,
by running Step 1 verbatim (`89c1f026` differs from this plan's anchor `9558c346` in six
developer-only files, none of which is a module any filter names). What Step 2 adds is the one thing
that run could not produce: the `privacy::` paths, which do not exist until Task 4 lands. Everything
else is a **re-run to confirm**, not a first measurement.

- [ ] **Step 3: Resolve every filter, and correct the ones that disagree — MEASURED**

Every `cargo test -p <pkg> --lib <FILTER>` line in this plan falls into exactly one of two sets, and
the gate in Step 5 asserts that partition. **42** `(package, filter)` pairs, all of them below.

**Resolves today — 30 pairs, with the pre-count the owning task must build its `pre + N` on:**

| Package | Filter | **Measured** |
|---|---|---|
| `biorouter` | `agents::agent` | **21** (`::tests` 14, `::rewrite_basis_tests` 2, `::stall_seam_tests` 5 — *three* modules, none discoverable from the filter) |
| `biorouter` | `agents::code_execution_extension` | 69 |
| `biorouter` | `agents::extension_manager` | **37** — ⚠ `::tests` is 33; the filter also catches `agents::extension_manager_extension::tests` (4) by substring |
| `biorouter` | `agents::extension_manager_extension` | 4 |
| `biorouter` | `agents::knowledge_tool` | 4 |
| `biorouter` | `agents::mcp_client` | 12 |
| `biorouter` | `agents::reply_parts` | 2 |
| `biorouter` | `agents::subagent_tool` | 16 |
| `biorouter` | `hooks` | 93 |
| `biorouter` | `knowledge::conversation_ingest` | 2 |
| `biorouter` | `providers` | 359 |
| `biorouter` | `scheduler` | 3 |
| `biorouter` | `session::session_manager` | 139 |
| `biorouter-cli` | `commands::knowledge` | 9 |
| `biorouter-cli` | `session` | 166 |
| `biorouter-mcp` | `agent_drafter` | 244 |
| `biorouter-mcp` | `agent_drafter::catalog` | 5 |
| `biorouter-mcp` | `agent_drafter::validate` | 9 |
| `biorouter-mcp` | `developer::shell` | 16 |
| `biorouter-mcp` | `knowledge` | **190** — ⚠ the plan said "~122"; see Task 10A Step 4 |
| `biorouter-mcp` | `knowledge::macros` | 10 |
| `biorouter-mcp` | `knowledge::macros::ingest` | 3 |
| `biorouter-mcp` | `knowledge::server` | 11 |
| `biorouter-mcp` | `knowledge::service` | 38 |
| `biorouter-mcp` | `memory` | 12 |
| `biorouter-sandbox` | `environment` | 1 |
| `biorouter-server` | `routes::agent` | **8** ✓ confirms the hand-measured figure |
| `biorouter-server` | `routes::apps` | 90 |
| `biorouter-server` | `routes::config_management` | 3 |
| `biorouter-server` | `routes::session` | **20** ✓ confirms the hand-measured figure |

**Deferred — 12 pairs, each with the task that creates it. A filter in neither list is the defect:**

| Package | Filter | Created by | Pre-count today |
|---|---|---|---|
| `biorouter` | `privacy` | Task 4 | **0** ✓ |
| `biorouter` | `privacy::tests` | Task 4 | **0** ✓ |
| `biorouter` | `providers::tier_tests` | Task 5 | **0** ✓ |
| `biorouter` | `privacy::extensions` | Task 8 | **0** ✓ |
| `biorouter` | `agents::chatrecall_extension` | Tasks 10, 17 | **0** ✓ confirms "no `#[cfg(test)]` at all" |
| `biorouter-mcp` | `knowledge::tier` | Task 10A | **0** ✓ |
| `biorouter` | `privacy::refusal` | Task 12 | **0** ✓ |
| `biorouter` | `session::chat_history_search` | Task 17 | **0** ✓ confirms "no `#[cfg(test)]` at all" |
| `biorouter` | `privacy::alt_provider` | Task 19 | **0** ✓ |
| `biorouter` | `privacy::visibility` | Task 21 | **0** ✓ |
| `biorouter` | `every_copy_path_carries_the_tier_and_the_provider` | Task 22 | **0** ✓ — a bare **test name**, not a module; it is in the `--lib -- …` form and is invisible to an audit of only the plain form |
| `biorouter` | `privacy::declassify` | Task 29 | **0** ✓ |

**What the measurement changed.** Three things, and none of them was catchable by reading:

1. `knowledge::` is **190**, not "~122" (a stale figure inherited from `CLAUDE.md`). Task 10A's
   `pre + 10` assertion built on 122 would have read a 68-test shortfall as a pass. Corrected in
   Task 10A Step 4.
2. `agents::extension_manager::tests` is **33**, not the 27 an earlier draft asserted, and the
   *filter* reports **37** because libtest substring-matches
   `agents::extension_manager_extension::tests`. Corrected in Task 10A Step 1's comment.
3. `agents::agent` spans **three** test modules — `tests`, `rewrite_basis_tests`, `stall_seam_tests`
   — of which only the first is named `tests`. That is the same shape as `routes::agent` and
   `routes::session`, which §"Which test filters are validated" found by hand; the pattern is
   general, and a hand search should not be trusted to have found all of it.

**What it confirmed.** Every "0 today" claim this plan makes: `agents::chatrecall_extension` and
`session::chat_history_search` really do have no tests, and `routes::agent` = 8 / `routes::session`
= 20 are exact. No filter in this plan names a path that resolves to *something else* — the failure
mode that would have been worst — and none is misspelled.

For every filter that resolves, record the pre-count above in its owning task, so the task asserts
`post == pre + N` rather than "non-zero".

- [ ] **Step 4: Run** — nothing to run beyond Step 1. This task compiles no code and changes no code.

- [ ] **Step 5: Gate**

```bash
cd /Users/wgu/Desktop/BioRouter-privacy
PLAN=docs/security/privacy-tiers-execution-plan.md
# (a) Every `-p <pkg> --lib <FILTER>` in the plan either resolves against that
#     package's listing, or is in the deferred set with the task that creates it.
#     A filter in NEITHER set is the BR-71 defect and fails here.
#
# ⚠ The deferred set is keyed on the (PACKAGE, FILTER) PAIR, not on the filter
# name. A name-only allowlist — which is what this gate used to be — excuses the
# filter in EVERY package: `cargo test -p biorouter-mcp --lib privacy::refusal`
# names nothing, will never name anything (`privacy/refusal.rs` is created in
# `biorouter`, see File structure), and was reported DEFER because a filter of
# the same name is expected in a different crate. That is the identical shape to
# the defect this whole task exists to catch — a filter that prints `0 passed`
# and exits 0 — reintroduced by the gate meant to catch it. The `PKG` column is
# what closes it.
#
# The 12 deferred rows from Step 3's second table, verbatim: package, filter,
# owning task, and the EVIDENCE grep that proves this plan really creates it.
# Every one measured 0 today.
cat > /tmp/56-filters/deferred.txt <<'ROWS'
biorouter|privacy|4|crates/biorouter/src/privacy/mod.rs
biorouter|privacy::tests|4|crates/biorouter/src/privacy/mod.rs
biorouter|providers::tier_tests|5|crates/biorouter/src/providers/tier_tests.rs
biorouter|privacy::extensions|8|crates/biorouter/src/privacy/extensions.rs
biorouter|agents::chatrecall_extension|10,17|crates/biorouter/src/agents/chatrecall_extension.rs
biorouter-mcp|knowledge::tier|10A|crates/biorouter-mcp/src/knowledge/tier.rs
biorouter|privacy::refusal|12|crates/biorouter/src/privacy/refusal.rs
biorouter|session::chat_history_search|17|crates/biorouter/src/session/chat_history_search.rs
biorouter|privacy::alt_provider|19|crates/biorouter/src/privacy/alt_provider.rs
biorouter|privacy::visibility|21|crates/biorouter/src/privacy/visibility.rs
biorouter|every_copy_path_carries_the_tier_and_the_provider|22|fn every_copy_path_carries_the_tier_and_the_provider
biorouter|privacy::declassify|29|crates/biorouter/src/privacy/declassify.rs
ROWS
wc -l < /tmp/56-filters/deferred.txt ; echo "expect: 12 — Step 3's second table has 12 rows"
# BOTH spellings. Measured at this revision: 79 occurrences of the plain form
# `--lib <FILTER>` and 7 of the `--lib -- <NAME> <NAME>` form, deduplicating to
# 42 (package, filter) pairs. The second pattern is not optional: the first
# misses it entirely, including the only two mentions of privacy::refusal and
# privacy::alt_provider anywhere in this plan.
{ grep -oE 'cargo test -p [a-z-]+ --lib [a-z_]+(::[a-z_]+)*' "$PLAN" \
    | sed -E 's/cargo test -p ([a-z-]+) --lib /\1 /'
  grep -oE 'cargo test -p [a-z-]+ --lib -- [a-z_: ]+' "$PLAN" \
    | sed -E 's/cargo test -p ([a-z-]+) --lib -- /\1 /' \
    | awk '{ for (i = 2; i <= NF; i++) print $1, $i }'
} | sort -u > /tmp/56-filters/wanted.txt
wc -l < /tmp/56-filters/wanted.txt ; echo "expect: 42 (pkg, filter) pairs audited"
while read -r pkg filter; do
  # ⚠ `|| n=0`, not `|| echo 0`: `grep -c` PRINTS 0 and EXITS 1 when it matches
  # nothing, so `$(grep -c … || echo 0)` yields the two-line string "0\n0" and the
  # `[ "$n" -gt 0 ]` below dies with "integer expression expected" — on every
  # deferred filter, i.e. exactly the rows this gate exists to classify.
  n=$(grep -c -- "$filter" "/tmp/56-filters/$pkg.txt" 2>/dev/null) || n=0
  # The deferral lookup is on the PAIR and is anchored at both ends of both
  # fields: `grep -Fx "$pkg|$filter"` on the row's first two columns. Unanchored,
  # `privacy` would excuse any future filter containing the word — a deferral
  # that never expires — and without the package it excuses the same filter in
  # a crate that will never define it.
  task=$(awk -F'|' -v p="$pkg" -v f="$filter" '$1==p && $2==f { print $3 }' \
           /tmp/56-filters/deferred.txt)
  if [ "$n" -gt 0 ]; then
    echo "OK      $pkg $filter ($n tests)"
  elif [ -n "$task" ]; then
    echo "DEFER   $pkg $filter (created by Task $task — see Step 3's table)"
  else
    echo "MISSING $pkg $filter — names no test in the listing and no task creates it"
  fi
done < /tmp/56-filters/wanted.txt > /tmp/56-filters/verdict.txt
sort /tmp/56-filters/verdict.txt
grep -c '^MISSING' /tmp/56-filters/verdict.txt ; echo "expect: 0"
grep -c '^DEFER'   /tmp/56-filters/verdict.txt ; echo "expect: 12 at this task; fewer at Task 20; 0 at Task 40"
grep -c '^OK'      /tmp/56-filters/verdict.txt ; echo "expect: 30 at this task (Step 3's first table)"
# Every deferred ROW is USED. The three counts above are satisfied by a deferred
# table with spare rows in it — an entry that excuses a filter nobody writes is
# a permanent hole, and it is how a wrong package sneaks back in.
while IFS='|' read -r pkg filter task _evidence; do
  grep -q "^DEFER   $pkg $filter " /tmp/56-filters/verdict.txt \
    || echo "UNUSED  $pkg $filter (Task $task) — deferred but no gate in the plan names it"
done < /tmp/56-filters/deferred.txt
echo "expect: no UNUSED lines"
echo "⚠ The COUNT is the gate, never the exit code — the same rule as every named"
echo "  cargo filter in this plan. And the loop reads from a file rather than a pipe"
echo "  precisely so a verdict cannot be lost in a subshell."
echo "A single MISSING line fails this gate. Do not 'fix' it by deleting the filter:"
echo "  either the module path is wrong (correct it here) or the task's tests do not"
echo "  exist yet (record a pre-count of 0 in that task, as Task 2 and Task 6 do)."
# (b) Every DEFERRED entry is something this plan actually creates. A deferred
#     entry nothing creates is a filter that stays green forever.
#
# ⚠ ALL TWELVE, from the same table, in a loop — not a hand-written list of the
#     six that happen to be new .rs files under `privacy/`. The six this used to
#     omit are exactly the six that are NOT a new file in that directory, and
#     they are the ones where "does the plan create it?" is a real question:
#     `privacy` and `privacy::tests` are test modules INSIDE a created file;
#     `providers::tier_tests` is a file in another directory;
#     `agents::chatrecall_extension` and `session::chat_history_search` are
#     `#[cfg(test)]` blocks added to files that ALREADY EXIST and today have
#     none; and `every_copy_path_carries_the_tier_and_the_provider` is a bare
#     test name with no file at all. Hence the fourth column: whatever proves
#     that row, whether a path or a `fn` name.
while IFS='|' read -r pkg filter task evidence; do
  n=$(grep -c -- "$evidence" "$PLAN") || n=0
  [ "$n" -gt 0 ] && echo "OK       $pkg $filter (Task $task) ← $evidence ($n)" \
                 || echo "UNBACKED $pkg $filter (Task $task) — nothing in this plan creates it"
done < /tmp/56-filters/deferred.txt
echo "expect: 12 OK lines, no UNBACKED. Measured today: privacy/mod.rs 14,"
echo "  providers/tier_tests.rs 3, chatrecall_extension.rs 7, chat_history_search.rs 8,"
echo "  the bare test name 1."
# (c) Re-runnable, and the phase gates re-run it: after Task 20 the DEFERRED set
#     must have lost knowledge::tier, privacy::extensions and privacy::refusal;
#     after Task 40 it must be EMPTY.
```

**What this catches.** The one defect class no amount of reading closes: a filter that names a module
by a nesting its file does not advertise, which libtest answers with `0 passed` and exit 0 — reported
by every phase gate in this plan as a pass. It also catches the inverse, which nobody has looked for:
a filter that resolves to *more* tests than the task believes, so a `pre + N` assertion is arithmetic
on the wrong base. Both are invisible to `grep`, and both are one `--list` away from being visible.

**This gate rejects: a `cargo test -p biorouter-mcp --lib privacy::refusal` anywhere in this plan.**
That command names no test in `biorouter-mcp`, will never name one — `privacy/refusal.rs` is created
in `biorouter` (File structure, Task 12) — and prints `0 passed; 0 failed` with exit 0 forever. Under
the name-only allowlist it was reported **DEFER**, because a filter spelled `privacy::refusal` is
expected in a *different* crate; it now fails as **MISSING**, because the deferral is keyed on the
`(package, filter)` pair. The same fix rejects the mirror — a real filter written against the wrong
package, e.g. `-p biorouter --lib knowledge::tier`, which is a `biorouter-mcp` module. And the two
new loops reject the two ways a table can lie about itself: a **deferred row nothing in the plan
names** (UNUSED — a standing excuse for a filter that will never be written), and a **deferred row
this plan never creates** (UNBACKED). Step 5(b) previously validated six of the twelve rows, and the
six it omitted were precisely the six that are not a new file under `privacy/` — the two `#[cfg(test)]`
blocks added to files that already exist with no tests at all, and one bare test name with no file.

⚠ **This task does not close the risk for the twelve deferred filters** — they name modules and one
test that do not exist yet and cannot be listed. It converts "unruled-out for all 42" into
"unruled-out for 12, each owned by a named task", which is a different order of risk: **30 of 42
pairs are now measured**, every "0 today" claim in this plan is confirmed, and two wrong pre-counts
were found and corrected. Task 20's Step 4b and Task 40's Step 2b re-run Step 5 with a shrunk and
then an **empty** `DEFERRED` set, which is what finishes the job.

- [ ] **Step 6: Commit**

```bash
git add docs/security/privacy-tiers-execution-plan.md
git commit -m "docs(privacy): resolve every test filter against a real cargo --list (#56)"
```

---

### Task 5: `Provider::tier()`, the private set, and the two demotion rules

**Files:**

| Action | Path | Anchor (re-verified at `9558c346`) |
|---|---|---|
| Modify | `crates/biorouter/src/providers/base.rs` | `ProviderMetadata` struct at `:146-165` (**eight** fields, ending `allows_unlisted_models`); the `Provider` trait |
| Modify | `crates/biorouter/src/providers/lead_worker.rs` | `get_name` at `:332-334` (returns the **lead's** name) |
| Modify | `crates/biorouter/src/providers/versa_azure.rs` | `VERSA_AZURE_ENDPOINT` at `:23` = `https://unified-api.ucsf.edu/general` |
| Modify | `crates/biorouter/src/providers/versa_bedrock.rs` | `VERSA_BEDROCK_DEFAULT_ENDPOINT` at `:55` = `https://unified-api.ucsf.edu/general/awsai` |
| Modify | `crates/biorouter/src/providers/ollama.rs` | `get_name` at `:163-165` returns the instance `name` field (`:37-43`); `from_env` reads `OLLAMA_HOST` at `:46-50`; `from_custom_config` at `:85-110` |
| Modify | `crates/biorouter/src/providers/llamacpp.rs` | `LLAMACPP_EXTERNAL_HOST` → `external_base` at `:296-297`, `:341-351` |
| Create | `crates/biorouter/src/providers/tier_tests.rs` | new; declared in `providers/mod.rs` as `#[cfg(test)] mod tier_tests;` — **not** `include!`, or the filter `providers::tier_tests` resolves to nothing and prints `0 passed` |
| Reference | `crates/biorouter/src/providers/azure.rs` | `AZURE_OPENAI_ENDPOINT` default is **the UCSF gateway** at `:204` — do not name-key |
| Reference | `crates/biorouter/src/providers/factory.rs` | `create` at `:139`; the `BIOROUTER_LEAD_MODEL` intercept at `:142-146`, **before** the registry lookup |
| Reference | `crates/biorouter/src/config/declarative_providers.rs` | ⚠ **`config/`, not `providers/`.** `register_declarative_provider` at `:285-313`, registering by `config.name` after the built-ins — so a user-authored JSON file named `versa_azure` **overwrites** the real registry entry (`config/provider_registry.rs:122` is a plain `self.entries.insert(config.name.clone(), ..)`). This is the bypass Step 1's third and fourth tests exist to close |

- [ ] **Step 1: Write the failing test**

```rust
// crates/biorouter/src/providers/tier_tests.rs
// Declared in providers/mod.rs as: #[cfg(test)] mod tier_tests;
#[test]
fn the_private_set_is_the_four_the_operator_named() {
    use crate::privacy::ProviderTier::{Private, Public};
    for name in ["versa_azure", "versa_bedrock", "llamacpp", "ollama"] {
        assert_eq!(tier_for_name_at_default_config(name), Private, "{name}");
    }
    // Everything hosted by an AI company or a large cloud is public — including
    // the ones whose names look institutional. azure.rs:204 ships the UCSF
    // gateway as AZURE_OPENAI_ENDPOINT's default, so a name-keyed rule would
    // call azure_openai Private; it must not.
    for name in ["anthropic", "openai", "azure_openai", "bedrock", "aws_bedrock",
                 "databricks", "vertex", "google", "groq", "unknown_provider"] {
        assert_eq!(tier_for_name_at_default_config(name), Public, "{name}");
    }
}

#[tokio::test]
async fn a_composite_takes_the_least_privileged_of_its_two_halves() {
    use crate::privacy::ProviderTier::{Private, Public};
    // get_name() on a composite returns the LEAD's name (lead_worker.rs:332),
    // so a name-keyed tier would badge private-lead/public-worker Private —
    // the exact inverse of R2.
    let lw = lead_worker_with_tiers(Private, Public);
    assert_eq!(lw.get_name(), "versa_azure");     // the lead's name
    assert_eq!(lw.tier(), Public);                // least(), not the name
    assert_eq!(lead_worker_with_tiers(Private, Private).tier(), Private);
    assert_eq!(lead_worker_with_tiers(Public, Public).tier(), Public);
}

#[test]
fn a_self_hosted_provider_pointed_off_the_machine_is_not_private() {
    use crate::privacy::ProviderTier::{Private, Public};
    // Open question 5 rates this ergonomics. It is a live bypass: config.yaml
    // is agent-writable (§9.3 C1 concedes SecretGuard cannot stop `shell`
    // writing it), and a declarative provider file whose engine is Ollama
    // yields an OllamaProvider with an arbitrary base_url. See the two
    // `declarative_providers` rows in this task's Files table for the anchors —
    // they are NOT repeated here, because a line number inside a code comment
    // is a citation no gate can check and no reviewer re-verifies.
    // Anyone who can write one JSON file would otherwise mint a Private-tier
    // provider pointing anywhere.
    assert_eq!(tier_for_self_hosted_base("http://localhost:11434"), Private);
    assert_eq!(tier_for_self_hosted_base("http://127.0.0.1:11434"), Private);
    assert_eq!(tier_for_self_hosted_base("http://[::1]:11434"), Private);
    assert_eq!(tier_for_self_hosted_base("http://gpu.lab.ucsf.edu:11434"), Public);
    assert_eq!(tier_for_self_hosted_base("https://api.example-saas.com"), Public);
}

#[test]
fn versa_demotes_when_its_endpoint_is_not_the_ucsf_gateway() {
    use crate::privacy::ProviderTier::{Private, Public};
    // versa_azure reads AZURE_OPENAI_ENDPOINT, the same key the public
    // azure_openai provider reads (versa_azure.rs:106-112 / azure.rs:115-118),
    // and versa_bedrock falls back to AWS_ENDPOINT_URL_BEDROCK_RUNTIME, which
    // bedrock.rs:92 sets PROCESS-GLOBALLY with std::env::set_var.
    assert_eq!(versa_tier_for_endpoint("https://unified-api.ucsf.edu/general"), Private);
    assert_eq!(versa_tier_for_endpoint("https://unified-api.ucsf.edu/general/awsai"), Private);
    assert_eq!(versa_tier_for_endpoint("https://evil.example.com/general"), Public);
}
```

- [ ] **Step 2: Run**

```bash
cargo test -p biorouter --lib providers::tier_tests
```

Expected: **COMPILE ERROR** — `no method named tier found for ... dyn Provider`.

- [ ] **Step 3: Implement**

In `providers/base.rs`, add the ninth `ProviderMetadata` field and the trait method:

```rust
    /// Whether models from this provider may be bound to a private session.
    /// Serialize + ToSchema, so it reaches every UI surface through
    /// `just generate-openapi` -> `npm run generate-api`.
    #[serde(default)]
    pub tier: crate::privacy::ProviderTier,
```

```rust
    /// The least-private component of what this **instance** actually resolved.
    ///
    /// An instance method, never a lookup on `get_name()`: `get_name()` on a
    /// composite returns the lead's name (`lead_worker.rs:332-334`), and
    /// `providers::create` can hand back something other than what was asked
    /// for (`factory.rs:142-146` intercepts `BIOROUTER_LEAD_MODEL` *before* the
    /// registry lookup, so `create("ollama", ..)` can return a composite whose
    /// lead is `anthropic`).
    ///
    /// DEFAULT = Public. Fail-safe: a provider module that forgets this gets
    /// less reach, never more — and a custom declarative provider that shadows
    /// a built-in name (see `crates/biorouter/src/config/declarative_providers.rs`,
    /// which registers by `config.name` after the built-ins, so a JSON file named
    /// `versa_azure` overwrites the real entry) loses a badge rather than forging
    /// one.
    fn tier(&self) -> crate::privacy::ProviderTier {
        crate::privacy::ProviderTier::Public
    }
```

Then **five** `impl Provider` overrides, each computed **at construction** from the resolved endpoint,
never from a name and never from a model id (`us.anthropic.claude-opus-4-8` appears in both
`BEDROCK_KNOWN_MODELS` and `VERSA_BEDROCK_KNOWN_MODELS`):

```rust
// providers/mod.rs — one shared host predicate, used by all four.

/// The compiled-in UCSF gateway host. `versa_azure` and `versa_bedrock` are
/// Private only while their resolved endpoint is on it.
pub(crate) const UCSF_GATEWAY_HOST: &str = "unified-api.ucsf.edu";

pub(crate) fn host_of(url: &str) -> Option<String> {
    url::Url::parse(url).ok()?.host_str().map(str::to_ascii_lowercase)
}

/// True only for a loopback host. R1 makes "self-hosted" private; a
/// non-loopback host is not evidence of self-hosting, and treating it as such
/// turns one writable config key into a forged private badge (Task 5 test 3).
pub(crate) fn is_loopback_host(url: &str) -> bool {
    match host_of(url).as_deref() {
        Some("localhost") | Some("127.0.0.1") | Some("::1") | Some("[::1]") => true,
        Some(h) => h.ends_with(".localhost"),
        None => false,
    }
}
```

```rust
// versa_azure.rs / versa_bedrock.rs
    fn tier(&self) -> ProviderTier {
        if crate::providers::host_of(&self.resolved_endpoint).as_deref()
            == Some(crate::providers::UCSF_GATEWAY_HOST)
        {
            ProviderTier::Private
        } else {
            // Demotion only, never promotion. versa_* shares all three
            // AZURE_OPENAI_* keys with the public azure_openai provider, and
            // bedrock.rs:92 sets AWS_ENDPOINT_URL_BEDROCK_RUNTIME process-globally.
            ProviderTier::Public
        }
    }

// ollama.rs / llamacpp.rs
    fn tier(&self) -> ProviderTier {
        if crate::providers::is_loopback_host(&self.effective_base_url) {
            ProviderTier::Private
        } else {
            ProviderTier::Public
        }
    }

// lead_worker.rs — the ONLY composite override.
    fn tier(&self) -> ProviderTier {
        ProviderTier::least(self.lead.tier(), self.worker.tier())
    }
```

Delete the two renderer `Set`s in `ui/desktop/src/components/settings/providers/providerOrdering.ts:4-5`
and switch `classifyProvider` (`:39-47`) onto the backend field, keeping `PRIORITY_ORDER` for
ordering. One list, one place, drift structurally impossible.

- [ ] **Step 4: Run**

```bash
cargo test -p biorouter --lib providers
just generate-openapi && (cd ui/desktop && npm run generate-api)
cd ui/desktop && npx tsc --noEmit
```

Expected: **PASS**. `openapi.json` gains `tier` on `ProviderMetadata`; commit the regeneration.

- [ ] **Step 5: Gate**

```bash
# The tier is never keyed on a name in the enforcement path. `-v '_test'` filters
# the PATH, and `tier_tests.rs` is the only file this task adds whose path
# carries it — a `mod tests` INSIDE a provider file is not excluded and must not
# name a private-provider list either.
grep -rn 'PRIVATE_PROVIDERS' crates/biorouter/src/providers/ | grep -v '_test' ; echo "expect: no output"
# The two renderer Sets are gone, and nothing reintroduces them. Match the CONST
# NAMES, not the first element: the sets are
# `const INSTITUTIONAL = new Set(['versa_azure', 'versa_bedrock']);` (:4) and
# `const LOCAL = new Set(['llamacpp', 'ollama']);` (:5), so a gate anchored on
# `new Set(['versa_azure'` scores 0 the moment someone reorders the members while
# leaving the hardcoded list fully intact. Measured today: 1 and 1.
grep -cE "^const (INSTITUTIONAL|LOCAL) = new Set\(" \
  ui/desktop/src/components/settings/providers/providerOrdering.ts ; echo "expect: 0 (2 today)"
grep -cE "new Set\(\[[^]]*'(versa_azure|versa_bedrock|llamacpp|ollama)'" \
  ui/desktop/src/components/settings/providers/providerOrdering.ts
echo "expect: 0 — any Set literal naming a provider, in any member order"
# ...and the grouping is derived from the tier the API now returns.
grep -c "\.tier" ui/desktop/src/components/settings/providers/providerOrdering.ts ; echo "expect: >= 1"
# ⚠ PRIORITY_ORDER (:7-19) also name-keys versa_azure/versa_bedrock/llamacpp/ollama
# and is DELIBERATELY left alone: it is display order WITHIN a group, not a tier,
# and deleting it silently reshuffles the provider grid. Do not "finish the job".
grep -c "PRIORITY_ORDER" ui/desktop/src/components/settings/providers/providerOrdering.ts
echo "expect: >= 2 — still defined and still used"
# Six tier() implementations. ENUMERATED, never counted: a bare `| wc -l` is
# satisfied by deleting versa_bedrock's override and adding one to anthropic.rs,
# which is precisely the direction that leaks. `diff` against the expected list
# makes the gate name the file that moved.
diff <(grep -rl "fn tier(&self)" crates/biorouter/src/providers/ | sort) <(cat <<'EOF'
crates/biorouter/src/providers/base.rs
crates/biorouter/src/providers/lead_worker.rs
crates/biorouter/src/providers/llamacpp.rs
crates/biorouter/src/providers/ollama.rs
crates/biorouter/src/providers/versa_azure.rs
crates/biorouter/src/providers/versa_bedrock.rs
EOF
) && echo "OK: exactly the six expected files"
# expect: no diff output, then "OK". base.rs is the trait DEFAULT (= Public);
# lead_worker is `least` of its two halves; llamacpp/ollama are loopback-only;
# the two versa_* are UCSF-gateway-host-only. Measured today: 0 files.
```

⚠ **`grep -rn "fn tier(&self)" … | wc -l ; expect: 6` was a counted gate**, and a count cannot
express what this task needs. `expect: 5` was wrong in the first version (the trait default in
`base.rs` matches too, so it is 6) and the fix round corrected the *number* without removing the
*shape*: 6 is still reachable by deleting `versa_bedrock`'s override and adding an override to
`anthropic.rs`, which is a leak that reads green. The `diff` above is the gate; there is no tripwire
count, because a tripwire that a wrong implementation satisfies is worse than none.

**What this catches.** Two wrong implementations at once. (1) A `PRIVATE_PROVIDERS: &[&str]` lookup
on `get_name()` — the obvious reading of "the list that already exists, moved from the renderer to
Rust" — which badges a private-lead/public-worker composite **Private**, inverting R2, and which no
single-provider test detects. The first grep and the composite test both fail it. (2) An
unconditional `Private` in `OllamaProvider::tier()`, the obvious reading of "self-hosted is
private", which lets anyone who can write one JSON file into
`~/.config/biorouter/custom_providers/` mint a Private-tier provider pointing at an arbitrary host
(`ProviderEngine::Ollama` + a remote `base_url` via `from_custom_config`, `ollama.rs:85-110`) and
bind it to a private session. Test 3 fails it; a test that only checks the default `OLLAMA_HOST`
passes it.

- [ ] **Step 6: Commit**

```bash
git add crates/biorouter/src/providers/ ui/desktop/src/components/settings/providers/providerOrdering.ts \
        ui/desktop/openapi.json ui/desktop/src/api
git commit -m "feat(providers): Provider::tier() computed from the resolved endpoint (#56)"
```

⚠ `crates/biorouter/src/providers/` has **no `mod tests` in `mod.rs`**, but twenty of its submodules
have one, so `cargo test -p biorouter --lib providers` in Step 4 does run something today. Record the
pre-count before Step 3 and assert `pre + 4` after; a `0 passed` from
`cargo test -p biorouter --lib providers::tier_tests` means the new file was `include!`d instead of
declared as a module.

---

### Task 6: The schema — `privacy_tier`, `privacy_reason`, `parent_session_id`, `classification_audit`

The load-bearing task of the plan. Everything downstream reads what it writes, and its one SQL
fragment is what makes the ratchet unreversible by any caller.

**Files:**

| Action | Path | Anchor (re-verified at `9558c346`) |
|---|---|---|
| Modify | `crates/biorouter/src/session/session_manager.rs` | `CURRENT_SCHEMA_VERSION` `:29` (**16**); `Session` struct `:118-162` (`diverged_from` `:151`, `branch_point_msg_uid` `:160`); `SessionUpdateBuilder` fields `:208-233`; `FromRow` reads `:1971-1977`; fresh-DB `CREATE TABLE sessions` `:2072` (`diverged_from TEXT,` `:2093`, last column `incarnation` `:2096` **no trailing comma**); `import_legacy_session` INSERT `:2244`/`:2264-2270`/bind `:2288`; `run_migrations` `:2325-2351` (reconcile calls `:2348-2349`); `reconcile_loop_schema` `:2354-2361`; `apply_migration` `:2487` (highest arm `16 =>` `:2727`, `_ =>` `:2732`); `table_has_column` `:2740`; `ensure_session_incarnation_schema` `:2782`; `create_session` INSERT `:2886-2921` (`RETURNING *` at `:2898-2913`); storage `get_session` `:2983` (projection `:2988-2991`); `add_update!` block `:3126-3132`; `list_sessions_by_types` `:4053` (projection `:4061-4066`); `list_session_summaries` `:4090-4113`; `SessionSummary` `:165-172` |
| Modify | `crates/biorouter/src/agents/knowledge_tool.rs` | test fixture, `diverged_from: None` at `:315` |
| Modify | `crates/biorouter/src/knowledge/conversation_ingest.rs` | test fixture, `diverged_from: None` at `:253` |

⚠ **The last two files are not optional.** `Session` is constructed as a struct literal at exactly
five sites — `grep -rn --include='*.rs' "diverged_from: None" crates/` returns
`session_manager.rs:859`, `:1804`, `:9109` and those two. Adding a field is `E0063` at all five and
`cargo test -p biorouter` will not build. `#[serde(default)]` governs deserialization, not
struct-literal construction.

⚠ **There are ten `CREATE TABLE sessions` statements in `session_manager.rs`.** One production DDL
at `:2072`; nine hand-rolled test fixtures at `:5642`, `:6255`, `:7633`, `:7755`, `:7909`, `:8107`,
`:10469`, `:10667`, `:10722`. Several are minimal (`id TEXT PRIMARY KEY, session_type TEXT`). The
fail-closed reader will make an unknown number of existing tests read Private. **Any migration gate
that asserts "the column exists" against a fixture DDL is meaningless** — Step 5's gate runs against
a DB built by the real migration path and additionally asserts a known-public row does not come back
defaulted.

- [ ] **Step 1: Write the failing tests — four, and each one catches a different wrong implementation**

```rust
#[tokio::test]
async fn a_fresh_database_defaults_every_session_public() {
    let temp = tempfile::TempDir::new().unwrap();
    let manager = SessionManager::new(temp.path().to_path_buf());
    let s = manager
        .create_session(temp.path().to_path_buf(), "s".into(), SessionType::User)
        .await
        .unwrap();
    assert_eq!(s.privacy_tier, SessionClassification::Public);
    assert_eq!(s.privacy_reason, None);
    assert_eq!(s.parent_session_id, None);
}

#[tokio::test]
async fn the_ratchet_raises_and_no_caller_can_lower_it() {
    let temp = tempfile::TempDir::new().unwrap();
    let manager = SessionManager::new(temp.path().to_path_buf());
    let s = manager
        .create_session(temp.path().to_path_buf(), "s".into(), SessionType::User)
        .await
        .unwrap();

    manager.update(&s.id)
        .raise_privacy(SessionClassification::Private, "turn:versa_azure")
        .apply().await.unwrap();
    assert_eq!(manager.get_session(&s.id, false).await.unwrap().privacy_tier,
               SessionClassification::Private);

    // The whole audit surface for "can the ratchet be reversed" is this
    // assertion plus one SQL fragment. The storage layer refuses, not the
    // caller — whatever it passes.
    manager.update(&s.id)
        .raise_privacy(SessionClassification::Public, "oops")
        .apply().await.unwrap();
    let reread = manager.get_session(&s.id, false).await.unwrap();
    assert_eq!(reread.privacy_tier, SessionClassification::Private,
               "a Public write must be a no-op on a private row");
    // The reason must not be rewritten by the refused write either, or the
    // provenance the declassify dialog grades on (§12.4) is destroyed.
    assert_eq!(reread.privacy_reason.as_deref(), Some("turn:versa_azure"));
}

#[tokio::test]
async fn every_projection_that_builds_a_session_reads_the_column() {
    // The fail-closed reader means a MISSED projection reads Private, so a
    // test that only checks a private row passes a broken projection. Seed a
    // known-PUBLIC row and assert each projection does not default it.
    let temp = tempfile::TempDir::new().unwrap();
    let manager = SessionManager::new(temp.path().to_path_buf());
    let s = manager
        .create_session(temp.path().to_path_buf(), "s".into(), SessionType::User)
        .await
        .unwrap();
    manager.add_message(&s.id, &user_message("hello")).await.unwrap();   // INNER JOIN messages

    assert_eq!(manager.get_session(&s.id, false).await.unwrap().privacy_tier,
               SessionClassification::Public, "get_session");
    let listed = manager.list_sessions_by_types(&[SessionType::User]).await.unwrap();
    assert_eq!(listed.iter().find(|x| x.id == s.id).unwrap().privacy_tier,
               SessionClassification::Public, "list_sessions_by_types");
    let summaries = manager.list_session_summaries(50, 0).await.unwrap();
    assert_eq!(summaries.iter().find(|x| x.id == s.id).unwrap().privacy_tier,
               SessionClassification::Public, "list_session_summaries");
}

#[tokio::test]
async fn the_reconcile_adds_the_columns_even_when_the_version_says_it_already_ran() {
    // O10. BR-71's branch already ships CURRENT_SCHEMA_VERSION = 17 with its
    // own `17 =>` arm. A database that ran that build has schema_version = 17
    // and would SKIP a numbered-arm-only implementation of this task entirely.
    let temp = tempfile::TempDir::new().unwrap();
    let db = temp.path().join("sessions.db");
    build_v16_database(&db).await;                       // real ladder, stops at 16
    force_schema_version(&db, 17).await;                 // pretend the other branch ran
    assert!(!column_exists(&db, "sessions", "privacy_tier").await);

    let _manager = SessionManager::new(temp.path().to_path_buf());   // opens + reconciles
    assert!(column_exists(&db, "sessions", "privacy_tier").await);
    assert!(column_exists(&db, "sessions", "privacy_reason").await);
    assert!(column_exists(&db, "sessions", "parent_session_id").await);
    assert!(table_exists(&db, "classification_audit").await);
}
```

- [ ] **Step 2: Run**

```bash
cargo test -p biorouter --lib -- \
  a_fresh_database_defaults_every_session_public \
  the_ratchet_raises_and_no_caller_can_lower_it \
  every_projection_that_builds_a_session_reads_the_column \
  the_reconcile_adds_the_columns_even_when_the_version_says_it_already_ran
```

⚠ Note the `--`: `cargo test --lib A B` is a hard error (`unexpected argument 'B' found`), not two
filters. Multiple filters go after `--`, where libtest ORs them.

⚠ And note the **absence** of a module positional. `cargo test --lib session::session_manager -- a b c`
does not run "these tests inside that module" — cargo hands its own `TESTNAME` positional to libtest
as *another* OR'd filter, so the whole `session::session_manager` module runs and the four names add
nothing. This step wants exactly four tests, so it passes only names.

Expected: **COMPILE ERROR** — `no field privacy_tier on Session`, `no method raise_privacy on
SessionUpdateBuilder`.

- [ ] **Step 3: Implement, in this order**

(a) **`CURRENT_SCHEMA_VERSION` 16 → 17** (`:29`).

(b) **`Session` struct** (after `branch_point_msg_uid`, `:160`):

```rust
    /// How sensitive this session's contents are (issue #56). A permanent
    /// ratchet: the storage layer's `CASE WHEN` refuses to lower it, and
    /// `privacy::declassify` is the only writer in the tree permitted to.
    #[serde(default = "SessionClassification::public")]
    pub privacy_tier: SessionClassification,
    /// Audit and UX only — never read by a gate. One of `turn:<provider>`,
    /// `mcp:<extension>`, `inherited:<parent_id>`, `diverged:<parent_id>`,
    /// `backfill:<provider>`, `declassified_by_user`. §12.4 grades the
    /// declassification confirmation on whether it has ever been `mcp:*`.
    #[serde(default)]
    pub privacy_reason: Option<String>,
    /// Id of the session that spawned this one as a subagent. Sibling of
    /// `diverged_from` (a user fork); this records a delegation, and it is what
    /// the §7 capability matrix's `L` axis reads. Co-landing note: BR-71 Task 1
    /// adds an identical column; see the reconcile helper below.
    #[serde(default)]
    pub parent_session_id: Option<String>,
```

(c) **Fresh-DB DDL** (`:2072`) — insert after `diverged_from TEXT,` (`:2093`), **not** at the end
(`incarnation` at `:2096` is last and carries no trailing comma):

```sql
                privacy_tier TEXT NOT NULL DEFAULT 'public',
                privacy_reason TEXT,
                parent_session_id TEXT,
```

(d) **The shape-guarded numbered arm** (after the `16 =>` arm ending at `:2731`):

```rust
            17 => {
                // Shape-guarded, unlike every arm above it, because BR-71's
                // branch already ships a `17 =>` that adds parent_session_id.
                // A raw ALTER on a column the other branch's build already
                // created is `duplicate column name`, which aborts startup.
                Self::ensure_privacy_schema(pool).await?;
            }
```

(e) **The unconditional reconcile.** Add the call to `reconcile_loop_schema` (`:2354-2361`), after
`ensure_session_incarnation_schema`:

```rust
        Self::ensure_privacy_schema(pool).await?;
```

and the helper beside `ensure_session_incarnation_schema` (`:2782`):

```rust
    /// Issue #56's columns, added idempotently and **version-independently**.
    ///
    /// The precedent is `ensure_session_incarnation_schema`, and the reason is
    /// the same one `run_migrations` records at :2344-2348: development builds
    /// have shipped overlapping migration numbers before. Here it is not
    /// hypothetical — `feat/br71-workspace-control` at `ea15a4de` already has
    /// `CURRENT_SCHEMA_VERSION = 17` and its own `17 =>` arm adding
    /// `parent_session_id`. With this helper the arm number stops being
    /// load-bearing and either merge order is safe.
    ///
    /// No backfill lives here. The migration backfill (Task 38) runs ONCE from
    /// the numbered arm, because a startup-repeating `WHERE provider_name IN
    /// (..)` would re-privatise a session the user has just declassified —
    /// `declassify_session` deliberately leaves `provider_name` untouched.
    async fn ensure_privacy_schema(pool: &Pool<Sqlite>) -> Result<()> {
        if !Self::table_has_column(pool, "sessions", "privacy_tier").await? {
            sqlx::query(
                "ALTER TABLE sessions ADD COLUMN privacy_tier TEXT NOT NULL DEFAULT 'public'",
            )
            .execute(pool)
            .await?;
        }
        if !Self::table_has_column(pool, "sessions", "privacy_reason").await? {
            sqlx::query("ALTER TABLE sessions ADD COLUMN privacy_reason TEXT")
                .execute(pool)
                .await?;
        }
        if !Self::table_has_column(pool, "sessions", "parent_session_id").await? {
            sqlx::query("ALTER TABLE sessions ADD COLUMN parent_session_id TEXT")
                .execute(pool)
                .await?;
        }
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS classification_audit (
              id                      INTEGER PRIMARY KEY AUTOINCREMENT,
              session_id              TEXT NOT NULL,
              from_classification     TEXT NOT NULL,
              to_classification       TEXT NOT NULL,
              reason                  TEXT NOT NULL,
              actor                   TEXT NOT NULL,
              actor_kind              TEXT NOT NULL,
              occurred_at             TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
              app_version             TEXT NOT NULL,
              provider_name_at_change TEXT,
              privacy_reason_before   TEXT,
              message_count_at_change INTEGER
            )
        "#,
        )
        .execute(pool)
        .await?;
        Ok(())
    }
```

(f) **The `FromRow` reads** (`:1971-1977`), and the comment above `branch_point_msg_uid` gains one
sentence so the next reader does not "fix" the new column back to tolerant:

```rust
            diverged_from: row.try_get("diverged_from").ok().flatten(),
            // Tolerant read: SELECTs that omit the column (e.g. the session
            // list) yield None rather than erroring, mirroring `model_config`.
            // The privacy columns below deliberately do NOT follow this
            // convention — see `SessionClassification::from_stored`.
            branch_point_msg_uid: row.try_get("branch_point_msg_uid").ok().flatten(),
            privacy_tier: row
                .try_get::<String, _>("privacy_tier")
                .map(|s| SessionClassification::from_stored(&s))
                .unwrap_or_else(|_| {
                    tracing::error!("privacy_tier missing from projection; reading Private");
                    SessionClassification::Private
                }),
            privacy_reason: row.try_get("privacy_reason").ok().flatten(),
            parent_session_id: row.try_get("parent_session_id").ok().flatten(),
```

and add `privacy_tier: SessionClassification::Public, privacy_reason: None, parent_session_id: None,`
to all five struct-literal sites (`:859`, `:1804`, `:9109`, `knowledge_tool.rs:315`,
`conversation_ingest.rs:253`).

(g) **The builder.** Field, setter, emission — and the emission is the load-bearing line:

```rust
    /// Raise-only. There is deliberately NO setter that accepts an arbitrary
    /// value, and the SQL refuses a lowering write even if one appeared.
    privacy_raise: Option<(SessionClassification, String)>,
```

```rust
    /// Raise the classification and record why. Monotone: passing `Public` to a
    /// row that is already `private` is a no-op in SQL, so no caller — a route
    /// handler, a CLI command, a test, a future BR-71 tool, a hand-written
    /// query through this builder — can lower the tier.
    pub fn raise_privacy(mut self, to: SessionClassification, reason: &str) -> Self {
        self.privacy_raise = Some((to, reason.to_string()));
        self
    }
```

In `apply_update`'s dynamic SET construction, immediately after the `add_update!` block
(`:3126-3132`), and modelled on the `COALESCE` accumulation eight lines above it (`:3104-3124`):

```rust
        // THE load-bearing line of issue #56. Emitted as a CASE so the storage
        // layer, not the caller, is what refuses a downgrade. Concurrency is
        // safe in both orderings. `privacy_reason` is guarded by the same
        // predicate, so a refused raise cannot rewrite the provenance the
        // declassification dialog grades on (§12.4).
        if builder.privacy_raise.is_some() {
            if !updates.is_empty() {
                query.push_str(", ");
            }
            updates.push("privacy_tier");
            query.push_str(
                "privacy_tier = CASE WHEN privacy_tier = 'private' THEN 'private' ELSE ? END, \
                 privacy_reason = CASE WHEN privacy_tier = 'private' THEN privacy_reason ELSE ? END",
            );
        }
```

with the two binds pushed in the same relative position in the bind sequence.

(h) **The three projections.**

- `get_session` (`:2983`), projection line `:2991`: append
  `, privacy_tier, privacy_reason, parent_session_id`.
- `list_sessions_by_types` (`:4053`), projection `:4065`: append
  `s.privacy_tier, s.privacy_reason, s.parent_session_id,` before `COUNT(m.id)`.
- `list_session_summaries` (`:4090`) and `SessionSummary` (`:165-172`): add `privacy_tier` to both.
  This is the third projection the design does not name; it backs `GET /sessions/sidebar`
  (`routes/session.rs:269-288`) → `useSidebarSessions` → `RecentChats.tsx`, and without it the
  sidebar recent-chats list cannot badge without an N+1.

`create_session`'s INSERT (`:2886-2921`) is an `INSERT ... RETURNING *` (`:2898-2913`) and needs
**no** projection edit — but confirm the fail-closed reader does not `error!` on it.
`import_legacy_session` (`:2244`, column list `:2264-2270`, binds from `:2288`) needs the three
columns added to its list, its `?` placeholders, and its binds.

- [ ] **Step 4: Run**

```bash
cargo test -p biorouter --lib session::session_manager
cargo test -p biorouter --lib -- agents::knowledge_tool knowledge::conversation_ingest
```

Expected: **PASS**. Record the `session::session_manager` count before Step 3; the post-task count
must be exactly `pre + 4`. If `cargo test -p biorouter` fails to **build**, the cause is almost
certainly one of the two out-of-file `Session` literals, not this file.

- [ ] **Step 5: Gate**

```bash
# The ratchet is one SQL fragment, and there is no other way to write the column.
grep -c "privacy_tier = CASE WHEN" crates/biorouter/src/session/session_manager.rs ; echo "expect: 1"
# No builder setter accepts an arbitrary classification.
grep -c "fn privacy_tier(mut self" crates/biorouter/src/session/session_manager.rs ; echo "expect: 0"
# Exactly one statement in the whole tree may lower it, and it does not exist yet
# (Task 29 adds it). This is the entire audit surface for "can it be reversed".
grep -rn --include='*.rs' "privacy_tier *= *'public'" crates/ | grep -v "DEFAULT 'public'" ; echo "expect: no output until Task 29, then exactly 1 (declassify.rs)"
# All three projections name the column.
grep -c "privacy_tier" crates/biorouter/src/session/session_manager.rs
# expect: >= 12 — but do NOT rely on this number; the behavioural test in Step 1
# (`every_projection_that_builds_a_session_reads_the_column`) is the real gate,
# because a comment satisfies a count and a seeded PUBLIC row does not.
```

**What this catches.** Three wrong implementations. (1) A plain `add_update!(builder.privacy_tier,
"privacy_tier")` — the shape every other column uses — which lets any caller write `'public'`; the
second test in Step 1 fails it, and the first grep returns 0. (2) A numbered-arm-only migration,
which a database that already ran BR-71's build silently skips; the fourth test fails it and a
`grep "17 =>"` would not. (3) A missed projection — the failure mode the design names and
`branch_point_msg_uid`'s absence from `list_sessions_by_types` already demonstrates in this file —
which compiles, reads Private through the fail-closed path, and is caught only by seeding a
known-**public** row. A test that asserts a private row comes back private passes all three.

- [ ] **Step 6: Commit**

```bash
git add crates/biorouter/src/session/session_manager.rs \
        crates/biorouter/src/agents/knowledge_tool.rs \
        crates/biorouter/src/knowledge/conversation_ingest.rs
git commit -m "feat(session): privacy_tier, privacy_reason, parent_session_id and the classification audit (#56)"
```

---

### Task 7: The `floor` caller audit and the `capability >= classification` property test

Small, and it is what keeps the two lattices from quietly merging over the next thirty tasks.

**Files:**

| Action | Path | Anchor |
|---|---|---|
| Modify | `crates/biorouter/src/privacy/mod.rs` | `floor` (Task 4) |

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn floor_is_crossed_only_where_a_capability_establishes_a_classification() {
    // The two lattices are independent by construction; `floor` is the only
    // crossing. This test is what keeps that true over the next thirty tasks.
    //
    // It asserts the exact (file, count) SET, not a total. Three reasons, each
    // learned from the first version of this test, which could not pass at any
    // point in this plan:
    //
    //  (a) A total is defeated by an UNRELATED symbol. The first version
    //      matched `line.contains("floor(")`, which already matches
    //      `session_manager.rs:688`'s `let lo = pos.floor() as usize;` — an f64
    //      method with nothing to do with this module. Its "baseline 0" was
    //      really 1 before a single line of #56 existed. Matching only the
    //      QUALIFIED path `privacy::floor(` cannot see a float method, ever.
    //  (b) A total is defeated by the plan's OWN later additions, silently: a
    //      task that adds two crossings where the number says one still passes
    //      if another task removed one. A set names the file.
    //  (c) The failure message has to be actionable. `assert_eq!` on a Vec of
    //      (file, count) prints exactly which file changed.
    //
    // The qualified-path rule needs a second assertion to hold: nobody may
    // `use` the bare name, or rule (a) is evaded by an import.
    //
    // EXPECTED grows twice, and each growth is one uncommented line in the diff
    // that causes it — Task 13 (Gate B's ratchet) and Task 23 (the spawn stamp).
    // A test written to accept `<= 2` would let a third crossing appear
    // unnoticed, which is the entire point of having this test.
    const EXPECTED: &[(&str, usize)] = &[
        // ("crates/biorouter/src/agents/agent.rs", 1),          // uncomment in Task 13
        // ("crates/biorouter/src/agents/subagent_tool.rs", 1),  // uncomment in Task 23
    ];

    // CARGO_MANIFEST_DIR is <workspace>/crates/biorouter; go up twice.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let mut calls: std::collections::BTreeMap<String, usize> = Default::default();
    let mut imports: Vec<String> = vec![];
    for entry in walkdir::WalkDir::new(root.join("crates"))
        .into_iter()
        .filter_map(Result::ok)
    {
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let rel = p
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if rel.ends_with("privacy/mod.rs") {
            continue; // the definition, and the induction test below, live here
        }
        let src = std::fs::read_to_string(p).unwrap_or_default();
        for (i, line) in src.lines().enumerate() {
            let code = line.trim_start();
            if code.starts_with("//") {
                continue;
            }
            if code.contains("privacy::floor(") {
                *calls.entry(rel.clone()).or_default() += 1;
            }
            if code.contains("use crate::privacy::floor")
                || (code.contains("privacy::{") && code.contains("floor"))
            {
                imports.push(format!("{rel}:{}", i + 1));
            }
        }
    }
    assert!(
        imports.is_empty(),
        "`floor` must be called through its qualified `privacy::floor(..)` path so this \
         audit can see it — do not import the bare name: {imports:#?}"
    );
    let found: Vec<(String, usize)> = calls.into_iter().collect();
    let want: Vec<(String, usize)> = EXPECTED
        .iter()
        .map(|(f, n)| ((*f).to_string(), *n))
        .collect();
    assert_eq!(
        found, want,
        "the set of `floor` callers changed. A new crossing between the capability and \
         classification lattices is a design change, not a refactor: argue it in the PR."
    );
}

#[test]
fn capability_never_falls_below_classification_for_any_legal_sequence() {
    use ProviderTier::{Private, Public};
    // The design's induction (§4), made executable. Every legal bind followed
    // by its ratchet must preserve capability >= classification.
    for binds in all_sequences_of_length(6, &[Private, Public]) {
        let mut classification = SessionClassification::Public;
        let mut capability = Public;
        for incoming in binds {
            if !bind_allowed(incoming, classification) {
                continue;                          // Gate A refuses; state unchanged
            }
            capability = incoming;                 // the bind succeeds
            classification = classification.max(floor(capability));   // Gate B ratchets
            assert!(
                visible_to(capability, classification),
                "capability {capability:?} fell below classification {classification:?}"
            );
        }
    }
}
```

- [ ] **Step 2: Run** → **COMPILE ERROR** if `walkdir` is not yet a dev-dependency; otherwise
      **PASS** with an empty `EXPECTED`, which is the correct state at this task. Confirm the
      emptiness is *earned* rather than accidental by temporarily writing
      `let _ = crate::privacy::floor(ProviderTier::Public);` into `agents/agent.rs`, re-running
      (it must **FAIL**, naming `crates/biorouter/src/agents/agent.rs`), then reverting. Do this
      once, here; it is the only thing that proves the matcher works before there is anything to
      match, and it takes thirty seconds.

⚠ **The `EXPECTED` set is a moving target by design**, and the moves are owned by later tasks: `[]`
here, `+agent.rs` in Task 13 Step 3 (Gate B's ratchet), `+subagent_tool.rs` in Task 23 Step 3 (the
spawn stamp). Each task uncomments exactly one line of `EXPECTED` **in the same commit that adds the
crossing**, so the increment is visible in the diff that causes it. Tasks 16 and 23 deliberately do
**not** add crossings: comparing a caller's capability with an *extension's* tier is a
`ProviderTier`↔`ProviderTier` question, and it goes through `privacy::refusal::privacy_refusal(..)`,
not through `floor`.

- [ ] **Step 3: Implement** — no production code. Add `walkdir` to `[dev-dependencies]` if absent.

- [ ] **Step 4: Run** → **PASS**.

- [ ] **Step 5: Gate**

```bash
cargo test -p biorouter --lib privacy:: 2>&1 | tail -3
# Expected: "test result: ok. 5 passed" — the count is the gate. A filter that
# names a nested module by the wrong path prints "0 passed" and EXITS 0.
#
# The 5 is derived, not guessed: Task 4 Step 1 wrote two tests and Task 4 Step 5
# added a third to the same `mod tests`; this task adds two more. Task 8 then
# creates `privacy::extensions`, which this same filter also matches, so from
# Task 8 onward quote a per-module count instead of this one.
```

**What this catches.** The refactor that adds a `From<ProviderTier> for SessionClassification` and
sprinkles `.into()` at four sites — which compiles, passes every gate test, and reintroduces exactly
the confusion the two-type split exists to prevent. The caller-set test fails it and names the files;
no grep in a later task would. It also catches the subtler version of the same mistake: reaching for
`floor` to answer a question that is not "what classification does this capability establish?" —
which is how the first version of this plan ended up with four crossings where it claimed two.

- [ ] **Step 6: Commit**

```bash
git add crates/biorouter/src/privacy/mod.rs crates/biorouter/Cargo.toml
git commit -m "test(privacy): pin the floor caller count and the capability>=classification induction (#56)"
```

---

### Task 8: `classify_extension` and the generated private set

**Files:**

| Action | Path | Anchor (re-verified at `9558c346`) |
|---|---|---|
| Create | `crates/biorouter/src/privacy/registry_private.rs` | `@generated` — Task 33 makes `build-registry.mjs` emit it; hand-write the initial content here and let Task 33's `--check` prove they agree |
| Create | `crates/biorouter/src/privacy/extensions.rs` | new |
| Modify | `crates/biorouter/src/agents/extension_manager.rs` | `struct Extension` `:56-72` (six fields); `add_extension` `:674` (early `contains_key -> Ok(())` at `:678`); `add_client` `:879`; `add_inprocess_server` `:901`; `Frontend` refusal `:833-836` |
| Reference | `landing/registry.json` | version 1, 37 extensions, 129 skills; keys `description download filename github id license name organization tags version`; `spokeagent-0.4.1` is the **only** version-suffixed id |
| Reference | `crates/biorouter-mcp/src/lib.rs` | `BUILTIN_EXTENSIONS` at `:96` (7 entries) |
| Reference | `crates/biorouter/src/agents/extension.rs` | `PLATFORM_EXTENSIONS` at `:43` (5 entries, asserted `len() == 5` at `:677`) |

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn the_private_set_is_exactly_the_two_the_registry_publishes() {
    use crate::privacy::ProviderTier::{Private, Public};
    assert_eq!(classify_extension("ucsfomopagent"), Private);
    assert_eq!(classify_extension("cdwagent"), Private);

    // R11(ii): anything not on BAAM is PUBLIC. Fail-open, by operator ruling.
    // `medcp` is enabled on the operator's own machine with CLINICAL_RECORDS_*
    // against a clinical MSSQL backend and stays fully callable — the badge is
    // a statement about provenance, not about the data behind the connector.
    for name in ["medcp", "msbaseagent", "spokeagent", "spokeagent-0.4.1",
                 "developer", "memory", "knowledge", "autovisualiser",
                 "computercontroller", "tutorial", "agent_drafter",
                 "todo", "chatrecall", "extensionmanager", "skills", "code_execution",
                 "appcontrol", "datasql", "files", "compute", "evidence",
                 "something-nobody-has-published"] {
        assert_eq!(classify_extension(name), Public, "{name}");
    }
}
```

⚠ **`developer` and `computercontroller` stay Public, and that is not the plan overlooking the
shell.** They are built-ins, R11 makes built-ins public, and reclassifying them Private would ban
the shell from every public chat — which is a far larger change than the one the threat needs. The
answer to "a public model can read `sessions.db` with `developer__shell`" is **DR-14**, a read-deny
sandbox on the *tools* (Tasks 14A–14C), not a tier on the *extension*. Gate C keeps letting a public
caller reach `developer`; the sandbox is what makes reaching it harmless. Do not "fix" this list.

```rust
#[test]
fn classification_is_case_and_whitespace_insensitive_the_way_the_key_is() {
    // `name_to_key` (config/extensions.rs:23) strips whitespace and lowercases,
    // then `normalize()` (extension_manager.rs:159) preserves `_`. The tier
    // must be resolved on the SAME key the manager stores, or a config entry
    // named "UCSFOMOPAgent" installs Private under one rule and Public under
    // the other.
    assert_eq!(classify_extension("UCSFOMOPAgent"), ProviderTier::Private);
    assert_eq!(classify_extension(" ucsfomopagent "), ProviderTier::Private);
}

#[tokio::test]
async fn all_three_admission_points_stamp_the_tier() {
    // add_extension, add_client and add_inprocess_server. One test per point:
    // an extension admitted through ANY of them comes back with the right tier,
    // because `Extension.tier` is what Gates C and E read.
    for admitted in [admit_via_add_extension("ucsfomopagent").await,
                     admit_via_add_client("ucsfomopagent").await,
                     admit_via_add_inprocess_server("ucsfomopagent").await] {
        assert_eq!(admitted, ProviderTier::Private);
    }
    assert_eq!(admit_via_add_inprocess_server("appcontrol").await, ProviderTier::Public);
}
```

- [ ] **Step 2: Run** → **COMPILE ERROR** (`cannot find function classify_extension`).

- [ ] **Step 3: Implement**

```rust
// crates/biorouter/src/privacy/registry_private.rs
//! @generated by `landing/scripts/build-registry.mjs` — do not edit.
//!
//! Regenerate with `node landing/scripts/build-registry.mjs`; verify with
//! `just check-privacy-registry`, which is wired into `just check-everything`.
//! Rust has no network path to the registry (the only fetch is Electron's
//! `main.ts:2832`), so without this file the CLI and the daemon can enforce
//! nothing.

/// Extension names (the bundle's `manifest.name`, NOT the registry `id`) that
/// `landing/registry.json` marks `"privacy": "private"`.
pub const PRIVATE_EXTENSIONS: &[&str] = &["cdwagent", "ucsfomopagent"];
```

```rust
// crates/biorouter/src/privacy/extensions.rs
use super::ProviderTier;

/// The single function implementing R11, both halves.
///
/// (i) **Nothing local can grant private.** The tier is resolved from the
///     generated registry set and a persisted last-good fetch, never from
///     `config.yaml` and never from the `.brxt` bundle — which self-declares
///     nothing the resolver reads, and whose install records no provenance at
///     all (`BrxtInstallModal.tsx:152-161` writes name/cmd/args/envs and no
///     registry id, source URL or hash).
/// (ii) **Anything not on BAAM is PUBLIC.** Fail-open, operator ruling. This is
///     the opposite fail direction from `Provider::tier`'s default and the
///     asymmetry is deliberate: an unknown model is a place data might *go*
///     (restrict it); an unknown extension is a place data might *come from*.
///
/// Freshness raises, never lowers:
///   `private_set = PRIVATE_EXTENSIONS ∪ private(last_good_fetch)`
/// An offline laptop can fail to *learn* a new private badge; it can never
/// *lose* one.
///
/// Reversing ruling (ii) later is a one-line change here, by design.
pub fn classify_extension(name: &str) -> ProviderTier {
    let key = crate::config::extensions::name_to_key(name);
    if super::registry_private::PRIVATE_EXTENSIONS.contains(&key.as_str())
        || last_good_fetch_private_set().contains(&key)
    {
        ProviderTier::Private
    } else {
        ProviderTier::Public
    }
}
```

On `struct Extension` (`extension_manager.rs:56-72`), a seventh field:

```rust
    /// Issue #56. Stamped once at admission from `classify_extension`.
    ///
    /// On the RECORD, never on `ExtensionConfig`: the config round-trips
    /// through user-writable `config.yaml`, which would make the badge locally
    /// forgeable and contradict R11(i); a new config field costs seven match
    /// arms plus an OpenAPI cycle; and `pool_key` (`extension.rs:483`) carries
    /// no session id, so one `ucsfomopagent` child process is shared across
    /// sessions and the badge cannot live on the process.
    tier: crate::privacy::ProviderTier,
```

stamped in all three admission points. `add_extension`'s early `contains_key -> Ok(())` at `:678`
means a re-add never restamps, which is correct: the tier is a pure function of the name.
`add_client`'s only caller is `biorouter-cli/src/scenario_tests/scenario_runner.rs:215` — a compiled
test harness, not a production path; stamp it for completeness and do not describe it otherwise.

- [ ] **Step 4: Run**

```bash
cargo test -p biorouter --lib privacy::extensions
cargo test -p biorouter --lib agents::extension_manager
```

- [ ] **Step 5: Gate**

```bash
# The badge is on the record, not the config: ExtensionConfig gained no field,
# so the OpenAPI schema for it is byte-identical.
just generate-openapi && git diff --exit-code ui/desktop/openapi.json ; echo "expect: exit 0"
# Exactly one function decides an extension's tier. ⚠ `grep -v _test` filters
# the PATH and excludes nothing that matters — this repo's Rust tests live in
# `#[cfg(test)] mod` blocks INSIDE the file they test, so a legitimate assertion
# in privacy/extensions.rs's own test module would be a second hit and read red.
# PRINT the hits and require the CONSUMER to be one function instead.
grep -rn --include='*.rs' "PRIVATE_EXTENSIONS" crates/ | grep -v "registry_private.rs"
echo "expect: all hits in crates/biorouter/src/privacy/extensions.rs and nowhere else."
awk '/pub fn classify_extension/,/^}/' crates/biorouter/src/privacy/extensions.rs \
  | grep -c "PRIVATE_EXTENSIONS" ; echo "expect: 1 — the one consumer, in the one function"
awk '/pub fn classify_extension/,/^}/' crates/biorouter/src/privacy/extensions.rs | wc -l
echo "expect: > 1 — a zero here means the fn is named something else and the"
echo "  count above is a vacuous pass over an empty awk range"
# The three admission points each stamp it, one apiece — a bare 3 is also
# satisfied by three calls in add_extension and none in the other two.
# Note `[(<]`, and the braces around the variable: `add_inprocess_server` is
# GENERIC (`pub async fn add_inprocess_server<S>(…)`, :901), so a pattern ending
# in `\(` never matches it and the range is empty — a silent 0 that reads as a
# pass. Measured spans with this pattern: 204, 14, 57.
for fn in add_extension add_client add_inprocess_server; do
  echo -n "$fn: "
  awk "/pub async fn ${fn}[(<]/,/^    }/" crates/biorouter/src/agents/extension_manager.rs \
    | grep -c "classify_extension("
done ; echo "expect: 1 each"
grep -c "classify_extension(" crates/biorouter/src/agents/extension_manager.rs ; echo "expect: 3"
```

**What this catches.** The wrong implementation puts `privacy: Option<String>` on `ExtensionConfig`
so the tier can be "configured" — which makes the badge forgeable from `config.yaml` (agent-writable,
per §9.3 C1) and contradicts R11(i). The OpenAPI diff catches it in one command, and no unit test
would. The second grep catches the other wrong implementation: a second `PRIVATE_EXTENSIONS.contains`
inlined at a call site, which is how the const stops being the single source when the reversal in
ruling (ii) is eventually asked for.

- [ ] **Step 6: Commit**

```bash
git add crates/biorouter/src/privacy/ crates/biorouter/src/agents/extension_manager.rs
git commit -m "feat(privacy): classify_extension from the generated registry set, stamped at admission (#56)"
```

---

### Task 9: Phase 1 gate

- [ ] **Step 1: Full suite + lints + OpenAPI**

```bash
cargo test --workspace --no-fail-fast 2>&1 | tail -20
cargo fmt --check && ./scripts/clippy-lint.sh
just generate-openapi && git diff --exit-code ui/desktop/openapi.json
cd ui/desktop && npx tsc --noEmit && npm run lint:check
```

- [ ] **Step 2: The migration is safe in both merge orders (the O10 proof)**

```bash
# Both directions, against real databases built by the real ladder.
# ⚠ Assert the count: two OR'd names that BOTH resolve to nothing print
# `0 passed` and exit 0, which reads exactly like success.
cargo test -p biorouter --lib -- \
  the_reconcile_adds_the_columns_even_when_the_version_says_it_already_ran \
  a_fresh_database_defaults_every_session_public \
  | grep "test result:" ; echo "expect: 2 passed; 0 failed"
# Then, by hand, against a copy of the operator's live DB:
cp ~/.local/share/biorouter/sessions/sessions.db /tmp/p1-check.db
sqlite3 /tmp/p1-check.db "select max(version) from schema_version;"   # expect 16
BIOROUTER_SESSIONS_DB=/tmp/p1-check.db cargo run -p biorouter-cli -- sessions list >/dev/null
sqlite3 /tmp/p1-check.db "select count(*) from pragma_table_info('sessions') where name in ('privacy_tier','privacy_reason','parent_session_id');"
# expect: 3
sqlite3 /tmp/p1-check.db "select count(*) from sessions where privacy_tier='private';"
# expect: 0 — Phase 1 adds the column, Task 38 adds the backfill. A non-zero
# here means a backfill leaked into the reconcile helper, which would
# re-privatise declassified sessions on every launch.
```

- [ ] **Step 3: Nothing yet enforces anything**

```bash
# Phase 1 is inert by construction. If any of these is non-zero, a gate landed
# early and the O1/O3/O4 orderings were broken.
grep -rn --include='*.rs' "bind_allowed(" crates/ | grep -v "privacy/mod.rs" ; echo "expect: no output"
grep -rn --include='*.rs' "visible_to(" crates/ | grep -v "privacy/mod.rs" ; echo "expect: no output"
```

- [ ] **Step 4: Commit the gate record in the PR description. No code.**

---

# Phase 2 — the gates

Fifteen tasks — eleven numbered plus **10A, 10B, 10C and 10D**. The design names five gates; this
phase ships **ten**, because adversarial review of the tree found five live paths the five do not
cover (Tasks 11, 18 and 19) and the second review round added the knowledge-base barrier under an
operator ruling (Tasks 10A–10D; see [Accepted risks](#accepted-risks)). Order inside the phase is O8
(the two fully-open reads first), then O12 (the KB tier, its ratchet, its barrier, its metadata
scope), then O3/O4 (bind, then turn), then the extension gates. O13 applies throughout: Tasks 10B,
10C, 10D and 11 each verify `cargo check --workspace --all-targets` before committing, because this
is the stretch where an earlier draft left nine consecutive commits red.

### Task 10: The chatrecall LOAD guard, and `ExtensionManager::capability_tier()`

`handle_chatrecall`'s LOAD branch has **no filter of any kind** — not even the `exclude_session_id`
guard SEARCH sets at `:188`, so a session can load itself. It takes a caller-supplied `session_id`,
calls `get_session(&sid, true)`, and builds a header carrying the target's name, working directory
and message count before any message text. Ship it first.

**Files:**

| Action | Path | Anchor (re-verified at `9558c346`) |
|---|---|---|
| Modify | `crates/biorouter/src/agents/chatrecall_extension.rs` | `handle_chatrecall` `:78`; LOAD branch `:90-159`; `get_session(&sid, true)` `:92`; the header `format!` `:113-119` (`loaded_session.name`, `sid`, `working_dir.display()`, `total`); SEARCH `:160-245`; `exclude_session_id` `:188` |
| Modify | `crates/biorouter/src/agents/extension_manager.rs` | `provider: SharedProvider` field at `:113` — **private**, so a new `pub` accessor is required |
| Reference | `crates/biorouter/src/agents/extension.rs` | `PlatformExtensionContext` `:109-113` — carries `extension_manager: Option<Weak<ExtensionManager>>`, populated for Platform extensions at `extension_manager.rs:799` |
| Reference | `crates/biorouter/src/agents/types.rs` | `SharedProvider = Arc<Mutex<Option<Arc<dyn Provider>>>>` at `:13` |

- [ ] **Step 1: Write the failing test**

```rust
#[tokio::test]
async fn load_refuses_a_private_session_without_naming_it() {
    // The leak is in the STRING, not the return value: a guard placed after
    // the header `format!` at :113 returns an error whose text already carries
    // the session name and the working directory. §11.4 classifies both as
    // CONTENT — a title in this product is LLM-generated from the conversation,
    // and a working dir routinely names a cohort, a study or a population.
    let target = private_session_named("OMOP diabetes cohort characterisation",
                                       "/data/phi/cohort-2026-dm2").await;
    let out = load_via_public_capability_caller(&target.id).await.unwrap();
    let text = out[0].as_text().unwrap().text.clone();

    assert!(text.contains("private"), "must say why: {text}");
    assert!(!text.contains("OMOP"), "leaked the session name: {text}");
    assert!(!text.contains("diabetes"), "leaked the session name: {text}");
    assert!(!text.contains("cohort-2026-dm2"), "leaked the working dir: {text}");
    assert!(!text.contains("/data/phi"), "leaked the working dir: {text}");
}

#[tokio::test]
async fn load_still_works_for_a_private_caller_and_for_public_targets() {
    let priv_target = private_session_named("OMOP cohort", "/data/phi/x").await;
    let pub_target  = public_session_named("weekly notes", "/tmp/notes").await;
    assert!(load_via_private_capability_caller(&priv_target.id).await.unwrap()[0]
            .as_text().unwrap().text.contains("OMOP cohort"));
    assert!(load_via_public_capability_caller(&pub_target.id).await.unwrap()[0]
            .as_text().unwrap().text.contains("weekly notes"));
}

#[tokio::test]
async fn a_dead_extension_manager_weak_refuses_rather_than_defaulting_open() {
    // PlatformExtensionContext holds a Weak. If it has died the caller's
    // capability is unknowable, and unknown must refuse — not fall through.
    let target = private_session_named("OMOP cohort", "/data/phi/x").await;
    let out = load_with_dead_weak(&target.id).await.unwrap();
    assert!(out[0].as_text().unwrap().text.contains("private"));
}
```

- [ ] **Step 2: Run** → **COMPILE ERROR** (`no method capability_tier on ExtensionManager`).

- [ ] **Step 3: Implement**

On `ExtensionManager`, one accessor that Gates C, D and E all read (the field at `:113` is private):

```rust
    /// The capability of the model currently bound to this session.
    ///
    /// Reads the SAME `Arc<Mutex<Option<..>>>` the Agent swaps in
    /// `update_provider` (`Agent::new` passes `provider.clone()` to
    /// `ExtensionManager::new` at `agent.rs:848`), so a mid-session model
    /// change is visible on the very next call with no plumbing and no TOCTOU
    /// window.
    ///
    /// `None` — legitimately the state before the first bind — resolves to
    /// **Public**, which is the safe direction for all three gates that read
    /// this: Gate C refuses private extensions, Gate D filters to public rows,
    /// Gate E hides private tools.
    pub async fn capability_tier(&self) -> crate::privacy::ProviderTier {
        self.provider
            .lock()
            .await
            .as_ref()
            .map(|p| p.tier())
            .unwrap_or(crate::privacy::ProviderTier::Public)
    }
```

In `chatrecall_extension.rs`, between `get_session` (`:92`) and the header `format!` (`:113`):

```rust
                Ok(loaded_session) => {
                    // Issue #56 Gate D (LOAD). BEFORE the header string is
                    // built, so neither the session name nor the working
                    // directory can escape — both are CONTENT under §11.4.
                    let caller = match self
                        .context
                        .extension_manager
                        .as_ref()
                        .and_then(|w| w.upgrade())
                    {
                        Some(em) => em.capability_tier().await,
                        // A dead Weak means the capability is unknowable.
                        // Refuse; never default open.
                        None => crate::privacy::ProviderTier::Public,
                    };
                    if !crate::privacy::visible_to(caller, loaded_session.privacy_tier) {
                        return Ok(vec![Content::text(CHATRECALL_LOAD_REFUSAL)]);
                    }
```

⚠ **The refusal is a file-local `const` in this task, not a call into `privacy::refusal`.**
`crates/biorouter/src/privacy/refusal.rs` does not exist until Task 12, which is three tasks away —
`crate::privacy::refusal::chatrecall_load_refusal()` here is an `unresolved module` compile error,
and Step 2 already expects a *different* compile error, so it would be indistinguishable from the
intended one. Declare it beside `handle_chatrecall`:

```rust
/// Moved into `crates/biorouter/src/privacy/refusal.rs` by Task 13, which is
/// the first task that has that module. Constant on purpose: a model that sees
/// a different string on retry concludes the refusal is transient and loops.
const CHATRECALL_LOAD_REFUSAL: &str = "…the text below…";
```

with the refusal a **constant** string that names no target:

> This chat history is private: it was created under a model hosted inside the institution, so only
> a private model may read it. This session is running on a public model. Ask the user to switch
> this chat to a private model — Settings → Models, or the model chip in the composer — and try
> again. Do not retry with a different session id or through another tool; the boundary is the same
> everywhere.

- [ ] **Step 4: Run**

```bash
cargo test -p biorouter --lib agents::chatrecall_extension
```

Expected: **PASS**, 3 new tests. ⚠ **The pre-count is zero**: `chatrecall_extension.rs` has **no
`#[cfg(test)] mod tests` today**, so this filter prints `0 passed` and exits 0 before Step 1 — which
is the BR-71 defect shape, and here it is the *correct* baseline rather than a broken filter. This
task creates the module. Assert `3 passed`, not "no failures".

- [ ] **Step 5: Gate**

```bash
# The guard precedes the header construction, not follows it. The range is
# LOAD MODE only: both delimiters are real comments in the file
# (`// LOAD MODE: Get session summary` at :91, `// SEARCH MODE: Search across all
# sessions` at :161), and the span is 71 lines containing exactly ONE
# "Working Dir:" — SEARCH has its own at :214 and must not be in the window.
awk '/\/\/ LOAD MODE:/,/\/\/ SEARCH MODE:/' \
  crates/biorouter/src/agents/chatrecall_extension.rs \
  | grep -n "visible_to\|Working Dir:"
# Expected: exactly TWO lines, `visible_to` on the SMALLER line number.
# THREE lines means the range leaked into SEARCH; ONE means the guard is not in
# LOAD at all.
#
# ⚠ The first version of this gate ranged `/fn handle_chatrecall/,/fn [a-z_]+\(.*SEARCH/`.
# The END pattern occurs NOWHERE in the file (measured: 0), so awk ran the range
# from :78 to EOF — 242 of the file's 319 lines, including the SEARCH builder and
# the test module Step 1 adds. It caught the target bug by luck and would have
# started printing a third and fourth line the moment anything else in the file
# mentioned a working directory.
# The refusal is constant and target-free.
grep -c "loaded_session.name" crates/biorouter/src/agents/chatrecall_extension.rs ; echo "expect: 1 (the header only)"
```

**What this catches.** The wrong implementation builds the header first and *then* returns an error
— which is what "add a check to LOAD mode" naturally produces, because `loaded_session` is already
in hand. The return value is an error either way, so a test asserting only that an error came back
passes it. The substring assertions in Step 1 are the only thing that fails it, and they are why the
fixture's name and working directory are unique sentinels.

- [ ] **Step 6: Commit**

```bash
git add crates/biorouter/src/agents/chatrecall_extension.rs crates/biorouter/src/agents/extension_manager.rs
git commit -m "fix(chatrecall): refuse LOAD of a private session before any of it is rendered (#56)"
```

---

### Task 10A: The knowledge-base tier — the store, the caller-capability channel, and the migration

**The operator has ruled.** Design §9.3 B4 forces a choice — ratchet a KB's classification on ingest,
or declare KBs a designed public sink — and refuses to allow deferral. The ruling is **ratchet**:
*a knowledge base takes the tier of the most sensitive session that has ingested into it, and a
public-capability session may not read a private KB.* The costs are real, were put to the operator,
and were accepted; they are written out in [Accepted risks](#accepted-risks) AR-1 and AR-2. Read them
before starting, because the first bug report after this ships will be one of them.

**Why every one of the nine gates misses this today.** Gate C sees public→public (`knowledge` is a
built-in, `crates/biorouter-mcp/src/lib.rs:126`, ⇒ Public by DR-6). Gate E lists the `kb_*` tools
legitimately. Gate D never touches `chat_history_search.rs`. Gate G (Task 11) checks *other sessions'
ids*, and a session ingesting **its own** transcript is not that. The sink is machine-wide
(`knowledge_root()` = `in_config_dir("knowledge")`, `knowledge/paths.rs:43-45`; `kb_root` is a bare
path join at `:47-49`), and `kb_id_or_primary`'s own doc comment says an explicit `kb_id` "always
wins and is **never filtered** against the session's set" (`knowledge/server.rs:308-311`). So: a
private session writes its OMOP notes into `default`, and the next chat on Claude reads them back
with `kb_search`. That is the whole laundering path, and it needs no bug.

This task adds the tier and nothing enforces it — the same Phase-1-style separation O1 uses, for the
same reason: Task 10C is then one branch over an already-tested lookup.

---

#### ⚠ Read this first: where the barrier goes, and why it is **not** the tool layer

The previous version of Tasks 10A–10C gated **sixteen enumerated `kb_*` tool call sites**. That
design was taken apart by verification and does not survive. Its four independent failures, each
measured against the tree:

1. **The enumeration was incomplete, and could not be completed by enumerating harder.** Four whole
   surfaces reach knowledge-base content without passing through a single one of the sixteen:
   `agent_drafter`'s `export_app` (`agent_drafter/mod.rs:1419-1429` → `svc.export_brkb(kb)`, with
   the ids taken from the **model-supplied** `include.knowledge_bases`, `:1397`), the app socket's
   `run_kb_read` (`routes/apps.rs:2376-2422`, which calls `store::search` / `svc.read_page` /
   `svc.get_graph` / `svc.list_history` **directly, never through `KnowledgeServer`**), that
   socket's `ingest` arm (`routes/apps.rs:2533`, `svc.add_raw_source`), and `KbToolDispatch`
   (`knowledge/subagent/kb_tools.rs:31-130`), a **second full KB tool surface** the macros' sub-agent
   drives straight into `store::*`.
2. **Six of the sixteen were unimplementable.** Nine of the nineteen `kb_*` tools take **no
   `RequestContext`** — `kb_create_base` `:357`, `kb_write_page` `:409`, `kb_add_raw_source` `:454`,
   `kb_restore_state` `:509`, `kb_begin_txn` `:527`, `kb_commit_txn` `:543`, `kb_abort_txn` `:562`,
   `kb_append_log` `:650`, `kb_export` `:737` — so they cannot learn the caller's capability at all.
   And `kb_import` takes `ImportArchiveParams { src_path }` (`server.rs:46-49`): **no `kb_id` exists**
   until `import_brkb` returns (`:771`), so a pre-write check has no subject.
3. **The ruling was not actually enforced.** `conversation_ingest::ingest_conversation` runs the
   ingest macro's sub-agent, which writes through `KbToolDispatch` → `store::write_page` /
   `svc.add_raw_source`. None of those was a raise site, so Task 11's headline test
   `ingesting_your_own_private_conversation_ratchets_the_knowledge_base` failed while every per-file
   gate reported green.
4. **Two of the sixteen encoded a regression.** `kb_create_base` and `kb_import` name a base that
   **does not exist yet**, and this task's own rule reads "no entry ⇒ private" — so a public session
   could never create or import a knowledge base.

Sixteen call sites becoming twenty-plus, six of them unbuildable, is the signature of gating at the
wrong layer. So the real question was asked and answered against the tree.

**A choke point exists.** Four of them, and together they cover every read and every write by
construction. Each was verified, not assumed:

| # | Choke point | Anchor | What it covers |
|---|---|---|---|
| CP1 | `<KnowledgeServer as ServerHandler>::call_tool` | today generated by `#[tool_handler(router = self.tool_router)]`, `server.rs:776-777` | **all nineteen** `kb_*` tools, including the nine that take no `RequestContext` — and the twentieth, the day it is written |
| CP2 | `macros::ingest::ingest` `:47`, `macros::query::query` `:46`, `macros::lint::lint` `:217` | each opens `let _lock = svc.lock_kb(&args.kb_id).await?;` then `let kb_root = paths::kb_root(..)` | the four HTTP macro routes, `conversation_ingest` and its three callers, `bin/knowledge_ingest_probe.rs`, and the **whole `KbToolDispatch` sub-agent surface** |
| CP3 | `routes/apps.rs::handle_kb_frame` `:2474` | the single funnel its three call sites (`:3288`, `:3513`, `:3847`) share, immediately after `resolve_kb_grant` `:2268` resolves the id | `run_kb_read` `:2376` (search / page / graph / history) **and** the `ingest` arm `:2533` |
| CP4 | `agent_drafter::stage_full_payload` `:1390` | one caller (`export_app`, `:2790`); `knowledge_service_for_export` `:1274` also has exactly one caller | `export_app`'s `svc.export_brkb(kb)` `:1423` — the drafter's only door to the knowledge **content** store |
| CP5 | `agent_drafter::catalog::Catalog::discover` `:69` (Task 10D) | 6 production callers, measured (`agent_drafter/mod.rs:1090` `:2071` `:2202` `:2511` `:2627`, `routes/apps.rs:772`) | the base **id and name** — `list_platform_catalog`, the three `validate::check_*` rejection strings, `capability_report`. CP1–CP4 are blind to it by construction: it returns metadata and touches no content, so neither new-surface detector's pattern names it |

**Why CP1 is a real seam and not a wish.** `#[tool_handler]` is not magic: `rmcp-macros-0.14.0`'s
`src/tool_handler.rs:25-63` appends exactly two methods to the `impl`, and `call_tool` is verbatim

```rust
async fn call_tool(
    &self,
    request: rmcp::model::CallToolRequestParams,
    context: rmcp::service::RequestContext<rmcp::RoleServer>,
) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
    let tcc = rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
    self.tool_router.call(tcc).await
}
```

`ToolCallContext::new` is `pub` (`rmcp-0.14.0/src/handler/server/tool.rs:40`) and `ToolRouter::call`
is `pub async fn` (`.../router/tool.rs:240`), so **hand-writing that body is a drop-in**, not a fork.
The handler receives the `RequestContext` for *every* tool — the individual `#[tool]` fn's signature
is irrelevant to it — and `request.arguments: Option<JsonObject>` (`rmcp-0.14.0/src/model.rs:1898`)
carries the `kb_id` before any tool body runs. That is the whole of failure (2), dissolved. The
channel itself is already proven in this file: `session_id_from_context` (`:222-224`) reads
`context.meta.0`, and that is the same `context`.

**Why not the three layers the brief proposed.** Each was measured and each fails:

- **`KnowledgeService` is not a choke point.** Nineteen sites call `store::list_pages` / `read_page`
  / `write_page` / `search` / `search_with_scope` **around** it — including `routes/apps.rs:2394`
  and all four content ops in `kb_tools.rs`. Making it one means first re-routing those through
  service methods that do not exist (there is no service equivalent of `search_with_scope` with a
  `SearchScope`), which is a behaviour-changing refactor inside a security task.
- **`store::` cannot carry the question.** Every function there takes `kb_root: &Path`
  (`store.rs:36`, `:82`, `:94`, `:224`, `:228`) — a resolved directory, with no kb id and no caller.
  It is downstream of the decision, not the place to make it.
- **`paths::kb_root` (`paths.rs:47-49`) genuinely is the universal resolver** — 44 call sites, and
  every service method that touches content opens with `let kb_root = paths::kb_root(&self.root,
  id);` — but it cannot be the gate, for three reasons. It returns `PathBuf`, not `Result`, so
  refusing there changes all 44 sites. It is a free function with no caller channel, and it cannot
  be given an ambient one: built-in MCP servers are served by a task `tokio::spawn`ed at extension
  init (`spawn_and_serve`, `crates/biorouter-mcp/src/lib.rs:60`, over a `DuplexStream` pair), so a
  `tokio::task_local` set in `dispatch_tool_call` **cannot** reach `KnowledgeServer` — the only
  channel across that seam is the request meta, which lands at `call_tool`. And fatally, `kb_root`
  also serves the surfaces this plan deliberately does **not** gate (the Knowledge view's HTTP
  reads, the CLI, `soul.rs`, `reset.rs`), so a check there would have to permit whenever the caller
  is unknown — inverting the fail direction for exactly the callers that matter.

#### Coverage self-review — every knowledge-base surface, and what covers it

Stated per surface rather than as a claim about the design, because the two leaks found in the fourth
adversarial round were both in the gap between "the choke points are right" (they are) and "the
choke points cover everything" (they do not — they cover everything they were derived over, which was
**content**). Anyone extending this work should read the last column, not the first.

| Surface | Covered by | How complete, honestly |
|---|---|---|
| All 19 `kb_*` MCP tools, and the 20th | **CP1** | **Complete by construction.** `call_tool` receives the `RequestContext` for every tool regardless of the tool's own signature. `every_kb_tool_is_gated_or_exempt_for_a_pinned_reason` (10C) turns a new tool into a test failure, and `no_exempt_tool_volunteers_a_private_bases_id_to_a_public_caller` covers the metadata half of a new EXEMPT one. |
| The three macros' sub-agent (`KbToolDispatch`, 7 tools) | **CP2** | **Complete by construction.** The dispatch is bound to one `kb_id` field at construction (`kb_tools.rs:22-30`) and every arm derives from it, so a new sub-agent tool is covered the day it is written. Note **7**, not the 5 an earlier pass listed — `kb_classify_source` `:122` and `kb_list_pages` `:42` are also there. |
| The 4 HTTP macro routes, `conversation_ingest` + its 3 callers, `bin/knowledge_ingest_probe.rs` | **CP2** | Complete *today*, and a required `caller_is_private` makes a new caller a compile error. A new macro that does not funnel through `ingest`/`query`/`lint` would not be covered. |
| The app socket: `run_kb_read` + the `ingest` arm | **CP3** | Complete for the socket, because `handle_kb_frame` is the single funnel its three call sites share. |
| `export_app` → `export_brkb` | **CP4** | Complete for the drafter's **content** door: `knowledge_service_for_export` has exactly one caller. |
| A base's **id and name** — `list_platform_catalog`, `validate::check_*` rejection strings, `capability_report` | **CP5** (Task 10D) | **Found in round four, not derived in round three.** CP1–CP4 were derived over content and CP5 was not in the enumeration; both of Task 10C's new-surface detectors are structurally blind to it, because neither pattern names `list_bases`. Task 10D adds a metadata detector. |
| The no-target/no-primary error id lists — `kb_id_or_primary` `:323-341`, `resolve_target_kb` `:149-159` | **Task 10C** and **Task 11** | Same class as CP5, same blind spot, two more instances. Both were found by sweeping `session_kb_ids` callers by hand; no detector in this plan would have found either. |
| The 7 `/knowledge/*` GUI **read** routes | **nothing, by decision** | The Knowledge view is the user, not a model (Task 10C's ⚠). [Open question 15](#open-questions) records that the asymmetry is undecided in the UI. |
| The `/knowledge/*` **write** routes, the CLI's write commands, `soul.rs`, `reset.rs` | **nothing, by decision** | No model is involved; there is no service-level write choke point to hang a raise on (Task 10B's second exclusion list). |
| Existence of a base, from a *guessed* id (`create_base`'s "already exists", `resolve_target_kb:141`) | **nothing, by decision** | DR-7 puts side channels out of scope. [AR-5](#ar-5--the-existence-of-a-private-knowledge-base-is-still-inferable). |
| A **future** surface of either kind | **a detector, not a construction** | Task 10C's Step 5 has two content detectors (expect 4 and 4); Task 10D's Step 5 has the metadata one, in **two** sweeps (27 hits / 18 production outside `knowledge/`, 22 / 5 inside it — the second added after a single-sweep version proved structurally unable to see `kb_get_active`), plus the metadata register, which classifies *tools* rather than call sites. All are counted enumerations that fail when they grow. That is a tripwire, not coverage. |

**What the operator is accepting by taking choke points rather than an enumeration.** Three things,
stated plainly so no one discovers them later:

- **A new *surface* is still a manual step, and round four is the proof.** CP1 covers any future
  `kb_*` tool for free, and CP2 covers any future sub-agent macro tool for free. A future way to
  reach the store that goes round both — a new HTTP route bypassing `KnowledgeServer`, a second
  in-process server — is not covered by construction. Task 10C's Step 5 gate is written to detect
  exactly that: it enumerates every `store::*` and `KnowledgeService` content call outside
  `knowledge/` and fails if a new one appears. **But a detector only sees what its pattern names.**
  Both content detectors missed `list_platform_catalog` — an *existing* surface, not a future one —
  because it goes through `list_bases`, which is neither a `store::` call nor a service content call.
  That is the whole of B1, and it is the reason Task 10D exists and ships a third detector keyed on
  metadata. Treat the three detectors as three tripwires with three specific tripping conditions, not
  as a proof of completeness.
- **CP1 resolves an omitted `kb_id` to the session's primary by calling `primary_kb_for_context`, the
  same function the tool will call moments later.** Two disk reads, so a concurrent `kb_set_active`
  between them could in principle have the handler check base A while the tool reads base B. The
  window is microseconds, it requires the user to move their own pointer mid-call, and the
  tool-layer design had the identical window. Recorded, not fixed.
- **The `.kb-tiers` file is machine-local and user-writable.** Deleting it re-runs the migration and
  reads every base public again. That is the same fail-open direction AR-2 already accepts, and the
  threat model here is "a public model reads private notes", not "a local attacker with a shell".

---

**Files:**

| Action | Path | Anchor (re-verified at `9558c346`) |
|---|---|---|
| Create | `crates/biorouter-mcp/src/knowledge/tier.rs` | new |
| Modify | `crates/biorouter-mcp/src/knowledge/mod.rs` | the `pub mod` list, `:1-18` — insert `pub mod tier;` between `store` `:15` and `subagent` `:16` |
| Modify | `crates/biorouter-mcp/src/knowledge/paths.rs` | add `kb_tiers_path` beside `primary_kb_path` `:62-64`, `primary_kb_sessions_dir` `:69-71`, `hidden_kbs_path` `:73-75`, `hidden_kb_sessions_dir` `:77-79`; `knowledge_root` `:43-45`; `kb_root` `:47-49`; `validate_kb_id` `:3-20` |
| Modify | `crates/biorouter-mcp/src/knowledge/service.rs` | `new` `:404` (best-effort migration); `root()` `:415`; `lock_root()` `:427` and `FileLockGuard::acquire` `:63-78` — **both private to this module**, which is why the lock wrappers live here; `create_base` `:447` (its `let _lock = self.lock_root()?;` is `:448`); `import_brkb` `:506` (lock at `:507`); `delete_base` `:657` (lock at `:658`); the `*_unlocked` convention this follows — 20 helpers, e.g. `set_hidden_path_unlocked` `:293`, `get_primary_persisted_unlocked` `:1123`, `selection_unlocked` `:1469` |
| Modify | `crates/biorouter-mcp/src/knowledge/server.rs` | `SESSION_ID_META_KEY` `:18`; `session_id_from_context` `:222-224`; `session_id` `:226-228`; `KnowledgeServer::new` `:214` — add the `caller_is_private` reader beside them |
| Modify | `crates/biorouter/src/agents/mcp_client.rs` | `McpMeta` `:136-144` (two fields today), `McpMeta::new` `:146-152`, `with_progress_token` `:156-159`, `inject_into_extensions` `:161-172` |
| Modify | `crates/biorouter/src/agents/extension_manager.rs` | the sole production `McpMeta::new(&session_id)` at `:1557`, inside `dispatch_tool_call`'s spawned future (`:1544-1570`) |
| Reference | `crates/biorouter-mcp/src/knowledge/manifest.rs` | `save` `:17-24` — the tmp-then-`rename` idiom to copy; and the reason the tier does **not** go in `Manifest` (`types.rs:58-66`): the manifest travels inside the `.brkb` archive |
| Modify | `crates/biorouter-mcp/src/knowledge/brkb.rs` | decision (2)(a). `export` `:8-19` writes the `<kb_id>/.brkb-provenance` entry into the `ZipWriter` after `walk` `:16` and before `zip.finish()` `:17`; `import` `:68-128` reads it, **skips it** in the extraction loop `:98-114`, and returns it beside the id. ⚠ It must sit INSIDE the single top-level directory — `:80-87` bails on anything else. `import`'s signature changes, and its two callers are `service.rs:509` and this task's tests |
| Modify | `crates/biorouter-mcp/src/knowledge/service.rs` (second row) | decision (2)(a). `export_brkb` `:494-504` and `import_brkb` `:506-520`, whose `raise` on `new_id` is `marker \|\| importer_is_private` — a **max**, never the marker alone |
| Reference | `crates/biorouter-mcp/src/knowledge/brkb.rs` | `import`'s collision loop — `while knowledge_root.join(&id).exists() { suffix += 1; id = format!("{original_id}-{suffix}"); }` — means an import **never** overwrites an existing base; it always lands on a fresh id. That is what makes Task 10B's "stamp after the import" safe |

⚠ **Five design decisions, each with a reason a reviewer will otherwise ask about.**

1. **A `bool`, not a third enum.** `crates/biorouter-mcp` **cannot** depend on `crates/biorouter` —
   the dependency runs the other way (`extension_manager.rs:1512` uses
   `biorouter_mcp::secret_guard`), which is the same constraint that made the knowledge macros take
   a `Box<dyn Completer>` instead of a `Provider`. So `ProviderTier` is not nameable here. The
   choices are a duplicate enum that must be kept in sync by discipline, or one boolean named
   `caller_is_private`. This plan takes the boolean, and the precedent is in this plan already:
   Task 12's `bind_provider_if_allowed(.., incoming_is_private: bool)`. Because `floor(Private) =
   Private` and `floor(Public) = Public`, the boolean *is* the crossing — which is why Task 7's
   `floor` caller set does not grow here.
2. **A sidecar, not `manifest.yaml` — *and* a raise-only provenance marker inside the archive,
   because the sidecar alone leaves a two-call laundering path.** The manifest is inside the KB's git
   tree and is carried by `export_brkb`/`import_brkb` (`service.rs:494`/`:506`), so a tier stored
   there is **attacker-supplied on import** — the exact shape Task 22 refuses for session imports.
   The sidecar sits beside `.active-kb` and `.hidden-kbs`, which are already machine-local, already
   outside every KB's repo, and already excluded from the archive. That much stands.

   But "the tier does not travel" was mistaken for the whole answer, and it is not. Because it does
   not travel, an imported base takes the *importer's* tier (Task 10B) — so **export from a private
   chat, import into a public one, and every page of a private base is now in a public base**. Two
   tool calls, no gate crossed: `kb_export` is permitted to the private caller because the base is
   its own, and `kb_import` is permitted to the public caller because the id it creates does not
   exist yet (decision 3). The user's own Knowledge-view export reaches the same state without a
   model involved at all. Task 10B's import test (as first written,
   `an_imported_base_takes_the_importing_sessions_tier_and_never_the_archives`) exercised only a
   **private** importer, so it asserts the safe half of exactly this rule and
   passes.

   Two changes close it, and they are deliberately different in kind:

   *(a) A raise-only marker in the archive.* `brkb::export` writes one extra zip entry,
   `<kb_id>/.brkb-provenance`, straight into the `ZipWriter` after `walk` (`brkb.rs:16-17`) — never
   onto disk, so the KB's git tree does not gain a file. It must live **inside** the single
   top-level directory: `import` (`:70-87`) bails unless there is exactly one, so a sibling entry
   breaks every archive. `import` reads it, skips it during extraction, and returns it alongside the
   new id; `import_brkb` then raises the new base to `max(marker, importer)`.

   **This dissolves decision (2)'s objection rather than contradicting it.** "Attacker-supplied" is
   only dangerous when the supplied value can *lower*. A marker that is read as a **floor** — the
   imported base is private if *either* the archive or the importer says so — gives a hostile
   archive exactly one power: to over-classify itself. An absent or malformed marker means
   "unknown", which is the importer's tier and is today's behaviour, so a foreign `.brkb` is
   unaffected.

   *(b) A model's export of a private base lands where the public capability cannot read it.*
   (a) is not a barrier on its own: anyone who can rewrite the archive can delete the entry, and the
   file sits outside all four DR-14 roots, so a public model with the shell can read it directly —
   a `.brkb` is a zip, and `unzip -p` needs no Biorouter at all. So `kb_export` called by a **model**
   for a **private** base ignores a `dest_path` outside the knowledge tree and writes into
   `<knowledge-root>/exports/`, returning that path. That directory is inside DR-14 deny root #2
   (`<config>/knowledge`), so the artifact is invisible to a public-capability session by the same
   kernel deny that hides the base it came from. The **user's** export — `GET /knowledge/.../export`
   and the CLI — is untouched and may write anywhere: the user is not a model, which is the same
   scope line Task 10C draws for the seven read routes.

   What remains after both is written down rather than closed: a **private**-capability model that
   also holds the shell can copy the artifact out of the deny root. That is not new and not
   specific to archives — a private model with a shell can copy the whole knowledge tree — and this
   design constrains what the *public* model can reach. Recorded as
   [AR-8](#ar-8--a-private-model-with-a-shell-can-still-carry-a-knowledge-base-out-of-the-deny-root).
3. **Three fail directions, and they differ on purpose** (DR-10's pattern, one module over).
   *Migration* → **public** (fail open; AR-2). *A kb id with no entry, in a store that exists, for a
   directory that does exist on disk* → **private** (fail closed: a base that appeared without
   going through `create_base` or `import_brkb` has unknown provenance). *A kb id with no directory
   on disk* → **permit** (there is no content to leak, and refusing here is what banned creation and
   import for public sessions in the previous draft). *An absent capability meta key* → the caller
   is **Public** (fail closed for reads, which is what Task 10C consumes it for).
4. **The capability meta key goes to built-in servers only.** `McpMeta::new` already ships the
   session id to *every* MCP server including third-party stdio ones; the capability tier
   deliberately does not follow that precedent, because "this user is on an institutional model" is
   a fact about the user's configuration and a third-party server has no business learning it. The
   injection is conditioned on `biorouter_mcp::BUILTIN_EXTENSIONS` membership
   (`crates/biorouter-mcp/src/lib.rs:96`, 7 entries).
5. **`create_base` / `import_brkb` / `delete_base` keep their signatures, and the `_unlocked`
   convention is why the store does not deadlock.** Two separate corrections to the previous draft:

   *(a) No required `caller_is_private` parameter.* The previous draft made it required on all three
   "so every call site is a compile error rather than an omission". Measured, that is **~90 edits**:

   | Function | Production call sites | Test call sites |
   |---|---|---|
   | `create_base` | 8 — `biorouter-cli/src/commands/knowledge.rs:388`, `:520`; `biorouter-mcp/src/knowledge/server.rs:364`; `biorouter-server/src/routes/knowledge.rs:359`; `biorouter-server/src/routes/reset.rs:123`; `biorouter-server/src/bin/knowledge_ingest_probe.rs:63`; `biorouter/src/agents/knowledge_tool.rs:135`; `biorouter/src/knowledge/soul.rs:76` | **~82** (`service.rs` alone has 30) |
   | `import_brkb` | 2 — `knowledge/server.rs:771`, `routes/knowledge.rs:1577` | 2 |
   | `delete_base` | 2 — `routes/knowledge.rs:447`, `routes/reset.rs:121` | 5 |

   Eight security-relevant edits buried in eighty-two mechanical ones is a worse review than no
   compile error at all, and six of the eight are user surfaces that would all pass `false`.
   Instead: **`create_base` and `import_brkb` register the new id as PUBLIC**, unconditionally, and
   the tier is then raised by whichever choke point the creating call came through (Task 10B). A
   base is born public and privatised in the same call when the creator is private — the same
   observable behaviour, reached without touching a signature that has ninety callers. The
   registration is not optional: decision (3) reads an unregistered *existing* directory as private,
   so a base created from the CLI with no registration would lock the user out of a base they just
   made.

   *(b) The lock discipline.* The previous draft said "take `KnowledgeService::lock_root()` around
   every read-modify-write" **and** register from inside `create_base` / `import_brkb` /
   `delete_base` — all three of which **already hold that lock** (`:448`, `:507`, `:658`).
   `FileLockGuard::acquire` (`:63-78`) opens a fresh fd and `flock`s it exclusively, so a second
   acquire in the same process blocks forever: the daemon would hang on the first `kb_create_base`
   while Step 1's tests, which call the store on a bare root, all passed. Compounding it,
   `lock_root` (`:427`) and `FileLockGuard` (`:58`) are **private to the `service` module** and a
   free function in `knowledge::tier` cannot reach them. The tree already has the answer and uses it
   twenty times: the **`_unlocked` suffix**. So `tier.rs` exposes *only* lock-free functions, and
   `KnowledgeService` owns the two wrappers that take the lock.

- [ ] **Step 1: Write the failing tests**

```rust
// crates/biorouter-mcp/src/knowledge/tier.rs, in its own #[cfg(test)] mod tests

#[test]
fn an_unmigrated_root_migrates_every_existing_base_to_public_exactly_once() {
    // AR-2, made executable. The tree keeps no record of which session wrote
    // which page — the git author is "Biorouter", not a session id — so there
    // is nothing to reason from and the migration fails OPEN, like the session
    // backfill (DR-10). It must run ONCE: a startup-repeating "add any base
    // missing from the file as public" would un-ratchet a base the day after a
    // private session raised it, which is Task 38's bug in a different store.
    let root = tempdir_with_bases(&["default", "omop"]);
    assert!(!tiers_file_exists(&root));

    ensure_migrated_unlocked(&root).unwrap();
    assert!(!is_private(&root, "default"));
    assert!(!is_private(&root, "omop"));

    raise_unlocked(&root, "omop", /* caller_is_private */ true).unwrap();
    ensure_migrated_unlocked(&root).unwrap();        // second launch
    assert!(is_private(&root, "omop"), "the migration re-ran and lowered a tier");
}

#[test]
fn a_base_that_never_went_through_create_or_import_reads_private() {
    // Fail-closed, and it is the difference between "known public" and
    // "unknown". A store that listed only the private ids could not tell them
    // apart, which is why the file is a map and not a list like `.hidden-kbs`.
    let root = tempdir_with_bases(&["default"]);
    ensure_migrated_unlocked(&root).unwrap();
    std::fs::create_dir_all(root.join("dropped-in-by-hand")).unwrap();
    assert!(is_private(&root, "dropped-in-by-hand"));
    assert!(!is_private(&root, "default"));
}

#[test]
fn a_base_that_does_not_exist_on_disk_is_reachable_by_anyone() {
    // Decision (3), third direction — and the bug the sixteen-site enumeration
    // encoded: `kb_create_base` and `kb_import` name a base that does not exist
    // yet, so a barrier that reads "no entry ⇒ private" bans a public session
    // from ever creating or importing one. There is no content to leak from a
    // directory that is not there.
    let root = tempdir_with_bases(&["default"]);
    ensure_migrated_unlocked(&root).unwrap();
    assert!(!is_private(&root, "not-created-yet"));
    assert_reachable(&root, "not-created-yet", /* caller_is_private */ false).unwrap();
}

#[test]
fn raise_is_monotone_and_registers_an_absent_base_at_the_callers_tier() {
    let root = tempdir_with_bases(&[]);
    ensure_migrated_unlocked(&root).unwrap();

    raise_unlocked(&root, "fresh", false).unwrap();  // created from a public chat
    assert!(!is_private(&root, "fresh"));
    raise_unlocked(&root, "fresh", true).unwrap();   // a private chat writes to it
    assert!(is_private(&root, "fresh"));
    raise_unlocked(&root, "fresh", false).unwrap();  // and a public chat writes again
    assert!(is_private(&root, "fresh"), "a public write lowered the tier");

    raise_unlocked(&root, "born-private", true).unwrap();
    assert!(is_private(&root, "born-private"));
}

#[test]
fn register_public_never_lowers_an_already_private_base() {
    // `create_base` registers unconditionally (decision 5a). If that registration
    // could overwrite, then re-creating a deleted id — or any future caller that
    // registers twice — would launder a private base to public.
    let root = tempdir_with_bases(&["omop"]);
    ensure_migrated_unlocked(&root).unwrap();
    raise_unlocked(&root, "omop", true).unwrap();
    register_public_if_absent_unlocked(&root, "omop").unwrap();
    assert!(is_private(&root, "omop"), "registration lowered a ratcheted base");
}

#[test]
fn deleting_a_base_forgets_its_tier_so_the_id_can_be_reused() {
    // Otherwise `kb_create_base("omop")` from a public chat, after a private
    // `omop` was deleted, silently inherits Private and the user cannot see why.
    let root = tempdir_with_bases(&["omop"]);
    ensure_migrated_unlocked(&root).unwrap();
    raise_unlocked(&root, "omop", true).unwrap();
    forget_unlocked(&root, "omop").unwrap();
    raise_unlocked(&root, "omop", false).unwrap();
    assert!(!is_private(&root, "omop"));
}

#[test]
fn the_store_is_written_atomically_and_never_leaves_a_tmp_file() {
    // manifest.rs:17-24's idiom. A torn write here reads as "no entry", which
    // fails CLOSED and locks the user out of their own knowledge base.
    let root = tempdir_with_bases(&["default"]);
    ensure_migrated_unlocked(&root).unwrap();
    raise_unlocked(&root, "default", true).unwrap();
    let names: Vec<_> = std::fs::read_dir(&root).unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string()).collect();
    assert!(names.iter().any(|n| n == ".kb-tiers"));
    assert!(!names.iter().any(|n| n.ends_with(".tmp")));
}

#[test]
fn a_private_export_cannot_be_laundered_by_importing_it_into_a_public_chat() {
    // The two-call bypass, end to end and in the ONE direction the previous
    // tests never ran: export PRIVATE, import PUBLIC. Before decision (2)'s
    // marker the imported base takes the importer's tier and every page of a
    // private base is readable by a public model, with no gate crossed —
    // `kb_export` is permitted (the base is the private caller's own) and
    // `kb_import` is permitted (the id it creates does not exist yet).
    let root = tempdir_with_bases(&["omop"]);
    ensure_migrated_unlocked(&root).unwrap();
    raise_unlocked(&root, "omop", true).unwrap();
    seed_page(&root, "omop", "knowledge/x.md", "SENTINEL-COHORT-N-412");

    let bytes = export_brkb_with_provenance(&root, "omop");     // private base
    let new_id = import_brkb_as(&root, &bytes, /* importer_is_private */ false);

    assert_ne!(new_id, "omop", "the collision loop must land on a fresh id");
    assert!(is_private(&root, &new_id),
            "a private base was laundered into a public one by export/import");
    // And the content really did arrive — otherwise the assertion above is
    // satisfied by an import that failed.
    assert!(page_body(&root, &new_id, "knowledge/x.md").contains("SENTINEL-COHORT-N-412"));
}

#[test]
fn the_provenance_marker_can_only_raise_and_a_foreign_archive_is_unaffected() {
    // Decision (2)'s whole safety argument, as three rows. The marker is
    // attacker-supplied by construction — it rides inside the zip — and is safe
    // ONLY because it is read as a floor.
    let root = tempdir_with_bases(&[]);
    ensure_migrated_unlocked(&root).unwrap();

    // (a) marker private + importer public  -> private   (the laundering case)
    assert!(is_private(&root, &import_brkb_as(&root, &archive("a", Some(true)), false)));
    // (b) marker PUBLIC + importer private  -> private   (a hostile archive
    //     claiming "public" cannot lower the importing session's own tier)
    assert!(is_private(&root, &import_brkb_as(&root, &archive("b", Some(false)), true)));
    // (c) NO marker (a foreign .brkb, or one written before this task) +
    //     importer public -> public: unknown means the importer's tier, which
    //     is today's behaviour and must not regress into "everything imported
    //     is private", a state with no declassification path (AR-1).
    assert!(!is_private(&root, &import_brkb_as(&root, &archive("c", None), false)));
    // (d) a malformed marker is read as absent, not as private — same reason.
    assert!(!is_private(&root, &import_brkb_as(&root, &archive_with_raw_marker("d", "yes"), false)));
}

#[test]
fn the_marker_rides_in_the_archive_and_never_lands_on_disk() {
    // It is written straight into the ZipWriter after `walk` (brkb.rs:16-17),
    // so the KB's git tree does not gain an untracked file — and it goes INSIDE
    // the single top-level directory, because `import` (:70-87) bails unless
    // there is exactly one and a sibling entry would break every archive.
    let root = tempdir_with_bases(&["omop"]);
    ensure_migrated_unlocked(&root).unwrap();
    raise_unlocked(&root, "omop", true).unwrap();
    let bytes = export_brkb_with_provenance(&root, "omop");
    assert!(zip_names(&bytes).contains(&"omop/.brkb-provenance".to_string()));
    assert!(!root.join("omop/.brkb-provenance").exists(), "the marker was written to disk");
    // …and it is not extracted back out into the imported base either.
    let new_id = import_brkb_as(&root, &bytes, false);
    assert!(!root.join(&new_id).join(".brkb-provenance").exists());
}

#[test]
fn the_refusal_names_no_base_and_no_page() {
    // One string serves CP1..CP4, so it is asserted once, here.
    let root = tempdir_with_bases(&["omop"]);
    ensure_migrated_unlocked(&root).unwrap();
    raise_unlocked(&root, "omop", true).unwrap();
    let s = assert_reachable(&root, "omop", false).unwrap_err().to_string();
    assert!(s.contains("private model"));
    assert!(!s.contains("omop"), "the refusal named the base: {s}");
}
```

```rust
// crates/biorouter-mcp/src/knowledge/service.rs, in its existing #[cfg(test)] mod tests

#[tokio::test]
async fn registering_a_tier_from_inside_the_root_lock_does_not_deadlock() {
    // The whole of decision (5b), as a test that TIMES OUT rather than fails if
    // the `_unlocked` convention is broken — a deadlock does not assert, it
    // waits. `create_base` holds `lock_root()` at :448; a `tier::raise` that
    // acquires it again blocks forever and the daemon stops answering on the
    // very first knowledge call, while every tier.rs unit test still passes
    // because they call the store on a bare root no service is holding.
    let d = tempfile::tempdir().unwrap();
    let svc = KnowledgeService::new(d.path().to_path_buf());
    let done = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            svc.create_base("k", "K", None)?;   // registers, inside the lock
            svc.raise_tier("k", true)?;         // the wrapper: takes the lock itself
            svc.delete_base("k")                // forgets, inside the lock
        }),
    )
    .await
    .expect("create_base / raise_tier / delete_base deadlocked on the root lock");
    done.unwrap().unwrap();
}

#[test]
fn a_base_created_by_any_surface_is_registered_public_rather_than_unknown() {
    // Decision (5a). Without the registration, decision (3) reads a freshly
    // created base as PRIVATE and Task 10C locks the user out of a base they
    // just made from the CLI or the Knowledge view.
    let d = tempfile::tempdir().unwrap();
    let svc = KnowledgeService::new(d.path().to_path_buf());
    svc.create_base("k", "K", None).unwrap();
    assert!(!crate::knowledge::tier::is_private(svc.root(), "k"));
}
```

```rust
// crates/biorouter/src/agents/extension_manager.rs, in its existing #[cfg(test)] mod tests
// (:1833 — NOT mcp_client.rs's `mod tests` at :891, which holds only
//  BioRouterClient helpers and none of the ExtensionManager fixtures.)
//
// ⚠ MEASURED by Task 4b, not counted by hand: `agents::extension_manager::tests`
// holds **33** tests, and the FILTER `agents::extension_manager` reports **37**,
// because libtest matches substrings and `agents::extension_manager_extension::tests`
// (4) contains it. An earlier draft said "27 tests" here. Assert 37 + N, not 27 + N.

#[tokio::test]
async fn a_third_party_extension_never_learns_the_capability_tier() {
    // Decision 4. The session id already goes everywhere; this does not.
    use biorouter_mcp::knowledge::tier::CAPABILITY_TIER_META_KEY as KEY;
    let em = manager_with(stdio_ext("some-third-party"), builtin_ext("knowledge")).await;
    bind_private_provider(&em).await;
    assert_eq!(meta_seen_by(&em, "some-third-party__ping").await.get(KEY), None);
    assert_eq!(meta_seen_by(&em, "knowledge__kb_list_bases").await
                   .get(KEY).and_then(|v| v.as_str()), Some("private"));
}
```

```rust
// crates/biorouter/src/agents/mcp_client.rs, in its existing #[cfg(test)] mod tests (:891)

#[test]
fn the_capability_tier_rides_the_same_meta_object_as_the_session_id() {
    let meta = McpMeta::new("sess-1").with_capability_private(true);
    let ext = meta.inject_into_extensions(Extensions::default());
    let m = ext.get::<Meta>().unwrap();
    assert_eq!(m.0.get("biorouter-session-id").and_then(|v| v.as_str()), Some("sess-1"));
    assert_eq!(m.0.get("biorouter-capability-tier").and_then(|v| v.as_str()), Some("private"));
}
```

- [ ] **Step 2: Run**

```bash
cargo test -p biorouter-mcp --lib knowledge::tier
cargo test -p biorouter-mcp --lib knowledge::service
cargo test -p biorouter --lib agents::mcp_client
cargo test -p biorouter --lib agents::extension_manager
```

Expected: **COMPILE ERROR** — `unresolved module tier` (`knowledge::tier` does not exist), `no method
named raise_tier`, and `no method named with_capability_private`. Not a FAIL: every one of these
tests names a symbol this task creates.

- [ ] **Step 3: Implement**

(a) `paths.rs`, beside the other four sidecar helpers:

```rust
/// Returns `<knowledge-root>/.kb-tiers` — the machine-local map of kb id →
/// privacy tier (issue #56).
///
/// Deliberately a sibling of `.active-kb` and `.hidden-kbs` rather than a field
/// in each base's `manifest.yaml`: the manifest is inside the base's git tree
/// and travels inside the `.brkb` archive, so a tier stored there would be
/// supplied by whoever authored the archive. This file never leaves the machine.
pub fn kb_tiers_path(root: &std::path::Path) -> std::path::PathBuf {
    root.join(".kb-tiers")
}
```

(b) `tier.rs` — the whole store, ~140 lines, no I/O outside `root`. **Every mutating function is
`_unlocked`**; nothing here takes a lock, because the only lock that would serve is
`KnowledgeService::lock_root` (`service.rs:427`) and it is private to that module — decision (5b):

```rust
//! The knowledge-base privacy tier (issue #56, design §9.3 B4).
//!
//! A knowledge base takes the tier of the most sensitive session that has
//! ingested into it. This module owns the store, the monotone raise and the
//! refusal; Task 10B calls `raise` from the four choke points and Task 10C
//! calls `assert_reachable` from the same four.
//!
//! A `bool` rather than an enum because `biorouter-mcp` cannot depend on
//! `biorouter`, where `ProviderTier` lives — see the task's decision (1).
//! `caller_is_private == true` is exactly `floor(Private) == Private`.
//!
//! ## Locking
//!
//! Every mutator here is `_unlocked` and takes NO lock. The knowledge root has
//! exactly one lock, `KnowledgeService::lock_root` (`service.rs:427`), and both
//! it and `FileLockGuard` (`:58`) are private to the `service` module — a free
//! function cannot reach them, and `create_base`/`import_brkb`/`delete_base`
//! are already inside it when they call these (`:448`, `:507`, `:658`). A
//! second `FileLockGuard::acquire` in the same process blocks forever
//! (`:63-78` opens a fresh fd and `flock`s exclusively). Callers OUTSIDE the
//! service use `KnowledgeService::raise_tier` / `forget_tier`, which take the
//! lock and delegate here. This is the tree's own `_unlocked` convention —
//! twenty helpers in `service.rs` already follow it.

const SCHEMA: u32 = 1;

/// The meta key the daemon writes the caller's capability into. Defined here,
/// not in `server.rs`, because `agent_drafter` (CP4) reads the same key and two
/// spellings of it is exactly how a barrier silently stops working.
pub const CAPABILITY_TIER_META_KEY: &str = "biorouter-capability-tier";

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct Store {
    schema: u32,
    /// kb id -> "public" | "private". An id ABSENT from a store that exists,
    /// for a directory that DOES exist, is unknown provenance and reads
    /// PRIVATE; the whole file being absent means the migration has not run.
    bases: std::collections::BTreeMap<String, String>,
}

/// The caller's capability, PUBLIC unless the meta says private.
///
/// Absent means one of: an older daemon, a non-built-in transport, or a direct
/// unit-test construction. All three are "unknown", and unknown must be the
/// restrictive answer for the reads Task 10C gates.
pub fn caller_is_private(meta: &rmcp::model::Meta) -> bool {
    meta.0.get(CAPABILITY_TIER_META_KEY).and_then(|v| v.as_str()) == Some("private")
}

/// One-time migration. Every base that exists when this first runs becomes
/// PUBLIC (fail-open, DR-10 and AR-2). Guarded by the file's absence, exactly
/// as `ensure_privacy_schema` is guarded by `table_has_column`: re-running it on
/// every startup would re-add a base whose entry a later `forget` removed, and
/// would race the ratchet.
pub fn ensure_migrated_unlocked(root: &std::path::Path) -> anyhow::Result<()> { … }

/// PRIVATE unless the store says otherwise — with one exception that is the
/// point of decision (3): a kb id with **no directory under `root`** is not
/// private, because there is nothing there to leak and refusing would ban a
/// public session from creating or importing a base.
///
/// Otherwise fail-closed on: no entry, an unparseable file, an unreadable file.
/// Each of those logs at `error!` and paints the base with a badge the user will
/// report on day one — the same trade `SessionClassification::from_stored`
/// makes for the same reason. Lock-free: the store is only ever replaced by
/// `rename`, so a reader sees the old file or the new one, never a torn one.
pub fn is_private(root: &std::path::Path, kb_id: &str) -> bool { … }

/// The single refusal for CP1..CP4. `Ok(())` permits.
pub fn assert_reachable(
    root: &std::path::Path,
    kb_id: &str,
    caller_is_private: bool,
) -> anyhow::Result<()> {
    if caller_is_private || !is_private(root, kb_id) {
        return Ok(());
    }
    anyhow::bail!(KB_PRIVATE_REFUSAL)
}

/// Monotone. Registers `kb_id` at the caller's tier if absent, raises it to
/// private if the caller is private, and can never lower it — the file-store
/// twin of the `privacy_tier = CASE WHEN` fragment in `session_manager.rs`.
pub fn raise_unlocked(
    root: &std::path::Path,
    kb_id: &str,
    caller_is_private: bool,
) -> anyhow::Result<()> { … }

/// Register `kb_id` as PUBLIC **only if it has no entry**. Called by
/// `create_base` and `import_brkb` (decision 5a): a base with no entry reads
/// private by decision (3), so an unregistered base would lock its own creator
/// out. Never lowers — see `register_public_never_lowers_an_already_private_base`.
pub fn register_public_if_absent_unlocked(
    root: &std::path::Path,
    kb_id: &str,
) -> anyhow::Result<()> { … }

/// Drop the entry when the base is deleted, so a later base reusing the id is
/// classified by its own creator rather than by a base that no longer exists.
pub fn forget_unlocked(root: &std::path::Path, kb_id: &str) -> anyhow::Result<()> { … }

/// Names no base, no page and no snippet. Constant, so a model that retries sees
/// the same string and stops rather than looping (the same rule Task 14's
/// `privacy_refusal` follows, and for the same reason).
pub const KB_PRIVATE_REFUSAL: &str = "\
This knowledge base is private: a session running an institutional or self-hosted model has \
ingested into it, so only a private model may read or write it. This session is running on a \
public model. Ask the user to switch this chat to a private model — Settings > Models, or the \
model chip in the composer — and try again. Do not retry with a different knowledge base id, \
through an export, or through a raw-source search; the boundary is the same everywhere.";
```

Write through the same tmp-then-`rename` idiom as `manifest::save` (`manifest.rs:17-24`).

⚠ **Known residual, stated rather than discovered.** `lock_root()` is in-process. Two Biorouter
processes (the desktop app and a terminal `biorouter`) raising two different bases at the same instant
can still lose one edit, and the lost edit could be a *raise*. This is the same read-modify-write
hazard `set_hidden_persisted`'s own doc comment already documents for `.hidden-kbs`, it predates this
work, and closing it needs an OS advisory lock the tree does not have anywhere. Do not silently widen
the scope to fix it; open a follow-up.

(c) `KnowledgeService` gains the three lock-taking wrappers and calls the `_unlocked` twins from
inside the three functions that already hold the lock:

```rust
    /// Take the root lock and raise `kb_id` to the caller's tier.
    ///
    /// For callers OUTSIDE this module. Inside it — `create_base` `:447`,
    /// `import_brkb` `:506`, `delete_base` `:657` — the lock is already held,
    /// so those call `tier::*_unlocked` directly. Calling this from there
    /// deadlocks (decision 5b).
    pub fn raise_tier(&self, kb_id: &str, caller_is_private: bool) -> anyhow::Result<()> {
        let _lock = self.lock_root()?;
        crate::knowledge::tier::raise_unlocked(&self.root, kb_id, caller_is_private)
    }

    pub fn forget_tier(&self, kb_id: &str) -> anyhow::Result<()> {
        let _lock = self.lock_root()?;
        crate::knowledge::tier::forget_unlocked(&self.root, kb_id)
    }

    /// Idempotent, and cheap on the common path: it stats `.kb-tiers` BEFORE
    /// taking the lock and returns immediately when it exists, so the ~90
    /// `KnowledgeService::new` calls in the test suite do not each `flock`.
    fn ensure_tiers_migrated(&self) -> anyhow::Result<()> {
        if crate::knowledge::paths::kb_tiers_path(&self.root).exists() {
            return Ok(());
        }
        if !self.root.exists() {
            return Ok(());   // no bases yet; the first create_base registers
        }
        let _lock = self.lock_root()?;
        crate::knowledge::tier::ensure_migrated_unlocked(&self.root)
    }
```

with `new` (`:404`) calling it best-effort — `new` returns `Self`, not `Result`, so it cannot `?`:

```rust
    pub fn new(root: PathBuf) -> Self {
        let svc = Self { root, locks: Arc::new(DashMap::new()) };
        // Issue #56. Best-effort: `new` is infallible and a failure here must
        // not stop the app from opening. A root that never migrates reads every
        // base PUBLIC (the file is absent ⇒ "not migrated"), which is AR-2's
        // accepted direction, not a new one.
        if let Err(e) = svc.ensure_tiers_migrated() {
            tracing::warn!("knowledge: could not migrate kb tiers: {e:#}");
        }
        svc
    }
```

and, **inside the existing `let _lock = self.lock_root()?;`**, one line each:

- `create_base` (after the base is on disk and committed, before `Ok(m)`):
  `crate::knowledge::tier::register_public_if_absent_unlocked(&self.root, id)?;`
- `import_brkb` (after `registry::register`, on `new_id`):
  `crate::knowledge::tier::register_public_if_absent_unlocked(&self.root, &new_id)?;`
- `delete_base` (beside the registry removal):
  `crate::knowledge::tier::forget_unlocked(&self.root, id)?;`

(d) `McpMeta` gains one optional field and one builder:

```rust
    /// Whether the model bound to this session is private (issue #56).
    ///
    /// `None` for every extension that is not a Biorouter built-in: the session
    /// id already goes to third-party MCP servers, and this deliberately does
    /// not follow that precedent — "this user is on an institutional model" is a
    /// fact about their configuration, not something a third-party server needs.
    /// A built-in receiving `None` reads it as PUBLIC, which is the safe
    /// direction for every gate that consumes it.
    pub capability_private: Option<bool>,
```

with `with_capability_private(bool)` beside `with_progress_token` (`:156-159`), writing
`"private"` / `"public"` under `biorouter_mcp::knowledge::tier::CAPABILITY_TIER_META_KEY` — the
const, **not** a second hand-typed copy of the string — into the **same** `Meta` object
`inject_into_extensions` already builds (`:161-172`), never `params.meta`, for the wire-collision
reason the existing comment at `:164-166` gives. The literal survives in exactly one test
(`the_capability_tier_rides_the_same_meta_object_as_the_session_id`), which is where pinning the
wire format is the point; everything else reads the const.

(e) In `dispatch_tool_call`, at the sole `McpMeta::new(&session_id)` (`:1557`):

```rust
            let mut meta = McpMeta::new(&session_id);
            if let Some(token) = progress_token {
                meta = meta.with_progress_token(token);
            }
            // Issue #56. Built-ins only — see (d). `caller_capability_for_builtin`
            // is computed OUTSIDE this future, because `capability_tier()` awaits
            // the provider mutex and this block owns no `&self`.
            if let Some(is_private) = caller_capability_for_builtin {
                meta = meta.with_capability_private(is_private);
            }
```

with `let caller_capability_for_builtin = if biorouter_mcp::BUILTIN_EXTENSIONS.contains_key(client_name.as_str())
{ Some(self.capability_tier().await.is_private()) } else { None };` resolved **before** the
`async move` block, beside the tier lookup Task 14 adds at the same seam.

(f) `KnowledgeServer` reads it through the shared helper, mirroring `session_id_from_context`
(`:222-228`) — note it *delegates* rather than re-implementing, so CP1 and CP4 cannot drift:

```rust
    fn caller_is_private(context: Option<&RequestContext<RoleServer>>) -> bool {
        context
            .map(|c| crate::knowledge::tier::caller_is_private(&c.meta))
            .unwrap_or(false)
    }
```

- [ ] **Step 4: Run**

```bash
cargo test -p biorouter-mcp --lib knowledge::tier
cargo test -p biorouter-mcp --lib knowledge::          # 190 today (MEASURED, Task 4b); assert 190 + 13
cargo test -p biorouter --lib agents::mcp_client
cargo test -p biorouter --lib agents::extension_manager
cargo test -p biorouter-server --test knowledge_routes # ~19 today; must be unchanged
cargo test -p biorouter-cli --lib commands::knowledge  # must be unchanged: no signature moved
```

Expected: **PASS**. `knowledge::` is the count that matters — this task adds a module to it, and the
per-module filter `knowledge::tier` proves the new tests are in the module the filter names rather
than somewhere that happens to compile. The last two lines are the evidence for decision (5a): if
`create_base`'s signature had changed, they would not compile.

⚠ **190, not "~122".** An earlier draft carried the figure from `CLAUDE.md`, which is stale.
[Task 4b](#task-4b-resolve-every-test-filter-against-a-real-cargo---list-docs-only) ran
`cargo test -p biorouter-mcp --lib -- --list` and measured **190** matching `knowledge::`, across 35
submodules (`knowledge::service::tests` alone is 38, `knowledge::store::tests` 14,
`knowledge::server::tests` 11). A `pre + 10` assertion built on 122 would have read a **68-test
shortfall** as a pass. The `+ 13` is 10 for the tier store and its two service tests, plus the
three archive-provenance tests decision (2) added.

- [ ] **Step 5: Gate**

```bash
cd /Users/wgu/Desktop/BioRouter-privacy
# The tier is not in the manifest, so it cannot ride a .brkb archive as an
# authority. NOT `grep -c "tier\|privacy"` — that is 8 today and can never be 0,
# because `CredibilityTier` has owned the word "tier" in this file since the
# credibility feature (types.rs:20, :93, + 6 in tests). Measured: 0 today.
grep -cE "privacy_tier|PrivacyTier|kb_tier" crates/biorouter-mcp/src/knowledge/types.rs ; echo "expect: 0"
# …and the thing that DOES ride the archive is read as a FLOOR, never as a
# value. This is the gate for decision (2)'s safety argument, and the previous
# version of it — "the tier does not travel" alone — is what left export-private
# / import-public open while passing.
grep -c "\.brkb-provenance" crates/biorouter-mcp/src/knowledge/brkb.rs ; echo "expect: 2 — written by export, read by import"
awk '/pub fn export</,/^}/' crates/biorouter-mcp/src/knowledge/brkb.rs \
  | grep -n "walk(\|brkb-provenance\|zip.finish" | head -3
echo "Expected, in this order: walk, .brkb-provenance, zip.finish — the entry is"
echo "  written into the ZipWriter, AFTER the disk walk and before finish, so it"
echo "  never exists as a file in the KB's git tree."
awk '/pub fn import</,/^}/' crates/biorouter-mcp/src/knowledge/brkb.rs \
  | grep -c "brkb-provenance" ; echo "expect: >= 2 — read it, and SKIP it when extracting"
# The raise is a max, in the service, on the new id. A plain `raise_unlocked(..,
# marker)` would LOWER a private importer's base to a public archive's claim.
awk '/pub fn import_brkb/,/^    }/' crates/biorouter-mcp/src/knowledge/service.rs \
  | grep -n "brkb::import\|register\|raise" | head -3
echo "Expected: brkb::import, register, then the raise — and the raise argument"
echo "  must be a disjunction of the marker and the importer, never the marker alone."
grep -n "provenance.*||\|||.*provenance" crates/biorouter-mcp/src/knowledge/service.rs
echo "expect: 1 — `marker || importer_is_private`. A bare `marker` is the (b) row of"
echo "  the_provenance_marker_can_only_raise_and_a_foreign_archive_is_unaffected."
# A MODEL's export of a private base cannot be aimed outside the deny root.
awk '/pub async fn kb_export/,/^    }/' crates/biorouter-mcp/src/knowledge/server.rs \
  | grep -n "is_private\|dest_path\|exports" | head -4
echo "Expected: the tier check BEFORE dest_path is honoured. The user's own export"
echo "  routes are untouched — assert that too, or the Knowledge view loses a feature:"
for h in export_brkb; do
  echo -n "routes/knowledge.rs $h: "
  awk "/pub async fn $h/,/^}/" crates/biorouter-server/src/routes/knowledge.rs | grep -c "exports"
done
echo "expect: 0 — the user is not a model (Task 10C's scope decision, same line)"
grep -rn "kb_tiers_path" crates/biorouter-mcp/src/knowledge/ | grep -v "fn kb_tiers_path"
echo "expect: tier.rs and service.rs::ensure_tiers_migrated only — nothing else opens the file"
# Migration is one-shot, not a per-startup repair. (Task 38 makes the identical
# distinction for sessions, and for the identical reason.) Written as "the guard
# mentions the store path at all", because `let p = kb_tiers_path(root); if
# p.exists()` is the natural spelling and an exact-string gate false-fails it.
awk '/fn ensure_tiers_migrated/,/^    }/' crates/biorouter-mcp/src/knowledge/service.rs \
  | grep -c "kb_tiers_path" ; echo "expect: 1 — the absence guard, before the lock"
# The lock discipline: tier.rs takes no lock, and the three in-service call
# sites use the _unlocked twins. This is the deadlock gate.
grep -c "lock_root\|FileLockGuard" crates/biorouter-mcp/src/knowledge/tier.rs ; echo "expect: 0"
for fn in create_base import_brkb delete_base; do
  echo -n "$fn: "
  awk "/pub fn $fn/,/^    }/" crates/biorouter-mcp/src/knowledge/service.rs \
    | grep -c "tier::.*_unlocked("
done
echo "expect: 1 each — never raise_tier/forget_tier, which re-acquire the lock they hold"
grep -c "pub fn raise_tier\|pub fn forget_tier" crates/biorouter-mcp/src/knowledge/service.rs
echo "expect: 2 — the wrappers for callers outside the module"
# The capability key is built-ins-only, and has exactly one spelling.
awk '/let caller_capability_for_builtin/,/;$/' crates/biorouter/src/agents/extension_manager.rs \
  | grep -c "BUILTIN_EXTENSIONS" ; echo "expect: 1"
grep -rl '"biorouter-capability-tier"' --include='*.rs' crates/ | sort
echo "expect: exactly 2 FILES — knowledge/tier.rs (the const) and agents/mcp_client.rs"
echo "        (the writer + its test). A third file has spelled the key by hand,"
echo "        which is how a barrier silently stops matching."
# Registration is in the SERVICE, so every surface gets it from one place, and
# create_base's ~90 callers were not touched (decision 5a).
grep -c "register_public_if_absent_unlocked(" crates/biorouter-mcp/src/knowledge/service.rs
echo "expect: 2 (create_base, import_brkb)"
git diff --stat HEAD -- crates/biorouter-cli crates/biorouter/src/knowledge/soul.rs \
  crates/biorouter-server/src/routes/reset.rs crates/biorouter-server/src/bin
echo "expect: empty — no create_base caller moved"
# And nothing ENFORCES anything yet: this task registers and migrates, nothing more.
grep -rn "tier::assert_reachable(\|assert_kb_reachable(" crates/ ; echo "expect: no output until Task 10C"
grep -rn "tier::raise_unlocked(\|raise_tier(" --include='*.rs' crates/ \
  | grep -v "knowledge/tier.rs\|knowledge/service.rs"
echo "expect: no output until Task 10B"
```

**What this catches.** Five wrong implementations. (1) Putting the tier on `Manifest` — the obvious
place, one field, no new file — which makes it travel inside `.brkb` and hands an importer authority
over the badge; the `types.rs` zero-count is the only cheap gate for it. (1b) Stopping there.
**This gate rejects: an implementation in which the tier does not travel and nothing else changes** —
which is what "a sidecar, not `manifest.yaml`" alone produces, and which leaves export-from-private
→ import-into-public copying every page of a private base into a public one in two permitted tool
calls. `a_private_export_cannot_be_laundered_by_importing_it_into_a_public_chat` is the only test
that fails it; Task 10B's import test as first written ran the
**private**-importer direction only and passed against the defect; it is now
`an_imported_base_takes_the_importing_sessions_tier_OR_THE_ARCHIVES_FLOOR` and carries both. It also rejects the two
plausible over-corrections, both in
`the_provenance_marker_can_only_raise_and_a_foreign_archive_is_unaffected`: reading the marker as a
*value* rather than a floor (row (b) — a hostile archive claiming "public" then lowers a private
importer's base), and treating an absent marker as private (row (c) — every foreign `.brkb` on the
internet imports private, into a state AR-1 says has no declassification path). (2) A migration that runs on
every startup "to pick up new bases", which silently lowers a base the day after a private session
raised it; test 1's second `ensure_migrated_unlocked` is what fails it, and no grep would. (3) A store
shaped like `.hidden-kbs` — a list of private ids — which cannot distinguish *known public* from
*unknown*, so a directory dropped into the knowledge root reads public; test 2 fails it. (4) Taking
the root lock inside `tier.rs`, which hangs the daemon on the first `kb_create_base` while every
`tier.rs` unit test still passes, because they call the store on a bare root that no service is
holding; `registering_a_tier_from_inside_the_root_lock_does_not_deadlock` is a *timeout*, not an
assertion, because a deadlock does not fail — it waits.

- [ ] **Step 6: Commit**

```bash
git add crates/biorouter-mcp/src/knowledge/tier.rs crates/biorouter-mcp/src/knowledge/mod.rs \
        crates/biorouter-mcp/src/knowledge/paths.rs crates/biorouter-mcp/src/knowledge/service.rs \
        crates/biorouter-mcp/src/knowledge/server.rs crates/biorouter/src/agents/mcp_client.rs \
        crates/biorouter/src/agents/extension_manager.rs
git commit -m "feat(knowledge): a per-knowledge-base privacy tier, its store and its migration (#56)"
```

---

### Task 10B: The knowledge-base ratchet — every write a model makes stamps the caller, at four choke points

The ratchet half of the ruling: *a KB takes the tier of the most sensitive session that has ingested
into it.* Nothing refuses anything yet; that is Task 10C. Both tasks hang off the **same four choke
points** (Task 10A's ⚠), which is why they are built in this order: 10B installs the capability at
each seam and proves it arrives, 10C then adds one line at each.

**Files:**

| Action | Path | Anchor (re-verified at `9558c346`) |
|---|---|---|
| Modify | `crates/biorouter-mcp/src/knowledge/server.rs` | **CP1.** Replace `#[tool_handler(router = self.tool_router)]` `:776` with a hand-written `call_tool` + `list_tools` in `impl ServerHandler for KnowledgeServer` `:777`. Plus a `RequestContext` on exactly **two** tools — `kb_create_base` `:357-368` and `kb_import` `:764-773` — whose subject id is not knowable before the call |
| Modify | `crates/biorouter-mcp/src/knowledge/macros/ingest.rs` | **CP2.** `IngestArgs` `:23`; `ingest` `:47`, whose first two statements are `svc.lock_kb` `:48` and `paths::kb_root` `:49` |
| Modify | `crates/biorouter-mcp/src/knowledge/macros/query.rs` | **CP2.** `QueryArgs` `:23`; `query` `:46`, lock `:47`, `kb_root` `:48` |
| Modify | `crates/biorouter-mcp/src/knowledge/macros/lint.rs` | **CP2.** `LintArgs` `:195`; `lint` `:217`, lock `:218`, `kb_root` `:219` |
| Modify | `crates/biorouter-server/src/routes/apps.rs` | **CP3.** `handle_kb_frame` `:2474-2481` gains `caller_is_private: bool`; its three call sites `:3288`, `:3513`, `:3847` — ⚠ **they do not all source it from the same agent**: `:3288` (the between-turns dispatch loop) and `:3513` (the queued/stray arm, which `continue`s at `:3522` before any turn starts) are the main agent's; `:3847` is inside the **turn** loop, after `turn_agent` is resolved at `:3541-3585`, and must use that. See the ⚠ "the mid-turn call has two agents in scope" below |
| Modify | `crates/biorouter-mcp/src/agent_drafter/mod.rs` | **CP4.** `stage_full_payload` `:1390-1394` gains `caller_is_private: bool`; its sole caller `export_app` `:2790` gains a `RequestContext` (`:2739-2742` has none today) |
| Modify | `crates/biorouter-server/src/routes/knowledge.rs` | the four macro routes pass the constructed provider's tier into the macro Args — `ingest` `:1122` (args at `:1142`), `ingest_conversation` `:1187` (args at `:1224`), `query_kb` `:1269` (args at `:1284`), `lint` `:1325` (args at `:1347`); `build_completer` `:899-914`, whose `TestModeCompleter` early return is `:903-907` |
| Modify | `crates/biorouter/src/knowledge/conversation_ingest.rs` | `ConversationIngestArgs` `:172-180` — **this task adds `caller_capability: ProviderTier` here**, not Task 11 (see ⚠ "the value at `:205`" below); the `IngestArgs` it builds at `:205` |
| Modify | `crates/biorouter-cli/src/commands/knowledge.rs` | `IngestArgs` `:457`, `ConversationIngestArgs` `:573`, `LintArgs` `:639`, `QueryArgs` `:718` |
| Modify | `crates/biorouter/src/agents/knowledge_tool.rs` | `ConversationIngestArgs` `:63` (the platform tool) |
| Modify | `crates/biorouter-server/src/bin/knowledge_ingest_probe.rs` | `IngestArgs` `:104` |
| Modify | `crates/biorouter-mcp/tests/knowledge_macros_e2e.rs` | **the integration test `--lib` cannot see.** `IngestArgs` `:115` and `:231`, `QueryArgs` `:157`. Measured: this is the **only** file outside `crates/*/src/` that constructs any of the three macro `Args` (`grep -rn "IngestArgs {\|QueryArgs {\|LintArgs {" --include='*.rs' crates/` → 25 hits, 22 of them under `src/`). Leaving it out is what made nine consecutive task commits fail `cargo test`: every `cargo test -p biorouter-mcp` in this plan is `--lib`, so nothing between here and Task 20's `cargo test --workspace` compiles it |
| Reference | `crates/biorouter-mcp/src/knowledge/subagent/kb_tools.rs` | `KbToolDispatch` `:22-30` — `pub kb_id: String`, fixed at construction (`ingest.rs:73`, `query.rs:75`, `lint.rs:257`). **Every** branch of `ToolDispatch::call` `:31-130` derives its path from `paths::kb_root(self.svc.root(), &self.kb_id)` `:33` or passes `&self.kb_id`, so the sub-agent cannot reach a second base and one check at the macro entry covers all five of its write tools |
| Reference | `crates/biorouter-server/src/routes/knowledge.rs` | the plain write routes that get **no** raise — `write_page` `:561` (which calls `store::write_page` directly, not a service method), `add_raw_source` `:1415`, `create_base` `:354`, `import_brkb` `:1552`, `restore_state` `:882` |

⚠ **Three exclusion lists, and none is an oversight.**

*Not ratcheted, because no content enters:* `kb_restore_state`, `kb_begin_txn`, `kb_commit_txn`,
`kb_abort_txn`. They move or discard content that is already in the base. Ratcheting on
`kb_abort_txn` — a *discard* — would let a session privatise a base by opening and abandoning a
transaction, a denial-of-service on the user's own knowledge base with no disclosure to justify it.
Task 10C's barrier still covers all four, and that is the control that matters for them.

*Not ratcheted, because no model is involved:* the plain `/knowledge/*` write routes, the CLI's
direct write commands, `soul.rs`, `reset.rs`. Those are the user typing in their own app — the same
scope line Task 10C draws for reads. There is no service-level write choke point to hang a raise on
anyway (`routes/knowledge.rs:571` calls `store::write_page` directly), so putting one there would
mean inventing one, and it would classify a base by *the user's own editing* rather than by what a
model saw. If a base needs privatising because of what the user pasted into it, that is a user action
and it wants a UI control, not a silent ratchet — [Open question 15](#open-questions).

*Ratcheted even though it is called "query":* **`macros::query::query` writes.** `QueryArgs` has
`file_as_page: bool` and the macro commits a page when it is set, but the deciding fact is harsher:
`tool_specs()` (`kb_tools.rs:224`) hands the sub-agent `kb_write_page`, `kb_append_log` **and**
`kb_add_raw_source` unconditionally, and the only thing between a `file_as_page: false` query and a
write is a sentence in the system prompt (`query.rs:70-71`: *"IMPORTANT: file_as_page is FALSE for
this call. Do NOT write any pages. Read-only."*).
A prompt is not a control. So `query` raises like the other two. The previous draft's "`query_kb`
reads; it gets Task 10C's barrier, not a raise" was wrong on the tree.

⚠ **The value at `conversation_ingest.rs:205`, and why `ConversationIngestArgs` gains its field
here rather than in Task 11.** Making `IngestArgs.caller_is_private` required makes `:205` — inside
`ingest_conversation` — a compile error, and that function has no capability of its own. The previous
draft reserved `caller_capability` for Task 11 and said nothing about what `:205` should pass in the
meantime. The only two things that compile at that point are a hardcoded `false` and a field this
task does not declare, and a hardcoded `false` **reproduces verbatim the failure this task's own ⚠(3)
says it fixed**: the platform tool, the CLI and `POST /ingest-conversation` would ratchet nothing
while `grep -c caller_is_private` reports non-zero in every file and Step 4 passes. So the field lands
here:

```rust
    /// The capability of whoever is asking (issue #56). Added by Task 10B
    /// because `ingest_conversation` must have something to put in
    /// `IngestArgs.caller_is_private`; **Task 11 adds the refusal that consumes
    /// it**, and the two are deliberately separate — this task plumbs, Task 11
    /// gates, exactly as 10B/10C split for the KB choke points.
    pub caller_capability: crate::privacy::ProviderTier,
```

Required and non-`Option`, so all three production constructors (`knowledge_tool.rs:63`,
`routes/knowledge.rs:1224`, `biorouter-cli/.../knowledge.rs:573`) are a compile error — the same
forcing function Task 11 was going to rely on, moved one task earlier. All three files are already in
this task's Files table and its `git add`. Task 11's Step 2 therefore expects **FAIL**, not COMPILE
ERROR, and says so.

⚠ **This task changes cross-crate signatures, so `--lib` is not enough to know the tree builds.**
`cargo test -p <pkg> --lib` compiles the lib target only; an integration test under `crates/*/tests/`
that constructs a changed struct is invisible to it and stays invisible until the next
`cargo test --workspace`, which is **Task 20**. That is how the previous draft left Tasks 10B through
19 — nine consecutive commits — with a red suite that a worker could not distinguish from a genuine
break. Step 4 therefore runs `cargo check --workspace --all-targets` **before** the per-crate filters,
and the same line appears in Tasks 10C, 10D and 11 for the same reason. It is the cheapest thing that
makes "each task's commit leaves the tree green" checkable rather than hoped.

- [ ] **Step 1: Write the failing tests**

```rust
// crates/biorouter-mcp/src/knowledge/server.rs, in its existing #[cfg(test)] mod tests (:805)

#[tokio::test]
async fn a_private_session_writing_one_page_ratchets_the_whole_base() {
    // THE test for the ruling, and the one that makes AR-1's cost visible in
    // CI: one page from one private chat privatises the machine-wide base.
    let (srv, root) = server_at_migrated_root(&["default"]);
    call_tool_as(&srv, "kb_write_page",
        json!({ "kb_id": "default", "path": "knowledge/omop.md",
                "content": "n=412 T2D patients", "commit_message": "x" }),
        Private).await.unwrap();
    assert!(tier::is_private(&root, "default"));
}

#[tokio::test]
async fn a_public_session_writing_never_lowers_a_ratcheted_base() {
    let (srv, root) = server_at_migrated_root(&["default"]);
    tier::raise_unlocked(&root, "default", true).unwrap();
    // Task 10C has not landed, so this write still SUCCEEDS. What must not
    // happen is the tier moving.
    call_tool_as(&srv, "kb_append_log",
        json!({ "kb_id": "default", "kind": "manual", "summary": "hi" }),
        Public).await.unwrap();
    assert!(tier::is_private(&root, "default"), "a public write lowered the tier");
}

#[tokio::test]
async fn every_tool_that_writes_content_ratchets_and_the_plumbing_ones_do_not() {
    // Parameterised over ALL NINETEEN tools, driven through call_tool BY NAME —
    // which is the point of CP1: nine of them take no RequestContext, so a test
    // that calls the `#[tool]` fn directly cannot express "as a private caller"
    // for them at all. A test on kb_write_page alone passes an implementation
    // that misses kb_add_raw_source, the tool the GUI ingest panel and the
    // `ingest` macro actually call, so the whole ingest path would launder.
    for probe in KB_TOOLS {                 // all 19, each with valid arguments
        let (srv, root) = server_at_migrated_root(&["default"]);
        let _ = call_tool_as(&srv, probe.name, probe.args_for("default"), Private).await;
        assert_eq!(tier::is_private(&root, "default"), probe.ratchets,
                   "{} ratchets={} but the store says otherwise", probe.name, probe.ratchets);
    }
    // The exclusion list as data, reviewable in one place:
    //   ratchets "default":       kb_write_page, kb_add_raw_source, kb_append_log
    //   ratchets its OWN new id:  kb_create_base, kb_import
    //   does not ratchet:         the other 14
}

#[tokio::test]
async fn a_base_created_from_a_private_chat_is_born_private() {
    let (srv, root) = server_at_migrated_root(&["default"]);
    call_tool_as(&srv, "kb_create_base", json!({ "id": "omop", "name": "OMOP" }), Private)
        .await.unwrap();
    assert!(tier::is_private(&root, "omop"));
    assert!(!tier::is_private(&root, "default"), "creating one base moved another");
}

#[tokio::test]
async fn a_public_chat_can_still_create_and_import_a_knowledge_base() {
    // The regression the sixteen-site enumeration encoded, as a test. A public
    // session must be able to make its own base; `assert_reachable` permits a
    // kb id with no directory on disk (Task 10A, decision 3).
    let (srv, root) = server_at_migrated_root(&["default"]);
    call_tool_as(&srv, "kb_create_base", json!({ "id": "notes", "name": "Notes" }), Public)
        .await.unwrap();
    assert!(!tier::is_private(&root, "notes"));
    call_tool_as(&srv, "kb_import", json!({ "src_path": brkb_fixture() }), Public)
        .await.unwrap();
}

#[tokio::test]
async fn an_imported_base_takes_the_importing_sessions_tier_OR_THE_ARCHIVES_FLOOR() {
    // `brkb::import` resolves collisions by suffixing, so an import always
    // lands on a FRESH id — which is what makes stamping after the call safe.
    let (srv, root) = server_at_migrated_root(&["default"]);
    let out = call_tool_as(&srv, "kb_import", json!({ "src_path": brkb_fixture() }), Private)
        .await.unwrap();
    assert!(tier::is_private(&root, imported_kb_id(&out)));

    // ⚠ The line above is the SAFE direction and, on its own, it is what let
    // export-private / import-public through a whole review round: a private
    // importer privatising what it imports proves nothing about a public one.
    // The unsafe direction is Task 10A's
    // `a_private_export_cannot_be_laundered_by_importing_it_into_a_public_chat`;
    // this is its tool-level twin, so the bypass is closed at the surface a
    // model actually calls and not only in the store.
    let out = call_tool_as(&srv, "kb_import",
                           json!({ "src_path": private_brkb_fixture() }), Public).await.unwrap();
    assert!(tier::is_private(&root, imported_kb_id(&out)),
            "a public chat imported a private base's archive and got a public base");
}
```

```rust
// crates/biorouter-server/tests/knowledge_routes.rs — the CALLER PROVENANCE
// MATRIX for the four HTTP macro routes.

#[tokio::test]
async fn each_macro_route_ratchets_from_the_provider_it_constructed_both_ways() {
    // The gate a `grep -c caller_is_private` cannot be: every route reports
    // NON-ZERO whether it passes the right value, a hardcoded `true`, or a
    // hardcoded `false`. Both rows, per route — the PUBLIC row is the one the
    // previous gate could not fail, and under-ratcheting is the direction that
    // launders (a private transcript into a base that stays public).
    for (route, args) in MACRO_ROUTES {          // ingest, ingest-conversation,
        for caller_is_private in [true, false] { //   query, lint
            let root = migrated_root_with_public_base("kb");
            let model = if caller_is_private { private_model_ref() } else { public_model_ref() };
            post_macro(&root, route, "kb", args(), model).await;
            assert_eq!(tier::is_private(&root, "kb"), caller_is_private,
                       "{route} with a {} model", if caller_is_private {"private"} else {"public"});
        }
    }
}

#[tokio::test]
async fn a_macro_route_ratchets_from_the_CONSTRUCTED_provider_not_the_requested_name() {
    // The other half of provenance, and the reason `build_completer` returns the
    // tier alongside the completer: `providers::create` can hand back something
    // else (`factory.rs:142-146`), and BIOROUTER_LEAD_MODEL is the live intercept.
    let root = migrated_root_with_public_base("kb");
    let _guard = lead_model_intercept_to(private_model_ref());
    post_macro(&root, "ingest", "kb", ingest_args(), public_model_ref()).await;
    assert!(tier::is_private(&root, "kb"),
            "the ratchet keyed on body.model.provider, not on the instance");
}
```

```rust
// crates/biorouter/src/agents/knowledge_tool.rs, in its #[cfg(test)] mod tests

#[tokio::test]
async fn the_platform_ingest_tool_ratchets_from_the_agents_own_provider_both_ways() {
    // Production caller #1 of ConversationIngestArgs. Same two rows.
    for caller_is_private in [true, false] {
        let agent = agent_with_messages_on(if caller_is_private { private_provider() }
                                           else { public_provider() }, "notes").await;
        let root = agent.kb_root();
        agent.handle_ingest_conversation(json!({ "kb_id": "default" })).await.unwrap();
        assert_eq!(tier::is_private(&root, "default"), caller_is_private);
    }
}
```

⚠ **Which production callers get a behavioural row, and which do not — stated, not implied.** Seven
production callers carry the capability after this task. Six are reachable from a test harness and
every one of them gets both rows: the four HTTP macro routes (above), the platform tool (above), CP1
(`every_tool_that_writes_content_ratchets_and_the_plumbing_ones_do_not`, which drives all nineteen
tools as `Private` and the ratchet-list assertion pins the `Public` direction), CP3
(`a_br_kb_ingest_from_a_private_app_session_ratchets_the_base` plus the mid-turn pair), and CP4
(Task 10C's export test). The two that no harness in this repo reaches are the **CLI**
(`biorouter-cli/src/commands/knowledge.rs`, whose 9 `--lib` tests do not construct a provider) and
the **probe binary** (`bin/knowledge_ingest_probe.rs`, which is not a test target at all). For those
two the gate is structural — Step 5 (i) forbids a literal in either direction and (ii) prints the
expression for a reviewer to trace to the session's bound provider — and that is a **weaker** gate,
so it is written down here rather than left to be discovered. It is also the smaller risk: both are
the user at a terminal, where the capability is the session's own provider and there is no second
agent to source it from by mistake.

```rust
// crates/biorouter-mcp/src/knowledge/macros/ingest.rs, in its #[cfg(test)] mod tests (:136)

#[tokio::test]
async fn the_ingest_macro_ratchets_before_its_sub_agent_runs() {
    // CP2, and the reason it exists. The sub-agent writes through
    // KbToolDispatch → store::write_page / svc.add_raw_source, which no MCP
    // tool gate can see. This is also the test that makes Task 11's headline
    // test reachable: `conversation_ingest::ingest_conversation` (:184) funnels
    // into this function, as do the four HTTP macro routes, the CLI and the probe.
    let (svc, root) = migrated_service_with_base("k");
    let args = IngestArgs { kb_id: "k".into(), caller_is_private: true,
                            completer: refuses_immediately(), ..fixture() };
    let _ = ingest(&svc, args).await;      // the sub-agent may fail; the raise stands
    assert!(tier::is_private(&root, "k"), "the raise ran after the sub-agent, or not at all");
}
```

```rust
// crates/biorouter-server/src/routes/apps.rs, in its existing `mod tests`

#[tokio::test]
async fn a_br_kb_ingest_from_a_private_app_session_ratchets_the_base() {
    // CP3. `run_kb_read` and this arm never touch KnowledgeServer, so CP1 is
    // blind to them; `resolve_kb_grant` (:2268) is an integrity control over a
    // manifest the DRAFTING MODEL authored, not a privacy control.
    let (state, root) = app_state_with_kb("kbx");
    handle_kb_frame(&bridge, &state.knowledge_service, Some(&cfg_granting("kbx", /*write*/ true)),
                    /* caller_is_private */ true, "ingest",
                    &json!({ "kb_id": "kbx", "text": "n=412" }), "r1").await;
    await_kb_result(&bridge).await;
    assert!(tier::is_private(&root, "kbx"));
}

#[tokio::test]
async fn a_mid_turn_br_kb_ingest_ratchets_from_the_TURN_agent_not_the_main_one() {
    // CP3's third call site (:3847), and the reason it is not `agent`. Driven
    // through the SOCKET, because the defect is which value the route reads —
    // calling `handle_kb_frame(.., true, ..)` directly would prove the parameter
    // works and say nothing about the bug.
    //
    // Public main + PRIVATE worker. The frame arrives while the worker's turn is
    // running, so the ingest is the worker's and the base must end private. Read
    // from `agent`, it ends public and a private worker has just laundered its
    // own output into a base every public chat can read.
    let app = app_with_worker_profile("analyst", /* main */ Public, /* worker */ Private).await;
    tier::raise_unlocked(&app.kb_root, "kbx", false).unwrap();
    app.start_turn_on_profile("analyst").await;             // turn_agent = the worker
    app.send_kb_frame_mid_turn("ingest", json!({ "kb_id": "kbx", "text": "n=412" })).await;
    await_kb_result(&app.bridge).await;
    assert!(tier::is_private(&app.kb_root, "kbx"),
            "the mid-turn ingest was attributed to the main agent");

    // And the mirror, which the same wrong line also breaks: PRIVATE main +
    // public worker must NOT ratchet a base to private on the worker's behalf.
    // Stated as a ratchet assertion here; Task 10C asserts the read half, which
    // is where this direction actually leaks.
    let app = app_with_worker_profile("analyst", /* main */ Private, /* worker */ Public).await;
    tier::raise_unlocked(&app.kb_root, "kby", false).unwrap();
    app.start_turn_on_profile("analyst").await;
    app.send_kb_frame_mid_turn("ingest", json!({ "kb_id": "kby", "text": "public note" })).await;
    await_kb_result(&app.bridge).await;
    assert!(!tier::is_private(&app.kb_root, "kby"),
            "a public worker's ingest was stamped with the main agent's private tier");
}

#[tokio::test]
async fn the_two_between_turn_kb_frames_still_read_the_main_agent() {
    // The over-correction: "use turn_agent" applied to all three sites. `:3288`
    // and `:3513` run where no turn exists — `:3513` `continue`s at `:3522`,
    // BEFORE turn_agent is resolved at `:3541` — so there is nothing else to
    // read, and a between-turns ingest is the main agent's by definition.
    let app = app_with_worker_profile("analyst", /* main */ Private, /* worker */ Public).await;
    app.send_kb_frame_between_turns("ingest", json!({ "kb_id": "kbz", "text": "n=7" })).await;
    await_kb_result(&app.bridge).await;
    assert!(tier::is_private(&app.kb_root, "kbz"));
}
```

- [ ] **Step 2: Run**

```bash
cargo test -p biorouter-mcp --lib knowledge::server
cargo test -p biorouter-mcp --lib knowledge::macros
cargo test -p biorouter-server --lib routes::apps
```

Expected: **COMPILE ERROR** first — `IngestArgs` has no field `caller_is_private`, `handle_kb_frame`
takes 6 arguments not 7, and the `call_tool_as` helper cannot be written until `call_tool` is
hand-written (the generated one is reachable only through the `ServerHandler` trait, which the test
must therefore import). Then, once those compile, **FAIL** on every ratchet assertion.

- [ ] **Step 3: Implement**

**CP1 — `KnowledgeServer::call_tool`.** Delete the `#[tool_handler(router = self.tool_router)]`
attribute at `:776` and write both methods it generated. The `list_tools` body is copied verbatim
from `rmcp-macros-0.14.0/src/tool_handler.rs:45-57`; only `call_tool` gains anything:

```rust
/// Tools whose `kb_id` argument names a base the caller must be allowed to
/// reach. One list, one rule — so a twentieth `kb_*` tool is gated the day it
/// is written, and opting out means editing a list this task's test enumerates.
const KB_ID_GATED_TOOLS: &[&str] = &[
    "kb_list_pages", "kb_read_page", "kb_get_graph", "kb_list_history",
    "kb_search", "kb_search_raw_sources", "kb_export",
    "kb_write_page", "kb_add_raw_source", "kb_append_log",
    "kb_restore_state", "kb_begin_txn", "kb_commit_txn", "kb_abort_txn",
];

/// The subset that resolves an omitted `kb_id` to the session's primary
/// (`kb_id_or_primary`, :312). For these an ABSENT id must be resolved and
/// checked too, or "just drop the kb_id" is the bypass.
const KB_PRIMARY_RESOLVING_TOOLS: &[&str] =
    &["kb_list_pages", "kb_read_page", "kb_get_graph", "kb_list_history"];

/// Content-bearing writes by a model: the base takes the caller's tier BEFORE
/// the write runs.
const KB_RATCHETING_TOOLS: &[&str] = &["kb_write_page", "kb_add_raw_source", "kb_append_log"];
```

```rust
impl ServerHandler for KnowledgeServer {
    fn get_info(&self) -> ServerInfo { /* unchanged */ }

    async fn list_tools(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::ListToolsResult, ErrorData> {
        Ok(rmcp::model::ListToolsResult {
            tools: self.tool_router.list_all(),
            meta: None,
            next_cursor: None,
        })
    }

    /// Issue #56, design §9.3 B4 as ruled. ONE seam for all nineteen `kb_*`
    /// tools, including the nine that take no `RequestContext` and therefore
    /// cannot learn the caller's capability inside their own body.
    ///
    /// This is `#[tool_handler]`'s generated body plus the gate:
    /// `rmcp-macros-0.14.0/src/tool_handler.rs:29-37` is exactly the last two
    /// statements. Re-check that file when bumping rmcp.
    async fn call_tool(
        &self,
        request: rmcp::model::CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::CallToolResult, ErrorData> {
        let caller_private = Self::caller_is_private(Some(&context));
        let name = request.name.to_string();

        if let Some(kb_id) =
            self.gated_kb_id(&name, request.arguments.as_ref(), Some(&context))?
        {
            // Task 10C adds `self.assert_kb_reachable(&kb_id, caller_private)?;`
            // HERE, on the line above the raise.
            if KB_RATCHETING_TOOLS.contains(&name.as_str()) {
                // BEFORE the write: a raise that only lands on success leaves
                // content in a base whose tier never moved if the write panics
                // or the process dies mid-commit. The failure direction of an
                // over-raise is a badge the user can see; the failure direction
                // of an under-raise is silent.
                self.service.raise_tier(&kb_id, caller_private).map_err(into_err)?;
            }
        }

        let tcc = rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
        self.tool_router.call(tcc).await
    }
}
```

and, on `KnowledgeServer`:

```rust
    /// The base this call names, or `None` when it names none.
    fn gated_kb_id(
        &self,
        tool: &str,
        args: Option<&rmcp::model::JsonObject>,
        context: Option<&RequestContext<RoleServer>>,
    ) -> Result<Option<String>, ErrorData> {
        if !KB_ID_GATED_TOOLS.contains(&tool) {
            return Ok(None);
        }
        if let Some(id) = args
            .and_then(|a| a.get("kb_id"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return Ok(Some(id.to_string()));
        }
        if !KB_PRIMARY_RESOLVING_TOOLS.contains(&tool) {
            // `kb_search` / `kb_search_raw_sources` with no kb_id fan out over
            // the visible set and filter per base (`search_visible_bases`,
            // :258-286) — Task 10C's fan-out test is the all-or-nothing bug
            // this avoids. `kb_export` and the writes REQUIRE kb_id, so an
            // absent one is the tool's own 400 and not ours to pre-empt.
            return Ok(None);
        }
        // Resolve exactly as the tool will (`kb_id_or_primary`, :312), so
        // omitting the kb_id is not the bypass. Its error case — no id and no
        // primary — is the tool's own message and must NOT become a privacy
        // refusal, so `None` falls through and the tool answers.
        self.primary_kb_for_context(context)
    }
```

**Two `#[tool]` signature changes, and exactly two.** `kb_create_base` (`:357`) and `kb_import`
(`:764`) each gain `context: RequestContext<RoleServer>` and raise *after* their call succeeds,
because their subject id does not exist beforehand — `kb_create_base`'s base is not on disk, and
`kb_import`'s id is chosen by `brkb::import`'s collision loop and is only known from its return
value. The "raise before the write" rule does not apply: a *create* that fails leaves no content at
all, so there is nothing to strand in an under-tiered base.

```rust
        let m = self.service.create_base(&p.id, &p.name, p.color.as_deref()).map_err(into_err)?;
        // Issue #56. AFTER: the base did not exist to be stamped before, and an
        // entry for a base that failed to create would block the id forever.
        self.service
            .raise_tier(&p.id, Self::caller_is_private(Some(&context)))
            .map_err(into_err)?;
```

The other seven context-less tools are untouched — CP1 already carries the capability for them.

**CP2 — the three macros.** Each `Args` struct gains a required field:

```rust
    /// The capability of the model this macro will run (issue #56). Required,
    /// so all four production callers are a compile error rather than an
    /// omission. A `bool` and not `ProviderTier` for the crate-dependency
    /// reason in Task 10A ⚠(1).
    pub caller_is_private: bool,
```

and each entry raises immediately after its existing `lock_kb`, before anything reads or writes:

```rust
pub async fn ingest(svc: &KnowledgeService, args: IngestArgs) -> Result<IngestResult> {
    let _lock = svc.lock_kb(&args.kb_id).await?;
    // Issue #56. The ratchet for EVERY sub-agent macro, because `KbToolDispatch`
    // (subagent/kb_tools.rs:22-30) is bound to this one `kb_id` and reaches
    // `store::*` directly — there is no lower seam, and no MCP gate can see it.
    // Before the sub-agent, not after: a run that fails halfway has already
    // written pages. Task 10C adds `tier::assert_reachable(..)` on the line above.
    svc.raise_tier(&args.kb_id, args.caller_is_private)?;
    let kb_root = paths::kb_root(svc.root(), &args.kb_id);
```

Identically in `query` (`:46-48`) and `lint` (`:217-219`).

`ConversationIngestArgs` (`conversation_ingest.rs:172`) gains `caller_capability: ProviderTier` per
the ⚠ above, and `:205` becomes one line:

```rust
        IngestArgs {
            kb_id: args.kb_id,
            // Issue #56. The ProviderTier -> bool crossing, and the only one:
            // `IngestArgs` lives in biorouter-mcp, which cannot name ProviderTier
            // (Task 10A ⚠(1)). Task 11 adds the refusal that reads the same field.
            caller_is_private: args.caller_capability.is_private(),
```

The three constructors then each pass their own: the platform tool from
`self.provider().await.map(|p| p.tier()).unwrap_or(ProviderTier::Public)`, the HTTP route from the
provider `build_completer` constructed, the CLI from the session's bound provider. **None of them may
hardcode `ProviderTier::Private`** — Step 5 greps for exactly that, because it is the plausible way to
make a caller compile and it reads as "this caller is trusted".

The four HTTP macro routes then pass the **constructed provider's** tier — not the requested model
name, because `providers::create` can hand back something else (`factory.rs:142-146`).
`build_completer` (`:899-914`) already constructs it; return the tier alongside the completer rather
than re-deriving it. ⚠ `build_completer` early-returns a `TestModeCompleter` at `:903-907` **before**
any provider exists: that branch returns **`false`** (public), because a test-mode completer reaches
no network and the fail-safe direction for a *ratchet* is not to privatise a base on a test path.

**CP3 — `handle_kb_frame`.** Add `caller_is_private: bool` between `cfg` and `op`, and raise inside
the `ingest` arm before the spawn:

```rust
        "ingest" => {
            if !kb_write_granted(cfg, &kb_id) { /* unchanged */ }
            // Issue #56. `resolve_kb_grant` (:2268) reads the app manifest,
            // which the drafting model authored (`agent_drafter/mod.rs:1731`
            // instructs it to) — an integrity control, not a privacy one.
            if let Err(e) = knowledge.raise_tier(&kb_id, caller_is_private) {
                emit_kb_error(ui_bridge, req_id, &e.to_string());
                return;
            }
```

⚠ **The mid-turn call has two agents in scope, and the wrong one compiles.** All three sites can
reach `agent` — the app's **main** agent — and two of them should. The third must not:

| Site | Where | Whose capability |
|---|---|---|
| `:3288` | the between-turns dispatch loop | `agent` — no turn is running, so there is no other agent |
| `:3513` | the queued/stray `ClientFrame::Kb` arm, which `continue`s at `:3522` | `agent` — it returns to the top of the loop **before** `turn_agent` is resolved at `:3541` |
| `:3847` | the **mid-turn** reader, inside the turn loop | **`turn_agent`** — resolved at `:3541-3585`, and `(h.agent.clone(), h.session_id.clone())` at `:3584` when the turn names a worker profile |

A worker really can be on a different provider: `configure_worker_provider` (`:1480-1503`) builds one
from the profile's own `cfg.model` and calls `agent.update_provider` on the worker's own session, and
`configure_worker_agent` (`:1556-1564`) grants that profile its own knowledge base. So
`turn_agent.provider()` and `agent.provider()` are genuinely different objects with genuinely
different tiers, and both are in scope at `:3847` — which is why passing `agent` there compiles,
type-checks, passes every single-agent test, and is wrong in **both** directions: a private main
agent laundering a **public** worker's ingest into a private base (10B), and a public main agent
letting a private worker's read of a private base be refused, or its ingest fail to ratchet (10C).
The precedent for reading `turn_agent` at exactly this depth is four cases down the same `match`:
`handle_action_required` takes `&turn_agent` at `:3944` under the comment *"Uses THIS turn's
agent/session (main or worker)"* (`:3939`). `turn_agent` is an owned `Arc`, so a second shared borrow
inside the loop costs nothing.

Each site therefore passes
`<that site's agent>.provider().await.map(|p| p.tier()).unwrap_or(ProviderTier::Public).is_private()`
(`Agent::provider` is `agent.rs:2511`; `Provider::tier` is Task 5). A dead or unbound provider
resolves to **Public**, the same direction `ExtensionManager::capability_tier` takes for the same
reason.

⚠ `ClientFrame::ModelStatus` sits beside every one of the three and reports `agent` at all three
(`:3299`, `:3525`, `:3858`). Leave it alone — it is the *app's* status card, not a capability
decision — but do not read it as the local convention for which agent to use.

**CP4 — `stage_full_payload`.** Add `caller_is_private: bool` as its fourth parameter and give
`export_app` (`:2739`) a `RequestContext<RoleServer>` to source it, using the shared reader:
`crate::knowledge::tier::caller_is_private(&context.meta)`. Task 10C adds the check; 10B only plumbs
the value, so the diff a reviewer reads at 10C is one `if`.

- [ ] **Step 4: Run**

```bash
# FIRST, and not optional: --lib does not compile crates/*/tests/, and this task
# changes three struct signatures that an integration test constructs.
cargo check --workspace --all-targets
# Pre-counts are MEASURED (Task 4b), so these assert `pre + N`, not "non-zero".
cargo test -p biorouter-mcp --lib knowledge::   2>&1 | grep "test result:"  # 190 + 4 (server) + 1 (ingest)
cargo test -p biorouter-mcp --lib agent_drafter:: 2>&1 | grep "test result:"  # 244, unchanged: 10B only plumbs CP4
cargo test -p biorouter-mcp --test knowledge_macros_e2e
cargo test -p biorouter-server --test knowledge_routes
cargo test -p biorouter-server --lib routes::apps 2>&1 | grep "test result:"  # 90 + 1
cargo test -p biorouter --lib knowledge::conversation_ingest 2>&1 | grep "test result:"  # 2, unchanged
cargo test -p biorouter-cli --lib commands::knowledge 2>&1 | grep "test result:"  # 9, unchanged
```

Expected: **PASS**, and `cargo check --workspace --all-targets` clean. The CLI line is not
decoration — it is the only crate that constructs all four Args types and never goes near an MCP
server, so it is the evidence that the required field reached every caller rather than only the ones
with tests. The `--test knowledge_macros_e2e` line is the one that used to be missing: it is the sole
out-of-lib constructor of `IngestArgs`/`QueryArgs`, and without it this commit and the eight after it
leave `cargo test` red.

- [ ] **Step 5: Gate**

```bash
cd /Users/wgu/Desktop/BioRouter-privacy
# CP1 exists, and the generated handler is gone. Both halves matter: leaving the
# attribute in place is a duplicate-definition compile error, but leaving a
# hand-written call_tool that forwards WITHOUT the gate compiles fine.
grep -c "tool_handler" crates/biorouter-mcp/src/knowledge/server.rs ; echo "expect: 0 (the import too)"
# ⚠ Assert the awk range is NON-EMPTY before reading the ordering below it.
# `async fn call_tool` does not exist in this file today — the macro generates
# it — so the range is 0 lines and every `grep` over it emits nothing. "No
# output" is not "the order is right"; it is "there is no function".
awk '/async fn call_tool/,/^    }/' crates/biorouter-mcp/src/knowledge/server.rs | wc -l
echo "expect: > 1 (0 today, before CP1 is hand-written)"
awk '/async fn call_tool/,/^    }/' crates/biorouter-mcp/src/knowledge/server.rs \
  | grep -n "gated_kb_id\|raise_tier\|tool_router.call" | head -3
# Expected, in this order: gated_kb_id, raise_tier, tool_router.call — the gate
# runs BEFORE the router, or it is not a gate. THREE lines, not fewer.
# Exactly two tools gained a RequestContext; the other seven did not.
for fn in kb_create_base kb_import; do
  echo -n "$fn: " ; awk "/pub async fn $fn/,/\) -> Result/" \
    crates/biorouter-mcp/src/knowledge/server.rs | grep -c "RequestContext"
done ; echo "expect: 1 each"
for fn in kb_write_page kb_add_raw_source kb_append_log kb_restore_state \
          kb_begin_txn kb_commit_txn kb_abort_txn kb_export; do
  echo -n "$fn: " ; awk "/pub async fn $fn/,/\) -> Result/" \
    crates/biorouter-mcp/src/knowledge/server.rs | grep -c "RequestContext"
done
echo "expect: 0 each — CP1 carries the capability for them; touching these is the old design"
# Five raise sites in total, as per-file counts rather than one repo grep — a
# repo-wide number would not say which surface lost its raise.
grep -c "raise_tier(" crates/biorouter-mcp/src/knowledge/server.rs
echo "expect: 3 — CP1's ratcheting branch, plus kb_create_base and kb_import"
grep -c "raise_tier(" crates/biorouter-mcp/src/knowledge/macros/ingest.rs \
                      crates/biorouter-mcp/src/knowledge/macros/query.rs \
                      crates/biorouter-mcp/src/knowledge/macros/lint.rs
echo "expect: 1 each — CP2"
grep -c "raise_tier(" crates/biorouter-server/src/routes/apps.rs ; echo "expect: 1 — CP3's ingest arm"
# CP3's THREE call sites do not all read the same agent, and the wrong one
# compiles because both are in scope at the mid-turn site. Print each call's
# capability expression with its line number and compare against :3541, where
# `turn_agent` is resolved: the two BEFORE it are the main agent's, the one
# AFTER it is the turn's. A total ("3 sites pass a capability") is satisfied by
# three copies of the wrong expression.
grep -n "handle_kb_frame(" crates/biorouter-server/src/routes/apps.rs \
  | grep -v "async fn handle_kb_frame"
echo "expect: 3 call sites — two below :3541 (the turn_agent binding) and one above it"
grep -n "turn_agent.provider()\|agent.provider()" crates/biorouter-server/src/routes/apps.rs
echo "expect: the CP3 expression at the MID-TURN site (~:3847) reads turn_agent;"
echo "  the two between-turns ones read agent. A `turn_agent.provider()` with a"
echo "  line number BELOW :3541 does not compile (it is not yet bound), so the"
echo "  only wrong implementation this can miss is 'agent everywhere' — which is"
echo "  exactly what the grep shows: zero turn_agent.provider() hits."
grep -c "turn_agent.provider()" crates/biorouter-server/src/routes/apps.rs
echo "expect: 1 — zero means the mid-turn site was attributed to the main agent"
# The raise precedes the sub-agent in all three macros. Anchored on `SubAgent`
# rather than on a write call, because the macro's first write is inside
# KbToolDispatch, one file over, and would not appear in this range at all.
for f in ingest query lint; do
  echo -n "$f: "
  grep -n "raise_tier(\|SubAgent {" crates/biorouter-mcp/src/knowledge/macros/$f.rs | head -2
done
# Expected for each: raise_tier on the SMALLER line number.
# The ratchet list holds only the three content-bearing writes.
# ⚠ `grep -o`, never `grep -c`, over a const array. `grep -c` counts LINES, and
# whether a Rust array occupies one line or fourteen is decided by rustfmt's
# 100-column budget, not by the author. Measured with this repo's own hermit
# rustfmt: `const KB_RATCHETING_TOOLS: &[&str] = &["kb_write_page",
# "kb_add_raw_source", "kb_append_log"];` is 94 characters, so it stays on ONE
# line and `grep -c '"kb_'` returns 1 against an `expect: 3`. Its sibling
# KB_ID_GATED_TOOLS is over budget, rustfmt explodes it one element per line, and
# the identical gate happens to measure 14. Two gates of the same shape landing on
# opposite sides of a formatter's line-wrap is not a gate; count the MATCHES.
awk '/const KB_RATCHETING_TOOLS/,/\];/' crates/biorouter-mcp/src/knowledge/server.rs \
  | grep -oE '"kb_[a-z_]+"' | sort | tr '\n' ' '
echo "expect exactly: \"kb_add_raw_source\" \"kb_append_log\" \"kb_write_page\""
awk '/const KB_RATCHETING_TOOLS/,/\];/' crates/biorouter-mcp/src/knowledge/server.rs \
  | grep -oE '"kb_[a-z_]+"' | wc -l ; echo "expect: 3"
# Nor do the plain HTTP write routes ratchet (the user typing in the Knowledge view).
for h in write_page create_base import_brkb restore_state add_raw_source; do
  echo -n "$h: "
  awk "/pub async fn $h/,/^}/" crates/biorouter-server/src/routes/knowledge.rs | grep -c "raise_tier"
done
echo "expect: 0 each"
# The macro Args carry the capability, and every PRODUCTION caller had to be
# edited. Enumerated per file rather than counted: the three Args types are also
# constructed ten times inside test modules, so a tree-wide count is unstable
# by construction and would go red the first time someone adds a macro test.
for f in crates/biorouter-mcp/src/knowledge/macros/ingest.rs \
         crates/biorouter-mcp/src/knowledge/macros/query.rs \
         crates/biorouter-mcp/src/knowledge/macros/lint.rs; do
  echo -n "$f field: " ; grep -c "pub caller_is_private: bool" "$f"
done ; echo "expect: 1 each"
for f in crates/biorouter-server/src/routes/knowledge.rs \
         crates/biorouter-cli/src/commands/knowledge.rs \
         crates/biorouter-server/src/bin/knowledge_ingest_probe.rs \
         crates/biorouter/src/knowledge/conversation_ingest.rs \
         crates/biorouter-server/src/routes/apps.rs \
         crates/biorouter-mcp/src/agent_drafter/mod.rs; do
  echo -n "$(basename $f): " ; grep -c "caller_is_private" "$f"
done
echo "expect: NON-ZERO in all six. Deliberately not exact numbers — 10C adds an"
echo "  assert_reachable to the last two files, so a fixed count here would go red"
echo "  one task later, which is the mirror-defect shape this plan keeps hitting."
echo "A ZERO is a caller that silently kept a default: routes (3 macro Args),"
echo "  CLI (3 macro Args), probe (1), conversation_ingest (the ProviderTier->bool"
echo "  crossing at :205), apps.rs (CP3's param + 3 call sites), drafter (CP4's param)."
echo "⚠ NON-ZERO IS NOT ENOUGH, and on its own it is not a gate: a file that passes"
echo "  a hardcoded literal, or a value read from the WRONG agent, counts just the"
echo "  same. The two blocks below are what make it one."
# (i) NO LITERAL, IN EITHER DIRECTION. The previous version of this gate forbade
# only the trusting value (`Private` / `true`), which leaves the mirror wide
# open: a caller hardcoded to `Public` / `false` compiles, reports NON-ZERO in
# the count above, ratchets NOTHING for a private session, and passes every
# public-caller test in this plan. Under-ratcheting is the direction that
# launders — a private transcript lands in a base that stays public — so it is
# the one that must not be reachable by the easy edit.
grep -rn "caller_is_private: *\(true\|false\)\|caller_capability: *ProviderTier::\(Private\|Public\)" \
  --include='*.rs' crates/*/src/ crates/*/bin/ 2>/dev/null
echo "expect: exactly ONE hit — routes/knowledge.rs's build_completer TestModeCompleter"
echo "  branch (:903-907), which returns public BEFORE any provider exists and is"
echo "  documented in Step 3. Every other production caller must pass an EXPRESSION."
echo "  (`unwrap_or(ProviderTier::Public)` is not matched and must not be: it is the"
echo "   fail-closed tail of a provider read, not a hardcoded caller.)"
# (ii) …and the expression is a provider read. Per file, printed, so a value
# sourced from a DIFFERENT agent than the one whose turn this is shows up as the
# wrong receiver rather than as a passing count (see CP3's three-site table).
for f in crates/biorouter-server/src/routes/knowledge.rs \
         crates/biorouter-cli/src/commands/knowledge.rs \
         crates/biorouter-server/src/bin/knowledge_ingest_probe.rs \
         crates/biorouter/src/agents/knowledge_tool.rs \
         crates/biorouter-server/src/routes/apps.rs \
         crates/biorouter-mcp/src/agent_drafter/mod.rs; do
  echo "--- $(basename $f)"
  grep -n "caller_is_private\|caller_capability" "$f" | grep -v "^\s*//"
done
echo "expect: every value traceable to a `.tier()` on a constructed provider, or to"
echo "  `knowledge::tier::caller_is_private(&context.meta)` (CP1/CP4). Read the"
echo "  RECEIVER, not just the method: `agent` vs `turn_agent` is the CP3 defect and"
echo "  no count distinguishes them."
# The ProviderTier that feeds :205 reaches all three of ITS callers.
grep -rn "caller_capability:" --include='*.rs' crates/ | grep -v "conversation_ingest.rs"
echo "expect: exactly 3 — agents/knowledge_tool.rs, routes/knowledge.rs, biorouter-cli/.../knowledge.rs"
# The integration test that --lib cannot see was updated, not left to rot.
grep -c "caller_is_private" crates/biorouter-mcp/tests/knowledge_macros_e2e.rs
echo "expect: 3 — IngestArgs :115, QueryArgs :157, IngestArgs :231"
# The HTTP macro routes ratchet from the CONSTRUCTED provider, not the requested name.
awk '/async fn build_completer/,/^}/' crates/biorouter-server/src/routes/knowledge.rs \
  | grep -c "tier()" ; echo "expect: 1"
grep -c "model.provider" crates/biorouter-server/src/routes/knowledge.rs
echo "expect: 1 — only the providers::create call itself; the tier is never keyed on the name"
```

**What this catches.** Four wrong implementations. (1) Ratcheting only in `kb_write_page`, the tool
whose name says "write" — which misses `kb_add_raw_source`, the one the GUI ingest panel and the
`ingest` macro actually call, so the entire GUI path launders silently. The nineteen-tool
parameterised test is the only thing that fails it. (2) Gating at the tool bodies instead of at
`call_tool`, which cannot express the nine context-less tools at all and leaves CP2/CP3/CP4 with
nothing — the `RequestContext` count gate above is what says "you took the old design". (3) Raising
on the *success* return, which the `kb_import` and macro paths make observable: a 400 MB archive or a
sub-agent that dies halfway has already written pages. (4) Keying the HTTP ratchet on
`body.model.provider` — the string the caller supplied — rather than on the instance
`providers::create` returned, which the `BIOROUTER_LEAD_MODEL` intercept can make different.
(5) Hardcoding the **public** value at a caller to make it compile — `caller_is_private: false`,
`caller_capability: ProviderTier::Public`. It is the mirror of (the previously gated) hardcoded
`Private`, it is the direction that **launders** rather than over-classifies, and until this round
every gate in this task passed it: `grep -c caller_is_private` reports NON-ZERO, every
public-caller test still passes, and only a *private* caller's ratchet silently stops happening.
Step 5 (i) now forbids the literal in both directions with exactly one named exemption, and the
per-caller matrices are what fail it behaviourally.
**This gate rejects: `IngestArgs { caller_is_private: false, .. }` in `routes/knowledge.rs`** — a
one-word edit that compiles, keeps `POST /knowledge/bases/{id}/ingest` working, and stops every
private HTTP ingest from ratcheting.
(6) Passing `agent` at all three CP3 call sites, which is the shape the previous draft prescribed
in one sentence. It compiles, type-checks and passes every single-agent app test, because `agent`
and `turn_agent` are both `Arc<Agent>` and both in scope at `:3847`; what it does is attribute a
**worker's** mid-turn KB access to the main agent, in both directions —
`a_mid_turn_br_kb_ingest_ratchets_from_the_TURN_agent_not_the_main_one` fails it, and the
`turn_agent.provider()` count is the grep that names it.

- [ ] **Step 6: Commit**

```bash
git add crates/biorouter-mcp/src/knowledge/ crates/biorouter-mcp/src/agent_drafter/mod.rs \
        crates/biorouter-mcp/tests/knowledge_macros_e2e.rs \
        crates/biorouter-server/src/routes/knowledge.rs crates/biorouter-server/src/routes/apps.rs \
        crates/biorouter-server/src/bin/knowledge_ingest_probe.rs \
        crates/biorouter/src/knowledge/conversation_ingest.rs \
        crates/biorouter/src/agents/knowledge_tool.rs \
        crates/biorouter-cli/src/commands/knowledge.rs
# The commit must leave the tree green. Verified here, not nine commits later.
cargo check --workspace --all-targets
git commit -m "feat(knowledge): ratchet a knowledge base to the tier of the sessions that ingest into it (#56)"
```

---

### Task 10C: The knowledge-base barrier — one line at each of the four choke points

The read half of the ruling, and the task the verifier's finding is really about: **`kb_search`'s
explicit-`kb_id` branch bypasses the visible-set logic entirely.** `kb_search` at
`knowledge/server.rs:590-592` joins `kb_root(self.service.root(), &kb_id)` directly and searches it;
only the `else` at `:602-604` goes through `search_visible_bases` (`:258-286`). Six more read paths
do the same thing, four of them through `kb_id_or_primary`, whose doc comment (`:308-311`) states the
bypass as a feature: *"An explicit `kb_id` always wins and is never filtered against the session's
set — that is how a hidden base (Soul) stays reachable."* Hiding is a *tidiness* control and that
sentence is correct for it. The privacy tier is not a tidiness control, and the same code path must
now answer both questions differently.

Task 10B installed the capability at all four choke points and proved it arrives. This task is
therefore **one `if` at each**, plus the two fan-out filters that make a KB-less search degrade
instead of failing.

**Files:**

| Action | Path | Anchor (re-verified at `9558c346`) |
|---|---|---|
| Modify | `crates/biorouter-mcp/src/knowledge/server.rs` | **CP1.** `assert_kb_reachable` beside `gated_kb_id`; one call in `call_tool` (Task 10B). The two fan-out filters: `visible_bases_for_session` `:240-249` (retain at `:247`) and `search_visible_bases` `:258-286` (per-base loop at `:266`), both gaining `caller_private: bool`; `visible_bases_for_context` `:251-256` derives it from the context it already has. **Plus the third filter: `kb_id_or_primary` `:312-342`**, whose no-primary error (`let ids` `:323`, `ids.join(", ")` `:338`) formats `service.session_kb_ids(..)` into `"Pass kb_id explicitly (one of: …)"` — see ⚠ "the barrier must not narrate what it refuses". ⚠ Anchor any `awk` on `fn kb_id_or_primary\(` **with the paren**: the existing test `kb_id_or_primary_errors_with_the_candidate_list` `:886` also matches the bare prefix, and the two ranges concatenate to 56 lines instead of 31. **Plus the fourth and fifth filters, the two pointer tools:** `selection_value` `:691-706` (17 lines; serialises `primary_kb`, `knowledge_bases` **and** the deprecated `active_kb` mirror) and `set_primary_json` `:667-683` (18 lines), reached from `kb_set_active` `:712` and `kb_get_active` `:725`; `selection_json` `:686-689` forwards. Four existing test call sites gain the new argument — `:956`, `:970`, `:980`, `:985`, all inside `set_primary_validates_membership_and_reports_the_set` `:946` |
| Modify | `crates/biorouter-mcp/src/knowledge/macros/ingest.rs` / `query.rs` / `lint.rs` | **CP2.** one `tier::assert_reachable(svc.root(), &args.kb_id, args.caller_is_private)?;` on the line above each `raise_tier` (`ingest.rs:48`, `query.rs:47`, `lint.rs:218`) |
| Modify | `crates/biorouter-server/src/routes/apps.rs` | **CP3.** one check in `handle_kb_frame` `:2474`, immediately after `resolve_kb_grant` returns `kb_id` and before the `match op` — so it covers `run_kb_read` `:2376` and the `ingest` arm `:2533` together |
| Modify | `crates/biorouter-mcp/src/agent_drafter/mod.rs` | **CP4.** one check in `stage_full_payload`'s kb loop at `:1418`, before `svc.export_brkb(kb)` `:1423`; the existing export-payload test is at `:3894-3899` |
| Reference | `crates/biorouter-mcp/src/knowledge/server.rs` | the tools deliberately **outside** `KB_ID_GATED_TOOLS`: `kb_list_bases` `:348`, `kb_create_base` `:357`, `kb_set_active` `:712`, `kb_get_active` `:725`, `kb_import` `:764` |

⚠ **Five tools are deliberately not in `KB_ID_GATED_TOOLS`, and each has a different reason.**

- **`kb_set_active` / `kb_get_active`** are not `kb_id`-gated — but they are **capability-aware in
  their own bodies**, and the previous draft's reason for exempting them outright was false on the
  tree. It said "the pointer is a bare kb id the session already had to know to pass". `kb_get_active`
  (`:725`) **takes no arguments at all**, and both tools return `selection_value` (`:691`), which
  serialises `knowledge_bases` — *every* visible id — alongside `primary_kb` and its deprecated
  `active_kb` mirror. So `kb_get_active {}` was a one-call enumeration of every base on the machine
  that the session can see, private ones included: the same list `kb_list_bases` omits four
  functions away, through a tool the exemption blessed. And a *failed* `kb_set_active` enumerates
  too — `apply_selection_unlocked`'s `PrimaryUpdate::Set` arm bails with
  `next_ids.join(", ")` (`service.rs:1645-1655`), the whole set. Step 3 (d) makes both filter the
  **view**; the store is untouched, which is what keeps the "one axis, one pointer" repair logic
  (`docs/knowledge-base/multi-kb-implementation-plan.md`; `repair_decision`, `service.rs:1405-1428`)
  working exactly as it does today — see the ⚠ below it for why filtering the store instead would
  be a persisted, machine-wide side effect of a *read*.
- **`kb_create_base` / `kb_import`** name a base that **does not exist yet**. There is nothing to
  leak, and gating them is what banned knowledge-base creation for public sessions in the previous
  draft. `tier::assert_reachable` would permit them anyway — a kb id with no directory on disk is
  reachable by Task 10A decision (3) — but they are kept off the list so the list means one thing.
- **`kb_list_bases`** must *omit*, not refuse: a single-base refusal would hide every base from a
  public session the moment one of them is private. It goes through `visible_bases_for_context`.

⚠ **The barrier must not narrate what it refuses.** `gated_kb_id` returns `Ok(None)` when a
primary-resolving tool has neither an explicit `kb_id` nor a primary, deliberately, so the tool
answers with its own error instead of a privacy refusal (Step 3). That error is
`kb_id_or_primary`'s, at `server.rs:323-341`:

```
kb_id not supplied and this session has no primary knowledge base. Pass kb_id
explicitly (one of: default, omop), …
```

built from `service.session_kb_ids(..)`, whose `session_kb_ids_unlocked` (`service.rs:1267-1274`)
filters on `hidden` and **nothing else**. So a public chat with no primary calling
`kb_read_page {path: …}` is handed `default, omop` — the exact list that the same task's
`kb_list_bases_omits_a_private_base_rather_than_redacting_it` asserts must read `["default"]`. A
barrier that refuses a read and then hands over the identifier of the thing it refused is not a
barrier; and the id it hands over is the one argument that makes the explicit-`kb_id` branch — the
finding this whole task exists to close — writable. So the id list in that error takes the same
filter as `visible_bases_for_session`, and when the filter empties it the message degrades to the
"this session has no knowledge bases" branch that already exists two lines above.

**And this is a decision about existence-leakage, so it is stated rather than assumed.** DR-7 rules
side channels — existence, counts, timing — **out of scope** for `chatrecall`, and this plan keeps
that ruling: nothing here pads a count, equalises a latency or plants a decoy, and `create_base`'s
pre-existing `"kb '{id}' already exists at {path}"` (`service.rs:451`) remains an existence oracle for
a *guessed* id, unchanged and unchased ([AR-5](#ar-5--the-existence-of-a-private-knowledge-base-is-still-inferable)).
What B2 and Task 10D close is a different thing, and the plan already ruled on it one test over: a
knowledge base's **id and name are user-authored content** — *"a KB name is user-authored and
routinely names a cohort or a study"* — which is why `kb_list_bases` omits rather than redacts.
Directly enumerating that content to a public model is not a side channel; it is the content
crossing. Refusing to be *asked* whether `omop` exists is out of scope. *Volunteering the string
`omop`* is not.

⚠ **The `/knowledge/*` HTTP routes the GUI uses are NOT gated, and this is the load-bearing scope
decision of the task.** DR-3 says *a public model* must never reach a private session. The Knowledge
view is the **user**, not a model. The seven ungated read handlers, all verified present in
`routes/knowledge.rs`, are `get_graph` `:466`, `list_pages` `:517`, `read_page` `:539`,
`get_page_body` `:817`, `list_history` `:843`, `preview_state` `:862` and `export_brkb` `:1518` —
that user reading their own knowledge base in their own app, and a barrier there would lock a user
out of their own notes with no model involved anywhere. The four macro routes **are** gated, at CP2,
because those run a model. If you find yourself adding a check to `get_page_body` or `list_pages`,
stop: that is a different product decision and it is [Open question 15](#open-questions).

- [ ] **Step 1: Write the failing tests**

```rust
// crates/biorouter-mcp/src/knowledge/server.rs, in its #[cfg(test)] mod tests

#[tokio::test]
async fn the_explicit_kb_id_branch_is_not_a_way_around_the_barrier() {
    // The finding, exactly. Before this task the `kb_id`-carrying branch at
    // :590-592 searches any base on the machine, and `search_visible_bases`
    // — the only code that consults the session's set — is in the `else`.
    let (srv, root) = server_at_migrated_root(&["default"]);
    tier::raise_unlocked(&root, "default", true).unwrap();
    seed_page(&root, "default", "knowledge/omop.md", "SENTINEL-COHORT-N-412");

    let out = call_tool_as(&srv, "kb_search",
        json!({ "kb_id": "default", "query": "cohort" }), Public).await;
    let text = refusal_text(&out);
    assert!(text.contains("private"), "must say why: {text}");
    assert!(!text.contains("SENTINEL-COHORT-N-412"), "leaked a snippet: {text}");
    assert!(!text.contains("knowledge/omop.md"), "leaked a page path: {text}");
}

#[tokio::test]
async fn no_tool_that_names_a_base_reaches_a_private_one_under_a_public_model() {
    // Parameterised over ALL NINETEEN, by name through call_tool — the shape
    // CP1 makes possible and the old per-tool design could not express for the
    // nine context-less tools. `kb_export` is the one to watch: it writes the
    // entire base to an attacker-named path on disk in one call (:744-752).
    let (srv, root) = server_at_migrated_root(&["omop"]);
    tier::raise_unlocked(&root, "omop", true).unwrap();
    seed_page(&root, "omop", "knowledge/x.md", "SENTINEL-BODY");

    for probe in KB_TOOLS {                          // the same 19-row table as 10B
        let out = call_tool_as(&srv, probe.name, probe.args_for("omop"), Public).await;
        assert_eq!(out.is_err(), probe.gated, "{} gated={}", probe.name, probe.gated);
        assert!(!rendered(&out).contains("SENTINEL-BODY"), "{} leaked a body", probe.name);
        if probe.gated {
            assert_eq!(bytes_written_since(&root, "omop"), 0, "{} wrote anyway", probe.name);
        }
    }
    for probe in KB_TOOLS {
        assert!(call_tool_as(&srv, probe.name, probe.args_for("omop"), Private).await.is_ok(),
                "{} refused a private caller", probe.name);
    }
}

#[tokio::test]
async fn omitting_the_kb_id_is_not_the_bypass() {
    // `kb_id_or_primary` (:312) resolves an absent id to the session's primary,
    // so a handler that only checks an EXPLICIT kb_id is bypassed by deleting
    // one argument. Four tools take that path.
    let (srv, root) = server_at_migrated_root(&["omop"]);
    tier::raise_unlocked(&root, "omop", true).unwrap();
    set_primary(&root, "sess-1", "omop");
    for tool in ["kb_read_page", "kb_list_pages", "kb_get_graph", "kb_list_history"] {
        let out = call_tool_as_session(&srv, tool, json!({ "path": "knowledge/x.md" }),
                                       "sess-1", Public).await;
        assert!(out.is_err(), "{tool} answered from the primary without a check");
    }
}

#[tokio::test]
async fn a_kb_less_search_still_serves_the_public_bases_it_can_see() {
    // The fan-out shape Task 15 gets wrong in the extension manager: a single
    // up-front refusal turns `search_visible_bases` into all-or-nothing, so one
    // private base in the session's set costs the user every other base.
    let (srv, root) = server_at_migrated_root(&["default", "omop"]);
    tier::raise_unlocked(&root, "omop", true).unwrap();
    seed_page(&root, "default", "knowledge/a.md", "public-hit cohort");
    seed_page(&root, "omop",    "knowledge/b.md", "private-hit cohort");

    let hits = search_hits(&srv, json!({ "query": "cohort" }), Public).await;
    assert_eq!(hits.iter().map(|h| h.kb_id.as_str()).collect::<Vec<_>>(), vec!["default"]);
    assert!(!text_of_hits(&hits).contains("private-hit"));
}

#[tokio::test]
async fn kb_list_bases_omits_a_private_base_rather_than_redacting_it() {
    // The §7 `appears_in_list` rule, one module over: a KB name is user-authored
    // and routinely names a cohort or a study. Omission also removes the
    // temptation to then pass the id explicitly, which is the very bypass this
    // task closes.
    let (srv, root) = server_at_migrated_root(&["default", "omop"]);
    tier::raise_unlocked(&root, "omop", true).unwrap();
    assert_eq!(base_ids(&srv, Public).await, vec!["default"]);
    assert_eq!(base_ids(&srv, Private).await, vec!["default", "omop"]);
}

#[tokio::test]
async fn the_no_primary_error_names_only_the_bases_the_caller_may_reach() {
    // The fall-through `gated_kb_id` deliberately leaves open (Step 3): with no
    // explicit kb_id and no primary, the TOOL answers — and its answer used to
    // be the full id list. Same leak class as `kb_list_bases` redacting instead
    // of omitting, one function over, and it hands the public caller the exact
    // argument the explicit-`kb_id` branch needs.
    let (srv, root) = server_at_migrated_root(&["default", "omop"]);
    tier::raise_unlocked(&root, "omop", true).unwrap();
    clear_primary(&root, "sess-1");

    let public = call_tool_as_session(&srv, "kb_read_page", json!({ "path": "knowledge/x.md" }),
                                      "sess-1", Public).await;
    let t = rendered(&public);
    assert!(t.contains("default"), "the public base must still be offered: {t}");
    assert!(!t.contains("omop"), "the no-primary error enumerated a private base: {t}");

    let private = call_tool_as_session(&srv, "kb_read_page", json!({ "path": "knowledge/x.md" }),
                                       "sess-1", Private).await;
    assert!(rendered(&private).contains("omop"), "a private caller lost its own base");
}

#[tokio::test]
async fn a_public_session_whose_only_base_is_private_is_told_it_has_none() {
    // The degrade direction. Filtering the list must not leave
    // "Pass kb_id explicitly (one of: )" — an empty parenthesis is both useless
    // and a tell. It falls through to the branch that already exists for a
    // session with no bases at all.
    let (srv, root) = server_at_migrated_root(&["omop"]);
    tier::raise_unlocked(&root, "omop", true).unwrap();
    clear_primary(&root, "sess-1");
    let t = rendered(&call_tool_as_session(&srv, "kb_list_pages", json!({}),
                                           "sess-1", Public).await);
    assert!(t.contains("no knowledge bases"), "{t}");
    assert!(!t.contains("one of:"), "left an empty enumeration: {t}");
}

#[tokio::test]
async fn kb_get_active_does_not_enumerate_a_private_base_or_point_at_one() {
    // The tool that takes NO arguments and returned the whole selection.
    // `selection_value` (:691) serialises `knowledge_bases`, `primary_kb` and
    // the deprecated `active_kb` mirror — all three are asserted, because
    // filtering two of the three is the natural half-fix and `active_kb` is
    // the one a reader forgets.
    let (srv, root) = server_at_migrated_root(&["default", "omop"]);
    tier::raise_unlocked(&root, "omop", true).unwrap();
    set_primary(&root, "sess-1", "omop");

    let v = call_tool_json_as_session(&srv, "kb_get_active", json!({}), "sess-1", Public).await;
    assert_eq!(v["knowledge_bases"], json!(["default"]));
    // The pointer is metadata too. It reads null rather than naming a base this
    // caller may not reach — the truthful answer for THIS caller, which has no
    // usable write target, and the same omission rule kb_list_bases takes.
    assert_eq!(v["primary_kb"], json!(null));
    assert_eq!(v["active_kb"], json!(null), "the deprecated mirror leaked it");

    let v = call_tool_json_as_session(&srv, "kb_get_active", json!({}), "sess-1", Private).await;
    assert_eq!(v["knowledge_bases"], json!(["default", "omop"]));
    assert_eq!(v["primary_kb"], json!("omop"));

    // And the STORE was not touched by the public read: `.active-kb-sessions`
    // still names omop. This is the assertion that fails the "filter it in
    // `service::selection`" implementation, which looks identical from the
    // tool's output and silently re-points the user's primary.
    assert_eq!(stored_primary(&root, "sess-1"), Some("omop".to_string()));
}

#[tokio::test]
async fn a_private_target_and_a_nonexistent_one_are_indistinguishable_to_kb_set_active() {
    // Two halves. (1) A public caller may not move the pointer onto a private
    // base. (2) The refusal must be BYTE-IDENTICAL to the answer a base that
    // does not exist gets — Task 10D's rule, one crate over: a message saying
    // "that base is private" confirms it exists, in a politer sentence.
    let (srv, root) = server_at_migrated_root(&["default", "omop"]);
    tier::raise_unlocked(&root, "omop", true).unwrap();

    let private_target = err_of(call_tool_as_session(&srv, "kb_set_active",
        json!({ "kb_id": "omop" }), "sess-1", Public).await);
    let absent_target = err_of(call_tool_as_session(&srv, "kb_set_active",
        json!({ "kb_id": "no-such-kb" }), "sess-1", Public).await);
    assert_eq!(private_target.replace("omop", "no-such-kb"), absent_target,
               "the two answers differ, so the difference is the oracle");
    assert!(!private_target.to_lowercase().contains("private"), "{private_target}");
    // The candidate list in the refusal is filtered, for the same reason
    // `the_no_primary_error_names_only_the_bases_the_caller_may_reach` exists.
    assert!(private_target.contains("default") && !private_target.contains("omop, "),
            "the refusal enumerated the set it refused: {private_target}");
    assert_eq!(stored_primary(&root, "sess-1"), None, "the refused set was written anyway");

    // A private caller still moves it.
    call_tool_as_session(&srv, "kb_set_active", json!({ "kb_id": "omop" }),
                         "sess-1", Private).await.unwrap();
    assert_eq!(stored_primary(&root, "sess-1"), Some("omop".to_string()));
}

/// A tool that is not in `KB_ID_GATED_TOOLS`, why, and **the test that pins the
/// behaviour that exemption claims**. The third field is the whole point: the
/// previous version of this table was five bare strings, and a bare string is a
/// claim with nothing behind it — which is how `kb_get_active`, a no-argument
/// tool that returned every visible base id, sat on this list being described as
/// "the caller already knows the pointer".
struct ExemptTool {
    name: &'static str,
    why: &'static str,
    pinned_by: &'static str,
}

const EXEMPT: &[ExemptTool] = &[
    ExemptTool { name: "kb_list_bases",
        why: "omits rather than refuses; a single-base refusal would hide every base",
        pinned_by: "kb_list_bases_omits_a_private_base_rather_than_redacting_it" },
    ExemptTool { name: "kb_get_active",
        why: "reports the selection; filters the VIEW — ids omitted, pointer null",
        pinned_by: "kb_get_active_does_not_enumerate_a_private_base_or_point_at_one" },
    ExemptTool { name: "kb_set_active",
        why: "moves the pointer; a private target is NOT A MEMBER, not 'private'",
        pinned_by: "a_private_target_and_a_nonexistent_one_are_indistinguishable_to_kb_set_active" },
    ExemptTool { name: "kb_create_base",
        why: "names a base that does not exist yet — nothing to leak (Task 10A (3))",
        pinned_by: "a_public_chat_can_still_create_and_import_a_knowledge_base" },
    ExemptTool { name: "kb_import",
        why: "same; the id is chosen by brkb::import's collision loop",
        pinned_by: "a_public_chat_can_still_create_and_import_a_knowledge_base" },
];

#[test]
fn every_kb_tool_is_gated_or_exempt_for_a_pinned_reason() {
    // The partition, unchanged: the router's own tool list must equal the gated
    // list plus the exemptions, nothing unaccounted for in either direction, so
    // a TWENTIETH tool is a test failure rather than a silent hole.
    let mut known: Vec<&str> = KB_ID_GATED_TOOLS
        .iter().copied().chain(EXEMPT.iter().map(|e| e.name)).collect();
    known.sort();
    let mut actual: Vec<String> = KnowledgeServer::tool_router()
        .list_all().into_iter().map(|t| t.name.to_string()).collect();
    actual.sort();
    assert_eq!(actual, known, "a kb_* tool is neither gated nor listed as exempt");
    // …and no exemption may be a bare assertion. Step 5 greps every `pinned_by`
    // for a real `fn` in this file; here we only pin that the field is filled,
    // because Rust has no way to name a test function from another test.
    for e in EXEMPT {
        assert!(!e.why.is_empty() && e.pinned_by.starts_with(|c: char| c.is_alphabetic()),
                "{} is exempt with no reason and no pinning test", e.name);
    }
}

#[tokio::test]
async fn no_exempt_tool_volunteers_a_private_bases_id_to_a_public_caller() {
    // The rule that REPLACES the blanket exemption, and the one that would have
    // caught `kb_get_active` before it shipped. `KB_ID_GATED_TOOLS` decides who
    // takes the CONTENT barrier and says nothing about METADATA; listing the
    // non-gated tools and stopping is not a completeness test, it is a
    // permission slip. This is universal over the exempt set, so a twentieth
    // exempt tool is covered the day it is written.
    //
    // ⚠ Every probe's arguments name ONLY the public base. That is the
    // volunteering/being-asked line this plan draws for AR-5 and DR-7: echoing
    // back an id the caller supplied is not a leak (`kb_set_active {kb_id:
    // "omop"}` must say so by name), whereas producing that id from arguments
    // that never mentioned it is the content crossing.
    let (srv, root) = server_at_migrated_root(&["default", "omop-cohort-412"]);
    tier::raise_unlocked(&root, "omop-cohort-412", true).unwrap();
    set_primary(&root, "sess-1", "omop-cohort-412");   // the pointer names it

    for e in EXEMPT {
        let out = call_tool_as_session(&srv, e.name, args_naming_only_default(e.name),
                                       "sess-1", Public).await;
        let text = rendered(&out);
        assert!(!text.contains("omop-cohort-412"),
                "{} volunteered a private base id to a public caller: {text}", e.name);
        assert!(!text.contains("OMOP Cohort"),   // the NAME, which is user-authored
                "{} volunteered a private base name: {text}", e.name);
    }

    // The same loop as a PRIVATE caller must still see it, or "no leak" is
    // satisfied by "the tools return nothing".
    let out = call_tool_as_session(&srv, "kb_get_active", json!({}), "sess-1", Private).await;
    assert!(rendered(&out).contains("omop-cohort-412"));
}
```

```rust
// crates/biorouter-server/tests/knowledge_routes.rs

#[tokio::test]
async fn a_public_model_macro_cannot_run_against_a_private_base_over_http() {
    let root = migrated_root_with_public_base("omop");
    tier::raise_unlocked(&root, "omop", true).unwrap();
    let r = post_query(&root, "omop", model_ref("anthropic", "claude-opus-4-8")).await;
    assert_eq!(r.status(), 409);
    assert!(r.text().await.contains("private"));
    // And the GUI's own read routes are untouched: the user is not a model.
    assert_eq!(get_page_body(&root, "omop", "knowledge/x.md").await.status(), 200);
}
```

```rust
// crates/biorouter-server/src/routes/apps.rs, in its existing `mod tests`

#[tokio::test]
async fn br_kb_reads_are_refused_on_a_private_base_even_with_a_manifest_grant() {
    // CP3, and the reason a manifest grant is not a privacy control: the app's
    // manifest was authored by the drafting model (`agent_drafter/mod.rs:1731`),
    // which learned the base ids from `discover_kbs` (`catalog.rs:125-141`).
    let (state, root) = app_state_with_kb("kbx");
    tier::raise_unlocked(&root, "kbx", true).unwrap();
    seed_page(&root, "kbx", "knowledge/x.md", "SENTINEL-BODY");
    handle_kb_frame(&bridge, &state.knowledge_service, Some(&cfg_granting("kbx", false)),
                    /* caller_is_private */ false, "search",
                    &json!({ "kb_id": "kbx", "query": "x" }), "r1").await;
    let f = await_kb_result(&bridge).await;
    assert!(f["error"].as_str().unwrap().contains("private"));
    assert!(!f.to_string().contains("SENTINEL-BODY"));
}

#[tokio::test]
async fn a_mid_turn_br_kb_read_is_refused_for_a_public_WORKER_under_a_private_main() {
    // The read half of Task 10B's `..._ratchets_from_the_TURN_agent_not_the_main_one`,
    // and the direction that actually leaks. Private main + public worker: the
    // base is private, the frame arrives while the WORKER's turn is running, and
    // reading `agent` (:3541's `else` branch is the main agent; :3584 is the
    // worker) evaluates the worker's read as private and hands it the body.
    let app = app_with_worker_profile("analyst", /* main */ Private, /* worker */ Public).await;
    tier::raise_unlocked(&app.kb_root, "kbx", true).unwrap();
    seed_page(&app.kb_root, "kbx", "knowledge/x.md", "SENTINEL-BODY");
    app.start_turn_on_profile("analyst").await;
    app.send_kb_frame_mid_turn("search", json!({ "kb_id": "kbx", "query": "x" })).await;
    let f = await_kb_result(&app.bridge).await;
    assert!(f["error"].as_str().unwrap().contains("private"),
            "a public worker read a private base mid-turn: {f}");
    assert!(!f.to_string().contains("SENTINEL-BODY"));

    // Public main + private worker: the worker may read its own private base.
    // A fix that hardcoded `false` at :3847 would pass the assertion above and
    // fail this one — which is why both directions are here.
    let app = app_with_worker_profile("analyst", /* main */ Public, /* worker */ Private).await;
    tier::raise_unlocked(&app.kb_root, "kbx", true).unwrap();
    seed_page(&app.kb_root, "kbx", "knowledge/x.md", "SENTINEL-BODY");
    app.start_turn_on_profile("analyst").await;
    app.send_kb_frame_mid_turn("search", json!({ "kb_id": "kbx", "query": "x" })).await;
    let f = await_kb_result(&app.bridge).await;
    assert!(f["error"].is_null(), "a private worker was refused its own base: {f}");
}
```

```rust
// crates/biorouter-mcp/src/agent_drafter/mod.rs, in its #[cfg(test)] mod tests

#[test]
fn export_app_leaves_a_private_knowledge_base_out_of_the_payload() {
    // CP4. `export_brkb` writes the WHOLE base into the payload, and `kb_ids`
    // comes from the model-supplied `include.knowledge_bases` (:1397) — a
    // strictly wider `kb_export` with no id gate anywhere before this task.
    // Skip-and-note rather than hard-fail, matching `search_visible_bases`:
    // the rest of the export is still useful and the user is told why.
    let (root, target) = drafter_fixture_with_kbs(&["pub-kb", "priv-kb"]);
    tier::raise_unlocked(&root, "priv-kb", true).unwrap();
    let staged = stage_full_payload(&manifest, &target,
                                    Some(&json!({ "knowledge_bases": ["pub-kb", "priv-kb"] })),
                                    /* caller_is_private */ false);
    let ids: Vec<_> = staged.knowledge_bases.iter().map(|k| k["id"].as_str().unwrap()).collect();
    assert_eq!(ids, vec!["pub-kb"]);
    assert!(staged.notes.iter().any(|n| n.contains("priv-kb") && n.contains("private")));
    assert!(!target.join("payload/knowledge/priv-kb.brkb").exists());
}
```

- [ ] **Step 2: Run** → **FAIL** on every gated probe, on the omitted-`kb_id` test, on both fan-out
      tests, on both no-primary-error tests, on **both pointer-tool tests**, on the app-socket test
      and on the drafter test; the HTTP test's 409 half fails and its 200 half passes.
      `every_kb_tool_is_gated_or_exempt_for_a_pinned_reason` passes from the start — it is a
      regression net, not a red test — while
      `no_exempt_tool_volunteers_a_private_bases_id_to_a_public_caller` **fails**, on
      `kb_get_active` and on `kb_list_bases`. No **compile** error anywhere: Task 10B put every signature in place, which is
      the whole point of splitting the two tasks. ⚠ The one exception is inside this task's own Step
      3: the moment `selection_value` / `set_primary_json` / `selection_json` take
      `caller_private`, the four existing call sites in
      `set_primary_validates_membership_and_reports_the_set` (`:956`, `:970`, `:980`, `:985`) stop
      compiling. That is the forcing function, not a break — pass `true` at all four.

- [ ] **Step 3: Implement** — one helper, four `if`s, two filters:

```rust
    /// The KB twin of `ExtensionManager::assert_extension_reachable`. `Err` is
    /// the refusal; `Ok(())` permits. Reads the base's stored tier, never the
    /// session's set — hiding and privacy are different questions and
    /// `kb_id_or_primary` (:312) answers only the first.
    fn assert_kb_reachable(&self, kb_id: &str, caller_private: bool) -> Result<(), ErrorData> {
        crate::knowledge::tier::assert_reachable(self.service.root(), kb_id, caller_private)
            .map_err(|e| ErrorData::invalid_request(e.to_string(), None))
    }
```

CP1 — one line in `call_tool`, above the raise Task 10B put there:

```rust
        if let Some(kb_id) =
            self.gated_kb_id(&name, request.arguments.as_ref(), Some(&context))?
        {
            self.assert_kb_reachable(&kb_id, caller_private)?;      // ← this task
            if KB_RATCHETING_TOOLS.contains(&name.as_str()) {
                self.service.raise_tier(&kb_id, caller_private).map_err(into_err)?;
            }
        }
```

CP2 / CP3 / CP4 — one `tier::assert_reachable(root, kb_id, caller_is_private)` each, at the anchors
in the Files table. CP4 turns the `Err` into a `note` and `continue`s rather than failing the export.

The two fan-out filters, which are the only places that must **not** refuse:

- `search_visible_bases` (`:258-286`) filters **inside** its per-base loop at `:266`, so a private
  base is skipped and the public ones still answer. A guard before the loop is the all-or-nothing bug
  test 4 exists to catch.
- `visible_bases_for_session` (`:240-249`) gains the same filter beside its `hidden.contains` retain
  at `:247`, which is what makes `kb_list_bases` omit rather than redact.

Both take `caller_private: bool`; `visible_bases_for_context` (`:251-256`) derives it from the
context it already has, so `kb_list_bases` needs no change of its own.

The third filter, in `kb_id_or_primary` (`:323-341`) — the ⚠ above is why:

```rust
        let ids: Vec<String> = self
            .service
            .session_kb_ids(Self::session_id(context))
            .map_err(into_err)?
            .into_iter()
            // Issue #56. `session_kb_ids_unlocked` (service.rs:1267) filters on
            // `hidden` only, and this string is read by the model. Same rule as
            // `visible_bases_for_session`: OMIT. When the filter empties the
            // list, the existing "this session has no knowledge bases" branch
            // below takes over — an empty `(one of: )` is both useless and a tell.
            .filter(|id| Self::caller_is_private(context)
                         || !crate::knowledge::tier::is_private(self.service.root(), id))
            .collect();
```

⚠ It reads the tier **per id**, not once: `is_private` is a lookup in a file the process has
already `stat`ed, and doing it per id is what lets the public bases survive the private one — the
same all-or-nothing trap the fan-out tests exist to catch, in a third place.

(d) **The fourth and fifth filters — the two pointer tools.** One shared helper, then three small
edits. The helper is the only new symbol:

```rust
    /// The ids in `selection` this caller may reach — the **view**, never the
    /// store.
    ///
    /// ⚠ The filter is HERE and not in `service::selection` (`service.rs:1461`)
    /// or `apply_selection_unlocked` (`:1604`). Those two feed `repair_decision`
    /// (`:1405-1428`), which promotes the primary to `next_ids.first()` and then
    /// **writes it to disk**. Filtering the service would therefore make a
    /// public model's `kb_get_active` silently re-point the user's primary at
    /// the lexicographically first public base — a persisted, machine-wide
    /// change as a side effect of a read, and one the Knowledge view would then
    /// show. The store keeps one truth; the two model-facing tools render a
    /// filtered projection of it.
    fn visible_kb_ids(
        selection: &crate::knowledge::service::KbSelection,
        root: &std::path::Path,
        caller_private: bool,
    ) -> Vec<String> {
        selection
            .kb_ids
            .iter()
            .filter(|id| caller_private || !crate::knowledge::tier::is_private(root, id))
            .cloned()
            .collect()
    }
```

`selection_value` (`:691`) becomes a method — it needs `self.service.root()` — and filters all
three fields it emits:

```rust
    fn selection_value(
        &self,
        selection: &crate::knowledge::service::KbSelection,
        caller_private: bool,
        ok: bool,
    ) -> serde_json::Value {
        let kb_ids = Self::visible_kb_ids(selection, self.service.root(), caller_private);
        // Issue #56. The POINTER is metadata too, and it is the single id that
        // makes the explicit-`kb_id` branch writable without guessing. A primary
        // the caller may not reach reads `null` — truthful for this caller (it
        // has no write target it can use) and the same OMISSION rule
        // `kb_list_bases` takes. `active_kb` is the deprecated mirror and must
        // move with it; filtering two of the three fields is the natural
        // half-fix and this is the field it forgets.
        let primary = selection
            .primary_kb
            .as_ref()
            .filter(|id| kb_ids.iter().any(|visible| visible == *id))
            .cloned();
        let mut v = serde_json::json!({
            "primary_kb": primary.clone(),
            "knowledge_bases": kb_ids,
            "active_kb": primary,
        });
        if ok {
            v["ok"] = serde_json::Value::Bool(true);
        }
        v
    }
```

`set_primary_json` (`:667`) decides membership against the caller's own view, and **answers there**,
so the service's candidate list is never reached from a tool:

```rust
    fn set_primary_json(
        &self,
        session_id: Option<&str>,
        kb_id: &str,
        caller_private: bool,
    ) -> Result<serde_json::Value, ErrorData> {
        crate::knowledge::paths::validate_kb_id(kb_id)
            .map_err(|e| ErrorData::invalid_params(e.to_string(), None))?;
        // Issue #56. Membership is decided against the set THIS CALLER can see.
        // A private base is not "refused" — it is NOT A MEMBER, byte-identical
        // to the answer an id that does not exist gets. Refusing it by name
        // would confirm it exists (Task 10D's `..._not_as_granted` rule).
        let selection = self.service.selection(session_id).map_err(into_err)?;
        let visible = Self::visible_kb_ids(&selection, self.service.root(), caller_private);
        if !visible.iter().any(|id| id == kb_id) {
            return Err(ErrorData::invalid_params(
                not_a_member(kb_id, &visible, session_id),
                None,
            ));
        }
        let selection = self
            .service
            .set_selection(
                session_id,
                None,
                crate::knowledge::service::PrimaryUpdate::Set(kb_id),
            )
            // Pre-checked above, so an `Err` here is a concurrent hide (or I/O).
            // Answer with OUR list either way: `apply_selection_unlocked`'s
            // message is built from `next_ids` — the WHOLE set — and would put
            // every private id into a public caller's error on a race.
            .map_err(|e| {
                tracing::warn!("kb_set_active: {e:#}");
                ErrorData::invalid_params(not_a_member(kb_id, &visible, session_id), None)
            })?;
        Ok(self.selection_value(&selection, caller_private, true))
    }
```

with the one spelling of the sentence, a free function beside them — deliberately a **verbatim**
mirror of `apply_selection_unlocked`'s two branches (`service.rs:1645-1655`), including its
session/no-session vocabulary split (D11), so that moving the decision up a layer does not invent a
second message the model can tell apart from the old one:

```rust
/// "That is not one of your knowledge bases", built from the ids the caller may
/// see. ONE sentence for a base that does not exist and for one that is private:
/// telling them apart is the leak (issue #56).
fn not_a_member(kb_id: &str, visible: &[String], session_id: Option<&str>) -> String {
    let available = if visible.is_empty() {
        "none".to_string()
    } else {
        visible.join(", ")
    };
    match session_id {
        Some(_) => format!(
            "knowledge base '{kb_id}' is not one of this session's knowledge bases \
             ({available}). Add it to the session first, or pass kb_id explicitly to read it once."
        ),
        None => format!(
            "knowledge base '{kb_id}' is not available ({available}) — it does not exist, or it \
             is hidden."
        ),
    }
}
```

Then `selection_json` (`:686`) takes `caller_private` and forwards it, and the two tools read the
capability from the context they already hold (`Self::caller_is_private(Some(&context))`, Task 10A
(f)) — `kb_get_active` `:725-731` and `kb_set_active` `:712-720`. Both already take a
`RequestContext`; **no `#[tool]` signature moves**, which is why these two tools do not appear in
Task 10B's "exactly two tools gained a `RequestContext`" gate.

⚠ **The four existing test call sites are the forcing function, and one of them already asserts the
new message.** `set_primary_validates_membership_and_reports_the_set` (`:946`) calls
`set_primary_json` three times (`:956`, `:970`, `:980`) and `selection_json` once (`:985`); each
needs the new argument (pass `true` — that test is about hiding, not privacy, and a private caller
sees the unfiltered set, so its expectations do not move). Its refusal assertion is
`err.message.contains("gamma") && err.message.contains("alpha, beta")` for a *hidden* base — which
`not_a_member` satisfies verbatim. That is the check that the mirrored sentence really is the same
sentence; if it fails, the spelling drifted.

⚠ **Do not put the check inside `kb_id_or_primary` (`:312`).** It looks like the choke point and is
not: `kb_search`, `kb_search_raw_sources`, `kb_export` and all nine writes take `kb_id` directly and
never call it. It is also how a *write* resolves its target, so a shared refusal there would report a
read error on a write. CP1 *calls* it — through `gated_kb_id` — rather than living inside it.

- [ ] **Step 4: Run**

```bash
cargo check --workspace --all-targets                  # see Task 10B's ⚠ on --lib
cargo test -p biorouter-mcp --lib knowledge::
cargo test -p biorouter-mcp --lib agent_drafter::
cargo test -p biorouter-mcp --test knowledge_macros_e2e
cargo test -p biorouter-server --test knowledge_routes
cargo test -p biorouter-server --test knowledge_routes_e2e
cargo test -p biorouter-server --lib routes::apps
```

⚠ `knowledge_macros_e2e` is here because this task adds `tier::assert_reachable` at CP2, which that
file's three macro runs now execute for real. They pass — every base it creates goes through
`create_base`, which registers public (Task 10A decision 5a), and its callers pass
`caller_is_private: false` — but "they pass" is a claim about a file no `--lib` filter compiles, so
it is run rather than assumed.

- [ ] **Step 5: Gate**

```bash
cd /Users/wgu/Desktop/BioRouter-privacy
# FOUR choke points, one check each. The smallness is the point: if these counts
# grow, someone re-scattered the barrier back across the tool surface.
grep -c "assert_kb_reachable(" crates/biorouter-mcp/src/knowledge/server.rs
echo "expect: 2 = 1 definition + 1 call, in call_tool"
grep -c "tier::assert_reachable(" crates/biorouter-mcp/src/knowledge/macros/ingest.rs \
        crates/biorouter-mcp/src/knowledge/macros/query.rs \
        crates/biorouter-mcp/src/knowledge/macros/lint.rs
echo "expect: 1 each — CP2"
grep -c "tier::assert_reachable(" crates/biorouter-server/src/routes/apps.rs ; echo "expect: 1 — CP3"
grep -c "tier::assert_reachable(" crates/biorouter-mcp/src/agent_drafter/mod.rs ; echo "expect: 1 — CP4"
# The barrier runs before the router, and before the ratchet. Non-emptiness
# first: `async fn call_tool` is macro-generated today (0 lines), and a grep over
# an empty range prints nothing, which reads exactly like "correctly ordered".
awk '/async fn call_tool/,/^    }/' crates/biorouter-mcp/src/knowledge/server.rs | wc -l
echo "expect: > 1"
awk '/async fn call_tool/,/^    }/' crates/biorouter-mcp/src/knowledge/server.rs \
  | grep -n "assert_kb_reachable\|raise_tier\|tool_router.call" | head -3
# Expected: THREE lines, in this order — assert_kb_reachable, raise_tier,
# tool_router.call. Fewer than three is a missing step, not a passing gate.
# CP4 runs before the export, not after.
awk '/fn stage_full_payload/,/^}/' crates/biorouter-mcp/src/agent_drafter/mod.rs \
  | grep -n "assert_reachable\|export_brkb(" | head -2
# Expected: assert_reachable on the SMALLER line number.
# CP3 runs before the op dispatch, so it covers reads and ingest together.
awk '/async fn handle_kb_frame/,/^}/' crates/biorouter-server/src/routes/apps.rs \
  | grep -n "resolve_kb_grant\|assert_reachable\|match op" | head -3
# Expected, in this order: resolve_kb_grant, assert_reachable, match op.
# The THREE fan-out sites filter INSIDE their loop, not before it. The third is
# the no-primary error's id list — omission, not an enumeration of what was just
# refused (see the ⚠ "the barrier must not narrate what it refuses").
for fn in search_visible_bases visible_bases_for_session; do
  echo -n "$fn: "
  awk "/fn $fn/,/^    }/" crates/biorouter-mcp/src/knowledge/server.rs \
    | grep -n "for base\|retain\|tier::is_private" | head -3
done
# Expected: the loop/retain line BEFORE (or containing) the tier check.
# ⚠ `fn kb_id_or_primary\(` WITH the paren. Measured: the bare prefix also matches
# the existing test `kb_id_or_primary_errors_with_the_candidate_list` (:886), awk
# restarts the range there, and the two concatenate to 56 lines — a gate reading a
# function it is not about. Anchored, the span is 31.
awk '/fn kb_id_or_primary\(/,/^    }/' crates/biorouter-mcp/src/knowledge/server.rs | wc -l
echo "expect: 31 today, > 1 after — assert the range before reading it"
awk '/fn kb_id_or_primary\(/,/^    }/' crates/biorouter-mcp/src/knowledge/server.rs \
  | grep -c "tier::is_private" ; echo "expect: 1 — the id list is filtered, not the whole error"
awk '/fn kb_id_or_primary\(/,/^    }/' crates/biorouter-mcp/src/knowledge/server.rs \
  | grep -n "session_kb_ids\|is_private\|ids.is_empty\|ids.join" | head -4
echo "Expected, in this order: session_kb_ids, is_private, ids.is_empty, ids.join —"
echo "  the filter runs BEFORE the empty check, or an all-private session gets"
echo "  'Pass kb_id explicitly (one of: )' instead of 'this session has none'."
# The gated list is exactly fourteen, and the five exemptions are not in it.
# ⚠ `grep -o | wc -l`, not `grep -c`: see Task 10B Step 5 — `grep -c` counts
# lines, and rustfmt decides how many lines a const array occupies. This one
# happens to explode one-element-per-line today; its sibling does not.
awk '/const KB_ID_GATED_TOOLS/,/\];/' crates/biorouter-mcp/src/knowledge/server.rs \
  | grep -oE '"kb_[a-z_]+"' | wc -l ; echo "expect: 14"
awk '/const KB_ID_GATED_TOOLS/,/\];/' crates/biorouter-mcp/src/knowledge/server.rs \
  | grep -oE '"kb_(list_bases|create_base|set_active|get_active|import)"' | wc -l ; echo "expect: 0"
# Every tool the router knows is either gated or explicitly exempt — the test
# that turns a twentieth tool into a failure rather than a silent hole.
# ⚠ Assert "1 passed", not the exit code: a libtest filter that matches nothing
# prints `0 passed` and exits 0 (see "Which test filters are validated").
cargo test -p biorouter-mcp --lib \
  knowledge::server::tests::every_kb_tool_is_gated_or_exempt_for_a_pinned_reason \
  | grep "test result:" ; echo "expect: 1 passed; 0 failed"
# EVERY exemption names a test that EXISTS. A `pinned_by` string is compiled but
# never resolved — Rust cannot name a test function from another test — so the
# one thing that stops it being decorative is this grep. An exemption whose
# pinning test does not exist is the blanket exemption back, wearing a field name.
awk '/const EXEMPT: &\[ExemptTool\]/,/^\];/' crates/biorouter-mcp/src/knowledge/server.rs \
  | grep -oE 'pinned_by: "[a-z_]+"' | sed -E 's/.*"(.*)"/\1/' | sort -u \
  | while read -r t; do
      echo -n "$t: "
      grep -rc "fn $t\b" crates/biorouter-mcp/src/knowledge/server.rs
    done
echo "expect: 1 each — a 0 means an exemption cites a test nobody wrote"
awk '/const EXEMPT: &\[ExemptTool\]/,/^\];/' crates/biorouter-mcp/src/knowledge/server.rs \
  | grep -c "name:" ; echo "expect: 5 — and each has a why and a pinned_by beside it"
# The universal metadata property runs, and it is the one that fails a leaking
# TWENTIETH exempt tool without anyone having to think of it.
cargo test -p biorouter-mcp --lib \
  knowledge::server::tests::no_exempt_tool_volunteers_a_private_bases_id_to_a_public_caller \
  | grep "test result:" ; echo "expect: 1 passed; 0 failed"
# NEW-SURFACE detector. These are the only ways to reach base CONTENT from
# outside `knowledge/`; a fifth one appearing is precisely what CP1..CP4 cannot
# cover by construction, so it is a gate rather than a hope. Both counts are
# measured against the tree at 9558c346 and must not grow.
grep -rn "store::\(list_pages\|read_page\|write_page\|search\|search_with_scope\)(" \
  --include='*.rs' crates/ | grep -v "src/knowledge/" | sort
echo "expect: exactly 4 — routes/apps.rs:2394 (run_kb_read, covered by CP3) and"
echo "        routes/knowledge.rs:523, :544, :571 (the Knowledge view, ungated by decision)"
grep -rln "\.\(export_brkb\|import_brkb\|read_page\|get_graph\|list_history\|add_raw_source\|restore_state\|preview_state\)(" \
  --include='*.rs' crates/*/src/ | grep -v "/knowledge/" | sort
echo "expect: exactly 4 FILES — agent_drafter/mod.rs (CP4), routes/apps.rs (CP3),"
echo "        routes/knowledge.rs (ungated by decision), bin/knowledge_ingest_probe.rs"
echo "        (a dev probe whose write goes through the ingest macro, CP2)"
echo "NOTE: crates/*/src/ excludes tests/, which legitimately call the service directly"
echo "      (knowledge_macros_e2e.rs, knowledge_revert_integration.rs, knowledge_e2e.rs)"
echo "⚠ BOTH detectors are blind to base METADATA by construction: neither pattern"
echo "  names list_bases or session_kb_ids, because those return ids and names"
echo "  rather than content. That is exactly the hole the two leaks in this round"
echo "  came through. Task 10D owns the metadata detector; run it too."
# The two POINTER tools filter the VIEW and never the store. This is the gate
# for the wrong implementation that is invisible in the tools' output: filtering
# inside `service::selection` produces identical JSON and silently re-points the
# user's primary, because `repair_decision` (service.rs:1405) writes
# `next_ids.first()` to disk.
grep -c "fn visible_kb_ids(" crates/biorouter-mcp/src/knowledge/server.rs ; echo "expect: 1"
grep -rn "tier::is_private\|caller_is_private\|caller_private" \
  crates/biorouter-mcp/src/knowledge/service.rs
echo "expect: no output — the SERVICE stays capability-blind. A hit here means the"
echo "  filter was pushed into the store, where repair_decision consumes it."
# Both tools reach the filter, and all THREE serialised fields move together.
awk '/fn selection_value/,/^    }/' crates/biorouter-mcp/src/knowledge/server.rs | wc -l
echo "expect: 17 today, > 1 after — assert the range before reading it"
awk '/fn selection_value/,/^    }/' crates/biorouter-mcp/src/knowledge/server.rs \
  | grep -c "visible_kb_ids" ; echo "expect: 1"
awk '/fn selection_value/,/^    }/' crates/biorouter-mcp/src/knowledge/server.rs \
  | grep -n "visible_kb_ids\|primary_kb\|active_kb" | head -4
echo "Expected, in this order: visible_kb_ids, then primary_kb, then active_kb —"
echo "  the deprecated mirror is derived from the filtered pointer, not from"
echo "  selection.primary_kb a second time (which is the half-fix)."
# `kb_set_active` answers from its own list, so the service's candidate list —
# built from the whole set — is unreachable from a tool.
awk '/fn set_primary_json/,/^    }/' crates/biorouter-mcp/src/knowledge/server.rs | wc -l
echo "expect: 18 today, > 1 after"
awk '/fn set_primary_json/,/^    }/' crates/biorouter-mcp/src/knowledge/server.rs \
  | grep -n "visible_kb_ids\|not_a_member\|set_selection" | head -4
echo "Expected, in this order: visible_kb_ids, not_a_member, set_selection, not_a_member —"
echo "  the membership decision precedes the write, and the write's own error is"
echo "  re-answered rather than propagated."
grep -c "fn not_a_member(" crates/biorouter-mcp/src/knowledge/server.rs ; echo "expect: 1"
# The refusal does not say 'private' — the same rule as Task 10D's validators.
awk '/fn not_a_member/,/^}/' crates/biorouter-mcp/src/knowledge/server.rs \
  | grep -ci "private" ; echo "expect: 0"
# The GUI read routes are untouched, deliberately.
for h in get_graph list_pages read_page get_page_body list_history preview_state export_brkb; do
  echo -n "$h: "
  awk "/pub async fn $h/,/^}/" crates/biorouter-server/src/routes/knowledge.rs \
    | grep -c "tier::is_private\|assert_reachable"
done
echo "expect: 0 each — the Knowledge view is the user, not a model (see the ⚠ above)."
echo "All seven names verified present as top-level 'pub async fn' at :466 :517 :539"
echo ":817 :843 :862 :1518, so this loop is not a vacuous pass over empty awk ranges."
# The refusal is one string in one place, so CP1..CP4 cannot drift apart.
grep -rn "KB_PRIVATE_REFUSAL" --include='*.rs' crates/ | grep -v "knowledge/tier.rs"
echo "expect: no output — every surface reaches it through tier::assert_reachable"
```

**What this catches.** Five wrong implementations. (1) Gating `kb_search` only — literally what the
finding names — which leaves `kb_read_page`, `kb_list_pages`, `kb_get_graph`, `kb_list_history`,
`kb_search_raw_sources` and `kb_export` open, and `kb_export` writes the whole base to disk in one
call. The nineteen-tool test is the only thing that fails it. (2) Checking only an **explicit**
`kb_id`, which four tools bypass by resolving the session's primary — `omitting_the_kb_id_is_not_the_bypass`
is the only test that fails it, and it is the defect the previous draft's "sixteen call sites"
wording made easy to write. (3) A single up-front guard in `search_visible_bases`, turning a KB-less
search into all-or-nothing so one private base costs the user every other base. (4) Filtering hits
*after* `search_with_scope` returns rather than skipping the base — which reads the private base's
index off disk, and is the same post-filter mistake Gate D's `LIMIT` test exists to catch one crate
over. (5) Gating CP1 and stopping, which leaves `run_kb_read` and `export_app` — the two surfaces
that never touch `KnowledgeServer` — exactly as open as they are today; the per-file
`assert_reachable` counts and the new-surface detector are what fail it. (6) Filtering every path
that returns base *content* and none that returns a base *id*, so the refusal itself names the base
it refused — `the_no_primary_error_names_only_the_bases_the_caller_may_reach` is the only test that
fails it, and neither new-surface detector can see it, because both key on `store::` and on service
content calls while this one goes through `session_kb_ids`. Task 10D closes the same class in
`agent_drafter`. (7) Leaving the two **pointer** tools alone because "the caller already knows the
id" — which is false for `kb_get_active`, a no-argument tool that returns the whole selection, so
one call enumerates every visible base including the private ones and names the primary;
`kb_get_active_does_not_enumerate_a_private_base_or_point_at_one` is the only test that fails it,
and its *store* assertion is the only thing that fails the plausible half-fix of filtering inside
`service::selection`, which produces identical JSON while re-pointing the user's primary on disk.
(8) **The completeness test blessing the hole.** Its previous form was
`const EXEMPT: &[&str] = &["kb_list_bases", "kb_create_base", "kb_set_active", "kb_get_active",
"kb_import"]` — five bare strings and an equality assertion, so it passed *because* the leaking
tools were named in it, and it would go on passing for a twentieth exempt tool that leaked in a new
way. **This gate rejects: adding a `kb_*` tool to `EXEMPT` to make the partition pass.** The
exemption now carries the test that pins the behaviour it claims, Step 5 greps every `pinned_by` for
a real `fn` (a `pinned_by` string is compiled and never resolved — Rust cannot name a test from
another test — so without that grep the field is decoration), and
`no_exempt_tool_volunteers_a_private_bases_id_to_a_public_caller` drives every exempt tool with
arguments that name **only** the public base and fails on any output containing the private base's
id or name. Arguments that name only the public base are what make it a rule about *volunteering*
rather than *echoing*, which is the AR-5/DR-7 line: `kb_set_active {kb_id: "omop"}` may say "omop"
back, because the caller supplied it; `kb_get_active {}` may not.

- [ ] **Step 6: Commit**

```bash
git add crates/biorouter-mcp/src/knowledge/ crates/biorouter-mcp/src/agent_drafter/mod.rs \
        crates/biorouter-server/src/routes/knowledge.rs crates/biorouter-server/src/routes/apps.rs
cargo check --workspace --all-targets
git commit -m "feat(knowledge): refuse a public model on a private knowledge base at all four choke points (#56)"
```

---

### Task 10D: The metadata surface — CP5, because a barrier that names what it refused has not refused it

Tasks 10B and 10C stop base **content** at four choke points. They do not stop the base's **id and
name**, and one model-facing tool hands both over for every base on the machine with no arguments at
all.

`list_platform_catalog` (`agent_drafter/mod.rs:2626`) serialises `catalog::Catalog::discover()`
whole; `discover_kbs` (`catalog.rs:125-141`) maps `service.list_bases()` to `{id, name}` for **every**
base with no filter of any kind. Its own tool description instructs the model to *"Call this BEFORE
configure_app"*, so it is not an edge case — it is the routine first call of every app-building turn.
And `validate.rs` renders `Catalog::render_list(&catalog.kb_ids())` into three `INVALID_PARAMS`
strings the model reads back (`:33`, `:42`, `:52`), which makes a deliberately-invalid
`configure_app {knowledge_base: "br.kb"}` an enumeration oracle that needs no valid input at all.

Neither of Task 10C's new-surface detectors sees it, and not by accident: one keys on
`store::(list_pages|read_page|write_page|search|search_with_scope)` and the other on
`KnowledgeService` **content** methods. `list_bases` is in neither, because it returns metadata. CP1
does not reach it either — `agent_drafter` keeps its own `#[tool_handler]` (`:2882`) — and CP4 covers
only `stage_full_payload`.

This contradicts the plan's own ruling, one crate over: Task 10C asserts
`kb_list_bases_omits_a_private_base_rather_than_redacting_it` *because* **"a KB name is user-authored
and routinely names a cohort or a study"**. The same list, from the same call, through a different
tool, cannot be public.

⚠ **What is in scope here and what is not — stated, because DR-7 rules the neighbouring thing out.**
DR-7 puts side channels (existence, counts, timing) out of scope for `chatrecall`, and this task
keeps that ruling exactly: nothing below pads a count, equalises a latency or plants a decoy. A
public session can still *ask* whether a given id exists and get a truthful answer —
`create_base` bails with `"kb '{id}' already exists at {path}"` (`service.rs:451`), and this task does
not touch it. That residual is [AR-5](#ar-5--the-existence-of-a-private-knowledge-base-is-still-inferable).
The line is: **being asked about one guessed id is a side channel; volunteering the whole list is the
content crossing.** A user-authored KB name is content by this plan's own rule, and the id is the one
argument that makes the explicit-`kb_id` branch — the finding Task 10C exists to close — writable
without guessing.

**CP5 is `Catalog::discover`, and it is a real choke point, measured.** Every consumer of a knowledge
base's metadata inside `agent_drafter` and inside the app runtime goes through it: `grep -rn
"Catalog::discover()" --include='*.rs' crates/` returns **12** hits — **6 in production**
(`agent_drafter/mod.rs:1090` via `persist_created_app`←`create_app`, `:2071` `configure_app`, `:2202`
`update_app`'s manifest path, `:2511` `declare_profiles`, `:2627` `list_platform_catalog`, and
`routes/apps.rs:772` `capability_report`), 4 in in-file test modules and 2 in
`crates/biorouter-mcp/tests/`. Giving it a required parameter makes all six production sites a
compile error and is a **twelve-edit** change a reviewer can read in one screen — the forcing function
Task 10A decision (5a) rejected for `create_base` only because that one measured ~90 sites.

**Files:**

| Action | Path | Anchor (re-verified at `9558c346`) |
|---|---|---|
| Modify | `crates/biorouter-mcp/src/agent_drafter/catalog.rs` | **CP5.** `Catalog::discover()` `:69-82` (14 lines) gains `caller_is_private: bool`; `discover_kbs()` `:125-141` (17 lines) gains the filter; `kb_ids` `:102-104`, `render_list` `:116-122`, `has_kb` `:90-92` all read from the filtered vector and need no change; `mod tests` `:265`, whose construction is `:270` |
| Modify | `crates/biorouter-mcp/src/agent_drafter/mod.rs` | the five production `Catalog::discover()` sites `:1090`, `:2071`, `:2202`, `:2511`, `:2627`; `list_platform_catalog` `:2626` (declared `:2615-2625`), `configure_app` `:2033` (`:2029`), `update_app` `:2130` (`:2126`) and `declare_profiles` `:2501` (`:2497`) each gain `context: RequestContext<RoleServer>`; `create_app` `:1975-1979` **already has one** and threads it through `create_app_inner` `:1987` into `persist_created_app` `:1072-1084`; `session_id_from_context` `:1575-1582` is the in-file precedent to mirror |
| Modify | `crates/biorouter-server/src/routes/apps.rs` | `capability_report` `:768-806` gains `caller_is_private: bool`; its sole caller `configure_agent` calls it at `:1257` — **and that call MOVES**, to below `configure_main_provider` `:1259` and `warn_invalid_model_routes` `:1260`. See the ⚠ "the report must be computed from the provider actually bound". Also `configure_worker_agent` `:1544-1564`, whose KB grant `:1561` runs with no report at all |
| Modify | `crates/biorouter-mcp/tests/catalog_write_boundary.rs` | `Catalog::discover()` `:54` — an integration test `--lib` cannot compile (Task 10B ⚠) |
| Modify | `crates/biorouter-mcp/tests/testdrive_corpus_relint.rs` | `Catalog::discover()` `:103` — same |
| Reference | `crates/biorouter-mcp/src/agent_drafter/validate.rs` | `check_knowledge_base` `:18-58` — the three `Catalog::render_list(&catalog.kb_ids())` renderings at `:33`, `:42`, `:52` (`:78` and `:98` are the skill and extension lists, out of scope). **Unchanged**, and that is the point: filtering at CP5 fixes all three at once, because they read the catalog they are handed; the in-file test constructions are `:179`, `:250`, `:261` |

⚠ **The metadata register — every model-facing tool in the tree that can return a KB id or name,
and what pins it.** The detectors below find *call sites*; this finds *tools*, which is the level a
leak is actually reasoned about. It is the artefact the round that produced Task 10D was missing:
both leaks it found (`list_platform_catalog`, and then `kb_get_active` one review later) were tools
nobody had enumerated, reached through call sites the detectors excluded.

| Tool | Surface | Behaviour required | Task | Pinned by |
|---|---|---|---|---|
| `kb_list_bases` | `KnowledgeServer` | omit private rows | 10C | `kb_list_bases_omits_a_private_base_rather_than_redacting_it` |
| `kb_get_active` | `KnowledgeServer` | omit ids; pointer `null` | 10C | `kb_get_active_does_not_enumerate_a_private_base_or_point_at_one` |
| `kb_set_active` | `KnowledgeServer` | not-a-member, filtered list | 10C | `a_private_target_and_a_nonexistent_one_are_indistinguishable_to_kb_set_active` |
| `kb_read_page`, `kb_list_pages`, `kb_get_graph`, `kb_list_history` | `KnowledgeServer`, **no-primary error path only** | filtered candidate list | 10C | `the_no_primary_error_names_only_the_bases_the_caller_may_reach` |
| every other `kb_*` | `KnowledgeServer` | volunteers nothing | 10C | `no_exempt_tool_volunteers_a_private_bases_id_to_a_public_caller` + the 19-tool probe |
| `platform__ingest_conversation` | `Agent` | filtered candidate list | 11 | `the_no_target_error_names_only_the_bases_the_caller_may_reach` |
| `list_platform_catalog` | `agent_drafter` | omit from the catalog | **10D** | `list_platform_catalog_is_scoped_to_the_calling_sessions_capability` |
| `create_app`, `configure_app`, `update_app`, `declare_profiles` | `agent_drafter` (validator strings) | omit from the catalog | **10D** | `every_drafter_tool_that_builds_a_catalog_scopes_it` |
| `br.kb` frames | the app socket | names no base the manifest did not | — | `resolve_kb_grant` (`routes/apps.rs:2268-2298`) reads the **manifest only** and never the store, so it cannot enumerate — measured, not assumed |
| `kb_create_base`, `kb_import` | `KnowledgeServer` | existence of a **supplied** id | AR-5 | out of scope by DR-7 — the caller named it |

**This gate rejects: a new model-facing tool that returns `{id, name}` for every base.** It is
rejected twice — once by whichever detector sweep sees its call site, and once by the register,
which a reviewer can read in one screen and which fails on the question "which row is this?".
Neither detector alone would have rejected `kb_get_active`.

⚠ **Filter the catalog, do not add a check to the validators.** The validators are the tempting
place — they are where the string is formatted — but there are three of them for knowledge bases
alone, plus `has_kb`, plus the report's `missing_knowledge_base`, and a private base that is *absent
from the catalog* fixes every one of them with no second rule to keep in sync. It also produces the
right behaviour for free: a public session that names a private base by hand gets *"knowledge base
'omop' is not installed on this Biorouter"* — the **omission** semantics Task 10C chose for
`kb_list_bases`, not a redaction and not a privacy refusal that would itself confirm the base exists.

⚠ **The report must be computed from the provider actually bound, so the call moves.** Today
`configure_agent` computes `let mut report = capability_report(cfg);` at `:1257` and *then* calls
`configure_main_provider(agent, session_id, manifest, cfg)` at `:1259`. Adding the capability read at
the existing position — which is what the previous draft said to do — reads the provider the session
was holding **before** the manifest's own `model` was bound, and an app's manifest routinely names a
different one (`configure_main_provider` `:809-855`: `cfg.model` → `create_provider` →
`agent.update_provider`, falling back to the global provider at `:834-854` when that fails). Both
inversions are wrong and both are silent:

| Global provider | Manifest `model` | Report computed at `:1257` | Consequence |
|---|---|---|---|
| private | public | **private** | the app's public model is handed the private catalog, and `configure_agent` `grant_knowledge_base`s a private base to it at `:1276` — arming its KB tools |
| public | private | **public** | the app's private model loses every private base for the whole session, for no reason the user can see |

So: bind first, then report. Move the `capability_report` call to **below** `configure_main_provider`
and `warn_invalid_model_routes`; nothing between the two positions reads `report` (measured — the
first consumer is `configure_main_extensions(agent, manifest, cfg, &report)` at `:1262`), so this is
a two-line move and not a restructuring:

```rust
    configure_main_provider(agent, session_id, manifest, cfg).await;
    warn_invalid_model_routes(manifest, cfg).await;

    // Issue #56. AFTER the bind, never before: `capability_report` used to run
    // above `configure_main_provider`, so it read whatever provider the session
    // held before the manifest's `model` was applied. Reading the provider the
    // agent ACTUALLY ended up with is also the only value that survives
    // `configure_main_provider`'s fallbacks (:830-855) and Gate A refusing the
    // manifest's provider on a private session (Task 12) — the same rule Task
    // 10B applies to the HTTP macro routes: the constructed instance, never the
    // requested name.
    let caller_is_private = agent
        .provider()
        .await
        .map(|p| p.tier())
        .unwrap_or(biorouter::privacy::ProviderTier::Public)
        .is_private();
    let mut report = capability_report(cfg, caller_is_private);
```

⚠ **The worker path grants a base with no report at all, and it is the same defect one function
over.** `configure_worker_agent` (`:1544`) calls `configure_worker_provider` (`:1553`) and then
`grant_knowledge_base(&state.knowledge_service, session_id, kb)` (`:1561`) straight from
`cfg.knowledge_base`, with no `capability_report` between them. `grant_knowledge_base` is
`include_kb(.., PrimaryUpdate::Set(kb))` (`:1234-1244`), so it **un-hides the base in that worker's
session and makes it the primary** — meaning a public worker profile that names a private base gets
that base pinned as its KB-less write target. Task 10C refuses the reads and Task 10B stamps the
writes, so this is not a content crossing; it is the same "arming a tool for a grant that cannot be
satisfied" the report's own comment (`:769-771`) exists to prevent, plus a moved pointer. Ordering
is already right here (provider at `:1553`, grant at `:1561`) — the fix is one `if` before the grant,
using the same expression, and it is in this task because it is the same read of the same value:

```rust
    if let Some(kb) = cfg.knowledge_base.as_ref() {
        // Issue #56. The worker's OWN capability — `configure_worker_provider`
        // ran four lines up and may have bound a different tier than the main
        // agent's. A base this profile may not read is not granted, for the
        // reason at :769-771: never arm a tool for a grant that cannot be
        // satisfied.
        let worker_is_private = agent.provider().await.map(|p| p.tier())
            .unwrap_or(biorouter::privacy::ProviderTier::Public).is_private();
        if !Catalog::discover(worker_is_private).has_kb(kb) {
            warn!(app = %manifest.id, profile = %profile_name, kb = %kb,
                  "profile names a knowledge base that is not available to it");
        } else if let Err(e) = grant_knowledge_base(&state.knowledge_service, session_id, kb) {
```

⚠ **`Catalog::discover` gains a `bool`, not a `ProviderTier`.** `agent_drafter` is in
`biorouter-mcp`, which cannot depend on `biorouter` — Task 10A decision (1), the same constraint that
made `IngestArgs` take a bool. `routes/apps.rs` (in `biorouter-server`) does the `ProviderTier → bool`
crossing at its one call site.

- [ ] **Step 1: Write the failing tests**

```rust
// crates/biorouter-mcp/src/agent_drafter/catalog.rs, in its #[cfg(test)] mod tests (:265)

#[test]
fn the_catalog_omits_a_private_knowledge_base_from_a_public_caller() {
    // The headline. `discover_kbs` had NO filter, and the tool that returns it
    // tells the model to call it before configure_app.
    let root = drafter_catalog_root_with_kbs(&["default", "omop"]);
    tier::raise_unlocked(&root, "omop", true).unwrap();

    let public = Catalog::discover(/* caller_is_private */ false);
    assert_eq!(public.kb_ids(), vec!["default"]);
    assert!(!serde_json::to_string(&public).unwrap().contains("omop"),
            "the id or the NAME survived serialisation");

    let private = Catalog::discover(true);
    assert_eq!(private.kb_ids(), vec!["default", "omop"]);
}

// The next two live in `validate.rs`'s own `#[cfg(test)] mod tests` (:158-159),
// NOT in catalog.rs's (:264-265) — they call `check_knowledge_base`, and a test
// in catalog.rs would have to reach it as `super::validate::…`. Filter:
// `cargo test -p biorouter-mcp --lib agent_drafter::validate`.

#[test]
fn a_rejection_message_cannot_be_used_to_enumerate_private_bases() {
    // validate.rs:33/:42/:52 render `render_list(&catalog.kb_ids())` into
    // INVALID_PARAMS strings the model reads. `br.kb` is the exact input the
    // 100-app test drive produced, so this is the live path, not a contrivance.
    let root = drafter_catalog_root_with_kbs(&["default", "omop"]);
    tier::raise_unlocked(&root, "omop", true).unwrap();
    let public = Catalog::discover(false);
    for probe in ["br.kb", "NOT A VALID ID", "clinvar"] {
        let e = validate::check_knowledge_base(probe, &public).unwrap_err();
        assert!(!e.contains("omop"), "{probe} enumerated a private base: {e}");
        assert!(e.contains("default"), "{probe} lost the public bases too: {e}");
    }
}

#[test]
fn a_public_session_cannot_configure_an_app_against_a_private_base() {
    // Omission, not refusal: the message must read "not installed", which is
    // what a public caller can truthfully be told. A message that said
    // "private" would confirm the base exists, which is the leak in a politer
    // sentence.
    let root = drafter_catalog_root_with_kbs(&["omop"]);
    tier::raise_unlocked(&root, "omop", true).unwrap();
    let e = validate::check_knowledge_base("omop", &Catalog::discover(false)).unwrap_err();
    assert!(e.contains("not installed"), "{e}");
    assert!(!e.to_lowercase().contains("private"), "{e}");
    assert!(validate::check_knowledge_base("omop", &Catalog::discover(true)).is_ok());
}
```

```rust
// crates/biorouter-mcp/src/agent_drafter/mod.rs, in its #[cfg(test)] mod tests

#[tokio::test]
async fn list_platform_catalog_is_scoped_to_the_calling_sessions_capability() {
    // Driven THROUGH the tool with a meta-carrying context, not by calling
    // `Catalog::discover(false)` directly — otherwise the test proves the
    // filter works and says nothing about whether the tool passes the right
    // argument, which is the whole of the bug.
    let (srv, root) = drafter_at_root_with_kbs(&["default", "omop"]);
    tier::raise_unlocked(&root, "omop", true).unwrap();

    let public = call_drafter_tool_as(&srv, "list_platform_catalog", json!({}), Public).await;
    assert!(!rendered(&public).contains("omop"));
    let private = call_drafter_tool_as(&srv, "list_platform_catalog", json!({}), Private).await;
    assert!(rendered(&private).contains("omop"));
}

#[tokio::test]
async fn every_drafter_tool_that_builds_a_catalog_scopes_it() {
    // Parameterised, for the same reason 10B/10C parameterise over all
    // nineteen kb_* tools: fixing the tool whose NAME says "catalog" leaves
    // four validators enumerating the same list through their error strings.
    let (srv, root) = drafter_at_root_with_kbs(&["default", "omop"]);
    tier::raise_unlocked(&root, "omop", true).unwrap();
    for (tool, args) in CATALOG_BUILDING_TOOLS {   // list_platform_catalog,
        let out = call_drafter_tool_as(&srv, tool, args(), Public).await;
        assert!(!rendered(&out).contains("omop"),  // create_app, configure_app,
                "{tool} leaked a private base id");//  update_app, declare_profiles
    }
}

#[tokio::test]
async fn every_drafter_tool_that_can_name_a_base_is_in_the_register() {
    // The register, as a test rather than only as a table: no drafter tool may
    // produce a base id it was not given. Universal over the WHOLE router, not
    // over a hand-picked list — `CATALOG_BUILDING_TOOLS` above is the five this
    // task knows about, and the leak this task exists to close was a tool
    // nobody had enumerated. Arguments name only the public base, so a hit is
    // volunteering and not echoing (Task 10C's rule, same sentence).
    let (srv, root) = drafter_at_root_with_kbs(&["default", "omop-cohort-412"]);
    tier::raise_unlocked(&root, "omop-cohort-412", true).unwrap();
    for tool in srv.tool_router().list_all() {
        let out = call_drafter_tool_as(&srv, &tool.name,
                                       benign_args_for(&tool), Public).await;
        assert!(!rendered(&out).contains("omop-cohort-412"),
                "{} volunteered a private base id — add it to the metadata register \
                 or scope it", tool.name);
    }
    // …and a private caller still sees it, so the assertion above cannot be
    // satisfied by a router that answers nothing.
    let out = call_drafter_tool_as(&srv, "list_platform_catalog", json!({}), Private).await;
    assert!(rendered(&out).contains("omop-cohort-412"));
}
```

```rust
// crates/biorouter-server/src/routes/apps.rs, in its existing `mod tests`

#[tokio::test]
async fn the_app_capability_report_follows_the_MANIFESTS_provider_not_the_global_one() {
    // Driven through `configure_agent`, never by calling `capability_report`
    // directly — a direct call proves the parameter works and says nothing
    // about WHERE it is read, which is the whole of the bug. `capability_report`
    // ran at :1257 and `configure_main_provider` at :1259, so the report saw the
    // provider the session held BEFORE the manifest's `model` was bound.
    //
    // Both inversions, because each is silent on its own and a fix that hardcodes
    // either literal passes one of them.
    let (state, root) = app_state_with_kb("omop");
    tier::raise_unlocked(&root, "omop", true).unwrap();

    // Global PRIVATE, manifest PUBLIC → the app runs public and must NOT get it.
    let agent = agent_bound_to(private_provider()).await;
    let report = configure_agent(&agent, &state, "app:x:c1",
                                 &manifest_with(public_model(), kb("omop")),
                                 &bridge, false).await;
    assert_eq!(report.granted_knowledge_base, None,
               "a public manifest model received the private catalog");
    assert_eq!(report.missing_knowledge_base.as_deref(), Some("omop"));
    // And the grant really did not happen — the report is only a claim.
    assert!(!session_kb_ids(&root, "app:x:c1").contains(&"omop".to_string()));

    // Global PUBLIC, manifest PRIVATE → the app runs private and must get it.
    let agent = agent_bound_to(public_provider()).await;
    let report = configure_agent(&agent, &state, "app:x:c2",
                                 &manifest_with(private_model(), kb("omop")),
                                 &bridge, false).await;
    assert_eq!(report.granted_knowledge_base.as_deref(), Some("omop"),
               "a private manifest model wrongly lost its own base");
}

#[tokio::test]
async fn a_public_worker_profile_is_not_granted_a_private_base() {
    // `configure_worker_agent` (:1544) grants `cfg.knowledge_base` at :1561 with
    // no report between it and `configure_worker_provider` (:1553), and
    // `grant_knowledge_base` is `include_kb(.., PrimaryUpdate::Set(kb))` — so
    // the base is un-hidden in that worker's session AND made its KB-less write
    // target. Main private, worker public: the worker must not receive it.
    let (state, root) = app_state_with_kb("omop");
    tier::raise_unlocked(&root, "omop", true).unwrap();
    let app = configure_app_with_worker(&state, "analyst",
                                        /* main */ private_model(),
                                        /* worker */ public_model(), kb("omop")).await;
    assert!(!session_kb_ids(&root, &app.worker_session_id("analyst"))
                .contains(&"omop".to_string()));
    assert_ne!(stored_primary(&root, &app.worker_session_id("analyst")).as_deref(), Some("omop"));
    // The main agent, which IS private, keeps it.
    assert!(session_kb_ids(&root, &app.main_session_id()).contains(&"omop".to_string()));
}
```

- [ ] **Step 2: Run**

```bash
cargo test -p biorouter-mcp --lib agent_drafter::catalog    # 5 today (measured, Task 4b); assert 5 + 1
cargo test -p biorouter-mcp --lib agent_drafter::validate
cargo test -p biorouter-mcp --lib agent_drafter::           # 244 today (measured); assert 244 + 6
cargo test -p biorouter-server --lib routes::apps           # 90 today (measured); assert 90 + 2
```

Expected: **COMPILE ERROR** first — `Catalog::discover` takes 0 arguments, `capability_report` takes
1, and four of the five drafter tools have no `context` to read. Then **FAIL** on every omission
assertion.

- [ ] **Step 3: Implement**

(a) `catalog.rs` — CP5, three lines:

```rust
    /// Scan this install, from the point of view of a caller with this
    /// capability (issue #56).
    ///
    /// `caller_is_private == false` omits every knowledge base whose tier is
    /// private, exactly as `kb_list_bases` does (Task 10C): a KB id and name are
    /// user-authored and routinely name a cohort or a study, so they are content
    /// and not an existence side channel (DR-7 covers the latter and this does
    /// not chase it — see the task's ⚠).
    ///
    /// A `bool` and not `ProviderTier` because `biorouter-mcp` cannot depend on
    /// `biorouter` — Task 10A ⚠(1).
    pub fn discover(caller_is_private: bool) -> Self {
        Self {
            knowledge_bases: discover_kbs(caller_is_private),
            …unchanged…
        }
    }
```

```rust
fn discover_kbs(caller_is_private: bool) -> Vec<KbEntry> {
    let Ok(service) = crate::knowledge::service::KnowledgeService::new_default() else {
        return Vec::new();
    };
    let root = service.root().to_path_buf();
    service
        .list_bases()
        .map(|bases| {
            bases
                .into_iter()
                // Issue #56. Per base, and BEFORE the map: a filter after the
                // KbEntry is built is the same code with one more chance to be
                // reordered into a post-filter on a serialised string.
                .filter(|m| caller_is_private || !crate::knowledge::tier::is_private(&root, &m.id))
                .map(|m| KbEntry { id: m.id, name: m.name })
                .collect()
        })
        .unwrap_or_default()
}
```

(b) `agent_drafter/mod.rs` — the four tools that need a capability gain a `RequestContext`, using
the in-file precedent (`session_id_from_context` `:1575`) and the shared reader:

```rust
    /// The caller's capability, from the request meta (issue #56). Delegates to
    /// `knowledge::tier` rather than re-reading the key, so CP4 and CP5 cannot
    /// drift from CP1 — the same reason `KnowledgeServer::caller_is_private`
    /// delegates (Task 10A (f)).
    fn caller_is_private(context: &RequestContext<RoleServer>) -> bool {
        crate::knowledge::tier::caller_is_private(&context.meta)
    }
```

`create_app` already has its `RequestContext`; thread the bool through `create_app_inner` beside the
`session_id: Option<String>` it already threads, into `persist_created_app`. `configure_app`,
`update_app` and `declare_profiles` each gain the parameter. Task 10B gave `export_app` one for CP4,
so after this task **six** drafter tools carry a context and the rest do not.

(c) `routes/apps.rs` — `capability_report(cfg, caller_is_private)`, resolved in `configure_agent`
from the `agent` it already holds, **at its new position below `configure_main_provider`** — the code
and the reason are in the ⚠ "the report must be computed from the provider actually bound" above,
together with the one `if` `configure_worker_agent` needs before its own grant. Same fail-closed
expression as CP3's three call sites, for the same reason.

- [ ] **Step 4: Run**

```bash
cargo check --workspace --all-targets                  # see Task 10B's ⚠ on --lib
cargo test -p biorouter-mcp --lib agent_drafter:: 2>&1 | grep "test result:"
cargo test -p biorouter-mcp --test catalog_write_boundary
cargo test -p biorouter-mcp --test testdrive_corpus_relint
cargo test -p biorouter-mcp --test ui_example_apps
cargo test -p biorouter-server --lib routes::apps 2>&1 | grep "test result:"
```

Expected: **PASS**. The two `--test` lines are the two integration files that construct a `Catalog`
and that no `--lib` filter compiles; `ui_example_apps` is here because it drives real drafter tools
whose signatures moved.

- [ ] **Step 5: Gate**

```bash
cd /Users/wgu/Desktop/BioRouter-privacy
# CP5 exists and is the ONLY constructor: a second discovery path is how this
# leak comes back.
grep -c "pub fn discover(" crates/biorouter-mcp/src/agent_drafter/catalog.rs ; echo "expect: 1"
awk '/^fn discover_kbs\(/,/^}/' crates/biorouter-mcp/src/agent_drafter/catalog.rs | wc -l
echo "expect: 17 today, > 1 after — assert the range before reading it"
awk '/^fn discover_kbs\(/,/^}/' crates/biorouter-mcp/src/agent_drafter/catalog.rs \
  | grep -n "list_bases\|tier::is_private\|KbEntry" | head -3
# Expected, in this order: list_bases, tier::is_private, KbEntry — the filter is
# on the manifest, before the entry is built, not on the rendered output.
# Every production caller passes a real capability; none hardcodes the trusting one.
grep -rn "Catalog::discover(" --include='*.rs' crates/*/src/ crates/*/tests/ | sort
echo "expect: 12 hits — 6 production (agent_drafter/mod.rs :1090 :2071 :2202 :2511 :2627,"
echo "  routes/apps.rs :772), 4 in-file tests (catalog.rs, validate.rs x3), 2 in crates/*/tests/"
grep -rn "Catalog::discover(true)" --include='*.rs' crates/*/src/ ; echo "expect: no output"
echo "  (a hardcoded 'true' is 'this caller is trusted' — the mirror of Task 10B's"
echo "   hardcoded caller_is_private, and the only way to compile while disabling CP5)"
# The four tools that had no context now have one, and the ones that need none did
# not grow one. PER TOOL, never a total: a total is satisfied by the wrong four.
# Every range below was run at 9558c346 and is non-empty with exactly ONE start
# (spans 7-12 lines), so this loop is not a vacuous pass over empty awk ranges.
for t in list_platform_catalog configure_app update_app declare_profiles export_app create_app; do
  echo -n "$t: " ; awk "/name = \"$t\"/,/-> Result<CallToolResult/" \
    crates/biorouter-mcp/src/agent_drafter/mod.rs | grep -c "RequestContext"
done
echo "expect: 1 each — six tools carry a context after Tasks 10B and 10D."
echo "  Measured today: create_app 1 (it already had one), the other five 0."
for t in list_apps read_app delete_app build_app launch_app; do
  echo -n "$t: " ; awk "/name = \"$t\"/,/-> Result<CallToolResult/" \
    crates/biorouter-mcp/src/agent_drafter/mod.rs | grep -c "RequestContext"
done ; echo "expect: 0 each — a context on a tool that needs none is scope creep"
# The key has ONE spelling, still. Task 10A pinned this at two files; CP5 must
# reach it through the const, not by hand-typing a third copy.
grep -rl '"biorouter-capability-tier"' --include='*.rs' crates/ | sort
echo "expect: still exactly 2 FILES — knowledge/tier.rs and agents/mcp_client.rs"
grep -c "knowledge::tier::caller_is_private" crates/biorouter-mcp/src/agent_drafter/mod.rs
echo "expect: 1 — the delegating reader, not a second implementation"
# The validators were NOT patched: filtering the catalog fixes all three at once,
# and a check inside them is a second rule to keep in sync.
grep -c "tier::is_private\|caller_is_private" crates/biorouter-mcp/src/agent_drafter/validate.rs
echo "expect: 0"
# The report is computed AFTER the manifest provider is bound. This is an
# ordering gate and it is the whole of the fix: the capability read is correct in
# isolation and wrong at :1257, and no per-file count can see the difference.
awk '/async fn configure_agent/,/^}/' crates/biorouter-server/src/routes/apps.rs | wc -l
echo "expect: > 1 (about 100 today) — assert the range before reading it"
awk '/async fn configure_agent/,/^}/' crates/biorouter-server/src/routes/apps.rs \
  | grep -n "configure_main_provider\|agent.provider()\|capability_report(\|configure_main_extensions" \
  | head -4
echo "Expected, in this order: configure_main_provider, agent.provider(),"
echo "  capability_report, configure_main_extensions. capability_report on a"
echo "  SMALLER line number than configure_main_provider is the defect — it reads"
echo "  the provider the session held before the manifest's model was applied."
# The worker's grant is gated by the WORKER's capability, and it is the worker's
# own provider that is read (configure_worker_provider runs four lines above it).
awk '/async fn configure_worker_agent/,/^}/' crates/biorouter-server/src/routes/apps.rs \
  | grep -n "configure_worker_provider\|agent.provider()\|has_kb\|grant_knowledge_base" | head -4
echo "Expected, in this order: configure_worker_provider, agent.provider(), has_kb,"
echo "  grant_knowledge_base. A missing has_kb line is a public worker profile"
echo "  being pinned to a private base as its KB-less write target."
# METADATA new-surface detector — the one Task 10C's two detectors are blind to
# by construction, and the reason this task exists. PRINT with line numbers and
# compare against the `#[cfg(test)]` boundaries; a bare count is the fragile shape
# this plan has already been burned by twice.
#
# ⚠ TWO sweeps, and `.selection(` in the pattern. The previous version was ONE
# sweep ending in `grep -v "src/knowledge/"`, which made it structurally unable
# to see the largest metadata leak in the tree: `kb_get_active`
# (`knowledge/server.rs:725`) reaches the whole set through `service.selection(`
# at `:687`, inside the excluded directory, through a verb the pattern did not
# name. A detector that excludes the module the surface lives in is not a
# detector. The exclusion had a real purpose — keeping the service's own
# internal reads out of the list — so it is kept as sweep (1) and paired with
# sweep (2) rather than deleted.
#
# (1) OUTSIDE knowledge/ — measured at 9558c346: 27 hits, 18 production.
grep -rn "\.list_bases()\|\.session_kb_ids(\|\.selection(" --include='*.rs' crates/*/src/ \
  | grep -v "src/knowledge/" | sort
echo "expect: 27 hits / 18 production, every one accounted for:"
echo "  agent_drafter/catalog.rs:130               CP5 — THIS TASK"
echo "  biorouter/src/agents/knowledge_tool.rs:149 the id LIST — Task 11 (same class as"
echo "                                             kb_id_or_primary, second file)"
echo "  biorouter/src/agents/knowledge_tool.rs:134 :141  existence of a SUPPLIED id, AR-5"
echo "  biorouter-server/routes/knowledge.rs:344   the Knowledge view — the user, ungated"
echo "  biorouter-server/routes/knowledge.rs:749   selection_response, GET /knowledge/active —"
echo "                                             the Knowledge view again; it returns kb_ids,"
echo "                                             primary_kb AND hidden_kbs, a superset of what"
echo "                                             kb_get_active returns, and is ungated for the"
echo "                                             same reason the seven read handlers are: no"
echo "                                             model is on that path (Task 10C's scope ⚠)"
echo "  biorouter-server/routes/reset.rs:118 :178  factory reset — the user, ungated"
echo "  biorouter-server/routes/workflow.rs:134 :151  workflow authoring — the user, ungated"
echo "  biorouter-cli x8 (commands/knowledge.rs:54 :123 :129 :214 :519, session/completion.rs:274,"
echo "                    session/tui/mod.rs:1626 :1754)  the terminal — the user, ungated"
echo "  9 test-module hits: biorouter-cli/commands/knowledge.rs (#[cfg(test)] :755) :1024 :1048"
echo "                      :1077 :1080; routes/reset.rs (:387) :418; routes/apps.rs (:4623)"
echo "                      :4661 :4673 :4734; routes/agent.rs (:1379) :1489"
# (2) INSIDE knowledge/ — measured: 22 hits, 5 production. This is the sweep that
# would have caught the pointer tools.
grep -rn "\.list_bases()\|\.session_kb_ids(\|\.selection(" --include='*.rs' crates/*/src/ \
  | grep "src/knowledge/" | sort
echo "expect: 22 hits / 5 production, every one accounted for:"
echo "  knowledge/server.rs:246  visible_bases_for_session — filtered, Task 10C (1st filter)"
echo "  knowledge/server.rs:325  kb_id_or_primary's candidate list — Task 10C (3rd filter)"
echo "  knowledge/server.rs:687  selection_json -> selection_value — Task 10C (4th/5th filters),"
echo "                           i.e. kb_get_active and kb_set_active"
echo "  knowledge/service.rs:1338  service-internal (effective_primary), reaches no caller"
echo "  biorouter/src/knowledge/soul.rs:73  the user's own Soul base, out of scope"
echo "  17 test-module hits: brkb.rs (#[cfg(test)] :132) :171; service.rs (:1841) :1879 and"
echo "                       fifteen more between :2413 and :3039"
echo "A hit outside these two accounted lists is a NEW metadata surface and must be"
echo "classified — against the register below — before it lands."
```

**What this catches.** Four wrong implementations. (1) Fixing `list_platform_catalog` only — the
tool whose name says "catalog" — while `create_app`, `configure_app` and `update_app` keep rendering
the same ids into rejection strings; `every_drafter_tool_that_builds_a_catalog_scopes_it` is the only
test that fails it, and it is the exact shape of Task 10B's "ratchet `kb_write_page` and call it
done". (2) Filtering the *serialised JSON* rather than the vector, which leaves `has_kb` true so a
public session can still configure an app against a private base and `capability_report` still arms
its tools. (3) Making the refusal say *"that base is private"*, which is a leak in a politer
sentence — `a_public_session_cannot_configure_an_app_against_a_private_base` asserts the message says
"not installed" and does **not** say "private". (4) Reading the meta key by hand in `agent_drafter`
instead of through `knowledge::tier::caller_is_private`, which compiles, passes every drafter test,
and silently stops matching the day the key changes; the two-file gate is what sees it. (5) Adding
the capability read at `capability_report`'s **existing** position (`:1257`), which is above
`configure_main_provider` (`:1259`) — so a global-private install serves a public manifest model the
private catalog and grants it the base, and a global-public install strips a private manifest model
of its own. The read is correct in isolation and wrong two lines early; only the `configure_agent`
ordering gate and `the_app_capability_report_follows_the_MANIFESTS_provider_not_the_global_one` see
it, and only because that test goes through `configure_agent` — a direct `capability_report(&cfg,
false)` passes against the defect. (6) Fixing the main agent and leaving `configure_worker_agent`'s
grant (`:1561`) ungated, which pins a private base as a public worker profile's KB-less write target.
(7) **A metadata detector that excludes the module the surface lives in.** Its previous form was one
sweep ending in `grep -v "src/knowledge/"`, so it could not — by construction, not by luck — see
`kb_get_active`, which reaches the whole set through `service.selection(` at `server.rs:687`, inside
the excluded directory and through a verb the pattern did not name. **This gate rejects: a metadata
leak inside `crates/*/src/knowledge/`,** via the second sweep (22 hits / 5 production, each
accounted for) and `.selection(` added to the pattern. The register above rejects it a second time,
at the level a leak is actually reasoned about — the *tool*, not the call site — and
`every_drafter_tool_that_can_name_a_base_is_in_the_register` makes the same rule executable over the
drafter's whole router rather than over the five tools this task happens to know about.

- [ ] **Step 6: Commit**

```bash
git add crates/biorouter-mcp/src/agent_drafter/ crates/biorouter-mcp/tests/catalog_write_boundary.rs \
        crates/biorouter-mcp/tests/testdrive_corpus_relint.rs \
        crates/biorouter-server/src/routes/apps.rs
cargo check --workspace --all-targets
git commit -m "feat(knowledge): scope the Agent Drafter catalog to the caller's capability (#56)"
```

---

### Task 11: Gate G — cross-session conversation ingest

The second fully-open cross-session read, and the design does not name it. Gates C, D and E all miss
it by construction: it is dispatched at `agent.rs:2660`, **before** the extension-manager
fall-through at `:2769`; it is not an MCP tool, so `filter_tools` cannot hide it; and it never
touches `chat_history_search.rs`. Its sink is worse than its source — a knowledge base is a
machine-wide tree (`knowledge_root()` = `in_config_dir("knowledge")`) that any session may name.

⚠ **Departure D8: the guard goes in the shared function, not in the platform tool.** Measured:
`grep -rn "conversation_ingest::ingest_conversation\|ingest_conversation(" --include='*.rs' crates/`
returns **three** production callers of
`biorouter::knowledge::conversation_ingest::ingest_conversation`, not one:

| # | Caller | What it supplies |
|---|---|---|
| 1 | `crates/biorouter/src/agents/knowledge_tool.rs:61` | the platform tool `platform__ingest_conversation`, whose `session_ids` array (`:32-41`) is model-supplied |
| 2 | `crates/biorouter-server/src/routes/knowledge.rs:1233` | `POST /knowledge/bases/{id}/ingest-conversation`, whose `session_ids` array is **caller-supplied** (`:1192-1197`) and loaded with `get_session(sid, true)` at `:1203`, with the model also caller-supplied (`build_completer` at `:1214`) |
| 3 | `crates/biorouter-cli/src/commands/knowledge.rs:571` | `biorouter knowledge ingest-conversation`, at a terminal |

The first version of this plan guarded only (1). (2) is the same primitive behind nothing but the
secret key — the credential §9.3 A1 calls reachable from any developer-enabled agent shell, and the
one Task 2 exists to stop leaking. A guard in the platform tool leaves it as an unguarded copy.
A **required** `caller_capability` field on `ConversationIngestArgs` makes all three a compile error,
which is the same forcing function Gate D uses for `ChatHistorySearch::new`'s 7th parameter.

⚠ **The field itself arrives in Task 10B, not here.** 10B makes `IngestArgs.caller_is_private`
required, which makes `conversation_ingest.rs:205` — inside this very function — a compile error with
nothing to pass; so 10B declares `caller_capability` and wires all three callers, and **this task adds
the refusal that reads it**. The split is deliberate and matches 10B/10C: one task plumbs, the next
gates. Practical consequence for the worker: Step 2 below expects **FAIL**, not COMPILE ERROR, and
the "a required field makes all three a compile error" argument above is the reason 10B was the right
place to spend it — not a description of what happens when you start this task.

**Files:**

| Action | Path | Anchor (re-verified at `9558c346`) |
|---|---|---|
| Modify | `crates/biorouter/src/knowledge/conversation_ingest.rs` | `ConversationIngestArgs` `:172-180` (7 fields today, ending `cancel`; **Task 10B adds the 8th, `caller_capability`**); `ingest_conversation` `:184-187`; the empty/undigestible early returns `:188-194`; `render_conversations(&args.sessions)` `:191` — the guard goes **before** it, so no transcript is rendered for a session that is about to be refused. Also the **new** `ConversationIngestResult` this task defines here — see Step 3 on why `refused` may not be a field on `IngestResult` |
| Modify | `crates/biorouter/src/agents/knowledge_tool.rs` | `handle_ingest_conversation` `:24-86`; `session_ids` parse `:32-41`; the load loop `:48-49` (`get_session(sid, true)`); the `ingest_conversation(` call at `:61`. **Plus `resolve_target_kb` `:120-161`**, whose no-target error at `:156-159` formats `svc.session_kb_ids(Some(session_id))` (`:149`) into `"pass kb_id (one of: …)"` — the same leak Task 10C closes in `kb_id_or_primary`, in a second file, on the model-facing path this task owns |
| Modify | `crates/biorouter-server/src/routes/knowledge.rs` | `ingest_conversation` `:1187-1258`; the `session_ids` load loop `:1202-1212`; the `ConversationIngestArgs` literal `:1224-1232` |
| Modify | `crates/biorouter-cli/src/commands/knowledge.rs` | `handle_ingest_conversation` `:500`; the `ingest_conversation(` call at `:571` |
| Reference | `crates/biorouter/src/agents/agent.rs` | dispatch `:2660`; advertisement `:3131` (`ingest_conversation_tool()`), with the surrounding comment "The conversation-ingestion tool is always available on the platform extension" |
| Reference | `crates/biorouter/src/agents/platform_tools.rs` | `PLATFORM_INGEST_CONVERSATION_TOOL_NAME` `:5`; `ingest_conversation_tool()` `:51`; the description telling the model to "Pass `session_ids`" at `:63-65` |

⚠ **`Agent` has no `capability_tier()`** — Task 10 put that method on `ExtensionManager`, whose
`provider` field is private. `handle_ingest_conversation` is `impl Agent` (`knowledge_tool.rs:23-24`),
so it resolves its own capability with `self.provider().await.map(|p| p.tier()).unwrap_or(ProviderTier::Public)`
— fail-closed to Public, and `Agent::provider()` is the accessor Task 13 has already hardened. The
same value serves `resolve_target_kb`, which is called from the same function at `:44`, four lines
earlier.

⚠ **The mirror of Task 10C's B2, found by looking for it.** `resolve_target_kb`
(`knowledge_tool.rs:120`) is `kb_id_or_primary`'s twin one crate over: an explicit `kb_id` wins
(`:139-145`), an absent one falls back to the primary (`:146-148`), and with neither it bails with
`svc.session_kb_ids(..).join(", ")` (`:149`, `:156-159`) — the full id list, to a model, on the
platform tool this task exists to gate. Task 10C's fix does not reach it: that one is in
`biorouter-mcp`'s `KnowledgeServer` and this one is in `biorouter`'s `Agent`. It takes the same
filter and the same degrade (`:150-155`'s "this chat has none" branch already exists). Its sibling
`"knowledge base '{id}' does not exist"` at `:141` is an existence answer about **one supplied id**
and stays as it is — [AR-5](#ar-5--the-existence-of-a-private-knowledge-base-is-still-inferable),
DR-7's side-channel scope, the same line Task 10D draws.

- [ ] **Step 1: Write the failing tests**

```rust
#[tokio::test]
async fn ingesting_another_sessions_conversation_obeys_the_barrier() {
    let private = private_session_with_messages("PHI cohort notes").await;
    let public_caller = public_capability_agent().await;

    let before = kb_bytes("default").await;
    let out = public_caller
        .handle_ingest_conversation(json!({ "session_ids": [private.id], "kb_id": "default" }))
        .await
        .unwrap();

    assert!(out.contains("private"), "must explain: {out}");
    assert!(!out.contains("PHI cohort notes"), "leaked content into the result: {out}");
    // The strong half: the sink is a machine-wide tree every other session can
    // read, so a refusal that still wrote is not a refusal.
    assert_eq!(kb_bytes("default").await, before, "knowledge base was modified");
}

#[tokio::test]
async fn ingesting_your_own_private_conversation_ratchets_the_knowledge_base() {
    // ⚠ REPLACES `ingesting_your_own_conversation_is_unaffected`, which asserted
    // the hole stays open. The default (no `session_ids`) is the current
    // session, and that call is the overwhelmingly common one — it must keep
    // working. What it must NOT do is drop a private transcript into a
    // machine-wide tree that a public chat can read back, which is the whole of
    // design §9.3 B4 and the operator's ruling (Tasks 10A-10C, AR-1).
    let agent = private_capability_agent_with_messages("my own notes").await;
    assert!(!kb_tier_is_private("default"), "fixture precondition");

    let out = agent.handle_ingest_conversation(json!({ "kb_id": "default" })).await.unwrap();
    assert!(out.contains("1 session"), "the common call must not regress: {out}");

    // The base now carries the tier of the session that fed it.
    assert!(kb_tier_is_private("default"), "a private transcript landed in a public base");
    // And the laundering path is closed end to end, which is the assertion the
    // first version of this test made impossible.
    let public_reader = public_capability_agent().await;
    let hits = public_reader.kb_search(json!({ "kb_id": "default", "query": "notes" })).await;
    assert!(hits.unwrap_text().contains("private"));
    assert!(!hits.unwrap_text().contains("my own notes"));
}

#[tokio::test]
async fn a_public_session_may_still_ingest_its_own_conversation() {
    // The other half of "must not regress": the ratchet is `max`, not `set`, so
    // a public chat ingesting itself leaves a public base public.
    let agent = public_capability_agent_with_messages("weekly notes").await;
    let out = agent.handle_ingest_conversation(json!({ "kb_id": "default" })).await.unwrap();
    assert!(out.contains("1 session"), "{out}");
    assert!(!kb_tier_is_private("default"));
}

#[tokio::test]
async fn the_no_target_error_names_only_the_bases_the_caller_may_reach() {
    // `resolve_target_kb` (:149, :156-159) is `kb_id_or_primary`'s twin in this
    // crate, and Task 10C's fix cannot reach it. Same rule: OMIT.
    let agent = public_capability_agent_with_bases(&["default", "omop"]).await;
    kb_raise("omop", true);
    clear_primary_for(&agent.session_id());

    let out = agent.handle_ingest_conversation(json!({})).await.unwrap_err().to_string();
    assert!(out.contains("default"), "the public base must still be offered: {out}");
    assert!(!out.contains("omop"), "the no-target error enumerated a private base: {out}");
}

#[tokio::test]
async fn the_http_route_is_gated_by_the_same_argument_not_by_a_second_copy() {
    // D8. The route is reachable with nothing but the secret key.
    let private = private_session_with_messages("PHI cohort notes").await;
    let before = kb_bytes("default").await;
    let r = post_ingest_conversation("default", &[&private.id],
                                     model_ref("anthropic", "claude-opus-4-8")).await;
    assert_eq!(r.status(), 409);
    assert!(!r.text().await.contains("PHI cohort notes"));
    assert_eq!(kb_bytes("default").await, before);
}

#[tokio::test]
async fn each_caller_of_the_guard_is_exercised_in_BOTH_directions() {
    // The row every test above is missing, and the one a caller hardcoded to
    // ProviderTier::PUBLIC passes all of them without: the same call, from a
    // PRIVATE caller, must SUCCEED. Without it, "refuse the public caller" is
    // satisfied by "refuse everyone", which is what a hardcoded Public produces
    // — and it is not a loud failure, because the feature merely stops working
    // for the sessions that need it and nothing in this task asserts otherwise.
    //
    // Both production callers a harness reaches, both rows. (The CLI is the
    // third and is covered structurally — see Task 10B's ⚠ on which callers get
    // a behavioural row.)
    let private = private_session_with_messages("PHI cohort notes").await;

    // (a) the HTTP route
    let r = post_ingest_conversation("default", &[&private.id], private_model_ref()).await;
    assert_eq!(r.status(), 200, "a private model was refused its own private chat");
    assert!(kb_tier_is_private("default"));

    // (b) the platform tool
    let public_caller = public_capability_agent().await;
    assert!(public_caller
        .handle_ingest_conversation(json!({ "session_ids": [private.id], "kb_id": "default" }))
        .await.unwrap().contains("private"));
    let private_caller = private_capability_agent().await;
    assert!(!private_caller
        .handle_ingest_conversation(json!({ "session_ids": [private.id], "kb_id": "default" }))
        .await.unwrap().contains("private"),
        "a private model was refused its own private chat through the platform tool");
}
```

- [ ] **Step 2: Run** → **FAIL**, not COMPILE ERROR. Task 10B already declared `caller_capability`
      and wired all three constructors (⚠ above), so everything here compiles; what fails is the
      refusal, the ratchet, the 409 and both metadata assertions. If you get a compile error at a
      `ConversationIngestArgs` literal, Task 10B did not land — stop and go back.

- [ ] **Step 3: Implement**

(a) `ConversationIngestArgs.caller_capability` **already exists** (Task 10B). For the record, and so
a reviewer reading this task alone knows what it is:

```rust
    /// The capability of whoever is asking. Required, and deliberately not
    /// `Option`: this struct has three production constructors (the platform
    /// tool, `POST /knowledge/bases/{id}/ingest-conversation`, and the CLI), and
    /// the first version of this plan guarded only the first. A required field
    /// makes a missed caller a compile error rather than a silent second copy of
    /// a one-call private -> public laundering primitive.
    pub caller_capability: crate::privacy::ProviderTier,
```

(b) In `ingest_conversation`, **before** `render_conversations` at `:191`:

```rust
    // Issue #56 Gate G. Per session, not once: `sessions` is a caller-supplied
    // LIST, and a single up-front check on the first element admits the rest.
    // Placed before `render_conversations` so no refused transcript is ever
    // rendered into a buffer, even one that is then dropped.
    let (allowed, refused): (Vec<_>, Vec<_>) = args
        .sessions
        .into_iter()
        .partition(|s| crate::privacy::visible_to(args.caller_capability, s.privacy_tier));
    if allowed.is_empty() && !refused.is_empty() {
        anyhow::bail!("{}", REFUSED_ALL_PRIVATE);
    }
```

carrying `refused.len()` out of the function in a **new type defined here**, so each caller can
report it:

```rust
/// What an ingest of other sessions' conversations produced, plus how many were
/// refused by the barrier above.
///
/// ⚠ NOT a `refused` field on `IngestResult`. That type is
/// `crates/biorouter-mcp/src/knowledge/macros/ingest.rs:40`, in a crate this
/// task's Files table and `git add` name no file from; it derives
/// `Serialize`/`Deserialize` and is the payload of the SSE macro routes, so a
/// field there is a wire change to three routes that have nothing to do with
/// Gate G. It is also a cross-crate edit inside a task whose commit would then
/// not build — the exact packaging defect that left nine consecutive commits red
/// (Task 10B ⚠ on `--lib`). All three callers of `ingest_conversation` are
/// already in this task's Files table, so changing its RETURN type is the
/// cheaper edit and the honest one.
pub struct ConversationIngestResult {
    pub ingested: biorouter_mcp::knowledge::macros::ingest::IngestResult,
    /// How many of the requested sessions the barrier refused. A count and
    /// nothing else — §11.4 classifies a session's id, title and working
    /// directory as content, and this product's titles are LLM-generated from
    /// the conversation itself.
    pub refused: usize,
}
```

and naming **only** the count and the reason:

```rust
/// Names no session, no title and no working directory — §11.4 classifies all
/// three as content, and a session title in this product is LLM-generated from
/// the conversation itself.
const REFUSED_ALL_PRIVATE: &str = "\
Those chats are private: they were created under a model hosted inside the institution, so only a \
private model may read them. This session is running on a public model. Ask the user to switch this \
chat to a private model and try again.";
```

(c) The three call sites **already** pass their own capability (Task 10B): the platform tool from
`self.provider().await.map(|p| p.tier()).unwrap_or(ProviderTier::Public)`; the HTTP route from the
provider `build_completer` constructed (the **instance**, not `body.model.provider` — `factory.rs:142-146`
can hand back something else); the CLI from the session's bound provider. This task changes none of
them; Step 5 re-asserts them anyway, because the value they pass is now load-bearing for a refusal
rather than only for a ratchet.

(d) The HTTP route maps a full refusal to **409**, not 500 — the same typed status Gate A uses
(Task 12), for the same reason: a barrier that surfaces as an internal error teaches the caller to
retry.

(e) **`resolve_target_kb` takes the same filter Task 10C put in `kb_id_or_primary`** (⚠ above). One
line in `knowledge_tool.rs:149`, using the capability `handle_ingest_conversation` has already
resolved five lines earlier for (c):

```rust
    let ids: Vec<String> = svc
        .session_kb_ids(Some(session_id))?
        .into_iter()
        // Issue #56. Per id, and before the `is_empty` check below, so a chat
        // whose only base is private is told it has none rather than being
        // handed `(one of: )`.
        .filter(|id| caller.is_private() || !tier::is_private(svc.root(), id))
        .collect();
```

⚠ It needs `resolve_target_kb` to take the caller's capability — a fourth parameter on a
`pub(crate)` function with **one** production caller (`:44`) and three test callers (`:245`, `:247`,
`:253`), all four in the same file. Measured, so nobody defers it as "a signature with callers".

(f) **The `IngestArgs` this function builds at `:205` already carries the same value across the
crate boundary** — Task 10B's `caller_is_private: args.caller_capability.is_private()`. That one line
is what makes this task's headline test — `ingesting_your_own_private_conversation_ratchets_the_knowledge_base`
— pass: the ratchet itself lives in `macros::ingest::ingest` (CP2), which this function funnels into,
and *not* in any `kb_*` tool. The sub-agent that does the writing reaches `store::write_page` and
`svc.add_raw_source` through `KbToolDispatch`, where no MCP-layer gate can see it. `ProviderTier`
becomes a `bool` there and only there, for the crate-dependency reason in Task 10A ⚠(1). Nothing in
this task touches it; it is stated because this task's headline test is the only place its absence
would be visible.

- [ ] **Step 4: Run**

```bash
cargo check --workspace --all-targets                  # see Task 10B's ⚠ on --lib
cargo test -p biorouter --lib -- agents::knowledge_tool knowledge::conversation_ingest
cargo test -p biorouter-server --test knowledge_routes
cargo test -p biorouter-cli --lib commands::knowledge
cargo test -p biorouter-mcp --lib knowledge::macros::ingest   # the ratchet this test depends on
```

`cargo check --workspace --all-targets` is here because this task changes `ingest_conversation`'s
**return type** (`IngestResult` → `ConversationIngestResult`) and `resolve_target_kb`'s signature.
Both are `pub`/`pub(crate)` and `--lib` alone would not tell you whether anything under
`crates/*/tests/` names them. Measured at `9558c346`: nothing does — `grep -rn
"ingest_conversation\|resolve_target_kb" crates/*/tests/` is empty — but the check is one command and
the measurement is what makes it cheap to keep true.

- [ ] **Step 5: Gate**

```bash
# ONE guard, in the shared function — not three copies in three callers.
grep -rn "visible_to" --include='*.rs' crates/biorouter/src/knowledge/conversation_ingest.rs \
  crates/biorouter/src/agents/knowledge_tool.rs \
  crates/biorouter-server/src/routes/knowledge.rs \
  crates/biorouter-cli/src/commands/knowledge.rs
echo "expect: exactly 1 hit, in conversation_ingest.rs"
# The partition precedes the render, so nothing refused is ever rendered.
awk '/pub async fn ingest_conversation/,/^}/' crates/biorouter/src/knowledge/conversation_ingest.rs \
  | grep -n "partition\|render_conversations" | head -2
# Expected: partition on the SMALLER line number.
# All three callers pass the field (a missed one would not compile since Task 10B,
# but a caller that hardcodes Private compiles fine and is the real risk).
grep -rn "caller_capability:" --include='*.rs' crates/ | grep -v conversation_ingest.rs
echo "expect: 3 — and NONE of them may hardcode a ProviderTier, in EITHER direction"
grep -rn "caller_capability: *ProviderTier::\(Private\|Public\)" --include='*.rs' crates/*/src/
echo "expect: no output. `Private` reads as 'this caller is trusted'; `Public` is the"
echo "  mirror and is WORSE here, because it turns the guard into 'refuse everyone' —"
echo "  which passes every refusal test in Step 1 and quietly breaks the feature for"
echo "  the private sessions it exists to serve. Task 10B Step 5 (i) forbids the same"
echo "  literal in the same two directions for the bool half."
# The refusal names no session.
grep -c "session.name" crates/biorouter/src/agents/knowledge_tool.rs ; echo "expect: 0"
# The refused COUNT does not travel as a field on a type in another crate.
grep -c "pub struct ConversationIngestResult" crates/biorouter/src/knowledge/conversation_ingest.rs
echo "expect: 1"
git diff --stat HEAD -- crates/biorouter-mcp
echo "expect: empty — this task touches no biorouter-mcp file, and a 'refused' field on"
echo "  IngestResult (macros/ingest.rs:40) would be a wire change to three SSE routes"
echo "  in a crate this task does not commit."
# The metadata twin: resolve_target_kb omits, exactly as kb_id_or_primary does.
# ⚠ `fn resolve_target_kb\(` with the paren — see Task 10C Step 5 for why the bare
# prefix is unsafe as an awk START. Measured today: 42 lines, one start.
awk '/fn resolve_target_kb\(/,/^}/' crates/biorouter/src/agents/knowledge_tool.rs | wc -l
echo "expect: > 1 (42 today)"
awk '/fn resolve_target_kb\(/,/^}/' crates/biorouter/src/agents/knowledge_tool.rs \
  | grep -n "session_kb_ids\|is_private\|ids.is_empty\|ids.join" | head -4
echo "Expected, in this order: session_kb_ids, is_private, ids.is_empty, ids.join."
echo "  is_private AFTER is_empty leaves a public chat whose only base is private"
echo "  reading 'pass kb_id (one of: )'."
# Both id-list sites are filtered, and they are the only two.
grep -rn "session_kb_ids(" --include='*.rs' crates/*/src/ | grep -v "src/knowledge/service.rs"
echo "expect: 7 hits / 3 production — knowledge/server.rs:325 (Task 10C),"
echo "  knowledge_tool.rs:149 (this task), biorouter-cli/src/commands/knowledge.rs:54"
echo "  (the terminal, ungated by decision). The other 4 (:1024 :1048 :1077 :1080)"
echo "  are below biorouter-cli/src/commands/knowledge.rs's #[cfg(test)] at :755 —"
echo "  compare against the boundary, do not count."
```

**What this catches.** Four wrong implementations. (1) The check placed before the loop, on
`session_ids[0]` or on the current session — which admits every other element of a caller-supplied
array; the `partition` shape makes that shape unwritable. (2) A refusal that returns an error but has
already called `kb_write_page`; the byte-equality assertion is the only thing that fails it.
(3) Guarding the platform tool and calling it done — leaving `POST /knowledge/bases/{id}/ingest-conversation`
as an unguarded copy; the required field and the cross-file `grep` are what fail it. (4) The
plausible-looking fix of hardcoding a `ProviderTier` at the HTTP or CLI call site to make it
compile. `Private` reads as "this caller is trusted" and is exactly wrong for the route that needs
the check most. **This gate also rejects its mirror, `caller_capability: ProviderTier::Public`,**
which every test in Step 1 passed before this round: the guard then refuses *everyone*, so the
public-caller assertions all still hold and the only observable effect is that a private session can
no longer ingest its own chats — a feature quietly ceasing to work for exactly the users it was
built for. `each_caller_of_the_guard_is_exercised_in_BOTH_directions` is the test that fails it, and
the two-direction grep is the gate. (5) Guarding the
transcripts and leaving `resolve_target_kb` handing the same public model the id list of every base
including the private ones — a refusal that names what it refused, and the second instance of the
class Task 10C closes in `kb_id_or_primary`; `the_no_target_error_names_only_the_bases_the_caller_may_reach`
is the only test that fails it.

- [ ] **Step 6: Commit**

```bash
git add crates/biorouter/src/knowledge/conversation_ingest.rs \
        crates/biorouter/src/agents/knowledge_tool.rs \
        crates/biorouter-server/src/routes/knowledge.rs \
        crates/biorouter-cli/src/commands/knowledge.rs
cargo check --workspace --all-targets
git commit -m "fix(knowledge): refuse cross-session ingest of a private conversation, in the shared function (#56)"
```

---

### Task 12: Gate A — the bind, its typed 409, and the refusal the GUI currently swallows

Ships as **one commit**. O3: `updateAgentProvider` is called without `throwOnError`
(`ModelAndProviderContext.tsx:282-290`) while `setConfigProvider` on the next line has it
(`:294-300`), so a refusal is discarded, `setCurrentProvider`/`setCurrentModel` fire and a green
toast at `:307-310` claims success — while the session is still bound to the private model.

**Files:**

| Action | Path | Anchor (re-verified at `9558c346`) |
|---|---|---|
| Create | `crates/biorouter/src/privacy/refusal.rs` | new — `PrivacyRefusal`. ⚠ **Task 12 creates it, not Task 14**: Tasks 12 and 13 both reference the module |
| Modify | `crates/biorouter/src/agents/agent.rs` | `update_provider` `:5655-5675` — swap at `:5663-5664`, persist at `:5666-5674`, the tree's **only** `.provider_name(` call at `:5670` |
| Modify | `crates/biorouter/src/session/session_manager.rs` | a new conditional-UPDATE method beside `apply_update` (`add_update!` block `:3126-3132`) |
| Modify | `crates/biorouter-server/src/routes/agent.rs` | `update_provider` handler `:713-729` (500-only mapping at `:725`); its `responses(..)` block; route registration `:1270` |
| Modify | `ui/desktop/src/components/ModelAndProviderContext.tsx` | `changeModel` `:267-321`; `updateAgentProvider` `:282-290` (**no `throwOnError`**); `setConfigProvider` `:294-300`; success toast `:307-310`; catch arm `:312-319` |
| Modify | `ui/desktop/openapi.json`, `ui/desktop/src/api` | regenerated |

- [ ] **Step 1: Write the failing tests**

```rust
#[tokio::test]
async fn a_public_provider_cannot_be_bound_to_a_private_session() {
    let (agent, session) = agent_on(private_provider()).await;
    ratchet_to_private(&session).await;

    let err = agent.update_provider(public_provider(), &session.id).await.unwrap_err();
    assert!(matches!(err.downcast_ref::<PrivacyRefusal>(),
                     Some(PrivacyRefusal::PublicModelOnPrivateSession { .. })));

    // The half that catches the wrong implementation: today the in-memory swap
    // at :5663-5664 PRECEDES the persist at :5666. A gate that checks the row
    // but leaves that order alone refuses the write and still leaves the chat
    // running on the public model in memory.
    assert_eq!(agent.provider().await.unwrap().get_name(), "versa_azure");
    // And the row is untouched.
    assert_eq!(reread(&session.id).await.provider_name.as_deref(), Some("versa_azure"));
}

#[tokio::test]
async fn a_private_provider_binds_to_anything_and_a_public_session_accepts_anything() {
    let (agent, s) = agent_on(public_provider()).await;
    agent.update_provider(private_provider(), &s.id).await.unwrap();   // upward: fine
    let (agent2, s2) = agent_on(private_provider()).await;
    ratchet_to_private(&s2).await;
    agent2.update_provider(private_provider2(), &s2.id).await.unwrap(); // private->private
}

#[tokio::test]
async fn a_bind_is_never_accepted_against_a_row_that_is_already_private() {
    // Interleaving (A), FORCED: the ratchet commits strictly BEFORE the bind's
    // UPDATE runs. This is the case the conditional UPDATE exists for, and the
    // one nothing could previously produce.
    let (agent, s) = agent_on(private_provider()).await;

    let reached = seams::arm_before_bind_update();
    let bind = tokio::spawn({ let a = agent.clone(); let id = s.id.clone();
                              async move { a.update_provider(public_provider(), &id).await } });
    let release = reached.await.unwrap();      // update_provider is parked at the seam
    ratchet_to_private(&s.id).await;           // runs alone, to completion
    release.send(()).unwrap();

    let err = bind.await.unwrap().unwrap_err();
    assert!(matches!(err.downcast_ref::<PrivacyRefusal>(),
                     Some(PrivacyRefusal::PublicModelOnPrivateSession { .. })),
            "the WHERE clause did not see a ratchet that committed before it");
    let row = reread(&s.id).await;
    assert_eq!(row.provider_name.as_deref(), Some("versa_azure"), "a refused bind wrote anyway");
    assert_eq!(agent.provider().await.unwrap().get_name(), "versa_azure");
}

#[tokio::test]
async fn a_ratchet_that_commits_after_a_legal_bind_lands_in_the_state_gate_b_owns() {
    // Interleaving (B), FORCED: the ratchet commits AFTER the bind's UPDATE and
    // BEFORE the in-memory swap. Both statements were legal when they ran, so
    // both succeed and the row ends (private, anthropic).
    //
    // ⚠ THIS IS NOT A BUG, AND THE PREVIOUS VERSION OF THIS TEST ASSERTED IT
    //   WAS. "The provider bound to a private session is always private" is not
    //   a sentence a conditional UPDATE can deliver. What it delivers is
    //   narrower and exact: *a bind is never accepted against a row that is
    //   already private*. A ratchet landing after a legal bind is a different
    //   event, and the state it produces — private row, public `provider_name`
    //   — is the SAME residual an LRU rehydration, a legacy row and
    //   `restore_provider_from_session`'s `Config::global()` fallback all
    //   produce. Task 13's `an_unrepairable_mismatch_refuses_this_turn_and_leaves_the_row_alone`
    //   is what owns it, and the repair card is what fixes it.
    let (agent, s) = agent_on(private_provider()).await;

    let reached = seams::arm_after_bind_before_swap();
    let bind = tokio::spawn({ let a = agent.clone(); let id = s.id.clone();
                              async move { a.update_provider(public_provider(), &id).await } });
    let release = reached.await.unwrap();
    ratchet_to_private(&s.id).await;
    release.send(()).unwrap();
    bind.await.unwrap().unwrap();              // the bind was legal when it ran: Ok

    let row = reread(&s.id).await;
    assert_eq!(row.privacy_tier, SessionClassification::Private);
    assert_eq!(row.provider_name.as_deref(), Some("anthropic"));
    // Not TORN: `provider_name` and `model_config_json` came from one UPDATE, so
    // no reader can see one provider's name beside another's model config.
    assert_eq!(model_name_of(&row), public_provider().get_model_config().model_name);
    // And the property that actually matters holds: the next turn refuses
    // rather than running a public model against a private session.
    let events = drain(agent.reply(user("hi"), cfg(&s), None).await.unwrap()).await;
    assert!(events.iter().any(is_refusal), "{events:#?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_unconstrained_race_observes_both_outcomes() {
    // The fuzz layer is KEPT — a seam only proves the two interleavings someone
    // thought of. What changes is that it must PROVE it raced.
    //
    // ⚠ `flavor = "multi_thread"` is load-bearing and its absence is half of why
    //   the previous version tested nothing: `#[tokio::test]` defaults to
    //   `current_thread`, where two `tokio::spawn`s cannot preempt each other at
    //   all — they interleave only at `.await` points, in the same order every
    //   iteration. Two hundred iterations of a deterministic schedule is one
    //   iteration, run two hundred times.
    let (mut bound, mut refused) = (0usize, 0usize);
    for _ in 0..200 {
        let (agent, s) = agent_on(private_provider()).await;
        let a = tokio::spawn({ let a = agent.clone(); let id = s.id.clone();
                               async move { a.update_provider(public_provider(), &id).await } });
        let b = tokio::spawn(ratchet_to_private_owned(s.id.clone()));
        let (a, b) = tokio::join!(a, b);
        b.unwrap();
        let bind_ok = a.unwrap().is_ok();
        if bind_ok { bound += 1 } else { refused += 1 }

        // The invariant that holds in EVERY interleaving, asserted
        // UNCONDITIONALLY — the previous version's `if row.is_private()` guard
        // made the whole assertion skippable, and the ratchet always wins the
        // row, so it was skipped in the only branch it could have caught.
        let row = reread(&s.id).await;
        assert_eq!(row.privacy_tier, SessionClassification::Private);
        assert_eq!(row.provider_name.as_deref() == Some("anthropic"), bind_ok,
                   "a refused bind wrote the row, or an accepted one did not");
    }
    assert!(bound > 0 && refused > 0,
            "200 iterations produced {bound} bound / {refused} refused — one-sided, so the loop \
             raced nothing. That is the state this test used to report as a pass.");
}
```

```ts
// ui/desktop/src/components/ModelAndProviderContext.test.tsx
it('does not report success when the session bind is refused', async () => {
  server.use(rest.post('/agent/update_provider', (_, res, ctx) =>
    res(ctx.status(409), ctx.json({ code: 'privacy_barrier', session_classification: 'private',
                                    provider_tier: 'public', available_private_providers: [] }))));
  const ok = await changeModel('sess-1', publicModel);
  expect(ok).toBe(false);
  expect(toastSuccess).not.toHaveBeenCalled();
  // P4: the global default must not be rewritten by a refused per-session bind.
  expect(setConfigProvider).not.toHaveBeenCalled();
  expect(toastError).toHaveBeenCalledWith(expect.objectContaining({
    title: expect.stringContaining("Can't switch this chat"),
  }));
});
```

- [ ] **Step 2: Run** → Rust: **COMPILE ERROR** (`PrivacyRefusal` unresolved). TS: **FAIL** — today
      it returns `true`, calls `setConfigProvider`, and fires the success toast.

- [ ] **Step 3: Implement**

(a) `SessionStorage`, a conditional UPDATE that is the whole check:

```rust
    /// Bind a provider, atomically refusing a public one on a private session.
    ///
    /// The predicate is in the `WHERE`, not in Rust, so a concurrent ratchet
    /// cannot interleave into "private session, public provider bound".
    ///
    /// ⚠ `rows_affected == 0` is NOT the refusal on its own: a nonexistent
    /// `session_id` produces exactly the same zero, so returning
    /// `Ok(Refused)` there would surface a stale or mistyped id as a 409
    /// privacy refusal instead of a 404 — and would make Step 1's first test
    /// pass for the wrong reason against a stale fixture id, which is the one
    /// way that test can lie. Distinguish the two with a single follow-up read
    /// in the zero case; it costs one query on a path that is already an error.
    async fn bind_provider_if_allowed(
        &self,
        session_id: &str,
        provider_name: &str,
        model_config_json: &str,
        incoming_is_private: bool,
    ) -> Result<BindOutcome> {
        let pool = self.pool().await?;
        let res = sqlx::query(
            r#"
            UPDATE sessions
               SET provider_name = ?, model_config_json = ?, updated_at = datetime('now')
             WHERE id = ?
               AND (privacy_tier = 'public' OR ? = 1)
            "#,
        )
        .bind(provider_name)
        .bind(model_config_json)
        .bind(session_id)
        .bind(i64::from(incoming_is_private))
        .execute(pool)
        .await?;
        if res.rows_affected() > 0 {
            return Ok(BindOutcome::Bound);
        }
        // Zero rows: either the row is private and the provider is public, or
        // there is no such row at all.
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sessions WHERE id = ?)")
            .bind(session_id)
            .fetch_one(pool)
            .await?;
        Ok(if exists {
            BindOutcome::RefusedByPrivacy
        } else {
            BindOutcome::NoSuchSession
        })
    }

/// Three outcomes, because two of them are indistinguishable by `rows_affected`
/// and they map to different HTTP statuses (409 vs 404).
pub enum BindOutcome {
    Bound,
    RefusedByPrivacy,
    NoSuchSession,
}
```

and add to Step 1 the test that pins the distinction, because it is the one a `bool` return silently
gets wrong:

```rust
#[tokio::test]
async fn a_nonexistent_session_is_not_reported_as_a_privacy_refusal() {
    let (agent, _s) = agent_on(private_provider()).await;
    let err = agent.update_provider(public_provider(), "no-such-session-id").await.unwrap_err();
    assert!(err.downcast_ref::<PrivacyRefusal>().is_none(),
            "a bad id surfaced as a privacy refusal: {err}");
    assert_eq!(post_update_provider("no-such-session-id").await.status(), 404);
}
```

(b) `Agent::update_provider` — persist **first**, swap second, inverting today's order:

```rust
    pub async fn update_provider(
        &self,
        provider: Arc<dyn Provider>,
        session_id: &str,
    ) -> Result<()> {
        let provider_name = provider.get_name().to_string();
        let model_config = provider.get_model_config();
        let tier = provider.tier();

        // Issue #56 Gate A. Persist FIRST: today the in-memory swap precedes
        // the persist, so a refused write would leave the chat running on the
        // refused model. The invariant this establishes is one sentence, and it
        // is narrower than the one an earlier draft claimed: **a bind is never
        // accepted against a row that is already private.** NOT "the provider
        // bound to a private session is always private" — a ratchet that
        // commits after a legal bind produces (private, public provider), and
        // that residual is Gate B's (Task 13), not this gate's. Overstating it
        // is what made Step 1's race test assert something false.
        #[cfg(test)]
        seams::before_bind_update().await;
        match self
            .config
            .session_manager
            .bind_provider_if_allowed(session_id, &provider_name, &model_config, tier.is_private())
            .await
            .context("Failed to persist provider config to session")?
        {
            BindOutcome::Bound => {}
            BindOutcome::RefusedByPrivacy => {
                return Err(PrivacyRefusal::PublicModelOnPrivateSession {
                    session_id: session_id.to_string(),
                    provider: provider_name,
                }
                .into());
            }
            BindOutcome::NoSuchSession => {
                return Err(anyhow!("No such session: {session_id}"));
            }
        }

        #[cfg(test)]
        seams::after_bind_before_swap().await;
        let mut current_provider = self.provider.lock().await;
        *current_provider = Some(provider);
        Ok(())
    }
```

(b″) **The two test seams, and why a seam rather than more iterations.** A concurrency gate that
cannot *force* the interleaving it is about is not a gate — and this one could not, twice over: two
`tokio::spawn`s a few microseconds long, on a `#[tokio::test]` runtime that is `current_thread` by
default and therefore cannot preempt them at all. The seams are `#[cfg(test)]`, so nothing of them
exists in a shipped binary:

```rust
/// Test-only rendezvous points inside `update_provider` (issue #56).
///
/// `arm_*` returns a receiver that fires when `update_provider` reaches the
/// seam, carrying the sender that releases it — so a test can run a whole
/// ratchet *inside* the window instead of hoping a spawn lands there. Two
/// channels and not a `Barrier`: a 2-party `Barrier::wait` releases both sides
/// at the rendezvous, which is the one thing this must not do.
#[cfg(test)]
pub(crate) mod seams {
    use std::sync::{Mutex, OnceLock};
    use tokio::sync::oneshot;

    type Slot = OnceLock<Mutex<Option<oneshot::Sender<oneshot::Sender<()>>>>>;
    static BEFORE_BIND: Slot = OnceLock::new();
    static AFTER_BIND: Slot = OnceLock::new();

    fn slot(s: &'static Slot) -> &'static Mutex<Option<oneshot::Sender<oneshot::Sender<()>>>> {
        s.get_or_init(|| Mutex::new(None))
    }

    fn arm(s: &'static Slot) -> oneshot::Receiver<oneshot::Sender<()>> {
        let (tx, rx) = oneshot::channel();
        *slot(s).lock().unwrap() = Some(tx);
        rx
    }

    async fn park(s: &'static Slot) {
        // The guard is dropped at the end of this statement, before the await:
        // holding a std::sync::MutexGuard across an await point is the classic
        // way to turn a test seam into a deadlock.
        let armed = { slot(s).lock().unwrap().take() };
        if let Some(reached) = armed {
            let (release_tx, release_rx) = oneshot::channel();
            let _ = reached.send(release_tx);
            let _ = release_rx.await;
        }
    }

    pub(crate) fn arm_before_bind_update() -> oneshot::Receiver<oneshot::Sender<()>> {
        arm(&BEFORE_BIND)
    }
    pub(crate) fn arm_after_bind_before_swap() -> oneshot::Receiver<oneshot::Sender<()>> {
        arm(&AFTER_BIND)
    }
    pub(super) async fn before_bind_update() { park(&BEFORE_BIND).await }
    pub(super) async fn after_bind_before_swap() { park(&AFTER_BIND).await }
}
```

⚠ `take()`, not `clone()`: the slot is consumed on first arrival, so an un-armed `update_provider`
— every other test in this file, and every one in Task 13 — walks straight through both seams with
one uncontended lock and no await.

(b′) **`crates/biorouter/src/privacy/refusal.rs` is created here, by Task 12** — not by Task 14, as
the first version of this plan said. Task 12's own code returns `PrivacyRefusal`, and Task 13's calls
`privacy::refusal::turn_refusal(row)`; both run before Task 14, so a module that first appears in
Task 14 is an `unresolved module` compile error in two earlier tasks. Each later task adds one item
and says which:

| Task | Adds to `privacy/refusal.rs` |
|---|---|
| **12** (here) | the `PrivacyRefusal` error enum with `PublicModelOnPrivateSession { session_id, provider }`, its `session_classification()` / `provider_tier()` accessors for the typed 409, and `std::error::Error` so `anyhow`'s `downcast_ref` works |
| **13** | `turn_refusal(&Session) -> String`, and moves Task 10's file-local `CHATRECALL_LOAD_REFUSAL` here as `chatrecall_load_refusal()` |
| **14** | `privacy_refusal(extension, extension_tier, caller_tier) -> Option<ErrorData>` |
| **23** | the `PrivateChildOfPublicParent { requested }` variant and its `PrivacyRefusal::spawn_upgrade(tier)` constructor |

⚠ `bind_provider_if_allowed` replaces the builder for this one write, so the `.provider_name(..)`
call at `:5670` disappears — and with it the tree's only use of that setter. Leave the setter in
place; `import_legacy_session` (`session_manager.rs:2288`) still binds the column directly.

(c) `routes/agent.rs` — the typed 409 (P3), replacing the 500-only mapping at `:725`:

```rust
    Err(e) => match e.downcast_ref::<PrivacyRefusal>() {
        Some(refusal) => (
            StatusCode::CONFLICT,
            Json(PrivacyBarrierBody {
                code: "privacy_barrier",
                session_classification: refusal.session_classification(),
                provider_tier: refusal.provider_tier(),
                available_private_providers: available_private_providers(&state).await,
            }),
        ).into_response(),
        None => (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to update provider: {e}"))
            .into_response(),
    },
```

plus the `409` entry in the `responses(..)` block, then
`just generate-openapi && cd ui/desktop && npm run generate-api`.

(d) `ModelAndProviderContext.tsx` — three edits inside `changeModel`:

```ts
        if (sessionId) {
          await updateAgentProvider({
            body: { session_id: sessionId, provider: providerName, model: modelName,
                    context_limit: model.context_limit, request_params: model.request_params },
            // Issue #56: without this the generated @hey-api client returns
            // {error} instead of throwing, so a 409 privacy refusal is
            // discarded, setConfigProvider rewrites the global default to the
            // refused provider, and a green toast claims the switch worked.
            throwOnError: true,
          });
        }
```

with `setConfigProvider` (`:294`) moved **after** a successful session bind (it already is,
positionally — the change is that it is now genuinely unreachable on refusal), and a `catch` arm
keyed on `code === 'privacy_barrier'` rendering the Gate A card from design §14.4.

- [ ] **Step 4: Run**

```bash
cargo test -p biorouter --lib agents::agent
cargo test -p biorouter --lib session::session_manager
cargo test -p biorouter-server --lib routes::agent
cd ui/desktop && npx vitest run ModelAndProviderContext 2>&1 | tail -5
```

⚠ A vitest filter that matches nothing **fails alone and passes in company** — one live term hides
any number of dead ones. State the expected **file and test counts** in the PR and check both.

⚠ **`cargo test -p biorouter-server --lib routes::agent` does NOT print `0 passed` today — it
prints `8 passed`.** `routes/agent.rs` has two `#[cfg(test)]` blocks, `mod working_dir_lock_tests`
(`:1279`) and `mod knowledge_selection_tests` (`:1380`), neither of them named `tests`, and the
module-path filter picks up both. A previous version of this note said the module was empty, which
is the direction that hides a no-op: a worker told to expect zero reads `8 passed` as "my tests
landed" when in fact none of them did. **Record the pre-count with the identical command before
Step 1 and assert `post == 8 + N`** for the N tests Step 1 writes. Never "assert a non-zero count".

- [ ] **Step 5: Gate**

```bash
# Persist precedes swap. Both line numbers come from the same function, so a
# reordering shows as an inversion here.
awk '/pub async fn update_provider/,/^    }/' crates/biorouter/src/agents/agent.rs \
  | grep -n "bind_provider_if_allowed\|\*current_provider = Some" 
# Expected: bind_provider_if_allowed on the SMALLER line number.
# The route no longer collapses every error to 500.
grep -c "privacy_barrier" crates/biorouter-server/src/routes/agent.rs ; echo "expect: >= 1"
# The 409 is on THIS operation. ⚠ `grep -c '"409"' openapi.json` is VACUOUS —
# it is 6 today (update_working_dir, interrupt, reply, reset_app_data,
# create_schedule, edit_message all declare one), so it is green before and
# after. Assert the response on the operation that gains it, by parsing:
python3 -c "
import json,sys
d=json.load(open('ui/desktop/openapi.json'))
op=d['paths']['/agent/update_provider']['post']
assert op['operationId']=='update_agent_provider', op['operationId']
codes=sorted(op['responses'])
print('update_agent_provider responses:', codes)
assert '409' in codes, 'the typed privacy 409 is not on the operation'
ref=json.dumps(op['responses']['409'])
assert 'PrivacyBarrierBody' in ref, ('409 has no typed body: '+ref)
print('OK — typed 409 on update_agent_provider')"
# expect: responses ['200','400','401','409','424','500'] and 'OK'. Today the
# list is ['200','400','401','424','500'] and the assert fires.
# ...and the generated client actually carries it, or (d) has nothing to catch.
grep -c "PrivacyBarrierBody" ui/desktop/src/api/types.gen.ts ; echo "expect: >= 1 (0 today)"
# The client throws.
awk '/const changeModel/,/^  \);/' ui/desktop/src/components/ModelAndProviderContext.tsx \
  | grep -c "throwOnError" ; echo "expect: 2 (updateAgentProvider AND setConfigProvider); 1 today"
# The concurrency gate can FORCE its interleavings, and its seams ship in no
# binary. A `#[cfg(test)]` that drifted off the module is a rendezvous point in
# production code.
awk '/pub async fn update_provider/,/^    }/' crates/biorouter/src/agents/agent.rs \
  | grep -n "cfg(test)\|seams::\|bind_provider_if_allowed\|\*current_provider = Some"
echo "Expected FOUR lines, in this order: seams::before_bind_update (under a"
echo "  cfg(test) on the line above it), bind_provider_if_allowed,"
echo "  seams::after_bind_before_swap (likewise), *current_provider = Some."
echo "  A seam call NOT preceded by #[cfg(test)] fails this gate."
grep -c "pub(crate) mod seams" crates/biorouter/src/agents/agent.rs ; echo "expect: 1"
grep -n -B1 "pub(crate) mod seams" crates/biorouter/src/agents/agent.rs | grep -c "cfg(test)"
echo "expect: 1 — the module itself is test-only"
# The fuzz loop is multi-threaded, or it schedules nothing. `#[tokio::test]`
# defaults to current_thread, where two spawns interleave only at await points
# and do so identically on all 200 iterations.
grep -n -A1 'flavor = "multi_thread"' crates/biorouter/src/agents/agent.rs \
  | grep -c "the_unconstrained_race_observes_both_outcomes" ; echo "expect: 1"
# …and it asserts it actually raced, rather than reporting a one-sided loop as a pass.
awk '/async fn the_unconstrained_race_observes_both_outcomes/,/^    }/' \
  crates/biorouter/src/agents/agent.rs | grep -c "bound > 0 && refused > 0" ; echo "expect: 1"
```

**What this catches.** The wrong implementation adds the check to `Agent::update_provider` as an
`if` **before** the existing body, leaving the swap-then-persist order at `:5663-5666` intact. It
passes any test that only asserts `Err`, and it leaves the live agent running on the refused public
model — the precise inverse of the design's promise. The first assertion of Step 1's first test and
the `awk` ordering gate are what fail it. Separately, shipping Gate A without (c) and (d) reproduces
the shipped bug exactly: a refusal rendered as a green success toast.

**And the concurrency gate.** Its previous form — 200 unconstrained `tokio::spawn` pairs and
`if row.privacy_tier.is_private() { assert_eq!(row.provider_name, Some("versa_azure")) }` — failed
in three independent ways at once, which is why it is rewritten rather than tuned. (i) It could not
produce the interleaving: `#[tokio::test]` is `current_thread`, so the two tasks cannot preempt each
other and interleave only at `.await` points, in the same order on every one of the 200 iterations.
(ii) Its assertion was guarded by an `if` on the very condition that decides whether anything is
checked, and the ratchet always wins the row, so the guard skipped the assertion in the only branch
that could have caught something. (iii) The assertion was **false for a correct implementation**: a
ratchet that commits after a legal bind produces (private, public provider), and that is the
residual Gate B owns, not a violation of Gate A. **This gate rejects: an implementation in which the
`WHERE` predicate is evaluated in Rust between two statements instead of inside the UPDATE** —
`a_bind_is_never_accepted_against_a_row_that_is_already_private` parks `update_provider` at a seam,
runs the whole ratchet inside the window, releases it, and requires a `PrivacyRefusal`; a
read-then-write implementation binds instead, and no amount of looping would have shown it. It also
rejects a fuzz loop that races nothing, because `bound > 0 && refused > 0` is now asserted.

- [ ] **Step 6: Commit**

```bash
git add crates/biorouter/src/privacy/refusal.rs crates/biorouter/src/agents/agent.rs \
        crates/biorouter/src/session/session_manager.rs \
        crates/biorouter-server/src/routes/agent.rs ui/desktop/openapi.json ui/desktop/src/api \
        ui/desktop/src/components/ModelAndProviderContext.tsx
git commit -m "feat(privacy): Gate A - refuse a public model on a private session, with a typed 409 (#56)"
```

---

### Task 13: Gate B — the turn, the ratchet, and the assertion on the non-reply completions

`Agent::reply` (`:3258`) is genuinely the sole turn entry: `grep -rn "\.reply(" --include='*.rs' crates/`
returns 8 non-test callers covering every surface — `routes/reply.rs:765`, `routes/apps.rs:1706`
and `:3707`, `scheduler.rs:897`, `subagent_handler.rs:239`, `biorouter-acp/src/server.rs:961`,
`biorouter-cli/src/session/mod.rs:1120` and `:1302`, `tui/mod.rs:529`, `commands/web.rs:604`.

⚠ **"Top of `Agent::reply`" is the wrong literal placement.** The prologue has two early returns
before any provider contact — the elicitation-response branch at `:3285-3320`, which persists the
user's message and returns a stream of already-built events. A gate at the literal top would refuse
to deliver an elicitation answer, or to record the daemon-restart interruption notice, on a session
in the residual state. **The seam is after the elicitation early-returns and before `restore_goal`
at `:3328`.** The session row is already loaded at `:3337` for hooks, so reuse it.

⚠ **A refusal must `yield` and `return` inside the stream, not `Err` out of `reply`.** `reply`
returns `Result<BoxStream<..>>` built by `async_stream::try_stream!`; the precedent is the
compaction-failure arm at `:3702-3709`. A bare `Err` surfaces as a 500 from `/reply`.

**Files:**

| Action | Path | Anchor (re-verified at `9558c346`) |
|---|---|---|
| Modify | `crates/biorouter/src/agents/agent.rs` | `reply` `:3258`; elicitation returns `:3285-3320`; `restore_goal` `:3328`; session load `:3337`; the compaction-failure `yield`+`return` precedent `:3702-3709`; `reply_internal` call `:3713`; `provider()` `:2511-2516`; `maybe_rename_session` `:5632-5648` |
| Reference | `crates/biorouter/src/session/session_manager.rs` | `maybe_update_name` `:1646` |
| Reference | `crates/biorouter/src/providers/base.rs` | `generate_session_name` → `complete_fast` `:675-687` |
| Reference | `crates/biorouter/src/context_mgmt/mod.rs` | `complete_summary` `:1018-1028` |
| Reference | `crates/biorouter/src/agents/stall.rs` | the stall judge at `:420` |

- [ ] **Step 1: Write the failing tests — three cases, and any single-case gate passes a wrong one**

```rust
#[tokio::test]
async fn a_repairable_mismatch_rebinds_silently_and_the_turn_runs() {
    // The residual state: privacy_tier=private, live agent holds a public
    // provider (LRU rehydration, the Config::global() fallback, a legacy row).
    // The row still names a private provider, so Gate B rebinds FROM THE ROW
    // and continues — the user never sees it. An implementation that only
    // refuses fails this, and it is the majority case on a real machine.
    let (agent, s) = agent_on(public_provider()).await;
    set_row(&s.id, privacy_tier = "private", provider_name = "versa_azure").await;

    let events = drain(agent.reply(user("hi"), cfg(&s), None).await.unwrap()).await;
    assert!(!events.iter().any(is_refusal), "{events:#?}");
    assert_eq!(agent.provider().await.unwrap().get_name(), "versa_azure");
}

#[tokio::test]
async fn an_unrepairable_mismatch_refuses_this_turn_and_leaves_the_row_alone() {
    let (agent, s) = agent_on(public_provider()).await;
    set_row(&s.id, privacy_tier = "private", provider_name = "anthropic").await;

    let events = drain(agent.reply(user("hi"), cfg(&s), None).await.unwrap()).await;
    assert!(events.iter().any(is_refusal));
    // A refusal, not a 500: the stream yields and returns.
    assert!(events.iter().all(|e| !matches!(e, Err(_))));
    let row = reread(&s.id).await;
    assert_eq!(row.privacy_tier, SessionClassification::Private);
    assert_eq!(row.provider_name.as_deref(), Some("anthropic"));
}

#[tokio::test]
async fn an_elicitation_answer_is_still_delivered_on_a_private_session() {
    // The seam matters: at the literal top of `reply` this refuses, and the
    // user's answer to a parked tool call is silently dropped.
    let (agent, s) = agent_on(public_provider()).await;
    set_row(&s.id, privacy_tier = "private", provider_name = "anthropic").await;
    let events = drain(agent.reply(elicitation_answer("yes"), cfg(&s), None).await.unwrap()).await;
    assert!(!events.iter().any(is_refusal));
}

#[tokio::test]
async fn the_first_turn_ratchets_and_a_permitted_bind_afterwards_is_refused() {
    let (agent, s) = agent_on(private_provider()).await;
    assert_eq!(reread(&s.id).await.privacy_tier, SessionClassification::Public);   // the bind did NOT ratchet
    let _ = drain(agent.reply(user("hi"), cfg(&s), None).await.unwrap()).await;
    let row = reread(&s.id).await;
    assert_eq!(row.privacy_tier, SessionClassification::Private);
    assert_eq!(row.privacy_reason.as_deref(), Some("turn:versa_azure"));
    assert!(agent.update_provider(public_provider(), &s.id).await.is_err());
}

#[tokio::test]
async fn auto_naming_a_private_transcript_on_a_public_provider_is_refused() {
    // Gate B'. `maybe_rename_session` -> `maybe_update_name` ->
    // `generate_session_name` -> `complete_fast` reads the entire transcript
    // and never passes `reply`. Same for compaction (context_mgmt:1018-1028)
    // and the stall judge (stall.rs:420).
    let (agent, s) = agent_on(private_provider()).await;
    let _ = drain(agent.reply(user("hi"), cfg(&s), None).await.unwrap()).await;   // ratchets
    swap_shared_provider_out_of_band(&agent, public_provider()).await;
    agent.maybe_rename_session(&s.id).await;
    assert_eq!(public_provider_completion_count(), 0);
}
```

- [ ] **Step 2: Run** → **FAIL** on all five (no gate exists; the first "passes" only because
      nothing refuses, so assert the rebind explicitly as shown).

- [ ] **Step 3: Implement**

Between `:3321` and `:3328`:

```rust
        // Issue #56 Gate B. Placed AFTER the elicitation early-returns above:
        // an elicitation answer is a user action on a parked tool call, not a
        // disclosure, and refusing it at the literal top of `reply` silently
        // drops the answer and the daemon-restart notice. Placed BEFORE
        // `restore_goal` so nothing runs on a session we are about to refuse.
        //
        // Repair-first, and the repair is the common case: LRU rehydration,
        // `restore_provider_from_session`'s Config::global() fallback, a
        // pre-fix diverge and every legacy row all land here.
        let privacy_row = session_manager.get_session(&session_config.id, false).await.ok();
        let mut privacy_refusal: Option<String> = None;
        if let Some(row) = privacy_row.as_ref() {
            let bound = self.provider().await.ok();
            let bound_tier = bound.as_ref().map(|p| p.tier()).unwrap_or(ProviderTier::Public);
            if !crate::privacy::bind_allowed(bound_tier, row.privacy_tier) {
                match self.rebind_from_row(row).await {
                    // 2. The row still names a provider whose tier satisfies
                    //    the classification: rebind and continue silently.
                    Ok(true) => {}
                    // 3. Otherwise refuse THIS TURN. The row is untouched, so
                    //    the repair card can still offer the one-click fix.
                    _ => privacy_refusal = Some(crate::privacy::refusal::turn_refusal(row)),
                }
            }
            // 1. The ratchet. It fires HERE and on a permitted private-extension
            //    dispatch (Gate C) — never on the bind (O5).
            if privacy_refusal.is_none() {
                let f = crate::privacy::floor(self.provider().await?.tier());
                if f > row.privacy_tier {
                    session_manager
                        .update(&session_config.id)
                        .raise_privacy(f, &format!("turn:{}", self.provider().await?.get_name()))
                        .apply()
                        .await?;
                }
            }
        }
        self.cached_classification
            .store(privacy_row.map(|r| r.privacy_tier).unwrap_or(SessionClassification::Private));
```

and inside the stream body, before `restore_goal`'s effects are consumed, following the
compaction-failure precedent at `:3702-3709`:

```rust
            if let Some(text) = privacy_refusal {
                yield AgentEvent::Message(Message::assistant().with_text(text));
                return;
            }
```

Gate B' in `Agent::provider()` (`:2511-2516`), which already returns
`Result<Arc<dyn Provider>>` (`Err(anyhow!("Provider not set"))` today):

```rust
    pub async fn provider(&self) -> Result<Arc<dyn Provider>, anyhow::Error> {
        let provider = self.provider.lock().await.clone()
            .ok_or_else(|| anyhow!("Provider not set"))?;
        // Issue #56 Gate B'. The non-`reply` completion paths — session
        // auto-naming (`maybe_rename_session` -> `complete_fast`), compaction
        // summarisation (`context_mgmt/mod.rs:1018-1028`) and the stall judge
        // (`stall.rs:420`) — all read the entire transcript and never pass
        // Gate B. `AgentManager` caches one Arc<Agent> per session id, so the
        // cached classification is sound, and it re-syncs at every reply entry.
        let cached = self.cached_classification.load();
        if !crate::privacy::bind_allowed(provider.tier(), cached) {
            return Err(PrivacyRefusal::PublicModelOnPrivateSession {
                session_id: self.session_id_for_diagnostics(),
                provider: provider.get_name().to_string(),
            }
            .into());
        }
        Ok(provider)
    }
```

⚠ `cached_classification` **initialises to `Private`**, not `Public`. It is read before the first
`reply` on a rehydrated agent, and the safe default there is the restrictive one.

⚠ **Two module edits ride this commit.** (1) `privacy/refusal.rs` (created in Task 12) gains
`turn_refusal(&Session) -> String` and takes ownership of Task 10's file-local
`CHATRECALL_LOAD_REFUSAL`, re-exported as `chatrecall_load_refusal()`; delete the `const` from
`chatrecall_extension.rs` in the same diff so there is never a second copy of a refusal string.
(2) Task 7's `EXPECTED` constant in `privacy/mod.rs` gains its first uncommented line,
`("crates/biorouter/src/agents/agent.rs", 1)` — **in this commit**, because this is the commit that
adds the crossing.

- [ ] **Step 4: Run**

```bash
cargo test -p biorouter --lib agents::agent
cargo test -p biorouter --test subagent_delegation
cargo test -p biorouter --test soft_interrupt_agent_loop
cargo test -p biorouter --test conversation_writeback_freshness
```

The three integration targets are not touched by any `--lib` filter and are where a
reply-prologue edit shows up as a reordering.

- [ ] **Step 5: Gate**

```bash
# The seam. The gate must sit between the elicitation return and restore_goal.
# ⚠ SYMBOL-anchored, not NR-anchored. The first version read
# `awk 'NR>=3258 && NR<=3400'`, which violates this plan's own rule ("the named
# SYMBOL is the anchor, never the line number") in the one file it warns about
# most — and this task's own Step 3 inserts ~15 lines INTO that window, so the
# fixed end at 3400 walks off the end of the prologue as the edit lands.
awk '/pub async fn reply\(/,/self\.restore_goal\(/' crates/biorouter/src/agents/agent.rs \
  | grep -n "ElicitationResponse\|bind_allowed\|restore_goal"
# Expected order: ElicitationResponse ... bind_allowed ... restore_goal.
# Measured today (no #56 code): 2 lines — ElicitationResponse at rel 23 and
# restore_goal at rel 71, i.e. a 71-line prologue. A run that prints ONE line is
# a range that did not terminate (check `self.restore_goal(` still occurs);
# a run that prints bind_allowed AFTER restore_goal is the seam bug this exists
# to catch.
# The refusal is a yield, never an Err out of reply. PRINT the lines rather than
# counting them: the count depends on how the implementation spells the guard,
# and a number that goes stale the first time someone merges two lines is the
# defect class this gate is supposed to catch, not commit.
awk '/pub async fn reply\(/,/^    }/' crates/biorouter/src/agents/agent.rs \
  | grep -n "privacy_refusal"
# Expected: FOUR lines, in this order and with these roles —
#   1. `let mut privacy_refusal: Option<String> = None;`   (declare)
#   2. `privacy_refusal = Some(crate::privacy::refusal::turn_refusal(row))`  (set)
#   3. `if privacy_refusal.is_none() {`                    (the ratchet's guard)
#   4. `if let Some(text) = privacy_refusal {`             (the yield)
# The load-bearing assertion is (4)'s neighbourhood, so check it directly:
awk '/if let Some\(text\) = privacy_refusal/,/^            }/' crates/biorouter/src/agents/agent.rs \
  | grep -c "yield AgentEvent::Message" ; echo "expect: 1 — a yield, not a return Err"
awk '/pub async fn reply\(/,/^    }/' crates/biorouter/src/agents/agent.rs \
  | grep -c "return Err(PrivacyRefusal" ; echo "expect: 0 — that is a 500 from /reply"
# The ratchet is on the turn, not the bind.
awk '/pub async fn update_provider/,/^    }/' crates/biorouter/src/agents/agent.rs \
  | grep -c "raise_privacy" ; echo "expect: 0"
awk '/pub async fn reply\(/,/^    }/' crates/biorouter/src/agents/agent.rs \
  | grep -c "raise_privacy" ; echo "expect: 1"
# The one new `floor` crossing, and Task 7's audit updated in the SAME commit.
awk '/pub async fn reply\(/,/^    }/' crates/biorouter/src/agents/agent.rs \
  | grep -c "privacy::floor(" ; echo "expect: 1"
cargo test -p biorouter --lib \
  privacy::tests::floor_is_crossed_only_where_a_capability_establishes_a_classification \
  | grep "test result:" ; echo "expect: 1 passed; 0 failed"
# ⚠ "Expected: PASS" is not a gate for a NAMED filter. A libtest filter that
# resolves to nothing prints `0 passed` and EXITS 0 — so a typo in the test name,
# or a test that Task 7 nested under a different module than `privacy::tests`,
# reads exactly like success. Assert the printed count. A failure here naming
# agent.rs means the constant was not bumped in this commit.
```

⚠ **`expect: 3 (declare, set, yield)` was wrong** in the first version of this plan and would have
failed a correct implementation: Step 3's own code has **four** occurrences, because the ratchet is
guarded by `if privacy_refusal.is_none()`. This is the reason the gate now prints and describes
rather than counting — a hand-computed occurrence count inside a plan goes stale the first time
anyone edits the implementation, and the worker's instinct on a red gate is to change the code.

**What this catches.** Four wrong implementations, one per test. (1) A refuse-only Gate B, which
bricks every rehydrated session on a machine measured at 57% private — test 1. (2) A gate at the
literal top of `reply`, which drops elicitation answers — test 3. (3) A ratchet at the bind, which
privatises a chat on a mis-click and still misses `POST /agent/call_tool` — test 4 plus the last two
greps. (4) A gate that lives only in `reply`, leaving `complete_fast` session-naming to send the
whole transcript to a public model — test 5, which is why it swaps the shared `Arc` out of band
rather than through `update_provider`.

- [ ] **Step 6: Commit**

```bash
git add crates/biorouter/src/agents/agent.rs crates/biorouter/src/privacy/
git commit -m "feat(privacy): Gate B - repair-first turn barrier, the ratchet, and the completion assertion (#56)"
```

---

### Task 14: Gate C — the dispatch choke point, and the refusal as a pure function

**Files:**

| Action | Path | Anchor (re-verified at `9558c346`) |
|---|---|---|
| Modify | `crates/biorouter/src/agents/extension_manager.rs` | `dispatch_tool_call` `:1438`; `get_client_for_tool` resolution `:1461-1470`; prefix strip `:1471-1481`; `is_tool_available` `:1483-1494`; the BR-23 SecretGuard block `:1496-1522` with its choke-point comment at `:1499-1502` |
| Modify | `crates/biorouter/src/privacy/refusal.rs` | **created by Task 12**, not here; this task adds `privacy_refusal(..)` beside the `PrivacyRefusal` enum and Task 13's `turn_refusal` |
| Modify | `crates/biorouter-server/src/routes/agent.rs` | `call_tool` `:1140-1176`, dispatch `:1160-1163`, and its `.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)` |
| Reference | `crates/biorouter/src/agents/extension_manager.rs` | `ExtensionManager::new` `:620-623` takes `session_manager: Arc<SessionManager>` and stores it at `:628` — which is what makes the permit-time ratchet below need no new plumbing |

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn the_refusal_is_pure_deterministic_and_forecloses_the_workaround() {
    use ProviderTier::{Private, Public};
    // Modelled on `check_enable_allowed` (extension_manager_extension.rs:97-125),
    // whose four tests at :538-576 need no global config precisely because it
    // is pure. Same register: name the state, name the reason, foreclose the
    // workaround, name the human action.
    assert!(privacy_refusal("ucsfomopagent", Private, Private).is_none());
    assert!(privacy_refusal("developer", Public, Public).is_none());
    assert!(privacy_refusal("developer", Public, Private).is_none());

    let e = privacy_refusal("ucsfomopagent", Private, Public).unwrap();
    let m = e.message.to_string();
    assert!(m.contains("ucsfomopagent"));
    assert!(m.contains("private"));
    assert!(m.contains("marketplace"));           // names the grantor (R11)
    assert!(m.contains("Settings"));              // names the human action
    assert!(m.contains("do not"));                // forecloses the workaround
    // Deterministic: a model that sees a different string on retry concludes
    // the refusal is transient and loops.
    assert_eq!(m, privacy_refusal("ucsfomopagent", Private, Public).unwrap().message.to_string());
}

#[tokio::test]
async fn every_convergent_path_into_the_manager_is_refused() {
    // Three separate assertions, one per production path. A single agent-loop
    // test passes an implementation written as a ToolInspector, which paths 2
    // and 3 bypass entirely — and path 4 runs BEFORE the turn.
    let text_from_agent_loop      = call_private_tool_via_agent_loop().await;
    let text_from_http_call_tool  = call_private_tool_via_http_call_tool().await;
    let text_from_execute_code    = call_private_tool_via_execute_code_bridge().await;
    let text_from_prefetch        = call_private_tool_via_call_prefetch_tool().await;
    for t in [text_from_agent_loop, text_from_http_call_tool,
              text_from_execute_code, text_from_prefetch] {
        assert!(t.contains("ucsfomopagent"), "refusal did not reach the caller: {t}");
        assert!(!t.contains("The user has declined"), "laundered as a decline: {t}");
    }
}

#[tokio::test]
async fn a_permitted_private_dispatch_ratchets_the_session() {
    let (agent, s) = agent_on(private_provider()).await;
    // No turn has run, so the row is still public.
    assert_eq!(reread(&s.id).await.privacy_tier, SessionClassification::Public);
    call_tool_via_http("ucsfomopagent__run_query", &s.id).await.unwrap();
    let row = reread(&s.id).await;
    assert_eq!(row.privacy_tier, SessionClassification::Private);
    assert_eq!(row.privacy_reason.as_deref(), Some("mcp:ucsfomopagent"));
}
```

- [ ] **Step 2: Run** → **COMPILE ERROR** (`cannot find function privacy_refusal`).

- [ ] **Step 3: Implement**

`privacy/refusal.rs` — pure, no I/O, no config:

```rust
/// Gate C's refusal. Returns `None` when the call is permitted, so the caller
/// reads as `if let Some(err) = privacy_refusal(..) { return Err(err.into()); }`.
///
/// `ErrorData` directly, NOT a `ToolInspector`. `handle_denied_tools`
/// (`agent.rs:2455-2508`) passes a real reason through for exactly three
/// inspector names — the hook inspector (`:2474`), `"security"` (`:2480`) and
/// the repetition inspector (`:2487`) — and everything else falls to
/// `DECLINED_RESPONSE` at `:2493`, which the code itself calls "actively
/// misleading". An inspector-shaped Gate C would also be invisible to
/// `POST /agent/call_tool` and to the `execute_code` bridge.
pub fn privacy_refusal(
    extension: &str,
    extension_tier: ProviderTier,
    caller_tier: ProviderTier,
) -> Option<ErrorData> {
    if extension_tier != ProviderTier::Private || caller_tier == ProviderTier::Private {
        return None;
    }
    Some(ErrorData::new(
        ErrorCode::INVALID_REQUEST,
        format!(
            "`{extension}` is a private extension: it reaches data held inside the institution, \
             so only a private model may call it. This session is running on a public model. \
             Ask the user to switch this chat to a private model — Settings > Models, or the \
             model chip in the composer — and then try again. This is a data-protection \
             boundary set by the Biorouter marketplace, not something to work around: do not \
             retry with a different tool name, through code execution, or through a resource \
             read."
        ),
        None,
    ))
}
```

In `dispatch_tool_call`, between the `is_tool_available` check ending at `:1494` and the SecretGuard
block beginning at `:1496`:

```rust
        // Issue #56 Gate C, beside BR-23's SecretGuard block for the reason its
        // own comment states at :1499-1502: this is the single choke point every
        // tool call flows through. FOUR production paths converge here and only
        // one carries a ToolInspector — the agent loop (agent.rs:2772),
        // POST /agent/call_tool (routes/agent.rs:1162), the execute_code JS
        // bridge (code_execution_extension.rs:1815) and Agent::call_prefetch_tool
        // (agent.rs:1618, which runs BEFORE the turn).
        //
        // Read off the RESOLVED record, never off the tool-name string:
        // `get_client_for_tool` (:1183) routes by `starts_with` over a HashMap
        // in nondeterministic order and `normalize()` permits `_`, so extensions
        // keyed `a` and `a__b` make `a__b__c` ambiguous.
        let ext_tier = self
            .extensions
            .lock()
            .await
            .get(&client_name)
            .map(|e| e.tier)
            .unwrap_or(ProviderTier::Private);   // unknown record => refuse
        let caller_tier = self.capability_tier().await;
        if privacy_tiers_enabled() {
            if let Some(err) = crate::privacy::refusal::privacy_refusal(
                &client_name, ext_tier, caller_tier,
            ) {
                return Err(err.into());
            }
        }
```

and, immediately after the refusal check — at **permit time**, before the future is built — the
second trigger of O5:

```rust
        // Issue #56, O5's second trigger. At PERMIT time, not on the tool's
        // result, and that is forced by the shape of this function rather than
        // chosen: `dispatch_tool_call` returns `Ok(ToolCallResult { result:
        // Box::new(fut.boxed()) })` at :1572-1575 BEFORE the tool has run, and
        // the `async move` at :1544 captures owned values only — it cannot hold
        // `&self`, so there is no `self` at the point the call succeeds.
        //
        // Permit-time is also the right direction. "The model was allowed to
        // ask a private extension a question" is the disclosure; whether the
        // extension answered is not the user's protection. Ratcheting on
        // success would leave a failed OMOP query — which still carried the
        // session's cohort definition to the connector — unrecorded.
        //
        // `self.context.session_manager` is the Arc `ExtensionManager::new`
        // takes at :620-623 and stores at :628, so this needs no new plumbing.
        if ext_tier.is_private() {
            self.raise_session_privacy(session_id, &format!("mcp:{client_name}")).await;
        }
```

⚠ **The first version of this plan said "on the success return of the dispatch (so a failed call does
not ratchet)". That is not implementable here** and the claim was false about the code. If a future
reviewer wants ratchet-on-success, it needs a cloned `Arc<SessionManager>` moved into the `async move`
block and a raise after the `.await` at `:1562` — write that trade down before doing it, because it
also moves the raise off the thread that holds the permit.

`privacy_tiers_enabled()` reads the **master** opt-out (DR-15) **inside** the gate, not through an
`is_enabled()` wrapper, following the `SensitiveOpsInspector` pattern, so a mid-session change is
honoured and the opt-out is one auditable line rather than an absent gate. Task 30 implements it;
until then it is `const fn … { true }` in `crates/biorouter/src/privacy/mod.rs`.

⚠ **It is the same predicate in every gate, and that is the whole design.** DR-9's Gate-C-only key
is retired. Do not introduce a second, narrower flag for any individual gate: the reason Task 30 can
gate the toggle with a behavioural on/off matrix over ten gates is that there is exactly one thing to
flip.

Fix `POST /agent/call_tool`'s error mapping in the same commit: `routes/agent.rs:1162-1164` does
`.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)`, which throws the refusal away. Return the
`ErrorData`'s message as the tool result, exactly as the agent loop does.

- [ ] **Step 4: Run**

```bash
cargo test -p biorouter --lib -- privacy::refusal agents::extension_manager
cargo test -p biorouter-server --lib routes::agent
```

- [ ] **Step 5: Gate**

```bash
# There is exactly ONE production call into an MCP client, and it is inside
# dispatch_tool_call. This is what makes the choke-point claim checkable rather
# than asserted, and it fails the day someone adds a second path.
#
# ⚠ The filter is an EXACT line, not a prefix. The first version excluded
# `^crates/biorouter/src/agents/extension_manager.rs:15`, which is a prefix match
# on the line NUMBER: it swallows :15, :150-:159 and all of :1500-:1599 in a
# 3206-line file — i.e. it would hide a brand-new `.call_tool(` added anywhere in
# that hundred-line window, which is the window `dispatch_tool_call` lives in.
grep -rn "\.call_tool(" --include='*.rs' crates/ \
  | grep -v "^crates/biorouter/src/agents/extension_manager.rs:1562:"
echo "expect: exactly 9 lines, in FOUR files, ALL of them tests — measured at 9558c346:"
echo "  crates/biorouter/src/agents/skills_extension.rs        :1229 :1306 :1326 :1345"
echo "     (inside #[cfg(test)] which begins at :798)"
echo "  crates/biorouter/src/agents/code_execution_extension.rs :2140 :2177 :2247"
echo "     (inside #[cfg(test)] which begins at :2115)"
echo "  crates/biorouter-mcp/tests/preview_fixture_dump.rs      :53"
echo "  crates/biorouter-mcp/tests/agent_drafter_registered.rs  :72"
echo "The first version named only two of the four files, so the two integration"
echo "files under tests/ read as unexplained hits and invite a worker to 'fix' them."
echo "A tenth line, or any line whose path is under a crate's src/ and not in one"
echo "of the two #[cfg(test)] spans above, is a new bypass — read the diff."
# The gate reads the RESOLVED RECORD, not the tool-name string. Asserted as two
# anchored patterns, positive and negative, rather than as a count over an awk
# range: the range `/pub async fn dispatch_tool_call/,/SecretGuard/` is 67 lines
# and spans the prefix-strip at :1471-1481, so `grep -c prefixed_name` over it
# returns 3 TODAY, before a line of #56 exists. That gate could never pass and
# never measured what its comment said.
# ⚠ `grep -c "\.get(&client_name)" ; expect: >= 1` was VACUOUS: it is already 1
# today, at :1483 (`self.extensions.lock().await.get(&client_name)`, the
# pre-existing is_tool_available check), so it was green before a line of #56
# existed. Assert that the PRIVACY LOOKUP is the thing keyed on client_name,
# scoped to the function, and pair it with the zero-count on the wrong key.
awk '/pub async fn dispatch_tool_call/,/^    }/' crates/biorouter/src/agents/extension_manager.rs \
  | grep -nE "privacy_refusal\(|capability_tier\(" 
echo "expect: at least one line, and EVERY one of them BELOW rel 23, where"
echo "  'let (client_name, client) = self.get_client_for_tool(..)' binds. A refusal"
echo "  computed above that line has no resolved record to read."
awk '/pub async fn dispatch_tool_call/,/^    }/' crates/biorouter/src/agents/extension_manager.rs \
  | grep -c "get(&client_name)" ; echo "expect: 2 — the pre-existing is_tool_available lookup at"
echo "  rel 46, plus this task's tier lookup. A 1 means the tier came from somewhere else."
grep -cE "privacy_refusal\(&(prefixed_name|tool_name)|classify_extension\(&(prefixed_name|tool_name)" \
  crates/biorouter/src/agents/extension_manager.rs
echo "expect: 0 — the tier is never resolved from the tool-name string"
# Gate C is not an inspector. ⚠ `grep -rn "PrivacyInspector"` is VACUOUS: it is
# a name this plan invented, it is 0 today, and it is 0 under every wrong
# implementation too — a worker who genuinely writes Gate C as an inspector will
# call it something else. Assert on the TRAIT instead, whose impl set is a
# closed, measured list; a new impl under any name trips it.
diff <(grep -rl "impl ToolInspector for" --include='*.rs' crates/ | sort) <(cat <<'EOF'
crates/biorouter/src/hooks/inspector.rs
crates/biorouter/src/permission/managed_inspector.rs
crates/biorouter/src/permission/permission_inspector.rs
crates/biorouter/src/security/security_inspector.rs
crates/biorouter/src/security/sensitive_ops.rs
crates/biorouter/src/tool_monitor.rs
crates/biorouter/tests/tool_inspection_manager_tests.rs
EOF
) && echo "OK: no new ToolInspector — Gate C is a branch in dispatch_tool_call, not a plugin"
# expect: no diff output, then "OK". Measured at 9558c346: exactly these 7 files
# (8 impls; tool_inspection_manager_tests.rs has two mocks). Registration is the
# other half — nothing this feature adds may reach the inspector manager:
grep -rn "add_inspector(\|register_inspector(" --include='*.rs' crates/ | grep -ci "privacy"
echo "expect: 0"
# The ratchet is at permit time, so it sits ABOVE the `let fut = async move` at :1544.
awk '/pub async fn dispatch_tool_call/,/^    }/' crates/biorouter/src/agents/extension_manager.rs \
  | grep -n "raise_session_privacy\|let fut = async move" | head -2
# Expected: raise_session_privacy on the SMALLER line number.
```

**What this catches.** The wrong implementation puts the check in `Agent::dispatch_tool_call`
(`agent.rs:2624`) — the function with the same name, one frame up, and the one that carries the
`ToolInspector` machinery. It passes an agent-loop test and misses `POST /agent/call_tool`,
`execute_code`'s inner bridge and `call_prefetch_tool`. Only the four-path test in Step 1 fails it.
The second wrong implementation keys on the tool-name prefix rather than the resolved
`client_name`, which disagrees with `get_client_for_tool` for any extension whose key contains
`__` — see Task 16, where the same divergence is the leaky direction.

- [ ] **Step 6: Commit**

```bash
git add crates/biorouter/src/privacy/refusal.rs crates/biorouter/src/agents/extension_manager.rs \
        crates/biorouter-server/src/routes/agent.rs
git commit -m "feat(privacy): Gate C - refuse a private extension under a public model at the dispatch choke point (#56)"
```

⚠ `chatrecall_extension.rs` was in this `git add` in the first version because Task 10's refusal
constant was supposed to move here. It moves in **Task 13** instead (see that task's second ⚠), so it
is not part of this commit.

---

### DR-14 is two layers, and the OS sandbox is the second one

Read this before Tasks 14A, 14B, 14D. The first two rounds of this plan treated the OS sandbox as
*the* mechanism, and both times a reviewer found a public tool that reads a private root without ever
spawning a process. That is not a gap in the list of tools; it is a category error. **BioRouter reads
files two different ways, and only one of them is a child process.**

| | **Layer A — the in-process path policy** | **Layer B — the OS sandbox** |
|---|---|---|
| Covers | every tool call, because the check is in the daemon's own dispatch path | processes the daemon spawns |
| Mechanism | a synchronous refusal at `ExtensionManager::dispatch_tool_call` (`extension_manager.rs:1438`) | Seatbelt / bubblewrap, wrapping the child |
| Enforced by | BioRouter | the kernel |
| Defeated by | nothing that is still a tool call | nothing, once the child is wrapped |
| Role | **PRIMARY** | **defence in depth** |
| Task | 14B (barrier) + 14D (seams) | 14A (mechanism) + 14B step (h) (wiring) |

**Why Layer A has to be primary.** `computercontroller__cache` reads a caller-supplied path with
`tokio::fs::read_to_string` at `computercontroller/mod.rs:1482`. `agent_drafter__read_app` reads app
bytes with `std::fs::read_to_string` at `agent_drafter/store.rs:637`. `developer__text_editor` opens
files at `text_editor.rs:641`. None of them spawns anything. **They are the daemon.** No sandbox the
daemon installs on its children can constrain the daemon, so on every platform — including the two
where Layer B works perfectly — those reads are governed by a check in the code path or by nothing at
all.

**Why the check must sit at a choke point rather than on a list of tools.** Enumerating readers has
now been defeated twice: round 1's list named `developer` and `computercontroller`, and round 2 found
`cache` *inside* `computercontroller`. It is not a discipline problem. A `grep` for
`fs::`/`File::open` **structurally cannot** find these readers, measured:

| Tool | How it reads the caller's path | Why a `fs::` grep misses it |
|---|---|---|
| `computercontroller__xlsx_tool` | `umya_spreadsheet::reader::xlsx::read(path)` — `computercontroller/xlsx_tool.rs:37` | no `fs::` token in the line |
| `computercontroller__pdf_tool` | `lopdf::Document::load(path)` — `computercontroller/pdf_tool.rs:34` | ditto |
| `computercontroller__docx_tool` | `fs::read(path)` — `computercontroller/docx_tool.rs:108` | found, but only because this one happens to use `fs` |
| `datasql__data_query` | `DataSql::open_readonly(&path)` (sqlx) — `datasql/server.rs:115` | ditto |
| `knowledge__kb_import` | `std::fs::read(&p.src_path)` — `knowledge/server.rs:769` | found; the *decoders* under `knowledge/brkb.rs:28,38` are not |

Measured on this tree: **125 `#[tool(name=…)]` declarations in `crates/biorouter-mcp/src`, 48 with a
path-shaped parameter** — and that 48 both over-counts (`agent_drafter__ui_render`'s `target` is a DOM
region) and under-counts (`analyze`'s params live in a different file, `developer/analyze/types.rs:6`).
A mechanical extractor written for this survey silently dropped **the entire developer server**
because `rmcp_developer.rs:337` contains an inner `#[cfg(test)]` and the extractor stopped there.
**Any gate phrased as "the list of tools that read files" is unfixable. The gate must be phrased as
"every tool call passes through symbol X."**

**The choke point exists and is already built.** `ExtensionManager::dispatch_tool_call`
(`extension_manager.rs:1438`) carries BR-23's SecretGuard argument scan at `:1505-1527`, whose own
comment at `:1497-1504` calls it *"the single choke point every tool call flows through."* It covers
the 7 built-ins, the 5 platform extensions, the in-process app servers (`add_inprocess_server` at
`:901` inserts into the *same* `self.extensions` map, `:909`/`:931`) and `code_execution`'s inner
bridge (`code_execution_extension.rs:1814` re-enters the **ExtensionManager's** dispatch, not the
Agent's). There is exactly one `client.call_tool` in production, at `:1562`, inside it. Task 14B
extends that mechanism instead of inventing a second one.

**What this reframing buys, and it is worth stating in the design.** Because Layer A holds on every
platform, Layer B's platform gaps stop being feature-killers. Landlock cannot subtract a read and
Windows has no unprivileged confinement — but that no longer means "a public session cannot read
files on those hosts", it means "a public session cannot **spawn a shell** on those hosts". The
fail-closed refusal (AR-6(1)) narrows from *every file tool* to *the spawned-shell tools*:
`developer__shell` and its background jobs, `computercontroller__automation_script` and
`computer_control`, and `compute_run`/`compute_python`. `text_editor`, `analyze`, `image_processor`,
`cache`, `xlsx_tool`, `pdf_tool`, `docx_tool` and every knowledge/memory/drafter tool keep working on
Windows, because Layer A does not need a kernel.

---

### Task 14A: Layer B — the read-deny sandbox policy, and what each platform can actually express

**Why this task exists.** Gates A–H, and CP1–CP5 over knowledge bases, all sit on *tool calls*. A
public-capability model does not have to defeat any of them: `developer__shell` runs an arbitrary
command (`rmcp_developer.rs:1307`), the code says in as many words that the shell is **not** jailed
by the file tools' base (`:1950`'s doc-comment, "the shell is not jailed by this base at all"), the
OS sandbox that could confine it defaults to **Off** (`shell_sandbox/mod.rs:244-260`), and
`computercontroller__automation_script` writes a model-supplied script to a temp file and executes it
(`computercontroller/mod.rs:833-…`). `sessions.db` is then a plain file with a **contentful** FTS5
mirror of every message by design (`session_manager.rs:14-28`, `MESSAGES_FTS_DDL`), so
`sqlite3 … "select text from messages_fts"` is the whole attack. SecretGuard does not cover it:
`DEFAULT_SECRET_PATTERNS` (`secret_guard.rs:33-45`) names credentials and nothing else, and its scan
is lexical and existence-gated (`candidate_is_denied` `:278-292`), so a computed path or a shell
expression walks past it. The design named this channel in §9.3 A2 and the plan never closed it.

**The operator has ruled** (DR-14): a public-capability session's process-spawning and path-reading
tools run under a **read-deny sandbox that is on by default**, hiding four directories — the session
store, the knowledge roots, the global memory root and the Agent Drafter app root — and nothing else.
Private-capability sessions are untouched; this is not a general jail and must not become one.

This task is the **mechanism half**, in `biorouter-sandbox` alone, with no privacy concepts in it:
teach `SandboxPolicy` to express a read-deny, make each backend say honestly whether it *can*, and
prove enforcement with a live kernel test. Task 14B wires it to capability. Splitting them this way
is deliberate: a reviewer of 14B should be reading one `if`, not a Seatbelt profile.

**Files:**

| Action | Path | Anchor (re-verified at `9558c346`) |
|---|---|---|
| Modify | `crates/biorouter-sandbox/src/shell_sandbox/mod.rs` | `SandboxPolicy` `:54-79` (two fields today); `ShellSandboxError` `:157-165`; `trait ShellSandbox` `:169-179`; `NullSandbox` impl `:186-199` |
| Modify | `crates/biorouter-sandbox/src/seatbelt.rs` | `BASE_POLICY` `:28-89` with `(allow file-read*)` at `:35`; `SeatbeltPolicy` `:94-99`; `profile()` `:117-139` (writable-roots block `:123-130`); `wrap()` `:145-163` |
| Modify | `crates/biorouter-sandbox/src/shell_sandbox/macos.rs` | `probe` `:19-32`, `wrap` `:34-45` |
| Modify | `crates/biorouter-sandbox/src/shell_sandbox/linux.rs` | `bwrap_on_path` `:99-101`; `run_selftest` `:124-136`; `effective_backend` `:140-178`; `wrap` `:210-226`; `wrap_bubblewrap` `:258-286`; `apply_landlock` `:403-445` — **read accesses are deliberately unhandled at `:413-418`** |
| Modify | `crates/biorouter-sandbox/src/shell_sandbox/windows.rs` | `probe` `:61-70`, `wrap` `:72-78` |
| Create | `crates/biorouter-sandbox/tests/read_deny.rs` | new — the live kernel proof |
| Reference | `crates/biorouter-sandbox/tests/sandbox.rs` | the existing integration binary; this is a **second** one, so the new tests get their own filter |

#### ⚠ What each platform can express, measured — not assumed

Every row below was **executed**, not reasoned: macOS on this host (macOS 26.5.2, build 25F84),
Linux in `docker run --privileged --security-opt seccomp=unconfined debian:bookworm-slim` with
`bubblewrap 0.8.0`. Where a claim in an earlier round of this plan or in a review did not survive
execution, the correction is marked ⚑ and the measured output is quoted.

| Platform | Mechanism | Read-deny of a subpath | Cost |
|---|---|---|---|
| **macOS** | Seatbelt (`sandbox-exec`) | **Yes, directly and measurably.** `(deny file-read* (subpath …))` appended to `BASE_POLICY` blocks the read (`cat: …: Operation not permitted`, exit 1) while an unrelated file still reads (exit 0). Verified in the **production shape** — `writable_roots: ["/"]`, i.e. an `(allow file-write* (subpath "/"))` that is an ancestor of every deny root: read, write and `rm` inside the deny root all fail, writes elsewhere all succeed. | Two string pushes and one `-D` argument per root. Nothing measurable. |
| **Linux + bubblewrap** | `bwrap` | **Yes, with `--tmpfs <root> --remount-ro <root>`.** After `--ro-bind / /`, `--tmpfs` overmounts the directory with an empty tmpfs in the child's own mount namespace. ⚑ `--tmpfs` **alone is writable** — measured, `rc=0` and the new file appears in the tmpfs — so the write half of the policy needs `--remount-ro` immediately after. | Needs `bwrap` installed **and** unprivileged user namespaces enabled — and ⚑ the former does not imply the latter (see the live probe below). One `execve` of a small setuid-free helper per command, which `wrap_bubblewrap` already pays today. |
| **Linux, Landlock only** | `landlock` + `seccompiler` | **No — not at all.** A Landlock ruleset is a set of **grants** over paths. There is no deny rule and no way to subtract a subpath from a broader grant. `apply_landlock` (`:413-418`) handles `AccessFs::from_write(abi)` *only*, precisely so reads stay unhandled and therefore open. | See below. |
| **Windows** | — | **No.** `WindowsSandbox::probe` already reports `tier: None`, and its module docs (`windows.rs:1-51`) work through why no unprivileged, general-purpose confinement exists there. AppContainer could express it and is the designed W2 tier, but it is unimplemented, breaks `git`/`node`/`python` without ACL work, and cannot be validated off a Windows runner. | — |

**Why the Landlock complement is not attempted in v1.** Expressing "deny `$HOME/.config/biorouter/knowledge`"
in a grant-only model means handling `ReadFile|ReadDir` and then granting read to the *complement*:
walk the ancestor chain of every deny root (`/`, `/home`, `$HOME`, `$HOME/.config`, `$HOME/.local`,
`$HOME/.local/share`) and add a `PathBeneath` rule for every sibling entry at every level. Three
measured consequences, and the third is disqualifying:

1. Every granted path needs an open fd during ruleset construction — a few hundred `open(2)`s per
   shell command on an ordinary `$HOME`.
2. The complement must be recomputed per command, because those directories change.
3. **Anything created inside an enumerated ancestor *after* the ruleset is built is unreadable for
   that command's lifetime.** `cd ~ && mkdir out && echo x > out/f && cat out/f` fails — the file
   exists, the command created it, and it cannot read it back. That is a policy whose failure mode
   is indistinguishable from a Biorouter bug, and it is exactly the class of thing that gets a
   security control switched off wholesale.

So on Linux the read-deny is expressible **only** through bubblewrap, and `LinuxSandbox` must invert
its usual preference (`effective_backend` picks Landlock first, `:162-177`) when a policy carries
deny roots. [Open question 17](#open-questions) keeps the complement approach on the record.

#### ⚑ Five claims that did not survive execution — four of them this plan's own

**⚑1. The SBPL last-match-wins rationale was backwards, and leaving it wrong is dangerous.**
An earlier draft of Step 3(b) said an early-emitted deny is *"a silent no-op with a passing test
suite."* Measured on macOS, a deny emitted **before** `(allow file-read*)` still denies:

```
$ /usr/bin/sandbox-exec -p '(version 1)(deny file-read* (subpath "…/secret"))(allow default)' \
    /bin/cat …/secret/page.md
cat: …/secret/page.md: Operation not permitted        exit=1
```

and in the production `unconfined()` shape, a deny emitted before the `(allow file-write* (subpath
"/"))` block still denies reads, writes and `rm`. **The described failure mode does not exist on
macOS.** What *does* defeat a deny on macOS is the opposite thing, measured:

```
deny X, then (allow file-read* (subpath X))        -> SECRET readable, exit 0   <-- RE-OPENED
deny X, then (allow file-read* (subpath PARENT-OF-X)) -> SECRET readable, exit 0 <-- RE-OPENED
deny X, then blanket (allow file-read*)            -> Operation not permitted    <-- still denied
```

So: an **unfiltered** later allow never overrides a path-filtered deny; a **path-filtered** later
allow whose `subpath` covers the deny root does. The real macOS hazard is therefore *"someone later
adds a read-side twin of the writable-roots loop"*, not *"someone emits the deny too early"*.

This correction is not cosmetic. Getting it wrong is what deletes the ordering pin: an implementer
who trusts the old comment, moves the block earlier on their Mac, sees nothing change, and concludes
the assertion is cargo-cult, will remove it — **and the Linux ordering, which IS load-bearing and
DOES fail open silently (⚑4), goes with it.** Keep the assertion; fix the reason.

**⚑2. `supports_read_deny()` must be a two-legged live probe, and round 2's "the sandbox actually
fails on this host" was a probe artifact.** Round 2 reported that
`/usr/bin/sandbox-exec -p '(version 1)(allow default)' /bin/true` exits 71 with `sandbox_apply:
Operation not permitted`. Re-run here, the exit code reproduces and the cause does not:

```
$ /usr/bin/sandbox-exec -p '(version 1)(allow default)' /bin/true; echo $?
sandbox-exec: execvp() of '/bin/true' failed: No such file or directory
71
$ ls /bin/true; ls /usr/bin/true
ls: /bin/true: No such file or directory
-rwxr-xr-x  1 root  wheel  84128 /usr/bin/true
$ /usr/bin/sandbox-exec -p '(version 1)(allow default)' /usr/bin/true; echo $?
0
```

**`/bin/true` does not exist on macOS.** Nested sandboxing also exits 0, so "already inside a
sandbox" is not the explanation either. Seatbelt is fully functional on this host and
`seatbelt::available()` is telling the truth *here* — **do not weaken this task to accommodate a
broken-Seatbelt Mac; that host was not observed.**

Round 2's *structural* criticism stands, though, and is what Step 3(c) below implements:
`available()` (`seatbelt.rs:168` — **not `:166`**, citation drift) is
`cfg!(target_os = "macos") && Path::new(SANDBOX_EXEC).exists()`, a file-existence check. The trait's
own doc forbids exactly that: `ShellSandbox::probe`'s comment at `shell_sandbox/mod.rs:167-170`
requires *"a real capability probe … never a version guess"*, and Linux honours it (`run_selftest()`
`linux.rs:120-134`, cached at `:141`) while macOS does not.

And exit 71 is **not** a usable signal on its own — measured, it covers at least three conditions:

| condition | stderr | exit |
|---|---|---|
| target binary missing | `execvp() … No such file or directory` | 71 |
| profile denies `process-exec` of the target | `execvp() … Operation not permitted` | 71 |
| SBPL syntax error | `sandbox-exec: syntax error: expecting ')'` | **65** |

**A one-legged probe is not enough either.** A probe that only asserts "the deny bites" returns
`supported` on a host where the sandbox cannot start *any* process — every read fails, so the deny
"looks" enforced. The probe below has two legs and was measured against all three broken-host
classes; the shape and the measurements are in Step 3(c).

**⚑3. `bwrap` on `PATH` does not mean bubblewrap works** — and neither review noticed. Under
**default** Docker seccomp, `bwrap` is installed, executable, and every invocation fails:

```
bwrap: No permissions to create new namespace, likely because the kernel does not allow
       non-privileged user namespaces.
```

`bwrap_on_path()` (`linux.rs:99-101`) would report available, and `probe()` (`:196-200`) would claim
`tier: Full, mechanism: "bubblewrap"` with nothing executed. Step 3(d)'s
`bubblewrap_can_deny_reads()` is a live probe for this reason, and it is the Linux twin of ⚑2.

**⚑4. On Linux the argv ordering IS load-bearing and DOES fail open silently.** This is the real
instance of the hazard ⚑1 wrongly attributed to macOS, and it is unreported by either review:

```
--tmpfs BEFORE the --bind of its parent  ->  NESTED-SECRET   rc=0   <-- DENY SILENTLY DEFEATED
--tmpfs AFTER  the --bind of its parent  ->  No such file or directory  rc=1   <-- correct
```

Three of the four DR-14 roots live under `$HOME`, which is routinely the session working directory
and therefore a writable `--bind` root, so this is the common case, not a corner. The same holds for
the `--ro-bind` of `config.yaml` that Task 14D adds: placed before the parent's writable bind, the
clobber succeeds (`CLOBBER2` written to the host file, `rc=0`).

**⚑5. A pre-planted hardlink defeats the deny on both platforms, and only Layer A closes it.**
Seatbelt matches **paths**, not inodes; `--tmpfs` hides a **path**, not an inode. Measured:

```
macOS  J1  sandboxed child tries to CREATE the hardlink   -> ln: Operation not permitted, rc=1
macOS  K1  sandboxed child reads a PRE-EXISTING hardlink  -> SECRET-KB-CONTENT, exit=0
Linux  10a same, under a --tmpfs deny                     -> SECRET-KB-CONTENT, rc=0
```

The sandboxed child cannot forge the link (creating it needs read access to the source, which is
denied). It is only reachable through an **unsandboxed in-process writer** — a public model asking
the daemon to `ln <deny-root>/page.md ./x` through a file tool, then reading `./x` from the shell.
That is a second, independent reason Layer A is the primary: with Layer A refusing the `ln`
argument, the hardlink is never planted. Recorded as [AR-9](#ar-9--layer-a-is-check-then-use-so-a-concurrently-running-shell-can-still-race-one-in-process-reader).

- [ ] **Step 1: Write the failing tests**

`crates/biorouter-sandbox/tests/read_deny.rs` — **live**, because no string assertion can
distinguish a backend that honours `deny_read_roots` from one that accepts the field and drops it on
the floor. That is the single most likely wrong implementation here: the code compiles, every
existing argv test still passes, `probe()` still says `Full`, and nothing is denied.

```rust
//! Kernel-level proof of the DR-14 read-deny. Two tests, and their `if`
//! conditions PARTITION the hosts: on a host that can express a read-deny the
//! first runs, on a host that cannot the second runs. Neither can be skipped
//! into vacuity, which is the failure mode a bare `if !supported { return; }`
//! would have.

use biorouter_sandbox::shell_sandbox::{
    detect, SandboxPolicy, ShellSandboxError, Wrapped,
};
use std::path::PathBuf;

fn run(w: &Wrapped, script: &str) -> std::process::Output {
    let mut args = w.prefix_args.clone();
    args.push("-c".to_string());
    args.push(script.to_string());
    std::process::Command::new(&w.program)
        .args(&args)
        .output()
        .expect("spawn the wrapper")
}

#[test]
fn a_deny_root_is_unreadable_and_the_rest_of_the_disk_is_not() {
    let backend = detect();
    if !backend.supports_read_deny() {
        eprintln!("host cannot express a read-deny; covered by the sibling test");
        return;
    }
    let secret_root = tempfile::tempdir().unwrap();
    std::fs::write(secret_root.path().join("transcript.txt"), "COHORT-SENTINEL").unwrap();
    let ordinary_root = tempfile::tempdir().unwrap();
    std::fs::write(ordinary_root.path().join("notes.txt"), "ORDINARY-SENTINEL").unwrap();

    let policy = SandboxPolicy::unconfined()
        .with_deny_read_roots(vec![secret_root.path().to_path_buf()]);
    let w = backend.wrap(&policy, "/bin/sh").expect("wrap");

    let denied = run(&w, &format!("cat {}/transcript.txt", secret_root.path().display()));
    assert!(
        !String::from_utf8_lossy(&denied.stdout).contains("COHORT-SENTINEL"),
        "the deny root was readable: {:?}",
        String::from_utf8_lossy(&denied.stdout)
    );
    assert!(!denied.status.success(), "reading a deny root must fail");

    let ok = run(&w, &format!("cat {}/notes.txt", ordinary_root.path().display()));
    assert!(
        String::from_utf8_lossy(&ok.stdout).contains("ORDINARY-SENTINEL"),
        "ordinary work must be untouched: {:?}",
        String::from_utf8_lossy(&ok.stderr)
    );
}

/// The ordering half, and the one a correct-looking implementation fails.
/// A deny root can sit INSIDE a writable root — the session working directory
/// is `$HOME` under the desktop app, and three of the four DR-14 roots are
/// under `$HOME`. On Linux the `--tmpfs`/`--remount-ro` options MUST come after
/// the `--bind`s: emitted before the writable bind of the parent, the deny is
/// silently defeated (measured: `NESTED-SECRET, rc=0`). On macOS SBPL order
/// does not decide this (see ⚑1) but the assertion is kept, because the two
/// backends share `deny_read_roots` and a reviewer who "simplifies" the shared
/// path breaks Linux with nothing failing on their Mac.
#[test]
fn a_deny_root_inside_a_writable_root_is_still_denied() {
    let backend = detect();
    if !backend.supports_read_deny() {
        return;
    }
    let home = tempfile::tempdir().unwrap();
    let inner = home.path().join("config").join("biorouter").join("knowledge");
    std::fs::create_dir_all(&inner).unwrap();
    std::fs::write(inner.join("page.md"), "COHORT-SENTINEL").unwrap();

    let policy = SandboxPolicy::new(vec![home.path().to_path_buf()])
        .with_deny_read_roots(vec![inner.clone()]);
    let w = backend.wrap(&policy, "/bin/sh").expect("wrap");

    let out = run(&w, &format!("cat {}/page.md", inner.display()));
    assert!(!String::from_utf8_lossy(&out.stdout).contains("COHORT-SENTINEL"));

    // …and the write side with it: a public model deleting the user's only
    // knowledge base is not a smaller problem than reading it.
    //
    // ⚠ This assertion USED to be `rm -f <page.md>` + `assert!(!status.success())`,
    // and it could not pass on Linux under any correct implementation. Measured:
    //
    //     $ rm -f /nonexistent/nope ; echo $?
    //     0
    //
    // POSIX `-f` suppresses the missing-operand diagnostic AND the exit status,
    // and under `--tmpfs` the file is *absent*, not *protected* — so `rm -f`
    // returns 0 on a correctly denied root. It also returns 0 under
    // `--tmpfs --remount-ro`, because `rm -f` never touches the filesystem. On
    // macOS the same line passes (`rm: Operation not permitted`), which is why
    // it survived review: it was green on the author's machine and unlandable on
    // the platform it was written for.
    //
    // Assert the OUTCOME instead of an errno, because the two kernels disagree
    // about the observable and always will: macOS returns EPERM and `ls` fails;
    // Linux returns ENOENT and `ls` SUCCEEDS with an empty listing (the tmpfs is
    // really there and really empty). The one thing both must guarantee is that
    // the host file is intact afterwards and that a *create* inside the root is
    // refused.
    let create = run(&w, &format!("echo x > {}/new.md", inner.display()));
    assert!(!create.status.success(), "a deny root must not be writable either");
    assert!(!inner.join("new.md").exists(), "a write leaked to the host");

    let delete = run(&w, &format!("rm {}/page.md", inner.display())); // NOT -f
    assert!(!delete.status.success(), "a deny root's contents must not be deletable");
    assert!(inner.join("page.md").exists(), "the host file was deleted");
    assert_eq!(std::fs::read_to_string(inner.join("page.md")).unwrap(), "COHORT-SENTINEL");
}

#[test]
fn a_backend_that_cannot_express_it_refuses_instead_of_dropping_it() {
    let backend = detect();
    if backend.supports_read_deny() {
        eprintln!("host CAN express a read-deny; covered by the sibling tests");
        return;
    }
    let policy = SandboxPolicy::unconfined()
        .with_deny_read_roots(vec![PathBuf::from("/nonexistent-private-root")]);
    assert!(
        matches!(
            backend.wrap(&policy, "/bin/sh"),
            Err(ShellSandboxError::PolicyUnsupported(_))
        ),
        "a backend that cannot subtract reads must say PolicyUnsupported, not \
         Unavailable and not Ok — the caller's fail-closed branch keys on it"
    );
}
```

and, in `seatbelt.rs`'s own `mod tests`, the profile-shape half:

```rust
#[test]
fn deny_roots_are_emitted_after_the_read_allow_and_after_the_write_block() {
    let p = SeatbeltPolicy::new(vec![PathBuf::from("/work")])
        .with_deny_read_roots(vec![PathBuf::from("/work/private")])
        .profile();
    let allow_read = p.find("(allow file-read*)").expect("base policy");
    let allow_write = p.find("(allow file-write*").expect("writable block");
    let deny_read = p.find("(deny file-read*").expect("deny block");
    let deny_write = p.find("(deny file-write*").expect("deny block");
    // ⚑1: this does NOT pin "otherwise it is a no-op" — measured, an
    // early-emitted deny still denies on macOS. It pins the ONE ordering rule
    // macOS really has: no path-filtered `allow` may follow a deny of a path it
    // covers. Emitting the denies last is the cheapest way to guarantee that
    // for every future block, and this assertion is what notices when a later
    // task adds a read-side twin of the writable-roots loop.
    assert!(deny_read > allow_read && deny_read > allow_write);
    assert!(deny_write > allow_read && deny_write > allow_write);
    // The rule itself, stated as an assertion rather than as a convention: no
    // `(allow file-...` block may appear after the denies.
    assert!(
        p[deny_read..].find("(allow file-").is_none(),
        "a path-filtered allow after a deny re-opens the deny root — measured, \
         `deny X` then `(allow file-read* (subpath X))` yields SECRET readable, exit 0"
    );
}

#[test]
fn a_deny_root_is_named_in_both_its_literal_and_canonical_spelling() {
    // `/tmp` canonicalizes to `/private/tmp` on macOS, and Seatbelt matches
    // `subpath` against the canonical path. A profile that names only one
    // spelling is a deny with a documented way around it.
    let policy = SeatbeltPolicy::new(vec![]).with_deny_read_roots(vec![PathBuf::from("/tmp")]);
    let (_prog, args) = policy.wrap("/bin/sh");
    let params: Vec<&String> = args.iter().filter(|a| a.starts_with("-DDENY_ROOT_")).collect();
    assert!(params.iter().any(|a| a.ends_with("=/tmp")), "{params:?}");
    if PathBuf::from("/tmp").canonicalize().map(|c| c != PathBuf::from("/tmp")).unwrap_or(false) {
        assert_eq!(params.len(), 2, "both spellings, deduped: {params:?}");
    }
    // profile() and wrap() must agree on the count, or a `(param "DENY_ROOT_1")`
    // references a parameter that was never passed and sandbox-exec errors out.
    assert_eq!(policy.profile().matches("DENY_ROOT_").count(), params.len());
}
```

- [ ] **Step 2: Run** → **COMPILE ERROR**
(`no method with_deny_read_roots on SandboxPolicy`, `no method supports_read_deny`).

```bash
cargo test -p biorouter-sandbox 2>&1 | tail -20
```

- [ ] **Step 3: Implement**

**(a) `shell_sandbox/mod.rs` — the policy gains one field and the trait one question.**

```rust
pub struct SandboxPolicy {
    /// Directories the sandboxed process may write to (subpaths included).
    /// Everything else on the filesystem is readable but not writable. Zero
    /// roots means "no writes anywhere" — never "all writes".
    pub writable_roots: Vec<PathBuf>,
    /// Directories the sandboxed process may neither **read** nor **write**
    /// (subpaths included), while the rest of the filesystem stays as
    /// `writable_roots` leaves it. Empty is the historical behaviour and the
    /// default.
    ///
    /// This is a *subtraction* from an otherwise-open read policy, which is why
    /// it is not expressible on every backend — see
    /// [`ShellSandbox::supports_read_deny`]. Writes are subtracted with reads
    /// because a deny root can sit inside a writable root (issue #56: the
    /// desktop app's working directory is often `$HOME`, and three of the four
    /// DR-14 roots are under it), and a read-only-by-omission rule would leave
    /// the user's knowledge base deletable by the model it was hidden from.
    pub deny_read_roots: Vec<PathBuf>,
    /// When false (the default), outbound network is denied.
    pub allow_network: bool,
}

impl SandboxPolicy {
    /// A policy that confines nothing: writes allowed everywhere, network
    /// allowed. Only useful with `deny_read_roots` — it is the shape the
    /// privacy read-deny (DR-14) needs, because that sandbox exists to subtract
    /// four directories and must change nothing else about how the shell
    /// behaves. `writable_roots = ["/"]` rather than a new boolean, so every
    /// backend's existing writable-root path carries it with no new branch.
    pub fn unconfined() -> Self {
        Self {
            writable_roots: vec![PathBuf::from(std::path::MAIN_SEPARATOR_STR)],
            deny_read_roots: Vec::new(),
            allow_network: true,
        }
    }

    pub fn with_deny_read_roots(mut self, roots: Vec<PathBuf>) -> Self {
        self.deny_read_roots = roots;
        self
    }
}
```

⚠ `SandboxPolicy::new` and `Default` keep their present meaning — `new` still yields
`deny_read_roots: []`, and `Default`'s `writable_roots: []` still means "no writes anywhere". The two
existing constructors are what BR-69 callers use and neither changes behaviour. `unconfined()` is new
and is used by exactly one caller (Task 14B).

```rust
pub enum ShellSandboxError {
    #[error("shell sandbox unavailable: {0}")]
    Unavailable(String),
    /// The backend can sandbox on this host but cannot express *this* policy —
    /// today, only a non-empty `deny_read_roots`. Deliberately distinct from
    /// `Unavailable`: the caller's fail-closed branch must be able to say
    /// "a sandbox exists, just not the one that was required", and must never
    /// treat it as "run unsandboxed".
    #[error("shell sandbox cannot express this policy: {0}")]
    PolicyUnsupported(String),
    #[error("{0}")]
    Other(String),
}

pub trait ShellSandbox: Send + Sync {
    fn probe(&self) -> SandboxReport;

    /// Whether this backend can honour a non-empty
    /// [`SandboxPolicy::deny_read_roots`] **on this host**.
    ///
    /// No default implementation, deliberately. A `{ false }` default lets a new
    /// backend inherit "cannot" silently and a whole platform quietly stops
    /// getting the control; a `{ true }` default lets one inherit a lie. Making
    /// it a required method puts the answer in the backend's own file, where the
    /// reviewer of that file can see it, and makes a new backend that forgets it
    /// a compile error.
    fn supports_read_deny(&self) -> bool;

    fn wrap(&self, policy: &SandboxPolicy, program: &str) -> Result<Wrapped, ShellSandboxError>;
}

impl ShellSandbox for NullSandbox {
    fn supports_read_deny(&self) -> bool {
        false
    }
    // probe() and wrap() unchanged.
}
```

**(b) `seatbelt.rs` — one field, one profile block, one param loop.**

```rust
pub struct SeatbeltPolicy {
    pub writable_roots: Vec<PathBuf>,
    pub deny_read_roots: Vec<PathBuf>,
    pub allow_network: bool,
}

impl SeatbeltPolicy {
    pub fn with_deny_read_roots(mut self, roots: Vec<PathBuf>) -> Self {
        self.deny_read_roots = roots;
        self
    }

    /// Every spelling Seatbelt has to be told about, deduped and in a stable
    /// order. `subpath` matches the **canonical** path, so `/tmp` and
    /// `/private/tmp` are two different rules on macOS; both are emitted
    /// because naming one is a deny with a documented way around it, and the
    /// cost of an extra `-D` is nothing.
    ///
    /// `profile()` and `wrap()` MUST both iterate this, or the profile
    /// references a `(param "DENY_ROOT_n")` that was never passed and
    /// `sandbox-exec` fails to start the command.
    fn deny_root_params(&self) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = Vec::new();
        for root in &self.deny_read_roots {
            let canonical = root.canonicalize().unwrap_or_else(|_| root.clone());
            for candidate in [root.clone(), canonical] {
                if !out.contains(&candidate) {
                    out.push(candidate);
                }
            }
        }
        out
    }
}
```

In `profile()`, **after** the writable-roots block and the network block — i.e. immediately before
the final `p.push('\n')`:

```rust
        // Issue #56 DR-14. Emitted LAST, after BASE_POLICY's `(allow file-read*)`
        // (:35) and after the writable-roots `(allow file-write*)` above.
        //
        // ⚠ The reason is not the one an earlier draft of this plan gave. That
        // draft said an early-emitted deny is "a silent no-op"; measured on
        // macOS 26, a deny emitted BEFORE either allow still denies reads,
        // writes and `rm`, including in the production `writable_roots: ["/"]`
        // shape. The real rule is narrower and is about what may come AFTER:
        //
        //   deny X, then (allow file-read*)                  -> still denied
        //   deny X, then (allow file-read* (subpath X))       -> RE-OPENED
        //   deny X, then (allow file-read* (subpath parent))  -> RE-OPENED
        //
        // i.e. an unfiltered later allow is harmless; a path-filtered later
        // allow covering the deny root evaporates it. Emitting the denies last
        // makes that impossible for every block that exists today and every one
        // added later, which is why the ordering assertion stays.
        //
        // Two operations rather than one: `file-read*` and `file-write*` are
        // the two SBPL operations BASE_POLICY and the writable block actually
        // grant, so denying exactly those two is a complete subtraction of what
        // was granted. (There is no need to reach for a broader `file*`.)
        let deny_roots = self.deny_root_params();
        if !deny_roots.is_empty() {
            p.push_str("\n\n; Private data roots (issue #56 DR-14): no read, no write.\n");
            for op in ["file-read*", "file-write*"] {
                p.push_str(&format!("(deny {op}"));
                for i in 0..deny_roots.len() {
                    p.push_str(&format!("\n  (subpath (param \"DENY_ROOT_{i}\"))"));
                }
                p.push_str(")\n");
            }
        }
```

and in `wrap()`, beside the `-DWRITABLE_ROOT_n=` loop:

```rust
        for (i, root) in self.deny_root_params().iter().enumerate() {
            args.push(format!("-DDENY_ROOT_{i}={}", root.display()));
        }
```

**(c) `seatbelt.rs` — the live two-legged probe, cached.**

```rust
/// Whether a read-deny is really enforced on this host. A **live self-test**,
/// because `available()` is a file-existence check and cannot tell a working
/// Seatbelt from a broken one (⚑2).
///
/// TWO legs, and both are required:
///
///   * NEGATIVE — reading inside the deny root must fail AND must not emit the
///     sentinel;
///   * POSITIVE — reading a control file OUTSIDE it must succeed AND emit its
///     own sentinel.
///
/// The positive leg is the one that is easy to leave out and impossible to do
/// without: a host where `sandbox-exec` cannot start ANY process fails the
/// negative leg for the wrong reason and would report `supported`. Measured
/// against the three broken-host classes:
///
///   (i)   sandbox cannot exec anything     -> negative "passes", positive rc=1  -> REJECTED
///   (ii)  deny inert (relative `subpath`)  -> negative rc=0 with the secret     -> REJECTED
///   (iii) deny declared uncanonicalized    -> negative rc=0 with the secret     -> REJECTED
///
/// Exit codes are deliberately NOT interpreted: measured, 71 covers "target
/// binary missing", "process-exec denied" and more, while a syntax error is 65.
/// The probe asserts on the BYTES, and uses the exit status only as a
/// secondary signal.
///
/// Cost: measured 29.9 ms for the pair (10 runs, 0.299 s total). That is why it
/// is behind a `OnceLock` — `seatbelt.rs`/`macos.rs` contain no cache today
/// (grep for `OnceLock|OnceCell|static` returns only the `BASE_POLICY` const at
/// `:23`), and a 30 ms live probe per `developer__shell` call is a real
/// regression. Linux's precedent is `effective_backend`'s
/// `static CACHE: OnceLock<Backend>` at `linux.rs:141`.
pub fn read_deny_selftest() -> bool {
    static CACHE: OnceLock<bool> = OnceLock::new();
    *CACHE.get_or_init(run_read_deny_selftest)
}

fn run_read_deny_selftest() -> bool {
    if !available() {
        return false;
    }
    let Ok(dir) = tempfile::tempdir() else { return false };
    // CANONICALIZE. `/var/folders/...` is a symlink to `/private/var/folders/...`
    // on macOS, and an uncanonicalized `subpath` denies nothing — which is
    // broken-host class (iii), i.e. the probe would be testing its own bug.
    let Ok(base) = dir.path().canonicalize() else { return false };
    let deny = base.join("deny");
    let ctrl = base.join("ctrl");
    if std::fs::create_dir_all(&deny).is_err() || std::fs::create_dir_all(&ctrl).is_err() {
        return false;
    }
    if std::fs::write(deny.join("s"), "DENYSENTINEL").is_err()
        || std::fs::write(ctrl.join("c"), "CTRLSENTINEL").is_err()
    {
        return false;
    }
    let profile = format!(
        "(version 1)\n(allow default)\n\
         (deny file-read*  (subpath (param \"DENY_ROOT_0\")))\n\
         (deny file-write* (subpath (param \"DENY_ROOT_0\")))\n"
    );
    let leg = |target: &std::path::Path| -> std::process::Output {
        std::process::Command::new(SANDBOX_EXEC)
            .arg("-p").arg(&profile)
            .arg(format!("-DDENY_ROOT_0={}", deny.display()))
            .arg("--").arg("/bin/cat").arg(target)
            .output()
            .unwrap_or_else(|_| std::process::Output {
                status: Default::default(),
                stdout: Vec::new(),
                stderr: Vec::new(),
            })
    };
    let negative = leg(&deny.join("s"));
    let positive = leg(&ctrl.join("c"));
    !String::from_utf8_lossy(&negative.stdout).contains("DENYSENTINEL")
        && !negative.status.success()
        && positive.status.success()
        && String::from_utf8_lossy(&positive.stdout).contains("CTRLSENTINEL")
}
```

⚠ `/bin/cat` is used deliberately and `/bin/true` is not: **`/bin/true` does not exist on macOS**
(only `/usr/bin/true`), and a probe that execs it fails with `execvp() … No such file or directory`
and exit 71 — which is exactly how round 2 concluded this host had a broken Seatbelt (⚑2). `/bin/cat`
exists on every macOS this app supports and is what makes the sentinel-byte assertion possible.

**(c′) `macos.rs` — thread the field, answer the question.**

```rust
    fn supports_read_deny(&self) -> bool {
        // ⚠ NOT `seatbelt::available()`. That is `cfg!(macos) && Path::new(
        // "/usr/bin/sandbox-exec").exists()` (`seatbelt.rs:168`) — a file
        // existence check, which `ShellSandbox::probe`'s own doc
        // (`shell_sandbox/mod.rs:167-170`) forbids: "MUST be a real capability
        // probe … never a version guess". Linux already honours that
        // (`run_selftest()` `linux.rs:120-134`); macOS is the backend that does
        // not, and this is where it starts to.
        seatbelt::read_deny_selftest()
    }

    fn wrap(&self, policy: &SandboxPolicy, program: &str) -> Result<Wrapped, ShellSandboxError> {
        let p = SeatbeltPolicy::new(policy.writable_roots.clone())
            .with_deny_read_roots(policy.deny_read_roots.clone())
            .with_network(policy.allow_network);
        let (program, prefix_args) = p.wrap(program);
        Ok(Wrapped { program, prefix_args })
    }
```

⚠ The existing `golden_argv_matches_seatbelt_policy` test (`macos.rs:56-73`) still passes
byte-for-byte, because a policy with no deny roots emits no deny block and no `-DDENY_ROOT_`. **Do
not weaken that test to accommodate this change** — it is the "no macOS behaviour change" guarantee
for every BR-69 caller, and this task must not consume it.

**(d) `linux.rs` — a live bubblewrap probe, and a forced backend choice.**

```rust
/// Whether bubblewrap can actually run here. `bwrap` on `PATH` is not enough:
/// `--unshare-user` needs unprivileged user namespaces, which several hardened
/// distros disable, and a PATH check would report `true` on a host where every
/// wrapped command dies. Same discipline the module docs demand of `probe()` —
/// run the real thing once, cache the answer.
fn bubblewrap_can_deny_reads() -> bool {
    static CACHE: OnceLock<bool> = OnceLock::new();
    *CACHE.get_or_init(|| {
        if !bwrap_on_path() {
            return false;
        }
        let mut command = std::process::Command::new("bwrap");
        command
            .args([
                "--unshare-user", "--die-with-parent",
                "--ro-bind", "/", "/", "--proc", "/proc", "--dev", "/dev",
                // The probe exercises the exact primitive the policy needs.
                "--tmpfs", "/tmp",
                "--", "/bin/true",
            ])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        crate::environment::strip_daemon_private_env_std(&mut command);
        command.status().map(|s| s.success()).unwrap_or(false)
    })
}
```

```rust
    fn supports_read_deny(&self) -> bool {
        // Landlock cannot express this AT ALL, and that is a property of the
        // mechanism, not of this host: a ruleset is a set of GRANTS with no
        // deny rule and no way to subtract a subpath from a broader grant.
        // `apply_landlock` (:403) leaves read accesses unhandled on purpose so
        // reads stay open; handling them would mean granting read to the
        // complement of the deny roots — see this task's ⚠ for the three
        // measured costs and why v1 declines. Bubblewrap can: `--tmpfs <root>`
        // after `--ro-bind / /` is a real subtraction.
        bubblewrap_can_deny_reads()
    }

    fn wrap(&self, policy: &SandboxPolicy, program: &str) -> Result<Wrapped, ShellSandboxError> {
        // A read-deny FORCES the backend choice, overriding `effective_backend`'s
        // Landlock-first preference (:162-177). Falling through to the helper
        // here is the silent-no-op failure: the argv looks right, the tier still
        // reads Full, and the deny roots are readable.
        if !policy.deny_read_roots.is_empty() {
            if !bubblewrap_can_deny_reads() {
                return Err(ShellSandboxError::PolicyUnsupported(
                    "Landlock grants access, it cannot subtract it, so it cannot hide a \
                     directory from a command; and bubblewrap is unavailable here (not \
                     installed, or unprivileged user namespaces are disabled)"
                        .to_string(),
                ));
            }
            return Ok(wrap_bubblewrap(policy, program));
        }
        match effective_backend() { /* unchanged */ }
    }
```

In `wrap_bubblewrap`, **after** the writable-root `--bind` loop and before `--unshare-net`:

```rust
    // AFTER the writable `--bind`s. bubblewrap applies filesystem operations in
    // argv order and the later one wins for an overlapping path, so a deny root
    // inside a writable root is only subtracted if its `--tmpfs` comes last.
    // ⚑4, MEASURED — this is not a style point:
    //     --tmpfs BEFORE the --bind of its parent -> NESTED-SECRET, rc=0
    //     --tmpfs AFTER  the --bind of its parent -> ENOENT, rc=1
    // Three of the four DR-14 roots are under $HOME, which is routinely the
    // session working dir and therefore a writable --bind root.
    //
    // `--remount-ro` immediately after each `--tmpfs`, because ⚑ a bare
    // `--tmpfs` is WRITABLE — measured, `echo x > <root>/newfile.md` returns 0
    // and the file appears in the tmpfs. The write is harmless (it lands in the
    // child's own tmpfs and the host file is untouched) but the POLICY says
    // these roots are neither readable nor writable, and a doc claim the code
    // does not keep is how the next reviewer loses an afternoon. With
    // `--remount-ro`: `cannot create …: Read-only file system`, rc=2.
    //
    // `if root.is_dir()` is NECESSITY, not tolerance. Measured: `--tmpfs` on a
    // destination that does not exist ABORTS bwrap outright —
    // `bwrap: Can't mkdir …: Read-only file system`, exit 1 — even when the
    // parent exists. Skipping absent roots is the only way the wrapper runs at
    // all. ⚠ It is NOT true that "a root that does not exist holds nothing to
    // read": see AR-10, where a background process creates the root two seconds
    // after the wrapper is built and the sandboxed job reads it at t=4 s.
    for root in &policy.deny_read_roots {
        if root.is_dir() {
            args.push("--tmpfs".to_string());
            args.push(root.display().to_string());
            args.push("--remount-ro".to_string());
            args.push(root.display().to_string());
        }
    }
```

⚠ **Do not hoist the `is_dir()` skip into a shared helper.** macOS is strictly better here and a
"simplification" would throw that away, measured: on macOS a deny root that does not exist is
harmless to declare (the profile starts fine, exit 0) **and the deny still applies once another
process creates it** (`Operation not permitted`). SBPL is a path-pattern match; bwrap needs a real
mountpoint. The skip belongs in `wrap_bubblewrap` and nowhere else, or macOS silently inherits
Linux's race (AR-10) for free.

⚠ `wrap_bubblewrap` today also passes `--unshare-pid`. Leave it exactly as it is: this task changes
which backend is chosen and adds `--tmpfs` lines, and nothing else. Widening or narrowing the
namespace set here would change BR-69 behaviour under a privacy commit.

**(e) `windows.rs`.**

```rust
    fn supports_read_deny(&self) -> bool {
        // See this module's header table. AppContainer is the designed W2 tier
        // and could express this by omission; it is unimplemented and cannot be
        // validated off a Windows runner, and an untested `unsafe` FFI sandbox
        // claiming to hide clinical data is the security theatre those docs
        // warn against.
        false
    }

    fn wrap(&self, policy: &SandboxPolicy, _program: &str) -> Result<Wrapped, ShellSandboxError> {
        if !policy.deny_read_roots.is_empty() {
            return Err(ShellSandboxError::PolicyUnsupported(
                "Windows has no unprivileged sandbox that can hide a directory from an \
                 arbitrary command"
                    .to_string(),
            ));
        }
        Err(ShellSandboxError::Unavailable(
            "no Windows shell sandbox tier is implemented (probe reports None)".to_string(),
        ))
    }
```

- [ ] **Step 4: Run**

```bash
cargo test -p biorouter-sandbox --lib 2>&1 | tail -5
# The pre-count is measurable and non-zero, so assert the delta, not "no failures".
# Measured at 9558c346: `cargo test -p biorouter-sandbox --lib -- --list` is the
# number to record here before Step 3, and this task adds 2 (both in seatbelt.rs).
cargo test -p biorouter-sandbox --test read_deny 2>&1 | tail -5
# Expect: `3 passed` on macOS and on a Linux host with bubblewrap; on a host
# without, still `3 passed` — one asserts enforcement, two assert the refusal.
# `0 passed` means the new test binary was not registered: `tests/read_deny.rs`
# must be a sibling of `tests/sandbox.rs`, not a module inside it.
cargo check --workspace --all-targets
```

⚠ O13: this task changes a `pub` struct and a `pub` trait, so `cargo check --workspace --all-targets`
runs in Step 4 **and** again before the commit. `SandboxPolicy` is constructed in
`biorouter-mcp/src/developer/shell.rs:131-138` and the trait is implemented four times; a struct
literal anywhere outside those would be a compile error this catches.

- [ ] **Step 5: Gate**

```bash
# (1) The trait question has NO default body. This is the difference between
#     "a future backend must answer" and "a future backend inherits an answer".
awk '/pub trait ShellSandbox/,/^}/' crates/biorouter-sandbox/src/shell_sandbox/mod.rs \
  | grep -c "fn supports_read_deny(&self) -> bool;"
echo "expect: 1 — a semicolon, not a block"

# (2) Every implementor answers for itself. FOUR impls today, measured:
#     NullSandbox mod.rs:186, SeatbeltSandbox macos.rs:18, WindowsSandbox
#     windows.rs:60, LinuxSandbox linux.rs:180.
grep -rn "impl ShellSandbox for" crates/biorouter-sandbox/src/ | wc -l ; echo "expect: 4"
grep -rn "fn supports_read_deny" crates/biorouter-sandbox/src/ | wc -l
echo "expect: 5 — one declaration + four implementations"

# (3) Landlock is UNTOUCHED. If someone 'implements' the read-deny by teaching
#     apply_landlock to handle reads, this fires — and that implementation is
#     the one whose failure mode is 'the file I just wrote is unreadable'.
grep -c "AccessFs::from_read\|AccessFs::ReadFile\|AccessFs::ReadDir" \
  crates/biorouter-sandbox/src/shell_sandbox/linux.rs
echo "expect: 0 — Landlock still handles write accesses only (:413-418)"

# (4) SBPL ordering, as a command rather than a claim. The whole macOS half
#     turns on it and it cannot be seen by reading the emitted profile in a
#     passing test, because a mis-ordered profile is still a valid profile.
python3 - <<'PY'
src = open('crates/biorouter-sandbox/src/seatbelt.rs').read().splitlines()
def first(pat):
    for i, l in enumerate(src, 1):
        if pat in l:
            return i
    return None
allow_read  = first('(allow file-read*)')       # BASE_POLICY :35
allow_write = first('(allow file-write*')       # profile()'s writable block
deny_read   = first('(deny file-read*')         # the new block
assert allow_read and allow_write and deny_read, \
    f'marker missing: {allow_read=} {allow_write=} {deny_read=}'
assert deny_read > allow_read,  'deny emitted before (allow file-read*) — no-op'
assert deny_read > allow_write, 'deny emitted before the writable block — no-op'
print(f'OK  allow-read:{allow_read}  writable:{allow_write}  deny:{deny_read}')
PY

# (5) profile() and wrap() iterate the SAME parameter list, or sandbox-exec
#     fails on an undefined `(param "DENY_ROOT_n")` at runtime — on the user's
#     machine, not in CI.
grep -c "deny_root_params()" crates/biorouter-sandbox/src/seatbelt.rs
echo "expect: 3 — the definition, profile()'s call, wrap()'s call"

# (6) The bubblewrap capability is a live probe, not a PATH check.
awk '/fn bubblewrap_can_deny_reads/,/^}/' crates/biorouter-sandbox/src/shell_sandbox/linux.rs \
  | grep -c '"--tmpfs"'
echo "expect: 1 — the probe runs the primitive it is vouching for"
```

**What wrong implementation each of these rejects.**
(1)+(2) A fifth backend added later that inherits a permissive default and silently stops denying on
a whole platform. (3) A "Linux read-deny" built on the Landlock complement — which would pass the
live test on a clean box and break on the first `mkdir` in `$HOME`. (4) The deny block emitted before
the allows: **the single most likely macOS mistake**, because the natural place to add a block is
next to the block it resembles, and the result compiles, runs, reports `Full`, and denies nothing.
(5) A `deny_root_params()` inlined in one of the two places and hand-rolled in the other, which
diverges the moment canonicalization changes the count. (6) `supports_read_deny() { bwrap_on_path() }`,
which is green on every hardened distro where `bwrap` exists and user namespaces are off. And the
live tests reject the base case the whole task exists for: a backend that accepts `deny_read_roots`
and ignores it.

- [ ] **Step 6: Commit**

```bash
cargo check --workspace --all-targets   # O13
git add crates/biorouter-sandbox/
git commit -m "feat(sandbox): a read-deny policy, and each backend's honest answer about it (#56)"
```

---


### Task 14B: Layer A — the barrier at the dispatch choke point, and the policy it hands to Layer B

Task 14A gave `biorouter-sandbox` a read-deny it can express and an honest answer about where it
cannot. That is Layer B, and it only ever covers a **child process**. This task builds **Layer A**,
the primary defence: a check inside the daemon's own dispatch path that refuses any tool call whose
arguments name a path inside a private root, on every platform, for every tool that exists today and
every tool anyone adds later. It then hands the same list to Layer B so a spawned shell gets a kernel
answer as well.

**It fails closed in one direction only, and that narrowing is the point.** A public-capability
session on a platform where the *kernel* deny cannot be established loses the five tools that spawn a
child (AR-6(1)). It does not lose `text_editor`, `analyze`, `image_processor`, `cache`, `xlsx_tool`,
`pdf_tool`, `docx_tool`, or any knowledge / memory / drafter tool, because Layer A needs no kernel
support.

#### ⚠ The barrier is a choke point, not a list — and the difference is testable

Round 1 of this plan wrote *"`developer` and `computercontroller` consume the guard"*. Round 2 found
`cache` **inside** `computercontroller`. The intro section
([DR-14 is two layers](#dr-14-is-two-layers-and-the-os-sandbox-is-the-second-one)) shows why a third
round of enumeration would lose too: the readers are not greppable, the tool count is 125 with 48
path-shaped parameters, and a mechanical extractor written for this round silently dropped the whole
developer server.

So the requirement is stated as a property rather than as coverage: **every tool call is evaluated,
because the evaluation happens at the one function every tool call passes through.** Step 1's
`a_tool_this_code_has_never_heard_of_is_covered_too` is what turns that from a claim into a gate — it
registers a brand-new in-process server at test time and asserts its `read_thing(path)` tool is
refused. **No list-based implementation can pass it.**

That choke point is `ExtensionManager::dispatch_tool_call` (`extension_manager.rs:1438`), and the
argument-scanning pattern this task needs is **already implemented there**, at `:1497-1527`, for
BR-23's `SecretGuard`. Its own comment (`:1497-1502`) calls it *"the single choke point every tool
call flows through"*. It covers the 7 built-ins, the 5 platform extensions, the in-process app
servers (`add_inprocess_server` `:901` inserts into the same `self.extensions` map) and
`code_execution`'s inner bridge (`code_execution_extension.rs:1814` re-enters the **ExtensionManager's**
dispatch, not the Agent's — which is also why a barrier in `Agent::dispatch_tool_call` would have
been bypassable). There is exactly one `client.call_tool` in production, at `:1562`, inside it.

**This task extends that mechanism. It does not build a second one.**

#### The five entries, resolved from real symbols

| # | Root | Resolved by | Why it is private |
|---|---|---|---|
| 1 | `<data>/sessions` | `Paths::data_dir()` (`config/paths.rs:35-37`) + `SESSIONS_FOLDER` (`session_manager.rs:30`). The production store is `SessionStorage::new(Paths::data_dir())` at `:115`, and `SessionStorage::new` joins `SESSIONS_FOLDER` then `DB_NAME` at `:2007-2009`. | `sessions.db` carries `messages_fts`, a **contentful** FTS5 mirror of every message (`:14-28`). The whole directory, not the file, so `-wal`/`-shm` and any future sibling are covered. |
| 2 | `<config>/knowledge` | `knowledge::paths::knowledge_root()` (`biorouter-mcp/src/knowledge/paths.rs:43-45`) → `crate::paths::in_config_dir("knowledge")` | The markdown tree CP1–CP5 exist to gate. Reading it off disk is the same crossing with none of the gates. |
| 3 | `<config>/memory` | `memory::global_memory_dir()` (`biorouter-mcp/src/memory/mod.rs:82-84`) | AR-3 already concedes the *local* store is ungated; the global one is the one Task 19 refuses to write from a private session, and reading it back is the same content. |
| 4 | `<config>/agent_drafter` | `agent_drafter::default_root()` (`biorouter-mcp/src/agent_drafter/mod.rs:815-817`) | App source, `.vault/` secrets, **and the app ids** — which matter for a reason spelled out in Task 14C. |

Both crates' config resolvers agree byte for byte and there is a test that says so
(`crates/biorouter/tests/path_resolver_agreement.rs`), so root 1 is computed from
`biorouter::config::Paths` and roots 2–4 from the symbols above without the two disagreeing.

⚠ **`<config>` is not `<data>`.** The design's §9.3 A2 already corrected this once: on this machine
`sessions.db` is under `~/.local/share/biorouter/`, while `knowledge/`, `memory/` and
`agent_drafter/` are under `~/.config/biorouter/`. A deny list written against one prefix misses
three of the four roots and every test that uses `BIOROUTER_PATH_ROOT` (which relocates *both*) still
passes.

⚠ **A fifth entry, and it is a FILE, not a root: `<config>/config.yaml`.** Round 2:

> There is also a direct self-disable path. The plan deliberately excludes `config.yaml` from the
> deny roots, while `TIERS_ENABLED` is loaded from configuration at startup. `config.yaml` is an
> ordinary, non-SecretGuard file. A public model can edit it through `developer__text_editor`; the
> next daemon restart loads privacy tiers off.

Confirmed on the tree: `CONFIG_YAML_NAME = "config.yaml"` (`config/base.rs:58`), the path is built at
`:210` as `Paths::config_dir().join(CONFIG_YAML_NAME)`, and `DEFAULT_SECRET_PATTERNS`
(`secret_guard.rs:33-45`) has `**/secrets.*` but nothing matching `config.yaml`. And it is **five**
write paths, not one — the earlier draft would have closed only the first:

| # | Tool | Write primitive | Closed by |
|---|---|---|---|
| 1 | `developer__text_editor` write / str_replace / insert | `text_editor.rs` | Layer A |
| 2 | `developer__shell` (`echo … > "$H/.config/biorouter/config.yaml"`) | child process | **Layer B only** — Layer A is textual and cannot see a path assembled at runtime |
| 3 | `computercontroller__cache` **delete** | `computercontroller/mod.rs:1504` `fs::remove_file(path)` | Layer A. A deleted config is as good as a rewritten one |
| 4 | `knowledge__kb_export` `dest_path` | `knowledge/server.rs:753` `fs::write(&dest, …)` | Layer A. A config that fails to parse is a config that does not say `privacy_tiers.enabled` |
| 5 | `agent_drafter__export_app` `target_dir` | `agent_drafter/mod.rs:2769` `fs::write(&full, content)` | Layer A |

Two decisions, both stated rather than assumed:

- **Read *and* write, not write-only.** Telling a read from a write means knowing which argument of
  which tool is a destination, and that is exactly the per-tool knowledge this design has abandoned
  as unfixable. One rule over one file is unfailable; a read/write split over 125 tools is the thing
  that lost twice. **The cost, plainly: a public-capability chat cannot view `config.yaml` through a
  tool**, so "why isn't my extension loading" debugging moves to a private chat or to Settings. The
  user can still open the file themselves — this constrains models, not people.
- **Public sessions only, exactly like the four roots.** The alternative — protect the switch in
  every session — would put a rule outside the capability axis and make the feature's own scope
  harder to state. A private-capability model is institution-hosted and already trusted with the
  material the switch protects.

**And it does not contradict "the four roots and nothing else."** The four roots are private
*content*, which is what DR-3 is about. `config.yaml` is on the list for a different reason: it is
the switch that governs the ruling itself, and a control a public model can turn off is not a
control. The rest of the config directory stays readable — `.biorouterignore`, the extension configs
and the skills tree are ordinary work — and the credential half remains SecretGuard's `**/secrets.*`
floor, a different control with a different owner (BR-23).

⚠ **The existence gate must not be inherited.** `SecretGuard::candidate_is_denied`
(`secret_guard.rs:340-352`) ends in `resolved.exists() || path.exists()`, which is right for a secret
file (a mention of `.env` in prose should not trip a boundary) and **wrong for a deny root**:
measured on this machine, `~/.config/biorouter/memory` does **not exist** (it is created lazily on
first write, `memory/mod.rs:82-84`), so an existence-gated barrier fails open on the memory root of
every fresh install. Layer A's verdict is existence-independent. This is the single most likely way
to implement this task wrongly while every test written against a populated fixture passes, and
Step 1's `an_absent_root_is_still_denied` is the only thing that notices.

#### ⚠ Read this first: nothing shared, nothing mutable, nothing captured at spawn

Round 2's first new defect:

> The proposed `Arc<RwLock<Vec<PathBuf>>>` guard is mutable session state, written before
> `dispatch_tool_call` returns. Current dispatch deliberately permits overlapping calls and returns
> the actual `call_tool` work as a boxed future. A public dispatch can set the guard, then a
> provider/toggle change plus a private dispatch can clear it before the first builtin reads it. The
> public call then runs unrestricted.

**Correct, and the answer is not a better lock — it is no shared state at all.** Both layers now
carry the capability *with the call*:

| | How the capability travels | Why it cannot be raced |
|---|---|---|
| **Layer A** | a **local** in `dispatch_tool_call`'s own stack frame: the same `caller_tier` Gate C computes at `:1607-1614`, used to build a `PrivatePathPolicy` and consumed before the function returns | the refusal is returned from the synchronous portion, **before** `let fut = async move` is constructed at `:1544`. A concurrent dispatch cannot reach into this frame; the worst it can do is change what the *next* call reads |
| **Layer B** | the per-call **`_meta`** map — the same channel `biorouter-session-id` already rides in (`mcp_client.rs:864-880`), read server-side from `RequestContext` (`knowledge/server.rs:222-224`) | two overlapping dispatches build two `McpMeta`s from two locals. There is no cell for a second call to clear |

This also kills the round-1 note about task-locals. That note was right about `spawn_and_serve`'s
`tokio::spawn` (`biorouter-mcp/src/lib.rs:60-75`) breaking a task-local set at dispatch — and
irrelevant, because `_meta` is not a task-local: it is part of the request, it crosses the duplex
pipe with the call, and `developer__shell` **already takes** the `RequestContext` that carries it
(`rmcp_developer.rs:1320-1324`).

It also answers the O6 hazard for free. Nothing is baked in at extension-admission time, so a
mid-session model swap takes effect on the very next tool call with no re-admission — because there
is nothing to re-admit.

⚠ **One disclosure to state, not to fix.** `_meta` travels to *every* server, including third-party
stdio extensions, so a third-party MCP server learns whether the session is public or private. That
is the same exposure `biorouter-session-id` already has on the same channel; it discloses a boolean
about the user's model choice and no content. Recorded here so it is a decision rather than a
surprise.

**Files:**

| Action | Path | Anchor (measured this round, on the current tree) |
|---|---|---|
| Modify | `crates/biorouter-mcp/src/secret_guard.rs` | promote the argument walker: `find_denied_path` `:277-286`, `walk_value` `:288-311`, `scan_string` `:313-338`, `key_is_pathlike` `:51-71`, `has_separator` `:355`. `DEFAULT_SECRET_PATTERNS` `:33-45` and `candidate_is_denied` `:340-352` are **not** touched |
| Create | `crates/biorouter-sandbox/src/private_data.rs` | new — `CALLER_CAPABILITY_META_KEY`, `PrivateDataPolicy` (the paths + the Layer-B wrap decision), `is_under_any`, `read_deny_unavailable_message` |
| Modify | `crates/biorouter-sandbox/src/lib.rs` | the `pub mod` list `:29-33` |
| Modify | `crates/biorouter-mcp/src/lib.rs` | one line in the re-export block beside `pub use biorouter_sandbox::shell_sandbox;` `:39` — this is how `biorouter`, which has **no** direct `biorouter-sandbox` dependency (measured: `crates/biorouter/Cargo.toml:97` lists `biorouter-mcp` only), reaches the type |
| Create | `crates/biorouter/src/privacy/path_policy.rs` | new — `PrivatePathPolicy::for_caller`, `first_violation`, the refusal |
| Create | `crates/biorouter/src/privacy/private_roots.rs` | new — the five entries, one function |
| Modify | `crates/biorouter/src/agents/mcp_client.rs` | `McpMeta` `:137-145`, `McpMeta::new` `:147-152`, `inject_into_extensions` `:161-172`, `inject_session_id_into_extensions` `:864-880` (the pattern the new key copies) |
| Modify | `crates/biorouter/src/agents/extension_manager.rs` | `dispatch_tool_call` `:1438`; the `caller_tier` Task 14 computes; the BR-23 block `:1497-1527`; the `McpMeta::new(&session_id)` at `:1553` |
| Modify | `crates/biorouter-mcp/src/developer/shell.rs` | `build_sandbox_policy` `:131`; `shell_sandbox_wrap` `:163` (the `mode == SandboxMode::Off` early return at `:168`); `shell_sandbox_status_line` `:222`; `configure_shell_command` `:330` (the strip at `:368`, which stays **last**) |
| Modify | `crates/biorouter-mcp/src/developer/rmcp_developer.rs` | `shell` `:1320-1324` (**already takes `RequestContext`** — the only spawning tool that does); the `configure_shell_command` call site |
| Modify | `crates/biorouter-mcp/src/developer/background.rs` | the `configure_shell_command` call `:128` |
| Modify | `crates/biorouter-mcp/src/computercontroller/mod.rs` | `automation_script_command` `:45-70` (the strip at `:69`); `automation_script` and `computer_control` handlers gain a `RequestContext` parameter |
| Modify | `crates/biorouter-mcp/src/compute_server/mod.rs` | `compute_run` `:81`, `compute_python` `:99` — **neither takes a `RequestContext` today** (measured: `grep -c 'context: RequestContext<RoleServer>'` → 0), and both spawn through `LocalProcessSandbox::exec` |
| Reference | `crates/biorouter-sandbox/src/local.rs` | `exec` `:53-64` spawns an **unsandboxed host child** with no `shell_sandbox` wrap; the module doc `:12-18` says so outright |
| Reference | `crates/biorouter/tests/path_resolver_agreement.rs` | the cross-crate pin that lets root 1 and roots 2–4 be computed in different crates |

- [ ] **Step 1: Write the failing tests**

**(1) The five entries.**

```rust
// crates/biorouter/src/privacy/private_roots.rs
#[test]
fn the_entries_are_the_ones_the_ruling_names_and_span_both_dirs() {
    let _g = env_lock::lock_env([("BIOROUTER_PATH_ROOT", Some("/tmp/pr-test"))]);
    let roots = all();
    assert_eq!(roots.len(), 5, "{roots:?}");
    assert!(roots.contains(&PathBuf::from("/tmp/pr-test/data/sessions")));
    assert!(roots.contains(&PathBuf::from("/tmp/pr-test/config/knowledge")));
    assert!(roots.contains(&PathBuf::from("/tmp/pr-test/config/memory")));
    assert!(roots.contains(&PathBuf::from("/tmp/pr-test/config/agent_drafter")));
    assert!(roots.contains(&PathBuf::from("/tmp/pr-test/config/config.yaml")));
    // The bug this catches: four of the five written against `config` and the
    // session store missed, which every BIOROUTER_PATH_ROOT test still passes
    // because that variable relocates both dirs under one parent.
    assert!(roots.iter().any(|r| r.starts_with("/tmp/pr-test/data")),
            "the session store is under data_dir, not config_dir");
    // And the config DIRECTORY is not on the list — only the one file. Denying
    // it wholesale takes .biorouterignore, the extension configs and the skills
    // tree with it, which is a general jail and is exactly what the ruling
    // forbids.
    assert!(!roots.contains(&PathBuf::from("/tmp/pr-test/config")));
}
```

**(2) Containment: symlinks, `..`, and the root that does not exist yet.**

```rust
// crates/biorouter/src/privacy/path_policy.rs
#[test]
fn a_symlink_or_a_dotdot_into_a_deny_root_is_still_under_it() {
    // The lexical check is the one a model beats in a single command:
    // `ln -s ~/.config/biorouter/knowledge ./kb && cat ./kb/page.md`.
    let root = tempfile::tempdir().unwrap();
    let secret = root.path().join("knowledge");
    std::fs::create_dir_all(&secret).unwrap();
    let link_dir = tempfile::tempdir().unwrap();
    let link = link_dir.path().join("kb");
    std::os::unix::fs::symlink(&secret, &link).unwrap();
    assert!(is_under_any(&link.join("page.md"), &[secret.clone()]));
    assert!(is_under_any(&secret.join("../knowledge/page.md"), &[secret.clone()]));
    // The route in from outside, which `starts_with` alone gets wrong:
    let outside = root.path().join("work");
    std::fs::create_dir_all(&outside).unwrap();
    assert!(is_under_any(&outside.join("../knowledge/page.md"), &[secret]));
}

#[test]
fn an_absent_root_is_still_denied() {
    // THE test for this task. `~/.config/biorouter/memory` does not exist on a
    // fresh install (created lazily on first write, memory/mod.rs:82-84) and is
    // ABSENT on the machine this plan was written on. An implementation that
    // reuses SecretGuard::candidate_is_denied inherits its
    // `resolved.exists() || path.exists()` gate (secret_guard.rs:351) and fails
    // OPEN on exactly that root, while every test written against a populated
    // fixture passes.
    let root = tempfile::tempdir().unwrap();
    let never_created = root.path().join("memory");
    assert!(!never_created.exists());
    assert!(is_under_any(&never_created.join("notes.txt"), &[never_created]));
}
```

**(3) The barrier covers a tool that did not exist when it was written.** This is the gate the last
two rounds did not have.

```rust
// crates/biorouter/src/agents/extension_manager.rs
#[tokio::test]
async fn a_tool_this_code_has_never_heard_of_is_covered_too() {
    // Registered at TEST time through the same `add_inprocess_server` (:901)
    // that `appcontrol`/`datasql`/`files`/`compute` use, so it lands in the same
    // `self.extensions` map and is in no way special. Production has never seen
    // this tool name, this extension name, or this parameter name.
    let (mgr, roots) = manager_on(public_provider()).await;
    mgr.add_inprocess_server("surprise", surprise_server()).await.unwrap();

    let err = dispatch(&mgr, "surprise__read_thing", json!({
        "location": roots.knowledge.join("page.md").display().to_string()
    })).await.unwrap_err();
    assert!(format!("{err:?}").contains("knowledge"), "{err:?}");

    // …and the barrier is not keyed on the extension name either: the same tool
    // with an ordinary path is permitted.
    assert!(dispatch(&mgr, "surprise__read_thing",
                     json!({"location": "/tmp/ordinary.txt"})).await.is_ok());
}
```

**This gate rejects:** every implementation that names tools or extensions — `match tool_name { "cache" | "text_editor" => … }`, a `HashSet` of guarded extension keys, a per-server `private_data`
field wired into `developer` and `computercontroller`. All of them pass a table of known readers and
fail this one test.

**(4) The known readers, as a sample of the property — including the four that round 2 named and the
five `config.yaml` writers.**

```rust
#[tokio::test]
async fn every_reader_family_is_refused_a_deny_root_path() {
    let (mgr, roots) = manager_on(public_provider()).await;
    let kb  = roots.knowledge.join("page.md").display().to_string();
    let cfg = roots.config_yaml.display().to_string();
    for (tool, args) in [
        // Round 2 finding 3: the two the earlier draft missed entirely.
        ("computercontroller__cache",       json!({"command":"view",  "path": kb})),
        ("computercontroller__cache",       json!({"command":"delete","path": cfg})),
        ("agent_drafter__export_app",       json!({"id":"a","target_dir": roots.config_dir_str()})),
        // The three readers a `fs::` grep cannot find at all.
        ("computercontroller__xlsx_tool",   json!({"path": kb})),
        ("computercontroller__pdf_tool",    json!({"path": kb})),
        ("computercontroller__docx_tool",   json!({"path": kb})),
        // The developer file tools, in Auto mode (see (5)).
        ("developer__text_editor",          json!({"command":"view","path": kb})),
        ("developer__text_editor",          json!({"command":"write","path": cfg,"file_text":"x"})),
        ("developer__analyze",              json!({"path": kb})),
        ("developer__image_processor",      json!({"path": kb})),
        // The knowledge server's own two unvalidated ends.
        ("knowledge__kb_import",            json!({"src_path": kb})),
        ("knowledge__kb_export",            json!({"kb_id":"k","dest_path": cfg})),
    ] {
        let err = dispatch(&mgr, tool, args.clone()).await
            .unwrap_err_or_else(|| panic!("{tool} was permitted with {args}"));
        let m = format!("{err:?}");
        assert!(m.contains("private model"), "{tool}: {m}");
        assert!(m.contains("Do not retry"), "{tool}: {m}");
    }
}
```

⚠ **This test is a sample, not the specification.** If a future reader is added and this list is not
updated, nothing here fails — and that is *fine*, because (3) is the test that says the property
holds. Do not convert this list into the gate; that is the mistake this task exists to undo.

**(5) The legitimate tools are untouched, which is what makes this a targeted deny.**

```rust
#[tokio::test]
async fn the_tools_that_own_these_roots_keep_working_in_a_public_session() {
    // DR-14 governs the FILESYSTEM channel — a path a caller names. The TOOL
    // channel into these roots is governed by the tier classification and each
    // server's own gates: `knowledge` by CP1-CP5, `memory`'s global store by
    // Task 19, `agent_drafter` by its Public classification (design:975). A
    // barrier that also refused these would be a second, contradictory
    // classification system, and it would break the Knowledge view.
    let (mgr, _) = manager_on(public_provider()).await;
    assert!(dispatch(&mgr, "knowledge__kb_read_page",
                     json!({"kb_id":"k","page_path":"topics/x.md"})).await.is_ok());
    assert!(dispatch(&mgr, "memory__retrieve_memories",
                     json!({"category":"research","is_global":true})).await.is_ok());
    assert!(dispatch(&mgr, "agent_drafter__read_app",
                     json!({"id":"a","path":"src/app.ts"})).await.is_ok());
    assert!(dispatch(&mgr, "agent_drafter__list_apps", json!({})).await.is_ok());
}

#[tokio::test]
async fn a_private_session_and_a_disabled_feature_are_both_unaffected() {
    let (mgr, roots) = manager_on(private_provider()).await;
    let kb = roots.knowledge.join("page.md").display().to_string();
    assert!(dispatch(&mgr, "computercontroller__cache",
                     json!({"command":"view","path": kb.clone()})).await.is_ok());

    let (mgr, roots) = manager_on(public_provider()).await;
    with_privacy_tiers_off(|| async {
        assert!(dispatch(&mgr, "computercontroller__cache",
                         json!({"command":"view","path": kb})).await.is_ok(),
                "the master toggle must remove the barrier, not soften it");
    }).await;
}
```

**(6) The interleaving — round 2's first new defect, as a forced test rather than a hope.**

```rust
/// Two overlapping dispatches on ONE session, with a model swap in the window.
///
/// `#[tokio::test]` is `current_thread` by default and two spawns a few
/// microseconds long cannot preempt each other, so this uses the same
/// `#[cfg(test)] mod seams` rendezvous Task 12 introduced: `arm_after_caller_tier`
/// returns a receiver that fires when `dispatch_tool_call` has read
/// `caller_tier` and is about to evaluate the policy, carrying the sender that
/// releases it. The whole swap runs INSIDE that window.
#[tokio::test]
async fn a_swap_to_private_mid_dispatch_does_not_release_the_public_call() {
    let (agent, s) = agent_on(public_provider()).await;
    let kb = private_roots().knowledge.join("page.md").display().to_string();

    let reached = seams::arm_after_caller_tier();
    let public_call = tokio::spawn({
        let agent = agent.clone();
        let kb = kb.clone();
        async move { call(&agent, "computercontroller__cache",
                          json!({"command":"view","path": kb})).await }
    });

    // Parked with caller_tier == Public already in hand.
    let release = reached.await.unwrap();
    // Everything the round-1 shared cell would have let a concurrent caller do:
    agent.update_provider(private_provider(), &s.id).await.unwrap();
    assert!(call(&agent, "computercontroller__cache",
                 json!({"command":"view","path": kb})).await.is_ok(),
            "the now-private session must be able to read it");
    release.send(()).unwrap();

    let err = public_call.await.unwrap().unwrap_err();
    assert!(format!("{err:?}").contains("private model"),
            "a call admitted as public completed with private privileges");
}
```

**This gate rejects:** the round-1 design — an `Arc<RwLock<Vec<PathBuf>>>` written at dispatch and
read inside the tool body. Under it the public dispatch returns `Ok`, the private dispatch clears the
cell, and the parked tool body reads an empty deny list and succeeds. It also rejects any variant
that reads the tier again *after* an `.await` instead of using the value it was admitted on.

**(7) Layer B: the policy composes with BR-69 rather than replacing it, and refuses when it cannot.**

```rust
// crates/biorouter-mcp/src/developer/shell.rs
#[test]
fn a_restricted_policy_composes_with_br69_instead_of_replacing_it() {
    // With BR-69's own gate OFF, the privacy sandbox subtracts the roots and
    // changes NOTHING else: writes stay open, network stays open. A policy that
    // quietly inherits BR-69's confinement ships "BR-69 on by default" wearing a
    // privacy label, and the first bug report is `pip install` failing.
    let p = build_sandbox_policy(Some(Path::new("/work")), &restricted(["/private-root"]));
    assert_eq!(p.deny_read_roots, vec![PathBuf::from("/private-root")]);
    assert!(p.allow_network, "the privacy sandbox must not deny the network");
    assert_eq!(p.writable_roots, vec![PathBuf::from(std::path::MAIN_SEPARATOR_STR)],
               "the privacy sandbox must not confine writes");
}

#[test]
fn an_unrestricted_policy_leaves_the_br69_policy_byte_for_byte() {
    assert_eq!(
        build_sandbox_policy(Some(Path::new("/work")), &PrivateDataPolicy::none()).writable_roots,
        vec![PathBuf::from("/work"), std::env::temp_dir()],
    );
}

#[test]
fn a_host_that_cannot_deny_reads_refuses_the_tool_and_names_both_exits() {
    let msg = read_deny_unavailable_message("developer__shell", &unsupported_report());
    assert!(msg.contains("developer__shell"));
    assert!(msg.contains("private model"));          // exit 1
    assert!(msg.contains("Settings"));               // exit 2
    assert!(msg.contains("Do not retry"));           // forecloses the workaround
    assert!(!msg.contains("unsandboxed"), "must not suggest a third way out");
    // ⚠ It must NOT claim the file tools are affected. On an unsupported host
    // they still work, because Layer A does not need a kernel — and a refusal
    // that overstates its own scope teaches the model to stop trying things
    // that would have succeeded.
    assert!(!msg.contains("text_editor"));
    assert_eq!(msg, read_deny_unavailable_message("developer__shell", &unsupported_report()));
}
```

**(8) The end-to-end one, which is the only test that fails a wiring that compiles but never fires.**

```rust
#[tokio::test]
async fn a_public_session_shell_cannot_read_the_session_database() {
    // Through the REAL agent-loop dispatch, so it exercises the meta injection
    // in dispatch_tool_call and the read of it in the shell handler.
    if !biorouter_mcp::shell_sandbox::detect().supports_read_deny() {
        return; // covered by (7)'s refusal test on such a host
    }
    let db = private_session_with_a_transcript_containing("COHORT-SENTINEL").await;
    // ⚠ Constructed at RUNTIME so Layer A cannot see it. That is deliberate:
    // this test must fail if Layer B is absent, and Layer A refusing the literal
    // path would mask that.
    let cmd = format!("sqlite3 \"$(printf '%s' {})\" 'select text from messages_fts'",
                      db.display());

    let out = shell_via_agent_loop_on(public_provider(), &cmd).await;
    assert!(!out.contains("COHORT-SENTINEL"), "the read-deny did not apply: {out}");

    // A private-capability session is UNAFFECTED — this is not a general jail.
    let out = shell_via_agent_loop_on(private_provider(), &cmd).await;
    assert!(out.contains("COHORT-SENTINEL"), "a private session must not be sandboxed: {out}");

    // …and the swap back is honoured on the NEXT call, without re-admitting the
    // extension: the O6 hazard, as an assertion.
    let (agent, s) = agent_on(private_provider()).await;
    assert!(shell_in(&agent, &s, &cmd).await.contains("COHORT-SENTINEL"));
    agent.update_provider(public_provider(), &s.id).await.unwrap();
    assert!(!shell_in(&agent, &s, &cmd).await.contains("COHORT-SENTINEL"));
}
```

- [ ] **Step 2: Run** → **COMPILE ERROR** (`unresolved module privacy::path_policy`;
`build_sandbox_policy` takes 1 argument not 2; `for_each_path_candidate` is private).

- [ ] **Step 3: Implement**

**(a) `secret_guard.rs` — promote the argument walker. One walker, two verdicts.**

`find_denied_path` (`:277`) already knows how to find every path-shaped token in a tool call's
arguments: which keys are path-like (`key_is_pathlike` `:51-71` — 20 names), when a bare token counts
(`has_separator` `:355`), how to recurse into arrays and nested objects (`walk_value` `:288-311`), and
how to split a shell command line into tokens (`scan_string` `:313-338`). **Layer A must walk
arguments identically, and the only way to guarantee that is to walk them with the same code.**

```rust
/// Walk a tool call's arguments and hand every path-shaped token to `probe`,
/// returning the first token `probe` accepts.
///
/// Split out of [`SecretGuard::find_denied_path`] so BR-23's secret scan and
/// issue #56's private-root barrier see **the same tokens**. They must not
/// drift: a `key_is_pathlike` name added for one is a name the other needs, and
/// a barrier that misses an argument shape the secret scan handles is a barrier
/// with a hole nobody will find by reading.
///
/// The verdicts stay separate on purpose. BR-23's is existence-gated and
/// unconditional; #56's is existence-INdependent and capability-conditional.
/// Merging them would either put the data directory into the always-on floor
/// (which D9 measured as wrong: it hides the user's own knowledge base from a
/// PRIVATE session) or make the secret floor conditional (which weakens BR-23).
pub fn find_path_candidate<F>(arguments: &Map<String, Value>, mut probe: F) -> Option<String>
where
    F: FnMut(&str) -> bool,
{
    let mut found = None;
    for (key, value) in arguments {
        walk_value(Some(key), value, &mut probe, &mut found);
        if found.is_some() {
            break;
        }
    }
    found
}
```

`walk_value` and `scan_string` move from `impl SecretGuard` to free functions taking `probe: &mut F`,
and the method becomes a one-liner:

```rust
    pub fn find_denied_path(&self, arguments: &Map<String, Value>) -> Option<String> {
        find_path_candidate(arguments, |c| self.candidate_is_denied(c))
    }
```

⚠ **Behaviour-preserving refactor, and its 19 existing tests are the proof.** Run
`cargo test -p biorouter-mcp --lib secret_guard::` **before** touching anything and record the
count — measured **19 passed** on this tree — then again after. A changed number means the extraction
changed behaviour, and this task is not the place to do that.

**(b) `biorouter-mcp/src/paths.rs` — the missing half of the resolver.**

`config_dir()`/`in_config_dir()` exist (`:39-49`); there is **no** `data_dir()`, and root 1 lives
under `<data>`. Add the mirror, including the pure `resolve_data_dir` split the module already uses
so it is testable without mutating process env:

```rust
/// Resolve the data dir the way `biorouter::config::Paths::get_dir(Data)` does.
pub fn data_dir() -> PathBuf {
    let root = std::env::var("BIOROUTER_PATH_ROOT").ok();
    resolve_data_dir(root.as_deref(), &platform_data_dir())
}
pub fn in_data_dir(sub: &str) -> PathBuf { data_dir().join(sub) }
```

and **add its assertion to `crates/biorouter/tests/path_resolver_agreement.rs`**, because this
module's own header says so in as many words: *"Adding a store here means adding its assertion too."*
Without it the two crates can drift on the one root whose directory differs from the other four —
which is the exact failure §9.3 A2 already caught once.

**(c) `biorouter-mcp/src/private_roots.rs` — five entries, ONE resolver, no second spelling.**

```rust
//! The paths DR-14 hides from a public-capability session (issue #56).
//!
//! In `biorouter-mcp` rather than in `biorouter` for one reason: `biorouter`
//! can see this crate and this crate cannot see `biorouter`, so putting the
//! list here means **one** spelling of each path for both layers. The earlier
//! draft resolved them in `biorouter` and would have needed a second copy in
//! the servers that enforce Layer B — and "two spellings of one path" is
//! precisely how a root silently stops being covered.
//!
//! Computed, never hardcoded: `BIOROUTER_PATH_ROOT` relocates all five, and a
//! literal `~/.config/biorouter/...` would make every sandboxed test read the
//! developer's real store.

use std::path::PathBuf;

/// The four directory roots. Layer B overmounts / denies these as subtrees.
pub fn directory_roots() -> Vec<PathBuf> {
    vec![
        // 1. The session store. Under DATA dir, not config — see the task's ⚠.
        crate::paths::in_data_dir("sessions"),
        // 2-4. Under config. Routed through each store's own accessor so a
        // future move follows automatically.
        crate::knowledge::paths::knowledge_root()
            .unwrap_or_else(|_| crate::paths::in_config_dir("knowledge")),
        crate::memory::global_memory_dir(),
        crate::agent_drafter::default_root(),
    ]
}

/// The one FILE on the list: the master switch itself. Separate from
/// `directory_roots` because the two need different treatment in Layer B —
/// `--tmpfs` needs a directory mountpoint, and SBPL wants `literal` rather than
/// `subpath` — and because conflating them is how someone eventually calls
/// `create_dir_all` on it (see (i)).
pub fn config_file() -> PathBuf {
    crate::paths::in_config_dir("config.yaml")
}

pub fn all() -> Vec<PathBuf> {
    let mut v = directory_roots();
    v.push(config_file());
    v
}
```

⚠ Two symbols need widening, and both are one word: `memory::global_memory_dir` is private to its
module (`memory/mod.rs:82`) and `"sessions"` is spelled `SESSIONS_FOLDER` in
`biorouter::session::session_manager:30`, which this crate cannot see. Make the first `pub(crate)`
and pin the second in the agreement test — `assert!(all().contains(&Paths::data_dir().join(SESSIONS_FOLDER)))`
— rather than exporting a constant across the circular boundary.

`crates/biorouter/src/privacy/private_roots.rs` is then a re-export plus the tests from Step 1(1):

```rust
pub use biorouter_mcp::private_roots::{all, config_file, directory_roots};
```

**(d) `biorouter/src/privacy/path_policy.rs` — Layer A's verdict.**

```rust
//! Issue #56 DR-14, Layer A: the in-process half of the private-data read-deny.
//!
//! Evaluated at `ExtensionManager::dispatch_tool_call`, the one function every
//! tool call passes through, from a `caller_tier` held in that call's own stack
//! frame. **It owns no state.** There is no cell, no lock and no `OnceCell`:
//! two overlapping dispatches build two policies, and neither can see the
//! other's. That is the whole answer to the round-2 race.

use std::path::{Path, PathBuf};

pub struct PrivatePathPolicy {
    /// Empty when privacy tiers are off, or the caller is private-capability.
    /// Emptiness IS the "no barrier" state — there is no second flag to forget.
    entries: Vec<PathBuf>,
}

impl PrivatePathPolicy {
    /// The one place the capability and the master opt-out are folded together.
    pub fn for_caller(caller: ProviderTier) -> Self {
        let entries = if crate::privacy::privacy_tiers_enabled() && !caller.is_private() {
            biorouter_mcp::private_roots::all()
        } else {
            Vec::new()
        };
        Self { entries }
    }

    pub fn is_empty(&self) -> bool { self.entries.is_empty() }

    /// The first argument token that lands inside a private entry, if any.
    /// `cwd` is the session working directory, so a relative token resolves the
    /// way the tool itself will resolve it.
    pub fn first_violation(
        &self,
        arguments: &serde_json::Map<String, serde_json::Value>,
        cwd: &Path,
    ) -> Option<String> {
        if self.is_empty() {
            return None;
        }
        biorouter_mcp::secret_guard::find_path_candidate(arguments, |candidate| {
            let resolved = cwd.join(expand_tilde(candidate));
            is_under_any(&resolved, &self.entries)
        })
    }
}
```

`is_under_any` is the containment test, and it has three jobs the obvious `starts_with` does not do:

```rust
/// Whether `path` lands inside one of `entries`.
///
/// Three properties, each with a test in Step 1(2):
///
///   1. **Not existence-gated.** `SecretGuard::candidate_is_denied`
///      (`secret_guard.rs:351`) ends in `resolved.exists() || path.exists()`,
///      which is right for a secret file and wrong for a deny root:
///      `~/.config/biorouter/memory` does not exist until the first
///      `remember_memory` and is ABSENT on the machine this plan was written
///      on. Reuse that helper and the memory root fails open on every fresh
///      install.
///   2. **Lexically normalised first**, so `<cwd>/../.config/biorouter/knowledge`
///      is caught even when nothing on that path exists yet.
///   3. **Then symlink-resolved**, by canonicalizing the deepest ancestor that
///      DOES exist and re-testing — the same technique
///      `SecretGuard::is_inside_root` (`:242-263`) already uses in this tree,
///      and the one that catches `ln -s ~/.config/biorouter/knowledge ./kb`.
///      Each entry is matched in both its literal and its canonical spelling,
///      because on macOS every `/var/folders/...` is really `/private/var/...`
///      and a single-spelling comparison silently matches nothing.
pub fn is_under_any(path: &Path, entries: &[PathBuf]) -> bool {
    if entries.is_empty() {
        return false;
    }
    let spellings: Vec<PathBuf> = entries
        .iter()
        .flat_map(|e| {
            let canonical = e.canonicalize().ok();
            std::iter::once(e.clone()).chain(canonical.filter(|c| c != e))
        })
        .collect();
    let hit = |p: &Path| spellings.iter().any(|e| p == e || p.starts_with(e));

    if hit(&lexical_normalize(path)) {
        return true;
    }
    let mut ancestor = path;
    loop {
        if let Ok(real) = ancestor.canonicalize() {
            let tail = path.strip_prefix(ancestor).unwrap_or(Path::new(""));
            return hit(&lexical_normalize(&real.join(tail)));
        }
        match ancestor.parent() {
            Some(parent) => ancestor = parent,
            None => return false,
        }
    }
}
```

⚠ **`p == e` as well as `p.starts_with(e)`**, so the `config.yaml` entry — a file, not a
directory — matches itself. A containment test written only as `starts_with` happens to work for
this case (a path is a prefix of itself) but stops working the moment someone "tidies" it into
`p.parent().is_some_and(|d| d.starts_with(e))`.

**(e) The barrier, in `dispatch_tool_call` — beside BR-23's, sharing Gate C's local.**

Immediately after the BR-23 `SecretGuard` block that ends at `:1527`, and still **above** the
`let fut = async move` at `:1544`:

```rust
        // Issue #56 DR-14, Layer A. Beside BR-23's scan for the reason its own
        // comment gives at :1497-1502 — "the single choke point every tool call
        // flows through" — and using the SAME `caller_tier` Gate C computed a
        // few lines up. One read, two decisions: it is not possible for the
        // extension gate and the path barrier to disagree about what this
        // session is.
        //
        // NO SHARED STATE. `policy` is a local. A concurrent dispatch cannot
        // reach into this frame, and the refusal is returned before the future
        // at :1544 exists — so there is no window in which this call has been
        // admitted as public and then runs with private privileges. That is the
        // defect round 2 found in the Arc<RwLock<Vec<PathBuf>>> design, and it
        // is answered by deleting the shared cell rather than by locking it
        // better.
        let policy = crate::privacy::path_policy::PrivatePathPolicy::for_caller(caller_tier);
        if !policy.is_empty() {
            if let Some(args) = tool_call.arguments.as_ref() {
                // `cwd` is already resolved above for the SecretGuard scan;
                // reuse it rather than awaiting twice.
                if let Some(hit) = policy.first_violation(args, &cwd) {
                    return Err(crate::privacy::path_policy::refusal(&prefixed_name, &hit).into());
                }
            }
        }
```

⚠ **The BR-23 block's `cwd` is currently scoped inside its `if let Some(args)` (`:1506`).** Lift the
`let cwd = self.resolve_working_dir().await;` out of that block so both scans share one resolution —
two calls would be two `.await`s and, worse, two chances for them to disagree about which directory a
relative argument means.

The refusal is a sibling of Task 12's, in the same register — name the state, name the reason,
foreclose the workaround, name the human action — and it names **which** private area was touched, so
the model stops rather than trying the other four:

```rust
pub fn refusal(tool: &str, hit: &str) -> ErrorData {
    ErrorData::new(
        ErrorCode::INVALID_PARAMS,
        format!(
            "`{tool}` cannot open `{hit}`.\n\n\
             That path is inside Biorouter's own private data — your saved chats, your knowledge \
             bases, your saved memories, your Biorouter apps, or this machine's Biorouter \
             settings file. This chat is running on a public model, hosted outside the \
             institution, so Biorouter does not let its tools read those directories off the \
             disk. Everything else on this machine is readable and writable exactly as before.\n\n\
             There are two ways forward and there is no third:\n\
             1. Switch this chat to a private model — Settings > Models, or the model chip in \
             the composer. Private chats are not restricted this way.\n\
             2. Turn privacy tiers off for this machine in Settings > Privacy. That removes \
             every privacy guardrail here, not just this one.\n\n\
             Do not retry this path, rewrite it, or route it through another tool: the answer is \
             the same everywhere and will not change."
        ),
        None,
    )
}
```

⚠ **It names the offending path and nothing else.** It must not enumerate the five entries: a
barrier that answers "no, and here is the list of directories I am hiding" has told a public model
where the user's knowledge bases and session store are, which is the same mistake Task 10C's *"the
barrier must not narrate what it refused"* ⚠ is about. The five categories are named in prose
(*"saved chats, knowledge bases, …"*) without paths.

**(f) Layer B's channel: one boolean in the per-call `_meta`.**

`McpMeta` (`mcp_client.rs:137-145`) already carries `session_id` and an optional progress token into
`params._meta`, and `inject_session_id_into_extensions` (`:864-880`) is the exact pattern to copy.
Add a third field and a third key:

```rust
pub struct McpMeta {
    session_id: String,
    progress_token: Option<String>,
    /// Issue #56 DR-14. True when this specific call must run under the
    /// private-data read-deny. Computed by `dispatch_tool_call` from the tier
    /// it admitted the call on, so it is per-call and cannot go stale.
    private_data_deny: bool,
}
```

serialised under `biorouter_sandbox::private_data::CALLER_PRIVATE_DATA_DENY_META_KEY`
(`"biorouter-private-data-deny"`), read server-side exactly the way `knowledge/server.rs:222-224`
reads the session id:

```rust
fn private_data_deny(context: &RequestContext<RoleServer>) -> bool {
    context.meta.0
        .get(CALLER_PRIVATE_DATA_DENY_META_KEY)
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}
```

Three decisions worth stating:

- **A boolean, not the path list.** The paths would also work and would need no second resolver, but
  `_meta` travels to **every** server including third-party stdio extensions, and shipping a user's
  home-directory layout to a third-party MCP server is a disclosure a boolean is not. The servers
  resolve the roots themselves from `biorouter_mcp::private_roots`, which is (c)'s single spelling.
- **`dispatch_tool_call` computes `privacy_tiers_enabled() && !caller_tier.is_private()` and puts
  the ANSWER in the field.** No server evaluates policy; the master toggle is folded in at the one
  place, and nothing below `biorouter` knows what a `ProviderTier` is.
- **Absent means unrestricted, and that is fail-open.** A server constructed outside an
  `ExtensionManager` — `biorouter mcp developer`, a unit test — receives no key and must not be
  restricted, or the standalone CLI breaks. No type can fix this: a guard nobody wired has nothing to
  deny. **What protects against "nobody wired it" is Step 1(8), the end-to-end test through the real
  agent loop, and nothing else.** Do not replace it with a unit test on this type.

**(g) `build_sandbox_policy` composes; `shell_sandbox_wrap` fails closed.**

```rust
/// BR-69's policy, plus issue #56's deny entries when this call is restricted.
///
/// The two compose rather than one winning. With BR-69's gate ON the user asked
/// for write confinement and a network deny and still gets them; with it OFF the
/// privacy sandbox must subtract the private entries and change **nothing
/// else**, or what ships is BR-69-on-by-default wearing a privacy label — and
/// the first bug report is `pip install` failing in every chat on a commercial
/// model.
fn build_sandbox_policy(
    working_dir: Option<&std::path::Path>,
    private: &PrivateDataPolicy,
) -> SandboxPolicy {
    let mut policy = if SandboxMode::from_env().is_on() {
        let mut roots = Vec::new();
        if let Some(dir) = working_dir { roots.push(dir.to_path_buf()); }
        roots.push(env::temp_dir());
        SandboxPolicy::new(roots).with_network(env_truthy("BIOROUTER_SHELL_SANDBOX_NETWORK"))
    } else {
        SandboxPolicy::unconfined()
    };
    policy.deny_read_roots = private.directory_roots();
    policy.deny_write_files = private.files();
    policy
}
```

```rust
fn shell_sandbox_wrap(
    program: &str,
    working_dir: Option<&std::path::Path>,
    private: &PrivateDataPolicy,
) -> Result<Option<(String, Vec<String>)>, String> {
    let mode = SandboxMode::from_env();
    // Issue #56 DR-14: a restricted call makes the sandbox mandatory, whatever
    // BIOROUTER_SHELL_SANDBOX says. Placed BEFORE the `mode == Off` early
    // return at :168 — after it, the deny entries are unreachable in the
    // default configuration, which is every user.
    if private.is_restricted() {
        let backend = shell_sandbox::detect();
        if !backend.supports_read_deny() {
            return Err(read_deny_unavailable_message("developer__shell", &backend.probe()));
        }
        let policy = build_sandbox_policy(working_dir, private);
        return match backend.wrap(&policy, program) {
            Ok(w) => Ok(Some((w.program, w.prefix_args))),
            // Fail CLOSED. `auto`'s warn-and-run-anyway is the right answer for
            // a control the user opted into; it is the wrong answer for one
            // that exists because a public model must not read private material.
            Err(e) => Err(read_deny_unavailable_message(
                "developer__shell",
                &SandboxReport::none("none", e.to_string()),
            )),
        };
    }
    if mode == SandboxMode::Off {
        return Ok(None);
    }
    /* …the rest is BR-69's, unchanged… */
}
```

`configure_shell_command` takes the policy as a parameter and passes it through; both production
callers (`rmcp_developer.rs`'s `shell` handler and `background.rs:128`) build it from the boolean in
their `RequestContext`. Its `Result<_, String>` already surfaces as a tool error, so the refusal
reaches the model with no new plumbing. The `strip_daemon_private_env` call at `:368` stays **last**,
after the wrap, for the reason its own doc-comment gives. `shell_sandbox_status_line` grows one
clause naming what is hidden, so the model stops trying rather than retrying blindly.

**(h) The config file is a FILE, and Layer B must not treat it as a root — measured on both kernels.**

macOS uses `literal` rather than `subpath`, emitted with the deny block from Task 14A step 3(b):

```
(deny file-read*  (literal (param "DENY_FILE_0")))
(deny file-write* (literal (param "DENY_FILE_0")))
```

Measured on this host: read → `Operation not permitted` rc=1; `echo bad > cfg` → `Operation not
permitted` rc=1; `rm -f cfg` → `Operation not permitted` rc=1; **a sibling file in the same directory
is still readable, rc=0**; host file 31 bytes, unchanged. The `-D` parameter form behaves
identically. The path must be canonical for the same reason every other entry must (Task 14A ⚑,
§2.4).

Linux binds `/dev/null` read-only over the file, because `--tmpfs` needs a directory mountpoint.
Measured in `debian:bookworm-slim` with `bubblewrap 0.8.0`:

```
--ro-bind /dev/null <config.yaml>
  cat        -> Permission denied              rc=1
  echo >     -> Permission denied              rc=2
  echo >>    -> Permission denied              rc=2
  rm -f      -> Device or resource busy        rc=1
  truncate   -> cannot open for writing        rc=1
  host file  -> 31 bytes, unchanged
```

⚠ **And it obeys the same argv-ordering rule as `--tmpfs`, measured, with the same silent failure.**
Emitted *before* the writable `--bind` of its parent directory:

```
bwrap --dev-bind / / --ro-bind /dev/null $CFG --bind $H $H  sh -c "echo CLOBBERED > $CFG"
rc=0
host now: CLOBBERED
```

The protection evaporates and nothing reports it. So the file bind goes in the **same loop position**
as the directory overmounts — after every `--bind` — and Task 14A's ordering assertion covers both.

**(i) Create the four directory roots at startup — the AR-10 mitigation, and never the fifth.**

`--tmpfs` on a destination that does not exist aborts bubblewrap outright (Task 14A ⚑, measured), so
absent roots must be skipped, and a skipped root is [AR-10](#ar-10--on-linux-a-deny-root-that-does-not-exist-when-a-job-starts-stays-visible-to-that-job-for-its-whole-life)'s
race. Shrink the window by creating them: in `wrap_bubblewrap`, immediately before the deny loop,

```rust
    // AR-10. `memory/` is created lazily on first write and is ABSENT on a
    // fresh install, so without this the memory root is skipped — and a job
    // started before the first `remember_memory` keeps reading it afterwards.
    // Creating an empty directory Biorouter would create anyway is the cheapest
    // fix; it is NOT a closure (a root deleted mid-session still races).
    //
    // ⚠ `directory_roots()` only. `create_dir_all` on the config FILE would
    // replace `config.yaml` with a directory and brick the install, which is
    // why (c) keeps the two lists apart at the type level rather than by
    // convention.
    for root in policy.deny_read_roots.iter() {
        let _ = std::fs::create_dir_all(root);
    }
```

**(j) The other three spawn families.** `computercontroller__automation_script` and
`computer_control`, and `compute__compute_run`/`compute_python`, spawn children and **none of their
handlers takes a `RequestContext` today** (measured: `grep -c 'context: RequestContext<RoleServer>'`
→ 2 in `computercontroller/mod.rs`, both on `list_resources`/`read_resource`; 0 in
`compute_server/mod.rs`). Each gains the parameter — the same one `developer__shell` already has
(`rmcp_developer.rs:1320-1324`) — and passes the boolean into its command builder.

`compute_run`/`compute_python` are the third child-spawner and the plan did not previously name them.
They reach `LocalProcessSandbox::exec` (`biorouter-sandbox/src/local.rs:53-64`), which spawns an
**unsandboxed host child** with `current_dir(workspace)` and no `shell_sandbox` wrap — its own module
doc says so at `:12-18` and `new()` logs a warning at `:30`. `strip_daemon_private_env` runs at `:63`,
so issue #57 holds there, but nothing else does. Route it through the same
`shell_sandbox_wrap`/refuse pair.

⚠ **`computer_control` on Windows is refused for a public-capability session** along with
`automation_script`, because Windows cannot express the read-deny at all — and *only* those, per
[AR-6](#ar-6--on-a-host-that-cannot-express-the-read-deny-a-public-session-loses-the-shell-and-two-costs-come-with-the-sandbox-itself)(1).
Layer A keeps every in-process tool working there.


- [ ] **Step 4: Run**

```bash
cargo test -p biorouter-mcp --lib secret_guard::          # expect the SAME count as before (a)
cargo test -p biorouter-mcp --lib -- private_roots paths::
cargo test -p biorouter --lib -- privacy::path_policy privacy::private_roots
cargo test -p biorouter --lib -- agents::extension_manager
cargo test -p biorouter-mcp --lib -- developer::shell
cargo test -p biorouter --test path_resolver_agreement
cargo check --workspace --all-targets     # O13: McpMeta, build_sandbox_policy, four handlers changed
```

⚠ **Record the pre-count for `secret_guard::`, `developer::shell` and `agents::extension_manager`
before Step 3.** All three have substantial existing suites, and "no failures" is satisfied by a run
in which none of this task's tests were compiled in. `secret_guard::` is the one that matters most:
measured **19 passed** on this tree, and (a) is a refactor that must not move it.

- [ ] **Step 5: Gate**

```bash
# (1) THE gate for this task: the barrier is evaluated at the choke point and
#     nowhere else, and it is not keyed on a name.
#
# There is exactly ONE production call into an MCP client and it is inside
# dispatch_tool_call (Task 14's gate proves that separately). This asserts the
# barrier sits in the same function, and that the whole feature is 1 call site.
grep -rn "PrivatePathPolicy::for_caller\|first_violation(" --include='*.rs' crates/ \
  | grep -v "^crates/biorouter/src/privacy/path_policy.rs:" | grep -v "mod tests"
echo "expect: exactly 2 lines, both in crates/biorouter/src/agents/extension_manager.rs,"
echo "        both inside dispatch_tool_call. A third site in Agent::dispatch_tool_call is"
echo "        Task 14D's job and is a DIFFERENT symbol - see that task."
awk '/pub async fn dispatch_tool_call\(/,/^    }/' crates/biorouter/src/agents/extension_manager.rs \
  | grep -c "PrivatePathPolicy::for_caller"
echo "expect: 1"

# (2) The barrier is not a list. Zero tool names, zero extension names, in the
#     policy module and in the barrier's own lines.
grep -cE '"(cache|text_editor|shell|xlsx_tool|pdf_tool|docx_tool|analyze|image_processor|read_app|export_app|kb_import|kb_export)"' \
  crates/biorouter/src/privacy/path_policy.rs
echo "expect: 0 - a tool name in the policy means someone rebuilt the enumeration"
grep -cE '"(developer|computercontroller|agent_drafter|knowledge|memory|compute)"' \
  crates/biorouter/src/privacy/path_policy.rs
echo "expect: 0 - same, one level up"

# (3) ONE argument walker, TWO verdicts. The extraction in (a) is only worth
#     anything if the privacy scan actually uses it; a second hand-rolled walk
#     is how the two drift.
grep -c "fn walk_value\|fn scan_string" crates/biorouter-mcp/src/secret_guard.rs
echo "expect: 2 - one definition each, still in this file"
grep -rn "walk_value(\|scan_string(" --include='*.rs' crates/ | grep -v "^crates/biorouter-mcp/src/secret_guard.rs:"
echo "expect: no output - nobody re-implements or re-calls the walker outside it"
grep -c "find_path_candidate(" crates/biorouter-mcp/src/secret_guard.rs \
                              crates/biorouter/src/privacy/path_policy.rs
echo "expect: 2 and 1 - the definition plus find_denied_path's use, and Layer A's use"

# (4) The verdict is NOT existence-gated. This is the single likeliest wrong
#     implementation: `find_path_candidate(args, |c| guard.candidate_is_denied(c))`
#     compiles, reads beautifully, passes every test written against a populated
#     fixture, and fails open on ~/.config/biorouter/memory, which does not exist
#     on a fresh install.
grep -c "candidate_is_denied\|\.exists()" crates/biorouter/src/privacy/path_policy.rs
echo "expect: 0"

# (5) The five entries, and both directories. A list written entirely against
#     config_dir passes every BIOROUTER_PATH_ROOT test and misses the session
#     database - the one the design named in 9.3 A2.
awk '/pub fn directory_roots\(\)/,/^}/' crates/biorouter-mcp/src/private_roots.rs \
  | grep -c "in_data_dir\|knowledge_root\|global_memory_dir\|default_root"
echo "expect: 4"
awk '/pub fn directory_roots\(\)/,/^}/' crates/biorouter-mcp/src/private_roots.rs | grep -c "in_data_dir"
echo "expect: 1 - the session store is under data_dir, and nothing else is"
grep -c "config.yaml" crates/biorouter-mcp/src/private_roots.rs
echo "expect: 1 - in config_file(), and NOT in directory_roots()"
awk '/pub fn directory_roots\(\)/,/^}/' crates/biorouter-mcp/src/private_roots.rs | grep -c "config.yaml"
echo "expect: 0 - create_dir_all over this list must never see the config FILE"
grep -cE "\"~/|\.config/biorouter" crates/biorouter-mcp/src/private_roots.rs
echo "expect: 0 - computed, never a literal, or every sandboxed test reads the real store"

# (6) ONE spelling of each path. The whole point of putting the resolver in
#     biorouter-mcp is that biorouter re-exports it rather than recomputing it.
grep -c "pub use biorouter_mcp::private_roots" crates/biorouter/src/privacy/private_roots.rs
echo "expect: 1"
grep -cE "in_config_dir|in_data_dir|knowledge_root|global_memory_dir|default_root" \
  crates/biorouter/src/privacy/private_roots.rs
echo "expect: 0 - a second resolver here is a second spelling, and a second spelling drifts"

# (7) NO shared mutable state anywhere in Layer A. This is round 2's first
#     finding, asserted structurally as well as behaviourally.
grep -cE "RwLock|Mutex|OnceLock|OnceCell|static " crates/biorouter/src/privacy/path_policy.rs
echo "expect: 0 - the policy is a local, built per call, dropped at the end of the call"

# (8) Layer B's ordering, twice over. Both are early returns that make the
#     control dead in the DEFAULT configuration while every unit test that calls
#     the inner function directly still passes.
python3 - <<'PY'
src = open('crates/biorouter-mcp/src/developer/shell.rs').read()
body = src[src.index('fn shell_sandbox_wrap'):]
body = body[:body.index('\n}\n')]
i_deny = body.index('private.is_restricted()')
i_off  = body.index('mode == SandboxMode::Off')
assert i_deny < i_off, 'the DR-14 arm must precede the mode==Off early return'
print('OK  deny arm at', i_deny, ' mode==Off at', i_off)
PY

# (9) Every spawn family decides. EIGHTEEN files in the two crates contain a
#     `Command::new(` outside tests, measured this round with
#     `grep -rn "Command::new(" --include='*.rs' crates/biorouter-mcp/src \
#      crates/biorouter-sandbox/src | grep -v "mod tests" | cut -d: -f1 | sort -u`.
#     A new file here is a new child that may run without the read-deny.
grep -rln "Command::new(" --include='*.rs' crates/biorouter-mcp/src crates/biorouter-sandbox/src | sort
# expect exactly these 18:
#   biorouter-mcp/src/agent_drafter/bundle.rs      esbuild      - no caller path
#   biorouter-mcp/src/agent_drafter/mod.rs         app smoke    - no caller path
#   biorouter-mcp/src/agent_drafter/render.rs      esbuild      - no caller path
#   biorouter-mcp/src/computercontroller/mod.rs    automation_script      -> WRAPPED (j)
#   biorouter-mcp/src/computercontroller/platform/linux.rs   computer_control -> WRAPPED (j)
#   biorouter-mcp/src/computercontroller/platform/macos.rs   computer_control -> WRAPPED (j)
#   biorouter-mcp/src/computercontroller/platform/windows.rs computer_control -> REFUSED (j)
#   biorouter-mcp/src/developer/background.rs      background shell -> WRAPPED (g)
#   biorouter-mcp/src/developer/paths.rs           `which`-style lookup - no caller path
#   biorouter-mcp/src/developer/rmcp_developer.rs  shell        -> WRAPPED (g)
#   biorouter-mcp/src/developer/shell.rs           shell        -> WRAPPED (g)
#   biorouter-mcp/src/knowledge/convert/pdf.rs     converter    - no caller path
#   biorouter-mcp/src/knowledge/source_paths.rs    HTTP-only ingest - no MCP tool reaches it
#   biorouter-sandbox/src/docker.rs                docker backend - opt-in, already confined
#   biorouter-sandbox/src/environment.rs           the strip's own test helper
#   biorouter-sandbox/src/local.rs                 compute_run/python -> WRAPPED (j)
#   biorouter-sandbox/src/seatbelt.rs              the wrapper itself
#   biorouter-sandbox/src/shell_sandbox/linux.rs   the wrapper itself
```

**What wrong implementation each rejects.**

| # | Rejects |
|---|---|
| (1) | A barrier added in a second place — `update_provider`, or a `ToolInspector` — which looks like where a capability changes and which `POST /agent/call_tool` never reaches. |
| (2) | **The enumeration, in any disguise.** A `match tool_name`, a `HashSet` of guarded extensions, a `private_data` field wired into `developer` and `computercontroller`. This is the shape that lost in round 1 and again in round 2. Step 1(3) fails it behaviourally; this fails it by reading. |
| (3) | A second, hand-rolled argument walk that misses `Value::Array`, nested objects, or shell-token splitting — so `{"paths": ["<deny root>/p.md"]}` sails through while `{"path": "…"}` is refused. |
| (4) | The existence-gated verdict. It passes every test whose fixture created the root, and fails open on the memory root of every fresh install — including the machine this plan was written on. |
| (5) | A list written entirely against `config_dir` (three of five missed, all tests green, because `BIOROUTER_PATH_ROOT` relocates both dirs under one parent); and `create_dir_all` reaching the config **file** and replacing it with a directory. |
| (6) | A second resolver in `biorouter`, which is how a root silently stops being covered after someone moves a store. |
| (7) | The `Arc<RwLock<Vec<PathBuf>>>` design round 2 raced. Step 1(6) fails it under a forced interleaving; this fails it at a glance. |
| (8) | The DR-14 arm added *after* the `mode == Off` return — the shape a reader gets from "add a branch to `shell_sandbox_wrap`". It compiles, the unit tests that build a restricted policy directly still pass, and the control is dead for every default install. |
| (9) | A fourth child-spawner nobody classified. `LocalProcessSandbox::exec` was exactly that until this round. |

And Step 1(8) rejects the base case that no structural gate can: everything wired, nothing injected,
the boolean permanently `false`.

- [ ] **Step 6: Commit**

```bash
cargo check --workspace --all-targets
git add crates/biorouter-mcp/src/secret_guard.rs crates/biorouter-mcp/src/paths.rs \
        crates/biorouter-mcp/src/private_roots.rs crates/biorouter-mcp/src/lib.rs \
        crates/biorouter-mcp/src/developer/ crates/biorouter-mcp/src/computercontroller/ \
        crates/biorouter-mcp/src/compute_server/ crates/biorouter-sandbox/ \
        crates/biorouter/src/privacy/ crates/biorouter/src/agents/ \
        crates/biorouter/tests/path_resolver_agreement.rs
git commit -m "feat(privacy): refuse a public session's tools any path inside Biorouter's private data (#56)"
```

---


### Task 14C: The other door — the daemon's own HTTP API, pinned rather than assumed

A sandboxed child cannot open `sessions.db`. It can still talk to the process that can:
`GET /sessions/{id}/export` returns a transcript as JSON to anyone holding
`BIOROUTER_SERVER__SECRET_KEY` (design §9.3 A1). DR-14's sandbox closes the front door; this task is
the audit that the back one is shut, and it is an audit rather than a fix because **the strip is
already there and already tested**. Recording *what was measured* is the point — the previous round
of this plan spent a whole task on a leak that had been closed two commits before the fork.

#### What was measured, at `9558c346`

| Spawn family the DR-14 sandbox relies on | Strip call site | Already pinned by |
|---|---|---|
| `developer__shell`, foreground | `developer/shell.rs:368`, **last** in `configure_shell_command`, after every `.env()` | `daemon_secret_never_reaches_a_shell_child` (`shell.rs:766`) — re-invokes the test binary with the canary exported and reads a real child's environment |
| `developer__shell background=true`, and the job control commands | `background.rs:128` builds through the same `configure_shell_command`; `:431`, `:680`, `:766`, `:802`, `:847` strip the control commands | **nothing** — see (a) below |
| `computercontroller__automation_script` | `computercontroller/mod.rs:69`, last in `automation_script_command` | `daemon_secret_never_reaches_an_automation_script_child` (`:1731`) |
| `computercontroller__computer_control` | `platform/macos.rs:14`, `platform/windows.rs:17`, `platform/linux.rs:135`/`:140`/`:277` | `daemon_secret_never_reaches_a_computer_control_child` (`:1812`) |
| stdio MCP extensions | `extension_manager.rs:399`, last in `prepare_child_environment` | `daemon_secret_never_reaches_an_extension_child` (`:3169`) |
| Agent Drafter's `esbuild` and app-smoke children | `agent_drafter/bundle.rs:938`-adjacent, `agent_drafter/mod.rs:884` | `daemon_secret_never_reaches_the_esbuild_child` (`bundle.rs:2102`), `..._the_app_smoke_child` (`mod.rs:4343`) |

`strip_daemon_private_env` (`biorouter-sandbox/src/environment.rs:54-65`) removes both the
**inherited** keys (`env::vars_os()`) and the ones **explicitly set on the command**
(`doomed_env_keys` `:81-88`), and `is_daemon_private_env_key` (`:36-50`) is deny-by-default inside
`BIOROUTER_SERVER__`/`GOOSE_SERVER__` plus a credential-shaped net over the rest of BioRouter's
namespace. **`BIOROUTER_PORT` is deliberately kept** (`:116`'s assertion) — so a sandboxed child
knows where the daemon is and has no way to authenticate to it. **That is the whole second-order
argument, and it has three holes worth pinning.**

**(a) Nothing asserts the background job *reaches* the strip.** It does today, because
`background.rs:128` calls `configure_shell_command` — but that is a routing fact, and Task 2 point 3
made exactly this argument about the extension spawn: *"a future third spawn that builds its own
transport would leak with no test failing."* Same shape, different function.

**(b) Nothing asserts the negative half.** Every existing test proves the secret is *absent* from
the child. None proves that a client holding what the child *does* have — `BIOROUTER_PORT`, and
nothing else — is actually refused. `auth.rs:115-126` is a plain header comparison and the rate
limiter is keyed on a peer IP that is always `127.0.0.1`, so the refusal rests entirely on the header
being absent. That is true, and it is one `.layer()` away from not being true.

**(c) `GET /apps/{id}` and `GET /apps/{id}/agent` are deliberately unauthenticated**, and the
sandbox does not close them. `check_token` (`auth.rs:80-100`) exempts `/status`, `/mcp-ui-proxy`,
`/mcp-app-proxy`, and any GET matching `is_public_app_get` (`:52-78`) — which admits
`/apps/{id}`, `/apps/{id}/agent`, `/apps/{id}/models`, `/apps/{id}/runstate` and the `dist`/`assets`
trees. `serve_index` (`apps.rs:168-184`) embeds the per-app socket token in the page it serves, and
`check_ws_auth` (`:539-551`) accepts a **missing** `Origin` because non-browser clients have none.
So a loopback client that knows an app id can fetch the page, read the token, and drive that app's
agent — including any knowledge base the manifest granted it.

**Three things stand between a public-capability shell and that path, and they are worth stating
because two of them are DR-14's doing:**

1. `GET /apps` — the list — is **not** exempt (`auth.rs:71-73`'s comment says why: "an id is all
   `/apps/{id}/agent` needs"), so the id cannot be enumerated over HTTP without the secret.
2. The other place ids exist is the app tree on disk, `<config>/agent_drafter` — which is **deny
   root #4**. This is the concrete reason the Agent Drafter root is on DR-14's list and not merely
   because apps hold vault secrets.
3. The socket carries the app's own capability grants, not the session's, so it is not a general
   read of the user's data.

**The residual is real and is [AR-6](#ar-6--on-a-host-that-cannot-express-the-read-deny-a-public-session-loses-the-shell-and-two-costs-come-with-the-sandbox-itself)'s
last paragraph: a public-capability model that already knows an app id can still drive that app.**
Closing it means authenticating the app socket with something the page has and a shell does not,
which is a change to how apps are served and is out of this feature's scope.
[Open question 18](#open-questions) carries it.

**Files:**

| Action | Path | Anchor (re-verified at `9558c346`) |
|---|---|---|
| Modify | `crates/biorouter-mcp/src/developer/background.rs` | the `configure_shell_command` call `:128`; the existing `#[cfg(test)] mod tests` |
| Modify | `crates/biorouter-server/src/auth.rs` | `check_token` `:80-131`; `is_public_app_get` `:52-78`; its `mod tests` `:128-…` |
| Reference | `crates/biorouter-sandbox/src/environment.rs` | `is_daemon_private_env_key` `:36-50`, `strip_daemon_private_env` `:54-65`, `doomed_env_keys` `:81-88`, the `BIOROUTER_PORT`-is-kept assertion `:116` |
| Reference | `crates/biorouter-server/src/routes/apps.rs` | `serve_index` `:168-184`, `ws_token_for` `:513-526`, `check_ws_auth` `:539-551` |

- [ ] **Step 1: Write the failing tests**

```rust
// background.rs — (a). The routing, not the mechanism.
/// A background job's child must be built through `configure_shell_command`,
/// which is where both the strip and the DR-14 wrap live. Asserting the strip
/// again would re-test `shell.rs`; this asserts the thing that can silently
/// change — that `BackgroundJobs::spawn` (`:113`, the one entry point) still
/// goes through it.
#[test]
fn a_background_job_is_built_through_the_shared_shell_builder() {
    let src = include_str!("background.rs");
    let start = src.find("    pub async fn spawn(").expect("BackgroundJobs::spawn");
    let body = &src[start..];
    let body = &body[..body.find("\n    }\n").unwrap_or(body.len())];
    assert!(
        body.contains("configure_shell_command("),
        "a background job that builds its own Command bypasses both the daemon-secret \
         strip (issue #57) and the DR-14 read-deny sandbox, with no other test failing"
    );
    assert!(
        !body.contains("process::Command::new("),
        "found a hand-rolled Command in the job spawn path"
    );
}

// auth.rs — (b). The negative half, at the layer that decides it.
#[test]
fn the_port_alone_does_not_authenticate() {
    // Everything a DR-14-sandboxed child is left holding is BIOROUTER_PORT. This
    // pins that knowing it buys nothing: no header, a wrong header, and an empty
    // header all fail, and `/sessions/{id}/export` is not on any exempt list.
    assert!(!secret_matches("", "the-real-secret"));
    assert!(!secret_matches("the-real-secre", "the-real-secret"));
    for path in [
        "/sessions/abc/export", "/sessions/abc", "/sessions", "/agent/call_tool",
        "/config/upsert", "/knowledge/bases", "/apps",
    ] {
        assert!(
            !is_public_app_get(&Method::GET, path),
            "{path} must require the secret"
        );
    }
}

/// (c), as a pin rather than a fix: the unauthenticated app surface is EXACTLY
/// these five shapes. It is a deliberate carve-out with a comment explaining
/// itself (`auth.rs:71-73`), and this test is what makes widening it a decision
/// somebody has to make on purpose.
#[test]
fn the_unauthenticated_app_surface_does_not_grow_by_accident() {
    for allowed in ["/apps/x", "/apps/x/", "/apps/x/agent", "/apps/x/models",
                    "/apps/x/runstate", "/apps/x/dist/main.js", "/apps/x/assets/a/b.png"] {
        assert!(is_public_app_get(&Method::GET, allowed), "{allowed}");
    }
    for denied in ["/apps", "/apps/x/export", "/apps/x/source", "/apps/x/vault",
                   "/apps/../sessions", "/apps/x/agent/extra"] {
        assert!(!is_public_app_get(&Method::GET, denied), "{denied}");
    }
    // Not a GET, not exempt — `POST /apps/x/agent` must still present the secret.
    assert!(!is_public_app_get(&Method::POST, "/apps/x/agent"));
}
```

- [ ] **Step 2: Run** → **PASS on all three.**

⚠ **All three pass before Step 3, and that is the honest outcome.** They are regression pins
on behaviour that is already correct, in the same spirit as Task 2. Do not manufacture a red by
weakening the production code first, and do not "fix" the first test by loosening its assertion when
it turns out `BackgroundJobs::spawn` was renamed — resolve the real entry point and pin *that*. A
`find(..).expect(..)` that panics is a broken test, not a failing one; if Step 2 panics, the anchor
moved and the test needs the new name before anything else is believed.

- [ ] **Step 3: Implement**

Nothing to implement unless Step 2's first test fails. If it does, route the job spawn through
`configure_shell_command` — do **not** add a second `strip_daemon_private_env` call, which would
leave two spawn paths to keep in step and is the divergence this task exists to prevent.

- [ ] **Step 4: Run**

```bash
cargo test -p biorouter-mcp --lib -- developer::background
cargo test -p biorouter-server --lib auth
# The two live child-env tests this task's argument rests on. They are #[ignore]d
# halves plus their drivers; run the drivers explicitly so this task's PR shows
# them green rather than citing them.
cargo test -p biorouter-mcp --lib -- daemon_secret_never_reaches_a_shell_child \
                                     daemon_secret_never_reaches_an_automation_script_child \
                                     daemon_secret_never_reaches_a_computer_control_child
cargo test -p biorouter --lib -- daemon_secret_never_reaches_an_extension_child
```

Expected counts, asserted rather than "no failures" — all four are ordinary `#[test]`s and a libtest
filter that matches nothing prints `0 passed` and exits 0, so a renamed test reads identically to a
passing one: the third line's three names must print **3 passed**, and the fourth **1 passed**.

- [ ] **Step 5: Gate**

```bash
# (1) Every spawn family the DR-14 argument names still calls the strip.
#     TWENTY files, measured at 9558c346 (`| wc -l` -> 20). A new one appearing
#     here is a new child that may carry the daemon's credentials past the
#     sandbox; a missing one is a spawn family that stopped stripping.
grep -rln "strip_daemon_private_env" --include='*.rs' crates/ | sort
# expect exactly these 20, in this order:
#   crates/biorouter-mcp/src/agent_drafter/bundle.rs
#   crates/biorouter-mcp/src/agent_drafter/mod.rs
#   crates/biorouter-mcp/src/computercontroller/mod.rs
#   crates/biorouter-mcp/src/computercontroller/platform/linux.rs
#   crates/biorouter-mcp/src/computercontroller/platform/macos.rs
#   crates/biorouter-mcp/src/computercontroller/platform/windows.rs
#   crates/biorouter-mcp/src/developer/background.rs
#   crates/biorouter-mcp/src/developer/paths.rs
#   crates/biorouter-mcp/src/developer/rmcp_developer.rs
#   crates/biorouter-mcp/src/developer/shell.rs
#   crates/biorouter-mcp/src/knowledge/convert/pdf.rs
#   crates/biorouter-mcp/src/knowledge/source_paths.rs
#   crates/biorouter-sandbox/src/docker.rs
#   crates/biorouter-sandbox/src/environment.rs
#   crates/biorouter-sandbox/src/local.rs
#   crates/biorouter-sandbox/src/shell_sandbox/linux.rs
#   crates/biorouter/src/agents/extension_manager.rs
#   crates/biorouter/src/providers/llamacpp_sidecar.rs
#   crates/biorouter/src/subprocess.rs
#   crates/biorouter/src/system.rs
# (`agents/retry.rs`, `hooks/command_runner.rs` and the five CLI-shaped providers
#  reach it through `subprocess::prepare_agent_child_command`, which is why they
#  are not in this list — grep for that name too if auditing by hand.)

# (2) The strip is LAST in the two builders the sandbox relies on. Placed before
#     a later `.env()`, it removes a key that is then put back — which no
#     environment test catches unless it happens to name that key.
python3 - <<'PY'
for f, fn in [('crates/biorouter-mcp/src/developer/shell.rs', 'pub fn configure_shell_command'),
              ('crates/biorouter-mcp/src/computercontroller/mod.rs', 'fn automation_script_command')]:
    src = open(f).read()
    body = src[src.index(fn):]
    body = body[:body.index('\n}\n')]
    i_strip = body.rindex('strip_daemon_private_env')
    envs = [i for i in range(len(body)) if body.startswith('.env(', i) or body.startswith('cmd.env(', i)
            or body.startswith('command.env(', i)]
    late = [i for i in envs if i > i_strip]
    assert not late, f'{f}: an .env() call at offset {late} runs AFTER the strip'
    print('OK', f)
PY

# (3) BIOROUTER_PORT is still deliberately preserved. If a future hardening pass
#     strips it too, the second-order argument in this task changes shape and
#     the prose above must be rewritten rather than silently becoming stronger.
grep -c 'BIOROUTER_PORT' crates/biorouter-sandbox/src/environment.rs
echo "expect: 2 — the assertion and its expected value, in the strip test"
```

**What this catches.** A future background-job path that builds its own `Command` — the exact defect
Task 2 point 3 names one layer down, where it would hand the daemon's secret to a child the DR-14
sandbox has already confined, defeating the sandbox through the API rather than the filesystem. A
`.env()` added after the strip in either builder. And an `is_public_app_get` widened by one more
`matches!` arm, which is a one-line change that turns an unauthenticated read of an app id into an
unauthenticated read of something else.

- [ ] **Step 6: Commit**

```bash
git add crates/biorouter-mcp/src/developer/background.rs crates/biorouter-server/src/auth.rs
git commit -m "test(privacy): pin the daemon-secret strip on every spawn the read-deny relies on (#56)"
```

---


### Task 15: Gate C's siblings — the eight other ways to reach an MCP server

`dispatch_tool_call` is a complete choke point for *tool calls*, not for *reaching an MCP server*.
Eight sibling entry points, three of which fan out over **every** installed extension.

**Files:**

| Action | Path | Anchor (re-verified at `9558c346`) |
|---|---|---|
| Modify | `crates/biorouter/src/agents/extension_manager.rs` | `read_resource_tool` `:1193` (loops every extension when `extension_name` is absent, `:1224-1243`, with `Err(_) => continue` at `:1240`); `read_resource` `:1266`; `get_ui_resources` `:1303` (loops all, `:1306-1332`); `list_resources_from_extension` **`:1337`** — *not named in the design*; `list_resources` `:1376`; `list_prompts_from_extension` `:1578`; `list_prompts` `:1608` (`FuturesUnordered` over every extension, `:1614-1624`); `get_prompt` `:1655` |

⚠ **`search_available_extensions` (`:1674`) is deliberately NOT a sibling.** It reads
`config_disabled_extension_lines(&get_all_extensions(), …)` and the manager's own key set — it never
contacts a server and returns no server-authored content. It does reveal that a private extension is
*installed*, which is an existence leak and explicitly out of scope (DR-7). Leave it alone, and say
so, so a later reviewer does not read the omission as a miss.

- [ ] **Step 1: Write the failing test — parameterised over all eight**

```rust
#[tokio::test]
async fn no_sibling_entry_point_reaches_a_private_extension_under_a_public_model() {
    let em = manager_with(private_ext("ucsfomopagent"), public_ext("developer")).await;
    bind_public_provider(&em).await;

    for probe in SIBLING_PROBES {          // eight closures, one per entry point
        let outcome = (probe.run)(&em).await;
        assert!(!outcome.contacted_private, "{} contacted the private server", probe.name);
        assert!(!outcome.leaked_private_names, "{} leaked its names", probe.name);
    }
}

#[tokio::test]
async fn the_resource_fanout_still_serves_the_public_extension() {
    // read_resource_tool with NO extension_name probes every extension in turn
    // and swallows failures (`Err(_) => continue` at :1240). If the guard is a
    // single up-front check the whole call fails; if it is inside the loop the
    // public server still answers. Assert on a CALL COUNTER inside a stub
    // client, not on the returned text: in the buggy case where the private
    // server was contacted first and its error swallowed, the text is identical.
    let em = manager_with(private_ext_serving("res://x"), public_ext_serving("res://x")).await;
    bind_public_provider(&em).await;
    let out = em.read_resource_tool(json!({ "uri": "res://x" })).await.unwrap();
    assert!(out.contains("from the public server"));
    assert_eq!(private_stub_call_count(), 0);
}

#[tokio::test]
async fn get_prompt_refuses_without_echoing_the_prompt_body() {
    // An MCP prompt body is server-authored text that lands in the transcript.
    let em = manager_with(private_ext_with_prompt("cohort", "SENTINEL-PROMPT-BODY")).await;
    bind_public_provider(&em).await;
    let err = em.get_prompt("ucsfomopagent", "cohort", None).await.unwrap_err();
    assert!(!format!("{err:?}").contains("SENTINEL-PROMPT-BODY"));
}
```

- [ ] **Step 2: Run** → **FAIL** on all eight probes.

- [ ] **Step 3: Implement** — one shared helper, eight call sites:

```rust
    /// Gate C's predicate, for the entry points that reach an MCP server
    /// without being a tool call. `Err` is the refusal; `Ok(())` permits.
    async fn assert_extension_reachable(&self, name: &str) -> Result<(), ErrorData> {
        let tier = self.extensions.lock().await.get(name).map(|e| e.tier)
            .unwrap_or(ProviderTier::Private);
        match crate::privacy::refusal::privacy_refusal(name, tier, self.capability_tier().await) {
            Some(e) if privacy_tiers_enabled() => Err(e),
            _ => Ok(()),
        }
    }
```

For the three fan-out sites (`read_resource_tool` when `extension_name` is `None`,
`get_ui_resources`, `list_prompts`) the call goes **inside the per-extension loop**, so a private
server is skipped and the public ones still answer. For the five targeted sites it goes immediately
after the name is known and before any client call.

- [ ] **Step 4: Run** → `cargo test -p biorouter --lib agents::extension_manager` → **PASS**.

- [ ] **Step 5: Gate**

```bash
# All eight, and no direct client reach beside them.
grep -c "assert_extension_reachable(" crates/biorouter/src/agents/extension_manager.rs
echo "expect: 9 = 1 definition + 8 call sites (read_resource_tool, read_resource,"
echo "         get_ui_resources, list_resources_from_extension, list_resources,"
echo "         list_prompts_from_extension, list_prompts, get_prompt)"
# ...and one hit per function, so the count above cannot be reached by putting
# two guards in one place and none in another.
#
# ⚠ TWO anchoring traps, both measured, both of which made the first version of
# this loop unreadable. (a) `/pub async fn list_prompts/` without the paren also
# matches `list_prompts_from_extension` (:1578); awk then re-triggers and
# concatenates BOTH functions into one 75-line range instead of list_prompts'
# real 46. Same for `read_resource` vs `read_resource_tool`. (b) dropping `pub `
# is worse: the trait stubs inside `#[cfg(test)]` (:1885 :1893 :1949 :1957 :1985
# :1993 :2029 :2037) are bare `async fn`, so `/async fn read_resource\(/` spans
# 170 lines across three definitions. Only `pub async fn <name>(` is one
# function — measured spans: 72, 36, 33, 61, 29, 46, 18.
for fn in read_resource_tool read_resource get_ui_resources list_resources \
          list_prompts_from_extension list_prompts get_prompt; do
  echo -n "$fn: "
  awk "/pub async fn $fn\(/,/^    }/" crates/biorouter/src/agents/extension_manager.rs \
    | grep -c "assert_extension_reachable("
done
# `list_resources_from_extension` is the one private helper (`async fn`, :1337,
# 38 lines) and its name is unique, so it takes the un-prefixed pattern:
echo -n "list_resources_from_extension: "
awk '/async fn list_resources_from_extension\(/,/^    }/' \
  crates/biorouter/src/agents/extension_manager.rs | grep -c "assert_extension_reachable("
echo "expect: 1 each, eight times — and 1+8 = the 9 above."
# The three fan-out sites guard INSIDE their loop, not before it. Each fan-out
# has ONE loop header and it is matched by name, because a bare `for ` also
# matches inner loops and prose: read_resource_tool's first `for ` is
# `for content in read_result.contents` at rel 17 — inside the EXPLICIT-name
# branch, not the fan-out — and its rel 26 is a COMMENT containing "for the
# resource". A `| head -3` over `for \|FuturesUnordered\|assert_...` therefore
# printed rel 17, 26, 34 and never reached the guard at all: uninterpretable,
# green or red at random.
echo -n "read_resource_tool: "
awk '/pub async fn read_resource_tool\(/,/^    }/' crates/biorouter/src/agents/extension_manager.rs \
  | grep -n "for extension_name in extension_names\|assert_extension_reachable"
echo -n "get_ui_resources: "
awk '/pub async fn get_ui_resources\(/,/^    }/' crates/biorouter/src/agents/extension_manager.rs \
  | grep -n "for (extension_name, client) in\|assert_extension_reachable"
echo -n "list_prompts: "
awk '/pub async fn list_prompts\(/,/^    }/' crates/biorouter/src/agents/extension_manager.rs \
  | grep -n "FuturesUnordered::new()\|assert_extension_reachable"
# Expected for each: exactly TWO lines, the loop header on the SMALLER one.
# All three loop headers verified present today at :1226, :1314 and :1612.
```

**What this catches.** Naming only `dispatch_tool_call` leaves eight live doors, of which
`read_resource_tool` is the worst: with no `extension_name` it actively probes private servers on
the model's behalf. And the naive fix — one guard at the top of each function — turns the three
fan-out sites into all-or-nothing, so a public model with one private extension installed loses
resource reads entirely. The paired `awk` gate distinguishes the two placements, which a
`grep -c` cannot.

- [ ] **Step 6: Commit**

```bash
git add crates/biorouter/src/agents/extension_manager.rs
git commit -m "feat(privacy): Gate C siblings - resources, prompts and the three fan-out probes (#56)"
```

---

### Task 16: Gate E — discovery, and the one prefix resolver

Not a veto: the reason a public model never sees a private server's tool names, descriptions or JSON
schemas in its system prompt. Schema text is content.

**Files:**

| Action | Path | Anchor (re-verified at `9558c346`) |
|---|---|---|
| Modify | `crates/biorouter/src/agents/extension_manager.rs` | `filter_tools` `:1027-1051` — a **sync `fn`** over `&[Tool]`, prefix derived at `:1036` as `tool.name.as_ref().split("__").next().unwrap_or("")`; callers `get_prefixed_tools` `:1014` and `get_prefixed_tools_excluding` `:1022`; the cache `get_all_tools_cached` `:1054` and `fetch_all_tools` `:1090`; `get_client_for_tool` `:1183` (`starts_with` over a `HashMap`) |
| Reference | `crates/biorouter/src/config/extensions.rs` | `name_to_key` `:23` — strips whitespace, lowercases, **preserves `_`** |
| Reference | `crates/biorouter/src/agents/code_execution_extension.rs` | `get_prefixed_tools_excluding(EXTENSION_NAME)` at `:1434` — the importable-module catalogue comes free |

- [ ] **Step 1: Write the failing tests**

```rust
#[tokio::test]
async fn the_allowed_set_follows_a_mid_session_model_swap() {
    // O6. `get_all_tools_cached` is guarded by `tools_cache_version`, bumped
    // only by extension add/remove — never by update_provider. Filtering
    // upstream of `filter_tools` freezes one model's allowed set. This is the
    // assertion a cache-level implementation fails.
    let em = manager_with(private_ext("ucsfomopagent"), public_ext("developer")).await;
    bind_private_provider(&em).await;
    let before = em.get_prefixed_tools(None).await.unwrap();
    assert!(before.iter().any(|t| t.name.starts_with("ucsfomopagent__")));

    swap_provider(&em, public_provider()).await;      // NO extension change
    let after = em.get_prefixed_tools(None).await.unwrap();
    assert!(!after.iter().any(|t| t.name.starts_with("ucsfomopagent__")));
    assert!(after.iter().any(|t| t.name.starts_with("developer__")));
    assert_ne!(before.len(), after.len());
}

#[tokio::test]
async fn an_embedded_double_underscore_cannot_smuggle_a_private_tool_into_the_list() {
    // `filter_tools` computes the prefix as split("__").next(); the dispatcher
    // resolves by `starts_with` over a HashMap with per-process-randomised
    // iteration order. `name_to_key` preserves `_`, so an extension whose
    // manifest.name contains `__` keeps it in the map key — reachable by
    // hand-installing a .brxt, which records no provenance at all
    // (BrxtInstallModal.tsx:152-161). With keys `a` (public) and `a__b`
    // (private), the tool `a__b__t` computes prefix `a` and would be ALLOWED,
    // exposing the private server's tool names, descriptions and JSON schemas.
    let em = manager_with(public_ext("a"), private_ext("a__b")).await;
    bind_public_provider(&em).await;
    let tools = em.get_prefixed_tools(None).await.unwrap();
    assert!(!tools.iter().any(|t| t.name.as_ref().starts_with("a__b__")),
            "leaked: {:?}", tools.iter().map(|t| t.name.as_ref()).collect::<Vec<_>>());
}
```

- [ ] **Step 2: Run** → test 1 **FAIL**, test 2 **FAIL**.

- [ ] **Step 3: Implement**

`filter_tools` is sync over `&[Tool]` and cannot `await` the extensions mutex, so the allowed set is
**precomputed by the async caller** and passed in. Both it and `get_client_for_tool` derive their
key from **one** resolver:

```rust
    /// The single prefix→extension resolver. Gate C (`get_client_for_tool`) and
    /// Gate E (`filter_tools`) MUST agree, or a tool is hidden by one and
    /// dispatched by the other. Longest-key-wins, so `a__b` beats `a` for
    /// `a__b__t`, and `HashMap` iteration order stops mattering.
    fn resolve_extension_key<'k>(keys: &'k [String], prefixed_name: &str) -> Option<&'k str> {
        keys.iter()
            .filter(|k| prefixed_name.starts_with(k.as_str())
                     && prefixed_name[k.len()..].starts_with("__"))
            .max_by_key(|k| k.len())
            .map(String::as_str)
    }
```

```rust
    pub async fn get_prefixed_tools(
        &self,
        extension_name: Option<String>,
    ) -> ExtensionResult<Vec<Tool>> {
        let all_tools = self.get_all_tools_cached().await?;
        // Issue #56 Gate E. Precomputed here, in the async caller: filter_tools
        // is sync and must not become async (the cache above it is keyed on a
        // version counter update_provider never bumps).
        let allowed = self.allowed_extension_keys().await;
        Ok(self.filter_tools(&all_tools, extension_name.as_deref(), None, &allowed))
    }
```

```rust
    /// The extension keys the currently-bound model may see.
    ///
    /// The predicate is Gate C's, verbatim — `privacy_refusal(..).is_none()` —
    /// and NOT `visible_to(caller, floor(ext.tier))`. Two reasons, and the
    /// second is the load-bearing one:
    ///
    ///  * Comparing a caller's capability with an extension's tier is a
    ///    ProviderTier-to-ProviderTier question. `floor` crosses a CAPABILITY
    ///    into a CLASSIFICATION, which is a different question; reaching for it
    ///    here is how the first version of this plan ended up with four
    ///    crossings where its own audit test asserted two.
    ///  * Gate C (dispatch) and Gate E (discovery) must agree on every input, or
    ///    a tool is hidden by one and dispatched by the other. Sharing one
    ///    function is the only way to guarantee that; sharing a *rule* is not.
    async fn allowed_extension_keys(&self) -> Vec<String> {
        let caller = self.capability_tier().await;
        let enforce = privacy_tiers_enabled();
        self.extensions
            .lock()
            .await
            .iter()
            .filter(|(k, e)| {
                !enforce
                    || crate::privacy::refusal::privacy_refusal(k, e.tier, caller).is_none()
            })
            .map(|(k, _)| k.clone())
            .collect()
    }
```

⚠ **`floor_ext` does not exist** — not in this plan, not in the tree, and it appeared exactly once,
in the first version of this code block. It was an invented name for the crossing above, and writing
it as `floor(..)` instead would have added a third and fourth `floor` caller that Task 7's audit test
does not expect. Use the shared refusal predicate.

and in `filter_tools`, replace the `split("__")` prefix derivation at `:1036` with
`Self::resolve_extension_key(allowed, tool.name.as_ref())`, dropping any tool whose key does not
resolve inside the allowed set.

⚠ `Agent::list_tools` (`agent.rs:3113`) appends the platform tools **after** `get_prefixed_tools`
returns, at `:3124`, `:3131`, `:3140`, so `filter_tools` cannot hide `platform__manage_schedule`,
`platform__ingest_conversation` or `platform__read_session_blob`. That is correct — they are public
and Task 11 gates the one that reads across sessions — but write it down so no one "fixes" it.

⚠ `GET /agent/tools` (`routes/agent.rs:573`) has an **empty-`session_id` branch** at `:589` that
resolves an extension straight out of `get_all_extensions()`. That is the Settings list, where a
private extension must stay **visible and badged**. Do **not** tier-filter it.

- [ ] **Step 4: Run**

```bash
cargo test -p biorouter --lib agents::extension_manager
cargo test -p biorouter --lib agents::code_execution_extension
```

- [ ] **Step 5: Gate**

```bash
# The filter is in filter_tools and nowhere upstream.
awk '/async fn get_all_tools_cached/,/^    }/' crates/biorouter/src/agents/extension_manager.rs \
  | grep -c "capability_tier\|allowed_extension_keys" ; echo "expect: 0"
awk '/async fn fetch_all_tools/,/^    }/' crates/biorouter/src/agents/extension_manager.rs \
  | grep -c "capability_tier\|allowed_extension_keys" ; echo "expect: 0"
# Both gates derive their key from ONE resolver, and the old split() rule is gone.
grep -c "resolve_extension_key" crates/biorouter/src/agents/extension_manager.rs ; echo "expect: 3 (1 def + filter_tools + get_client_for_tool)"
grep -c 'split("__").next()' crates/biorouter/src/agents/extension_manager.rs ; echo "expect: 0"
```

**What this catches.** Two wrong implementations. (1) Filtering in `get_all_tools_cached`, which is
the natural place (it is where the list is built) and which freezes the allowed set across a
mid-session swap — test 1 is the only thing that fails it, and it is why the test changes the
provider *without touching the extension set*. (2) Leaving the two prefix rules disagreeing: with
keys `a` and `a__b`, Gate E allows `a__b__t` while Gate C resolves it nondeterministically. A
single-extension fixture cannot catch it; test 2 constructs exactly that pair.

- [ ] **Step 6: Commit**

```bash
git add crates/biorouter/src/agents/extension_manager.rs
git commit -m "feat(privacy): Gate E - hide private tools from a public model, on one prefix resolver (#56)"
```

---

### Task 17: Gate D — `chatrecall` SEARCH, both builders

**Files:**

| Action | Path | Anchor (re-verified at `9558c346`) |
|---|---|---|
| Modify | `crates/biorouter/src/session/chat_history_search.rs` | struct `:49-56`; `new` `:59-74` (six params); `execute` `:77` branching on `fts_available()` `:108`; `fetch_rows_fts` `:122` (join `:135`, optional clauses `:140-148`, `ORDER BY … LIMIT ?` `:150`, positional binds `:152-163`); `build_sql` `:199` (join `:211`, optional clauses `:233-242`, `ORDER BY … LIMIT ?` `:244`); `fetch_rows_like` `:168-187`; `process_rows` `:253`; `get_session_totals` `:293`; `convert_to_results` `:322` |
| Modify | `crates/biorouter/src/session/session_manager.rs` | `SessionStorage::search_chat_history` `:5122` (constructs at `:5133`); `SessionManager::search_chat_history` `:1740` |
| Modify | `crates/biorouter/src/agents/chatrecall_extension.rs` | the SEARCH call at `:190-199` |

- [ ] **Step 1: Write the failing tests**

```rust
#[tokio::test]
async fn both_query_paths_filter_private_rows() {
    for path in [QueryPath::Fts, QueryPath::LikeFallback] {   // drop messages_fts to force LIKE
        let db = seeded(path, &[private_chat_containing("cohort"), public_chat_containing("cohort")]).await;
        assert_eq!(search_as(ProviderTier::Public, &db, "cohort").await.results.len(), 1);
        assert_eq!(search_as(ProviderTier::Private, &db, "cohort").await.results.len(), 2);
    }
}

#[tokio::test]
async fn the_limit_is_applied_after_the_filter_not_before() {
    // THE test. 10 private rows ranking above 3 public ones with limit=5: a
    // public caller must get all 3 public rows, not 0. A Rust-side post-filter
    // — the obvious implementation, and the one that needs no SQL change —
    // returns 0 here, silently and non-deterministically, with no error.
    // SQLite applies LIMIT ? at :150 / :244.
    for path in [QueryPath::Fts, QueryPath::LikeFallback] {
        let db = seeded(path, &vec_of(10, private_chat_ranking_high("cohort"))
                              .chain(vec_of(3, public_chat_ranking_low("cohort")))).await;
        let r = search_with_limit(ProviderTier::Public, &db, "cohort", 5).await;
        assert_eq!(r.results.len(), 3, "post-filtered in Rust instead of in SQL");
    }
}

#[tokio::test]
async fn no_content_field_of_a_private_row_survives() {
    // §11.4: session_description is the LLM-generated title, produced FROM the
    // conversation, and is the field most likely to be mislabelled as metadata.
    let db = seeded(QueryPath::Fts, &[private_chat_titled("PHI cohort characterisation",
                                                          "/data/phi/x", "cohort")]).await;
    let r = search_as(ProviderTier::Public, &db, "cohort").await;
    let rendered = render_for_model(&r);
    for leak in ["PHI cohort characterisation", "/data/phi", "cohort characterisation"] {
        assert!(!rendered.contains(leak), "{leak} survived: {rendered}");
    }
}
```

- [ ] **Step 2: Run** → **COMPILE ERROR** on the `search_as` helper's tier argument
      (`ChatHistorySearch::new` takes six parameters).

- [ ] **Step 3: Implement**

`ChatHistorySearch::new` takes `caller_capability: ProviderTier` as a **required 7th parameter** —
not a builder setter, not an `Option` — so the three call sites become compile errors. Then, in
each builder, one **literal** immediately before the `ORDER BY … LIMIT ?` push:

```rust
        // Issue #56 Gate D. `sessions s` is already joined (:135 / :211), so
        // this is one clause each. A SQL LITERAL, never a `?`: both builders
        // bind strictly positionally (:152-163 / :170-187) with optionals in a
        // fixed order, so an inserted placeholder shifts every later ordinal
        // and mis-binds SILENTLY — no error, wrong results. The literal is a
        // compile-time constant of the code path, not user input.
        if self.caller_capability == ProviderTier::Public {
            sql.push_str(" AND s.privacy_tier = 'public'");
        }

        sql.push_str(" ORDER BY bm25(messages_fts) ASC LIMIT ?");
```

Nothing downstream changes: `process_rows` (`:253`), `get_session_totals` (`:293`, which counts only
ids already in the filtered map at `:304-309`) and `convert_to_results` (`:322`) are tier-blind, and
`total_matches` (`:365`) is summed after filtering.

The tier is resolved with no new plumbing: `PlatformExtensionContext` (`extension.rs:109-113`)
carries `extension_manager: Option<Weak<ExtensionManager>>`, populated at `extension_manager.rs:799`,
and Task 10 added `capability_tier()`. **`caller_is_public` is the live provider's tier, not the
caller session's stored classification** — the question is who is about to read this, and the reader
is the model. It also means a session in the residual state reads as a public caller, which is the
safe direction.

- [ ] **Step 4: Run**

```bash
cargo test -p biorouter --lib -- session::chat_history_search agents::chatrecall_extension
cargo test -p biorouter --lib session::session_manager
```

⚠ **Both filters in the first command have a pre-count of zero.** Neither
`crates/biorouter/src/session/chat_history_search.rs` nor
`crates/biorouter/src/agents/chatrecall_extension.rs` has a `#[cfg(test)] mod tests` on `main`;
Task 10 creates the second one. So `0 passed, exits 0` is the correct baseline here and a
**meaningless** pass afterwards. Assert `3` new tests in `chat_history_search` and the exact
`chatrecall_extension` delta, and paste both numbers into the PR.

- [ ] **Step 5: Gate**

```bash
# BOTH builders, and the count is exact — `>= 1` passes an implementation that
# filters only the FTS path, which leaks on any un-migrated profile
# (`execute` branches on a sqlite_master probe at :108).
grep -c "s.privacy_tier = 'public'" crates/biorouter/src/session/chat_history_search.rs
echo "expect: 2"
# It is a literal, not a bind.
grep -c "privacy_tier = ?" crates/biorouter/src/session/chat_history_search.rs ; echo "expect: 0"
# The parameter is required, so a missed call site is a compile error.
grep -c "caller_capability: Option" crates/biorouter/src/session/chat_history_search.rs ; echo "expect: 0"
```

**What this catches.** The wrong implementation post-filters in Rust after `execute()` returns —
which needs no SQL change, passes any test that has no `LIMIT` pressure, and silently returns fewer
public hits than exist whenever private rows outrank them. Test 2 is the only thing that fails it,
and it is why the fixture stacks 10 private rows above 3 public ones with `limit = 5`. The
FTS-only variant is caught by running the same fixture with `messages_fts` dropped.

- [ ] **Step 6: Commit**

```bash
git add crates/biorouter/src/session/chat_history_search.rs \
        crates/biorouter/src/session/session_manager.rs \
        crates/biorouter/src/agents/chatrecall_extension.rs
git commit -m "feat(privacy): Gate D - filter chatrecall in SQL, in both query builders (#56)"
```

---

### Task 18: Gate F — the two extension channels that are not tool calls

Neither is in the design. Both are one-line fixes and both are live today.

**F1 — a public model can *enable* a private extension.** `extensionmanager__manage_extensions`
(declared `extension_manager_extension.rs:342`, handler `:190`, impl `:212`) calls
`get_extension_entry_by_name` → `check_enable_allowed` (`:97-125`) → `add_extension` (`:249`).
Nothing in that chain reads a tier, so a public session can spawn `ucsfomopagent`'s process —
pulling `CLINICAL_RECORDS_*` from the keychain and opening a session to the UCSF CDW — and only
then be refused at the tool call.

**F2 — a private server's own instructions reach a public model's system prompt.**
`get_extensions_info` (`extension_manager.rs:960`) maps every extension to
`ExtensionInfo::new(name, ext.get_instructions(), ..)`, consumed at `reply_parts.rs:156` →
`prompt_manager.builder().with_extensions(..)` → the system prompt for **every turn**. Gate E
filters `filter_tools`, a different function on a different path. For a clinical connector that
instruction text describes table names, cohort semantics and credential scope.

**Files:**

| Action | Path | Anchor (re-verified at `9558c346`) |
|---|---|---|
| Modify | `crates/biorouter/src/agents/extension_manager_extension.rs` | `check_enable_allowed` `:97-125` (a **pure** fn, single call site at `:247`), its four tests at `:538-576`; `MANAGE_EXTENSIONS_TOOL_NAME` `:75`; declaration `:342`; dispatch `:467` |
| Modify | `crates/biorouter/src/agents/extension_manager.rs` | `get_extensions_info` `:960`, the `.map` at `:963` |
| Reference | `crates/biorouter/src/agents/reply_parts.rs` | `:156` — the system-prompt consumer |
| Reference | `crates/biorouter/src/agents/agent.rs` | `:5777` — the workflow-creation consumer |

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn a_public_caller_may_not_enable_a_private_extension() {
    use ProviderTier::{Private, Public};
    // Pure, like its four siblings at :538-576, so it needs no global config.
    let e = check_enable_allowed(Some(entry_for("ucsfomopagent")), false, "ucsfomopagent", Public)
        .unwrap_err();
    assert!(e.message.contains("marketplace"));
    assert!(e.message.contains("private"));
    check_enable_allowed(Some(entry_for("ucsfomopagent")), false, "ucsfomopagent", Private)
        .expect("a private caller may enable it");
    check_enable_allowed(Some(entry_for("developer")), false, "developer", Public)
        .expect("public extensions are unaffected");
}

#[tokio::test]
async fn a_private_servers_instructions_do_not_reach_a_public_system_prompt() {
    // Assert on the RENDERED SYSTEM PROMPT via the real reply_parts path.
    // Asserting on the tool list instead is the wrong-implementation trap:
    // Gate E already hides the tools and the instructions still ship.
    let em = manager_with(private_ext_instructed("ucsfomopagent", "SENTINEL-INSTRUCTIONS")).await;
    bind_public_provider(&em).await;
    assert!(!build_system_prompt(&em).await.contains("SENTINEL-INSTRUCTIONS"));
    bind_private_provider(&em).await;
    assert!(build_system_prompt(&em).await.contains("SENTINEL-INSTRUCTIONS"));
}
```

- [ ] **Step 2: Run** → test 1 **COMPILE ERROR** (`check_enable_allowed` takes three arguments);
      test 2 **FAIL**.

- [ ] **Step 3: Implement**

Add a 4th parameter to `check_enable_allowed`, so the single call site at `:247` is a compile error,
and a new arm **before** `Some(entry) => Ok(entry.config)` at `:123`:

```rust
        Some(entry) if crate::privacy::classify_extension(extension_name).is_private()
                    && caller == ProviderTier::Public => Err(ErrorData::new(
            ErrorCode::INVALID_REQUEST,
            format!(
                "Extension '{extension_name}' is a private extension: the Biorouter marketplace \
                 marks it as reaching data held inside the institution, so only a private model \
                 may enable or call it. This session is running on a public model, so do not \
                 enable it. If it is needed for this task, ask the user to switch this chat to a \
                 private model first — in the desktop app under Settings > Models, or with the \
                 model chip in the composer."
            ),
            None,
        )),
```

and in `get_extensions_info`'s `.map` at `:963`, a `.filter` on the same predicate.

- [ ] **Step 4: Run**

```bash
cargo test -p biorouter --lib -- agents::extension_manager_extension agents::extension_manager agents::reply_parts
```

- [ ] **Step 5: Gate**

```bash
# check_enable_allowed still has exactly ONE production call site (so the
# compile error was fixed there rather than the parameter defaulted away).
grep -n "check_enable_allowed(" crates/biorouter/src/agents/extension_manager_extension.rs
# expect: 9 lines total = 1 definition (:97) + 1 production call site (inside
#         manage_extensions_impl, :247 today) + 4 pre-existing tests (:538,
#         :548, :570, :576) + 3 new test calls.
# The load-bearing one is the production call site: it must pass a RESOLVED
# tier, not a literal. Anchored on the ENCLOSING FUNCTION, never on a line
# number — this task's own implementation inserts a ~13-line match arm into
# `check_enable_allowed` (:97-125), which is ABOVE :247, so `awk 'NR==247'`
# reads unrelated code after the edit and passes vacuously. That is the
# "assertion true for all inputs" pattern, and it was in the first version of
# this gate.
awk '/async fn manage_extensions_impl/,/^    }/' \
  crates/biorouter/src/agents/extension_manager_extension.rs \
  | grep -c "check_enable_allowed(" ; echo "expect: 1"
awk '/async fn manage_extensions_impl/,/^    }/' \
  crates/biorouter/src/agents/extension_manager_extension.rs \
  | grep "check_enable_allowed(" | grep -c "ProviderTier::"
echo "expect: 0 — the call site passes a resolved tier, not a hardcoded one"
grep -c "caller: ProviderTier" crates/biorouter/src/agents/extension_manager_extension.rs ; echo "expect: 1"
# The instructions filter is on get_extensions_info, not somewhere downstream.
awk '/pub async fn get_extensions_info/,/^    }/' crates/biorouter/src/agents/extension_manager.rs \
  | grep -c "visible_to\|is_private" ; echo "expect: 1"
```

**What this catches.** For F1, a wrong implementation that adds the refusal to
`manage_extensions_impl` after `check_enable_allowed` returns — which works, but leaves the
predicate untestable without a live extension registry and diverges from the issue-#42 gate it sits
beside. Making the parameter required is what forces it into the pure function. For F2, an
implementation that filters the **tool list** and calls it done: test 2 asserts on the rendered
system prompt, which is the only place the leak appears.

- [ ] **Step 6: Commit**

```bash
git add crates/biorouter/src/agents/extension_manager_extension.rs \
        crates/biorouter/src/agents/extension_manager.rs
git commit -m "feat(privacy): Gate F - refuse enabling a private extension and hide its instructions (#56)"
```

---

### Task 19: Gate H — the three alternate-provider construction sites, and memory's global write

Three verified paths hand a **session's** content to a provider the session row never records, none
of which touches `Agent::update_provider` or `Agent::reply`. Gates A–F are all blind to them.

⚠ **The fourth site the first version of this plan listed — `routes/knowledge.rs:910` — moved to
Task 10C, and it had to.** `assert_alt_provider_allowed` compares a provider's tier with a
**`SessionClassification`**, and `build_completer` (`:899-914`) has no session: `POST
/knowledge/bases/{id}/ingest`, `/query` and `/lint` carry a KB id and a `ModelRef` and nothing else.
The check that belongs there is the KB-keyed one, which is what Task 10C installs. The one macro
route that *does* have sessions, `/ingest-conversation`, is Task 11's. Listing it here was a latent
type error, not just a misfiling.

**Files:**

| Action | Path | Anchor (re-verified at `9558c346`) |
|---|---|---|
| Create | `crates/biorouter/src/privacy/alt_provider.rs` | the one shared helper |
| Modify | `crates/biorouter-cli/src/session/mod.rs` | `get_reasoner` `:2264-2295` (`BIOROUTER_PLANNER_PROVIDER` at `:2271`, the global fallback warning at `:2274`); called at `:789` and `:884` with `self.messages.clone()`; consumed by `plan_with_reasoner_model` `:980-989` at `reasoner.complete(..)` |
| Modify | `crates/biorouter/src/hooks/mod.rs` | `resolve_prompt_provider` `:690-726`, invoked at `:657-681` |
| Reference | `crates/biorouter/src/hooks/prompt_runner.rs` | `run_prompt_hook` `:45-58`, `complete_fast` at `:57` |
| Reference | `crates/biorouter/src/agents/agent.rs` | the Stop hook's payload — `transcript_tail(&conversation)` at `:5495-5496` |
| Modify | `crates/biorouter/src/agents/knowledge_tool.rs` | `build_model_ref_completer` `:183-193`; `should_use_knowledge_default_model` `:179-181` |
| Modify | `crates/biorouter-mcp/src/memory/mod.rs` | `remember_memory` `:491-540` (the `is_global` disclosure copy it already emits at `:523-548` is the precedent to extend); `is_global` field `:30`; `tags` `:26-28`; `remember` `:374`/`:387-388`; `retrieve_all` `:346`; `compose_instructions` `:277` with `GLOBAL_INDEX_HEADER` `:89-94` (emitted `:302-307`) and `LOCAL_SECTION_HEADER` `:97` (bodies inlined **in full** `:310-322`); the #58 doc comment `:245-273` |
| ~~Modify~~ | ~~`crates/biorouter-server/src/routes/knowledge.rs:910`~~ | **moved to Task 10C** — see the ⚠ above |

- [ ] **Step 1: Write the failing tests**

```rust
#[tokio::test]
async fn cli_plan_mode_refuses_to_ship_a_private_transcript_elsewhere() {
    // A documented first-class feature and a complete private->public transcript
    // leak: :789/:884 clone the WHOLE message list and hand it to a provider
    // built from BIOROUTER_PLANNER_PROVIDER (or the global default).
    let sess = cli_session_marked_private().await;
    let err = sess.plan("summarise").await.unwrap_err();
    assert!(err.to_string().contains("BIOROUTER_PLANNER_PROVIDER"));
    assert_eq!(planner_completion_count(), 0);
}

#[tokio::test]
async fn a_prompt_hook_on_a_public_provider_is_skipped_for_a_private_session() {
    // agent.rs:5495-5496 fires the Stop hook with transcript_tail(&conversation)
    // at the end of EVERY turn. Global hooks load from config.yaml
    // unconditionally; project hooks from .biorouter/hooks.yaml when
    // allow_project_hooks is set (the field is hooks/config.rs:88, parsed at
    // :115-116; its default of FALSE is asserted at :240).
    let s = private_session().await;
    run_stop_hook_with_prompt_provider(&s, "anthropic").await;
    assert_eq!(hook_provider_completion_count(), 0);
    assert!(last_warning().contains("private"));
}

#[tokio::test]
async fn the_knowledge_default_model_obeys_the_barrier() {
    let s = private_scheduled_session().await;
    let err = ingest_with_kb_manifest_model(&s, ModelRef::new("anthropic", "claude")).await.unwrap_err();
    assert!(err.to_string().contains("private"));
}

#[tokio::test]
async fn a_private_session_may_not_write_a_global_memory() {
    // Issue #63's residual, mirrored. Global memories are index-only since #58
    // (mod.rs:302-307), but retrieve_memories(category="*", is_global=true) at
    // :542 is a TOOL CALL ON A PUBLIC BUILT-IN, so Gate C (both ends public)
    // and Gate E (the tool is legitimately listed) both miss it, and Auto mode
    // auto-approves. Refusing the WRITE from a private-capability session needs
    // no storage change and is the exact mirror of Gate C.
    let out = remember_memory_as(ProviderTier::Private, json!({
        "category": "cohorts", "data": "n=412 T2D patients", "is_global": true })).await;
    assert!(out.is_err());
    assert!(remember_memory_as(ProviderTier::Private, json!({
        "category": "cohorts", "data": "n=412", "is_global": false })).await.is_ok());
}

#[tokio::test]
async fn a_private_local_memory_write_says_who_will_be_able_to_read_it() {
    // AR-3, the half that is affordable in v1. The LOCAL store is NOT gated:
    // `compose_instructions` (mod.rs:277) inlines local memories in full at
    // :310-322 into the system prompt of every session opened in that directory,
    // including one on a public model. Gate F2 cannot help — it filters by
    // EXTENSION tier and `memory` is Public.
    //
    // Closing it properly needs provenance per stored memory, and the on-disk
    // format is a `# {tags}` line plus bare lines (:387-388, read back at
    // :414-418 keyed by the TAG STRING, not the category), while
    // `compose_instructions` runs once at MemoryServer::new (:108) rather than
    // per turn — so a capability-aware filter there would also freeze across a
    // mid-session model swap, which is the exact O6 hazard Gate E exists to
    // avoid. Open question 14 carries the real fix.
    //
    // What v1 ships is the disclosure, extending the copy `remember_memory`
    // already emits for `is_global` (:523-548): the model is told, in the
    // transcript the user can read, who will be able to read this note.
    let out = remember_memory_as(ProviderTier::Private, json!({
        "category": "cohorts", "data": "n=412", "is_global": false })).await.unwrap();
    assert!(out.contains("any session opened in this directory"), "{out}");
    assert!(out.contains("including one on a public model"), "{out}");

    // And the public-capability write keeps the shorter, existing copy.
    let pubout = remember_memory_as(ProviderTier::Public, json!({
        "category": "notes", "data": "x", "is_global": false })).await.unwrap();
    assert!(!pubout.contains("including one on a public model"), "{pubout}");
}
```

- [ ] **Step 2: Run** → all five **FAIL**.

- [ ] **Step 3: Implement** — one helper, three call sites:

```rust
// crates/biorouter/src/privacy/alt_provider.rs

/// Refuse to hand a session's content to a provider that is not the one bound
/// to it. Three production paths build such a provider and none of them passes
/// `Agent::update_provider` or `Agent::reply`, so none of Gates A-F sees them:
///
/// 1. CLI plan mode — `get_reasoner` (`biorouter-cli/src/session/mod.rs:2264`)
/// 2. Prompt hooks   — `resolve_prompt_provider` (`hooks/mod.rs:690`)
/// 3. Knowledge default model — `build_model_ref_completer` (`knowledge_tool.rs:183`)
///
/// A fourth site, the HTTP knowledge macros (`routes/knowledge.rs:899-914`), is
/// NOT here: it has a knowledge-base id and no session, so the predicate it
/// needs is the KB-keyed one in Task 10C, not this session-keyed one.
pub fn assert_alt_provider_allowed(
    what: &str,
    provider: &dyn Provider,
    session: SessionClassification,
    env_key_to_name: &str,
) -> Result<()> {
    if crate::privacy::bind_allowed(provider.tier(), session) {
        return Ok(());
    }
    Err(anyhow!(
        "This chat is private, so {what} cannot run on `{}`, which is a public model. \
         Set {env_key_to_name} to a private model, or start this work in a public chat.",
        provider.get_name()
    ))
}
```

and, for memory, two changes in `remember_memory` (`:491-540`), wired through the same
`PlatformExtensionContext`/`capability_tier()` route Task 10 built: a **refusal** when
`is_global == true` and the caller's capability is Private, and — for `is_global == false` from a
private-capability caller — a **disclosure sentence appended to the existing result copy** at
`:523-548`, which already exists precisely to tell the user which store a note landed in:

> Stored memory locally in category: `{category}`. Local memories stay in this project's
> `.biorouter/memory` and are read by **any session opened in this directory, including one on a
> public model** — this chat is private, that directory is not.

- [ ] **Step 4: Run**

```bash
cargo test -p biorouter --lib -- privacy::alt_provider hooks agents::knowledge_tool
cargo test -p biorouter-cli --lib session
cargo test -p biorouter-mcp --lib memory
```

- [ ] **Step 5: Gate**

```bash
# One helper, three sites — and no site builds a provider around it. PRINT the
# hit list and read the FILES: `| wc -l ; expect: 3` cannot distinguish three
# production sites from one production site plus two calls inside a test module
# in the same file, and every Rust test in this repo lives in the file it tests.
grep -rn "assert_alt_provider_allowed(" --include='*.rs' crates/ | grep -v "privacy/alt_provider.rs"
echo "expect: the three files below and NO fourth. 0 hits today."
# ...and each one is in the production path, not only in that file's tests. The
# three function names come from this task's own Files table and every range was
# run against the tree: get_reasoner :2264 (32 lines), resolve_prompt_provider
# :690 (37 lines), build_model_ref_completer :183 (11 lines).
# ⚠ get_reasoner and build_model_ref_completer are TOP-LEVEL fns and close with
# `^}`; resolve_prompt_provider is a method and closes with `^    }`. Using
# `/^    }/` on get_reasoner terminates the range at its first `    } else {`
# (line 2273) after ten lines — before the `create(&provider, …)` call the gate
# is about — and reports a confident 0.
echo -n "cli get_reasoner:            "
awk '/^async fn get_reasoner/,/^}/' crates/biorouter-cli/src/session/mod.rs \
  | grep -c "assert_alt_provider_allowed"
echo -n "hooks resolve_prompt_provider:"
awk '/async fn resolve_prompt_provider/,/^    }/' crates/biorouter/src/hooks/mod.rs \
  | grep -c "assert_alt_provider_allowed"
echo -n "kb build_model_ref_completer: "
awk '/async fn build_model_ref_completer/,/^}/' crates/biorouter/src/agents/knowledge_tool.rs \
  | grep -c "assert_alt_provider_allowed"
echo "expect: 1 each. ⚠ If a range prints 0, first check it is NON-EMPTY (pipe to"
echo "  wc -l instead): an awk START that never matches yields no output, and"
echo "  grep -c over no output is 0 — a silent pass. All three STARTs are"
echo "  verified present at 9558c346; if Step 3 puts the call in a different"
echo "  function, change the pattern here rather than deleting the check."
# The KB-keyed check lives in Task 10C, not here — a copy in this file is a
# second taxonomy for the same question.
grep -c "assert_alt_provider_allowed" crates/biorouter-server/src/routes/knowledge.rs ; echo "expect: 0"
# The local-memory disclosure is on the result the transcript shows, not a log line.
awk '/pub async fn remember_memory/,/^    }/' crates/biorouter-mcp/src/memory/mod.rs \
  | grep -c "including one on a public model" ; echo "expect: 1"
awk '/pub async fn remember_memory/,/^    }/' crates/biorouter-mcp/src/memory/mod.rs \
  | grep -c "tracing::warn\|tracing::info" ; echo "expect: 0 — the user reads the transcript, not the log"
```

**What this catches.** The wrong implementation fixes the CLI plan-mode path — the one the design
names as P6 — and treats the other two as follow-ups, which is what §18.4 currently invites. The
per-file count is what makes the omission visible; a repo-wide `grep -c` returning 1 would read as
"the helper exists". Prompt hooks in particular carry `transcript_tail`, i.e. real transcript text,
to an endpoint chosen by an agent-writable config file. For memory, it catches the version that logs
the local-store disclosure at `warn!` — where the model, which is the thing being steered, never sees
it, and neither does the user.

**What this deliberately does NOT close: AR-3.** The local memory *store* is still ungated, and
`compose_instructions` still inlines it in full into every session in that directory. This task ships
the disclosure and Open question 14 carries the fix. Do not let the disclosure read, in review or in
the release notes, as though the channel were closed.

- [ ] **Step 6: Commit**

```bash
git add crates/biorouter/src/privacy/alt_provider.rs crates/biorouter-cli/src/session/mod.rs \
        crates/biorouter/src/hooks/mod.rs crates/biorouter/src/agents/knowledge_tool.rs \
        crates/biorouter-mcp/src/memory/mod.rs
git commit -m "feat(privacy): Gate H - refuse alternate-provider paths and global memory writes from private sessions (#56)"
```

---

### Task 20: Phase 2 gate

- [ ] **Step 1: Full suite, lints, OpenAPI, frontend**

```bash
cargo test --workspace --no-fail-fast 2>&1 | tail -20
cargo fmt --check && ./scripts/clippy-lint.sh
just generate-openapi && git diff --exit-code ui/desktop/openapi.json
cd ui/desktop && npx tsc --noEmit && npm run lint:check && npm run test:run 2>&1 | tail -8
```

- [ ] **Step 2: The integration targets no `--lib` filter reaches**

```bash
cargo test -p biorouter --test subagent_delegation
cargo test -p biorouter --test soft_interrupt_agent_loop
cargo test -p biorouter --test conversation_writeback_freshness
cargo test -p biorouter --test conversation_writeback_stress
cargo test -p biorouter-server --test knowledge_routes
cargo test -p biorouter-server --test knowledge_routes_e2e
# ⚠ `-p biorouter-mcp` is WRONG here and cargo hard-errors
# ("no test target named `mcp_integration_test` in default-run packages").
# The file is crates/biorouter/tests/mcp_integration_test.rs — verified.
cargo test -p biorouter --test mcp_integration_test
cargo test -p biorouter-mcp --lib knowledge::
# Tasks 10B and 10D change struct signatures that these three construct, and no
# `--lib` filter compiles any of them. If any of the three fails here, one of the
# nine commits between 10B and 19 shipped a tree that does not build (O13).
cargo test -p biorouter-mcp --test knowledge_macros_e2e
cargo test -p biorouter-mcp --test catalog_write_boundary
cargo test -p biorouter-mcp --test testdrive_corpus_relint
```

Task 13 edits `Agent::reply`'s prologue, which is where a reordering shows up in
`conversation_writeback_freshness`'s three #59 ordering tests. Tasks 10A–10D touch every knowledge
entry point, which is what the last five targets cover.

- [ ] **Step 3: Every gate is where it is supposed to be, and nowhere else**

```bash
# O7: exactly one production path into an MCP client. Measured at 9558c346, the
# full hit list is 10 lines: extension_manager.rs:1562 (the ONLY production one,
# inside dispatch_tool_call's spawned future), plus code_execution_extension.rs
# :2140/:2177/:2247 and skills_extension.rs :1229/:1306/:1326/:1345 — all inside
# `#[cfg(test)] mod tests` (they begin at :2115 and :798 respectively) — plus two
# integration files under tests/. Print it and read it; a `grep -vc "cfg(test)"`
# does NOT work here, because the hit lines do not themselves contain that text.
grep -rn "\.call_tool(" --include='*.rs' crates/
grep -c "\.call_tool(" crates/biorouter/src/agents/extension_manager.rs ; echo "expect: 1"
echo "and every other hit must be in a tests module or under tests/ — a new one is a new bypass"
# O5: the ratchet fires on the turn and on a private dispatch, not on the bind.
grep -rn "raise_privacy(" --include='*.rs' crates/ | grep -v session_manager.rs
echo "expect: exactly 2 — agent.rs (Gate B) and extension_manager.rs (Gate C)"
# O6: nothing above filter_tools consults a tier.
grep -c "allowed_extension_keys" crates/biorouter/src/agents/extension_manager.rs ; echo "expect: 3"
# Gate D is in both builders.
grep -c "s.privacy_tier = 'public'" crates/biorouter/src/session/chat_history_search.rs ; echo "expect: 2"
# No privacy check is expressed as an inspector, and none returns Err from inspect.
# ⚠ NOT `grep -rn "PrivacyInspector"` — that name is this plan's invention, is 0
# today and is 0 under every wrong implementation, so it was green both ways.
# Assert the trait's impl set is unchanged, exactly as Task 14 Step 5 does.
diff <(grep -rl "impl ToolInspector for" --include='*.rs' crates/ | sort) <(cat <<'EOF'
crates/biorouter/src/hooks/inspector.rs
crates/biorouter/src/permission/managed_inspector.rs
crates/biorouter/src/permission/permission_inspector.rs
crates/biorouter/src/security/security_inspector.rs
crates/biorouter/src/security/sensitive_ops.rs
crates/biorouter/src/tool_monitor.rs
crates/biorouter/tests/tool_inspection_manager_tests.rs
EOF
) && echo "OK: no privacy control was written as an inspector"
# O12 — the knowledge-base barrier at its FIVE choke points (Task 10A's ⚠), and
# the ratchet at five sites. These are counts of choke points, not of tool call
# sites: if they grow, someone re-scattered the control back across the tools.
grep -c "assert_kb_reachable(" crates/biorouter-mcp/src/knowledge/server.rs
echo "expect: 2 — 1 definition + 1 call in call_tool (CP1)"
grep -c "tier::assert_reachable(" crates/biorouter-mcp/src/knowledge/macros/ingest.rs \
        crates/biorouter-mcp/src/knowledge/macros/query.rs \
        crates/biorouter-mcp/src/knowledge/macros/lint.rs ; echo "expect: 1 each (CP2)"
grep -c "tier::assert_reachable(" crates/biorouter-server/src/routes/apps.rs ; echo "expect: 1 (CP3)"
grep -c "tier::assert_reachable(" crates/biorouter-mcp/src/agent_drafter/mod.rs ; echo "expect: 1 (CP4)"
grep -c "raise_tier(" crates/biorouter-mcp/src/knowledge/server.rs ; echo "expect: 3"
grep -c "raise_tier(" crates/biorouter-mcp/src/knowledge/macros/ingest.rs \
                      crates/biorouter-mcp/src/knowledge/macros/query.rs \
                      crates/biorouter-mcp/src/knowledge/macros/lint.rs ; echo "expect: 1 each"
grep -c "raise_tier(" crates/biorouter-server/src/routes/apps.rs ; echo "expect: 1"
# The generated tool handler is gone, which is what makes CP1 exist at all.
grep -c "tool_handler" crates/biorouter-mcp/src/knowledge/server.rs ; echo "expect: 0"
# CP5 — the metadata choke point (Task 10D), which the two content detectors below
# are blind to by construction. One constructor, and the drafter reads the meta
# key through the shared const rather than a third hand-typed copy.
grep -c "pub fn discover(" crates/biorouter-mcp/src/agent_drafter/catalog.rs ; echo "expect: 1 (CP5)"
grep -rn "Catalog::discover(true)" --include='*.rs' crates/*/src/ ; echo "expect: no output"
grep -rl '"biorouter-capability-tier"' --include='*.rs' crates/ | wc -l
echo "expect: 2 FILES — knowledge/tier.rs and agents/mcp_client.rs (Task 10A, held by 10D)"
# No new way to reach base content appeared outside `knowledge/`. 4 today:
# routes/apps.rs:2394 (CP3) + routes/knowledge.rs:523/:544/:571 (ungated user routes).
grep -rn "store::\(list_pages\|read_page\|write_page\|search\|search_with_scope\)(" \
  --include='*.rs' crates/ | grep -v "src/knowledge/" | wc -l
echo "expect: 4 — see Task 10C Step 5 for the enumeration"
# ...and no new way to reach base METADATA. BOTH sweeps: 27 outside knowledge/
# (18 production) and 22 inside it (5 production), at 9558c346. A hit beyond
# those is a new surface and must be classified against Task 10D's register.
# This is the detector the fourth adversarial round showed was missing — both
# content detectors above pass a tree in which `list_platform_catalog` hands
# every base id to a public model — and the `grep -v src/knowledge/` half is
# why the fifth round then missed `kb_get_active`.
grep -rn "\.list_bases()\|\.session_kb_ids(\|\.selection(" --include='*.rs' crates/*/src/ \
  | grep -v "src/knowledge/" | wc -l
echo "expect: 27 — see Task 10D Step 5 sweep (1)"
grep -rn "\.list_bases()\|\.session_kb_ids(\|\.selection(" --include='*.rs' crates/*/src/ \
  | grep "src/knowledge/" | wc -l
echo "expect: 22 — see Task 10D Step 5 sweep (2), the one that sees the pointer tools"
# The two id-list error messages omit rather than enumerate (Tasks 10C, 11).
awk '/fn kb_id_or_primary\(/,/^    }/' crates/biorouter-mcp/src/knowledge/server.rs \
  | grep -c "tier::is_private" ; echo "expect: 1"
awk '/fn resolve_target_kb\(/,/^}/' crates/biorouter/src/agents/knowledge_tool.rs \
  | grep -c "is_private" ; echo "expect: 1"
# Only tier.rs (and the service's migration guard) opens the store, and the tier
# never reached the manifest. NOT `grep -c "tier\|privacy"` on types.rs: that is
# 8 today and unsatisfiable — `CredibilityTier` lives there.
grep -rn "kb_tiers_path" crates/biorouter-mcp/src/knowledge/ | grep -v "fn kb_tiers_path"
echo "expect: tier.rs and service.rs::ensure_tiers_migrated only"
grep -cE "privacy_tier|PrivacyTier|kb_tier" crates/biorouter-mcp/src/knowledge/types.rs ; echo "expect: 0"
# Gate G is one guard in the shared function, not three copies in three callers.
grep -rn "visible_to" --include='*.rs' crates/biorouter/src/knowledge/ \
  crates/biorouter/src/agents/knowledge_tool.rs crates/biorouter-cli/src/commands/knowledge.rs \
  crates/biorouter-server/src/routes/knowledge.rs
echo "expect: exactly 1 hit, in conversation_ingest.rs"
```

- [ ] **Step 4: Headless end-to-end, with a real private extension (manual, once)**

⚠ **Do not use the operator's own config for this.** `cdwagent` and `ucsfomopagent` are both
installed with `enabled: false`, and the only clinical-reaching extension that is switched on is
`medcp` — which is unlisted on BAAM and therefore **Public**, and stays fully callable. On this
machine, with this config, *nothing will be refused*. Seed a fixture config.

```bash
export XDG_CONFIG_HOME=/tmp/privacy-gate-check      # sandbox: the dev GUI clobbers ~/.config/biorouter
# Seed a config enabling a stub extension NAMED ucsfomopagent, then:
just debug-server &
SID=$(curl -s -X POST http://127.0.0.1:3000/agent/start -H 'X-Secret-Key: test' \
      -H 'Content-Type: application/json' -d '{"working_dir":"/tmp"}' | python3 -c 'import json,sys;print(json.load(sys.stdin)["id"])')

# 1. Bind a public model, then try the private tool through the inspector-free route.
curl -s -X POST http://127.0.0.1:3000/agent/update_provider -H 'X-Secret-Key: test' \
  -H 'Content-Type: application/json' -d "{\"session_id\":\"$SID\",\"provider\":\"anthropic\",\"model\":\"claude-opus-4-8\"}"
curl -s -X POST http://127.0.0.1:3000/agent/call_tool -H 'X-Secret-Key: test' \
  -H 'Content-Type: application/json' \
  -d "{\"session_id\":\"$SID\",\"name\":\"ucsfomopagent__ping\",\"arguments\":{}}" | python3 -m json.tool
# Expected: the refusal TEXT, naming ucsfomopagent and the marketplace.
# NOT "Internal server error" and NOT "The user has declined to run this tool".

# 2. The tool is absent from the model's own list.
curl -s -H 'X-Secret-Key: test' "http://127.0.0.1:3000/agent/tools?session_id=$SID" \
  | python3 -c 'import json,sys; n=[t["name"] for t in json.load(sys.stdin)]; assert not any(x.startswith("ucsfomopagent__") for x in n), n; print("Gate E OK")'

# 3. Switch to a private model and the same call succeeds, ratcheting the row.
curl -s -X POST http://127.0.0.1:3000/agent/update_provider -H 'X-Secret-Key: test' \
  -H 'Content-Type: application/json' -d "{\"session_id\":\"$SID\",\"provider\":\"versa_azure\",\"model\":\"gpt-5.5-2026-04-24\"}"
curl -s -X POST http://127.0.0.1:3000/agent/call_tool -H 'X-Secret-Key: test' \
  -H 'Content-Type: application/json' -d "{\"session_id\":\"$SID\",\"name\":\"ucsfomopagent__ping\",\"arguments\":{}}" >/dev/null
sqlite3 "$XDG_CONFIG_HOME/../sessions.db" "select privacy_tier, privacy_reason from sessions where id='$SID';"
# Expected: private|mcp:ucsfomopagent

# 4. And now the bind back is refused with a 409, not a 500.
curl -s -o /dev/null -w '%{http_code}\n' -X POST http://127.0.0.1:3000/agent/update_provider \
  -H 'X-Secret-Key: test' -H 'Content-Type: application/json' \
  -d "{\"session_id\":\"$SID\",\"provider\":\"anthropic\",\"model\":\"claude-opus-4-8\"}"
# Expected: 409

# 5. The knowledge base ratcheted with the session, and a public chat cannot
#    read it back. This is the laundering path (AR-1), end to end, by hand.
cat "$XDG_CONFIG_HOME/biorouter/knowledge/.kb-tiers"
# Expected: the base the private session wrote to now reads "private".
SID2=$(curl -s -X POST http://127.0.0.1:3000/agent/start -H 'X-Secret-Key: test' \
       -H 'Content-Type: application/json' -d '{"working_dir":"/tmp"}' \
       | python3 -c 'import json,sys;print(json.load(sys.stdin)["id"])')
curl -s -X POST http://127.0.0.1:3000/agent/update_provider -H 'X-Secret-Key: test' \
  -H 'Content-Type: application/json' -d "{\"session_id\":\"$SID2\",\"provider\":\"anthropic\",\"model\":\"claude-opus-4-8\"}"
curl -s -X POST http://127.0.0.1:3000/agent/call_tool -H 'X-Secret-Key: test' \
  -H 'Content-Type: application/json' \
  -d "{\"session_id\":\"$SID2\",\"name\":\"knowledge__kb_search\",\"arguments\":{\"kb_id\":\"default\",\"query\":\"cohort\"}}" \
  | python3 -m json.tool
# Expected: the KB refusal text. NOT a hit list, and NOT an empty result set —
# an empty result set is what a wrong implementation that filters hits AFTER
# reading the index returns, and it is indistinguishable from "no matches".
```

- [ ] **Step 4b: Re-run Task 4b's filter audit with a shrunk deferred set**

```bash
# Nine of Task 4b's twelve deferred filters were created by Tasks 4-19, so only
# three may still be deferred here. Re-run Task 4b Steps 1 and 5, deleting the
# nine landed ROWS from /tmp/56-filters/deferred.txt and keeping these three:
#   biorouter|privacy::visibility|21|crates/biorouter/src/privacy/visibility.rs
#   biorouter|privacy::declassify|29|crates/biorouter/src/privacy/declassify.rs
#   biorouter|every_copy_path_carries_the_tier_and_the_provider|22|fn every_copy_path_carries_the_tier_and_the_provider
# Delete rows rather than editing a regex: the deferral is keyed on the
# (package, filter) PAIR, and a nine-term alternation is where a package gets
# dropped (Task 4b's "What this catches").
# Expect: 0 MISSING, 3 DEFER, 0 UNUSED, and OK with a non-zero count for all nine of
# privacy, privacy::tests, privacy::extensions, privacy::refusal,
# privacy::alt_provider, providers::tier_tests, knowledge::tier,
# agents::chatrecall_extension and session::chat_history_search.
# A DEFER on any of those nine means the module landed under a different path
# from the one this plan filters on — the BR-71 defect, caught here rather than
# at the release gate, where forty tasks of gates have already quoted it.
```

- [ ] **Step 5: Adversarial review of the phase diff, every finding addressed**

Permission-relevant code requires human review per `.github/copilot-instructions.md`. Every task in
this phase qualifies. Flag Tasks 12, 13, 14 and 17 explicitly in the PR description.

- [ ] **Step 6: Record the gate in the PR description. No code.**

---

# Phase 3 — the barrier surfaces

Five tasks: lineage, spawn inheritance, the three copy paths, and the two places a shipped feature
breaks under the gates.

### Task 21: The capability matrix as one predicate

Design §7 is a nine-column table over three inputs. Written once, as a pure function, it is
unit-testable without a database and BR-71's tool handlers can call it rather than re-deriving it.

**Files:**

| Action | Path | Anchor |
|---|---|---|
| Create | `crates/biorouter/src/privacy/visibility.rs` | new |
| Reference | `crates/biorouter/src/session/session_manager.rs` | `Session.parent_session_id` (Task 6) |

- [ ] **Step 1: Write the failing test — the design's table, cell for cell**

```rust
#[test]
fn the_capability_matrix_matches_the_design_table_cell_for_cell() {
    use Lineage::{Child, Other, Zelf};
    use ProviderTier::{Private as CPriv, Public as CPub};
    use SessionClassification::{Private as TPriv, Public as TPub};
    // Columns A..G of design §7. `self` and `child` behave identically under
    // every rule and are merged in the table; D and F are what prove it, so
    // both are enumerated here rather than assumed.
    let cases = [
        //  caller     target   lineage  read   write  list-visible
        ( CPub,  TPub,  Zelf,  true,  true,  true ),   // A
        ( CPub,  TPub,  Child, true,  true,  true ),   // A
        ( CPub,  TPub,  Other, true,  false, true ),   // B — R6's read-only floor
        ( CPub,  TPriv, Zelf,  false, false, false),   // C — row OMITTED, not redacted
        ( CPub,  TPriv, Child, false, false, false),   // C
        ( CPub,  TPriv, Other, false, false, false),   // C
        ( CPriv, TPub,  Zelf,  true,  true,  true ),   // D
        ( CPriv, TPub,  Child, true,  true,  true ),   // D
        ( CPriv, TPub,  Other, true,  false, true ),   // E
        ( CPriv, TPriv, Zelf,  true,  true,  true ),   // F
        ( CPriv, TPriv, Child, true,  true,  true ),   // F
        ( CPriv, TPriv, Other, true,  false, true ),   // G
    ];
    for (c, t, l, read, write, list) in cases {
        assert_eq!(may_read(c, t), read, "read {c:?}/{t:?}/{l:?}");
        assert_eq!(may_write(c, t, l), write, "write {c:?}/{t:?}/{l:?}");
        assert_eq!(appears_in_list(c, t), list, "list {c:?}/{t:?}/{l:?}");
    }
}

#[test]
fn a_grandchild_is_other_and_a_null_parent_is_other() {
    // Lineage is ONE hop: R6 says "sessions the caller DID spawn", and a
    // grandchild was spawned by the child. NULL parent is `other` => read-only,
    // which is the safe direction and is what every pre-upgrade subagent has.
    assert_eq!(lineage_of(Some("me"), "me"), Lineage::Child);
    assert_eq!(lineage_of(Some("my-child"), "me"), Lineage::Other);
    assert_eq!(lineage_of(None, "me"), Lineage::Other);
}

#[test]
fn a_downgrade_write_is_permitted_but_flagged_for_first_crossing_approval() {
    // R4 permits a private session to spawn public children, and a rule that
    // lets you spawn one but never send it a prompt makes the permission
    // useless. The prompt text IS private-origin content crossing into a
    // public model, so the FIRST crossing per (caller,target) discloses it.
    assert!(may_write(CPriv, TPub, Zelf));
    assert!(requires_first_crossing_approval(CPriv, TPub));
    assert!(!requires_first_crossing_approval(CPriv, TPriv));
    assert!(!requires_first_crossing_approval(CPub, TPub));
}
```

- [ ] **Step 2: Run** → **COMPILE ERROR** (`unresolved module visibility`).

- [ ] **Step 3: Implement** — the three rules of design §7, verbatim:

```rust
//! The §7 capability matrix, as one predicate per verb.
//!
//! ```text
//! VIS(T)     <=>  T <= C                      // a public caller sees public only
//! READ       <=>  VIS                         // any lineage — R6's read-only floor
//! WRITE      <=>  VIS && L in {self, child}
//! BIND(P->T) <=>  WRITE && tier(P) >= T       // Gate A, evaluated on the target
//! ```
//!
//! Delegation is not amplification: VIS is evaluated against the CHILD's own
//! capability, never its parent's, so a private parent's public child sees only
//! public sessions. A public parent cannot spawn a private child at all
//! (Task 23), so it can never mint a private-capability agent to read through.

pub enum Lineage { Zelf, Child, Other }

pub fn lineage_of(target_parent: Option<&str>, caller_session_id: &str) -> Lineage { … }
pub fn may_read(c: ProviderTier, t: SessionClassification) -> bool { visible_to(c, t) }
pub fn may_write(c: ProviderTier, t: SessionClassification, l: Lineage) -> bool {
    visible_to(c, t) && !matches!(l, Lineage::Other)
}
/// `workspace_list` OMITS private rows rather than redacting them: a row
/// carries a title, and a session title in this product is LLM-generated from
/// the conversation, i.e. content. Omission is one WHERE clause and removes the
/// temptation to then call read_conversation on the id.
pub fn appears_in_list(c: ProviderTier, t: SessionClassification) -> bool { visible_to(c, t) }
pub fn requires_first_crossing_approval(c: ProviderTier, t: SessionClassification) -> bool {
    c.is_private() && !t.is_private()
}
```

- [ ] **Step 4: Run** → `cargo test -p biorouter --lib privacy::visibility` → **PASS**, 3 tests.

- [ ] **Step 5: Gate**

```bash
cargo test -p biorouter --lib privacy::visibility 2>&1 | tail -3
# Expected: "3 passed". A filter that names a nested module by the wrong path
# prints "0 passed" and EXITS 0 — the count is the gate, not the exit code.
# Nobody re-derives the matrix. ⚠ `grep -v _test` filters the PATH, and almost
# every Rust test in this repo lives in a `#[cfg(test)] mod` INSIDE the file it
# tests — so it excludes nothing that matters and will not exclude a legitimate
# assertion written inside, say, session_manager.rs's own test module. PRINT the
# hits and read them rather than demanding silence: the gate is "no PRODUCTION
# consumer re-derives the matrix", which a path filter cannot express.
grep -rn --include='*.rs' "privacy_tier *== *\(SessionClassification::\)\?Private" crates/ \
  | grep -v "^crates/biorouter/src/privacy/"
echo "expect: no output. If a line IS printed, it must be inside a #[cfg(test)]"
echo "  module AND must be asserting a stored value, not deciding an access —"
echo "  every consumer calls may_read/may_write/appears_in_list. Measured today: 0."
```

**What this catches.** The matrix inlined as `if session.privacy_tier == Private && caller == Public`
at each of BR-71's eight tool handlers, which is how a table becomes eight slightly-different tables.
The second grep is what makes that visible, and it is why the predicate is a function rather than
documentation.

- [ ] **Step 6: Commit**

```bash
git add crates/biorouter/src/privacy/visibility.rs
git commit -m "feat(privacy): the capability matrix as one predicate per verb (#56)"
```

---

### Task 22: Session copy — three hand-rolled builders become one derived-session helper

Three copy paths carry the conversation and not the provider, so a branch of a private chat resolves
through `restore_provider_from_session`'s `Config::global()` fallback (`agent.rs:5685`) and runs
private history on the user's default public model, with no prompt.

**Departure D2:** the design says to parameterise `create_session`. Measured:
`grep -rn --include='*.rs' "\.create_session(" crates/` returns **104** call sites. Collapsing the
three hand-rolled builders into one shared helper is a better trade and closes the same hole.

**Files:**

| Action | Path | Anchor (re-verified at `9558c346`) |
|---|---|---|
| Modify | `crates/biorouter/src/session/session_manager.rs` | `copy_session` `:4710-4741` (`create_session` `:4718-4724`, builder `:4726-4733`, `replace_conversation` `:4736`); `diverge_session_for_edit` `:4743-4773`; `diverge_session` `:4776-4841` (**the primary GUI diverge, and it does NOT call `copy_session`** — `create_session` `:4816-4822`, builder `:4824-4836`, `replace_conversation` `:4838`); `import_session` `:4668-4707` (builder `:4683-4700`, `replace_conversation` `:4703`); the two builder setters this helper must use correctly — `provider_name(impl Into<String>)` `:972-975` and `model_config(ModelConfig)` `:977-980`, **both taking values, not `Option`s** |
| Reference | `crates/biorouter-server/src/routes/session.rs` | `POST /sessions/{id}/diverge` at `:1029`. ⚠ **`routes/session.rs` already has 20 tests** in two `#[cfg(test)]` blocks — `mod diverge_tests` (`:1038`, 11) and `mod edit_message_tests` (`:1417`, 9). Neither is named `tests`, and the module-path filter picks up both, so Step 4's `cargo test -p biorouter-server --lib routes::session` prints **`20 passed`** before this task, not `0 passed`. A previous version of this row said the module was empty. **Record the pre-count and assert `pre + N`** — `mod diverge_tests` is also exactly where this task's route test belongs |
| Reference | `crates/biorouter/src/session/session_manager.rs` | `diverge_session_at` `:1562` — checked, and it is a thin wrapper onto the same storage `diverge_session` at `:4776`, so the three-path coverage below really is complete. Say so, or the next reviewer re-derives it |

- [ ] **Step 1: Write the failing tests — one per path, and one that enumerates**

```rust
#[tokio::test]
async fn every_copy_path_carries_the_tier_and_the_provider() {
    // A test on copy_session alone passes an implementation that misses the GUI
    // path entirely — which is exactly how this bug shipped: routes/session.rs:1029
    // reaches diverge_session, and diverge_session does NOT call copy_session.
    for path in [CopyPath::Copy, CopyPath::DivergeForEdit, CopyPath::Diverge] {
        let parent = private_session_on("versa_azure").await;
        let child = run_copy(path, &parent).await.unwrap();
        assert_eq!(child.privacy_tier, SessionClassification::Private, "{path:?}");
        assert_eq!(child.provider_name.as_deref(), Some("versa_azure"), "{path:?}");
        assert!(child.model_config.is_some(), "{path:?}");
        assert_eq!(child.privacy_reason.as_deref(),
                   Some(format!("diverged:{}", parent.id).as_str()), "{path:?}");
    }
}

#[tokio::test]
async fn an_import_with_no_tier_is_private_and_one_with_a_tier_is_only_raised_by_it() {
    // Read the imported field ONLY in the raising direction — never as authority
    // to set public. An imported transcript of unknown provenance is sensitive:
    // unlike migration, there is no local evidence to reason from.
    assert_eq!(import_json_without_tier().await.privacy_tier, SessionClassification::Private);
    assert_eq!(import_json_with("private").await.privacy_tier, SessionClassification::Private);
    assert_eq!(import_json_with("public").await.privacy_tier, SessionClassification::Private);
}

#[test]
fn no_copy_path_hand_rolls_its_own_builder_any_more() {
    // The design's enumeration test, aimed at the three functions that matter
    // rather than at all 104 create_session call sites.
    let src = std::fs::read_to_string("src/session/session_manager.rs").unwrap();
    for f in ["copy_session", "diverge_session_for_edit", "diverge_session"] {
        let body = fn_body(&src, f);
        assert!(body.contains("create_derived_session"), "{f} does not use the shared helper");
        assert!(!body.contains(".extension_data("), "{f} still hand-rolls its carry-over");
    }
}
```

- [ ] **Step 2: Run** → tests 1 and 3 **FAIL**, test 2 **FAIL**.

- [ ] **Step 3: Implement**

```rust
    /// Create a session derived from `source`, carrying everything a branch must
    /// inherit. The three copy paths (`copy_session`, `diverge_session_for_edit`,
    /// `diverge_session`) each hand-rolled their own builder, and none of them
    /// carried `provider_name`/`model_config`/`privacy_tier` — so a branch of a
    /// private chat resolved through `restore_provider_from_session`'s
    /// `Config::global()` fallback and ran private history on the user's default
    /// public model, with no prompt (issue #56 §9.3 B1).
    ///
    /// Callers add only their own extras (`user_provided_name`, `diverged_from`,
    /// `branch_point_msg_uid`), so the carry-over cannot be missed by one of them.
    async fn create_derived_session(
        &self,
        session_manager: &SessionManager,
        source: &Session,
        new_name: String,
        reason: &str,
    ) -> Result<Session> {
        let new_session = self
            .create_session(source.working_dir.clone(), new_name, source.session_type)
            .await?;
        let mut update = session_manager
            .update(&new_session.id)
            .extension_data(source.extension_data.clone())
            .schedule_id(source.schedule_id.clone())
            .workflow(source.workflow.clone())
            .user_workflow_values(source.user_workflow_values.clone())
            .raise_privacy(source.privacy_tier, reason);
        // ⚠ The two provider setters take VALUES, not Options:
        // `provider_name(impl Into<String>)` (session_manager.rs:972-975) and
        // `model_config(ModelConfig)` (:977-980) — each wraps its argument as
        // `Some(Some(v))`, because `Option<Option<T>>` is how this builder
        // distinguishes "leave alone" from "set to NULL". There is no
        // `provider_name_opt`, and passing `source.model_config.clone()`
        // straight in is a type error. A source with no provider must leave the
        // child's column untouched rather than writing NULL over the default.
        if let Some(name) = source.provider_name.clone() {
            update = update.provider_name(name);
        }
        if let Some(cfg) = source.model_config.clone() {
            update = update.model_config(cfg);
        }
        update.apply().await?;
        self.get_session(&new_session.id, false).await
    }
```

and for `import_session`, the raise-only rule:

```rust
        let imported = import.privacy_tier;      // parsed, but never authoritative for `public`
        builder = builder.raise_privacy(
            SessionClassification::Private.max(imported),
            "imported",
        );
```

- [ ] **Step 4: Run**

```bash
cargo test -p biorouter --lib session::session_manager
cargo test -p biorouter-server --lib routes::session
```

- [ ] **Step 5: Gate**

```bash
# One carry-over, three users.
grep -c "create_derived_session" crates/biorouter/src/session/session_manager.rs
echo "expect: 4 (1 definition + 3 call sites)"
# The setter that does not exist stayed non-existent.
grep -c "provider_name_opt" crates/biorouter/src/session/session_manager.rs ; echo "expect: 0"
# The imported tier can only raise.
awk '/async fn import_session/,/^    }/' crates/biorouter/src/session/session_manager.rs \
  | grep -c "SessionClassification::Private.max" ; echo "expect: 1"
# And a behavioural cross-check no grep gives you: the GUI diverge route.
# ⚠ `| tail -3` with no stated number is not a gate. This filter names ONE test
# and nothing else; if the name is misspelled, or Step 1 nested it somewhere the
# filter does not reach, libtest prints `0 passed` and EXITS 0 — the BR-71
# defect this plan exists to avoid. Assert the count.
cargo test -p biorouter --lib -- every_copy_path_carries_the_tier_and_the_provider \
  | grep "test result:" ; echo "expect: 1 passed; 0 failed"
```

**What this catches.** A test on `copy_session` alone — the only path the design's §9.3 B1 originally
named — passes an implementation that misses `diverge_session`, which is the one
`POST /sessions/{id}/diverge` actually reaches. That is precisely how the bug shipped. The
parameterised loop in Step 1 and the enumeration test in Step 3 are what make the omission
impossible to miss.

- [ ] **Step 6: Commit**

```bash
git add crates/biorouter/src/session/session_manager.rs
git commit -m "fix(session): carry tier, provider and model config across all three copy paths (#56)"
```

---

### Task 23: Spawn — reorder, stamp, filter, and the spawn matrix

⚠ **The design's §8.2 sketch does not survive the tree's ordering.** `create_subagent_session` runs
at `subagent_tool.rs:507` and `:526`; `overridden_task_config` (→ `apply_settings_overrides`) runs
at `:508` and `:527`. The child row is INSERTed **before** its tier is known, and a spawn refusal
leaves an orphaned `SubAgent` row — worse on the background path at `:507`, which then runs in a
detached `tokio::spawn`. Stamping the tier "in `create_subagent_session`'s INSERT" therefore
requires **reordering the two calls first**.

**Files:**

| Action | Path | Anchor (re-verified at `9558c346`) |
|---|---|---|
| Modify | `crates/biorouter/src/agents/subagent_tool.rs` | background spawn `:507-508`; blocking spawn `:526-527`; `create_subagent_session` `:545-560`; `overridden_task_config` `:564`; `apply_settings_overrides` `:756-795` (gate condition `:761-762`, `providers::create` `:778`, the **name-only** extension narrowing at `:788-791`) |
| Modify | `crates/biorouter/src/agents/subagent_task_config.rs` | `TaskConfig` `:16-22` — `{ provider, parent_session_id, parent_working_dir, extensions, max_turns }` |
| Reference | `crates/biorouter/src/agents/agent.rs` | `TaskConfig::new(provider, &session.id, &session.working_dir, extensions)` at `:2727` — the parent's **same `Arc`**, which is what makes R5 already true |

- [ ] **Step 1: Write the failing tests — the spawn matrix, row by row**

```rust
#[tokio::test]
async fn the_spawn_matrix_holds() {
    // Design §8.2. Validated on the CONSTRUCTED INSTANCE, never the requested
    // name: providers::create can return something else (the BIOROUTER_LEAD_MODEL
    // intercept at factory.rs:142-146 fires BEFORE the registry lookup), and
    // when only `model` is given today's code keeps the parent's provider_name
    // and swaps the model string — harmless, because the tier is a property of
    // the instance and never of the model id.
    assert_child(parent = Priv, request = Inherit,  ok(), tier = Private, prompt = false).await;
    assert_child(parent = Priv, request = Private,  ok(), tier = Private, prompt = false).await;
    assert_child(parent = Priv, request = Public,   ok(), tier = Public,  prompt = true ).await;
    assert_child(parent = Pub,  request = Inherit,  ok(), tier = Public,  prompt = false).await;
    assert_child(parent = Pub,  request = Public,   ok(), tier = Public,  prompt = false).await;
    // R4: a public session may never gain private reach. Hard refusal.
    assert_spawn_refused(parent = Pub, request = Private).await;
}

#[tokio::test]
async fn a_downgraded_child_is_born_public_not_inheriting_the_parents_private() {
    // Otherwise it is born in the stuck residual state. It receives only the
    // task prompt — none of the parent's history, none of its private
    // extensions — which is exactly why the confirmation shows the prompt: the
    // prompt is the entire disclosure.
    let child = spawn(parent = Priv, request = Public).await.unwrap();
    assert_eq!(row(&child.id).await.privacy_tier, SessionClassification::Public);
    assert!(child.conversation_carried_from_parent().is_none());
}

#[tokio::test]
async fn a_refused_spawn_leaves_no_orphan_row() {
    // The ordering bug. Today create_subagent_session runs at :507/:526 BEFORE
    // overridden_task_config at :508/:527, so a refusal leaves a durable
    // SubAgent row with no provider and no parent.
    let before = session_count().await;
    let _ = spawn(parent = Pub, request = Private).await.unwrap_err();
    assert_eq!(session_count().await, before);
}

#[tokio::test]
async fn a_public_child_does_not_inherit_its_parents_private_extensions() {
    // apply_settings_overrides narrows by NAME only
    // (`task_config.extensions.retain(|ext| extension_names.contains(&ext.name()))`,
    // :788-791), never by tier — so today a session holding ucsfomopagent can
    // spawn a public-model child that inherits it verbatim.
    let child = spawn_with_parent_extensions(parent = Priv, request = Public,
                                             &["ucsfomopagent", "developer"]).await.unwrap();
    assert_eq!(child.extension_names(), vec!["developer"]);
    assert!(child.tool_result_text().contains("ucsfomopagent"),
            "the drop must be reported so the model does not silently lose a capability");
}
```

- [ ] **Step 2: Run** → tests 1, 3 and 4 **FAIL**; test 2 **FAIL**.

- [ ] **Step 3: Implement**

(a) **Reorder both call sites** so the tier is known before the row exists:

```rust
    // Issue #56: resolve the child's provider and tier BEFORE creating its row.
    // A refusal must not leave a durable SubAgent session behind, and the
    // background path below runs the whole stretch in a detached tokio::spawn.
    let task_config = overridden_task_config(task_config, &params).await?;
    let session = create_subagent_session(&config, working_dir, task_config.privacy_tier).await?;
```

(b) **`TaskConfig` gains two fields** — `privacy_tier: SessionClassification` and
`requires_downgrade_confirmation: bool`.

(c) **`apply_settings_overrides`** (`:756-795`), after `providers::create` at `:778`:

```rust
        let child_tier = task_config.provider.tier();     // the INSTANCE, post-construction
        let parent_cap = parent_provider.tier();          // least() over a composite

        if child_tier.is_private() && !parent_cap.is_private() {
            return Err(PrivacyRefusal::spawn_upgrade(child_tier).into());   // R4
        }
        if !child_tier.is_private() && parent_cap.is_private() {
            task_config.requires_downgrade_confirmation = true;             // R4 permits; disclose
        }
        // The ONE crossing this task adds: the child's CAPABILITY establishes
        // the CLASSIFICATION its row is born with. Task 7's EXPECTED gains
        // ("crates/biorouter/src/agents/subagent_tool.rs", 1) in this commit.
        task_config.privacy_tier = crate::privacy::floor(child_tier);
        // The tier filter the name-only narrowing at :788-791 does not do.
        //
        // NOT `visible_to(child_tier, floor(classify_extension(..)))`: both
        // sides of that comparison are ProviderTiers, so `floor` has no business
        // in it, and writing it that way would add a SECOND crossing here — which
        // is how the first version of this plan got to four `floor` callers while
        // its own audit test asserted two. Use Gate C's predicate, which is also
        // what Gate E uses (Task 16), so all three agree by construction.
        task_config.extensions.retain(|e| {
            crate::privacy::refusal::privacy_refusal(
                &e.name(),
                crate::privacy::classify_extension(&e.name()),
                child_tier,
            )
            .is_none()
        });
```

and, in `privacy/refusal.rs` (created by Task 12), the variant and constructor this task is the first
to need — **`PrivacyRefusal::spawn_upgrade` is defined here, and nowhere before**:

```rust
    /// R4: a public-capability session may never gain private reach, not even
    /// through a child it spawns. `requested` is the child's tier, never the
    /// parent's, so the message can say what was asked for without naming the
    /// parent's provider.
    PrivateChildOfPublicParent { requested: ProviderTier },
```

```rust
impl PrivacyRefusal {
    pub fn spawn_upgrade(requested: ProviderTier) -> Self {
        Self::PrivateChildOfPublicParent { requested }
    }
}
```

(d) **`create_subagent_session`** takes the tier and stamps it in the same statement, with reason
`inherited:<parent_id>`.

R5's "the same worker/leader mode the parent is operating under" needs no mechanism: `TaskConfig`
holds the parent's **same `Arc<dyn Provider>`** (`agent.rs:2727`), so a child of a lead/worker parent
runs the identical composite with the identical split, literally rather than by copying settings.
One consequence to write down so a future reader does not "fix" it: sharing the instance also shares
its mutable `turn_count`/`failure_count`/`in_fallback_mode`, so a subagent's turns advance the
parent's lead→worker transition. Pre-existing and orthogonal — cloning the wrapper to fix it would
split the tier computation.

- [ ] **Step 4: Run**

```bash
cargo test -p biorouter --lib agents::subagent_tool
cargo test -p biorouter --test subagent_delegation
```

- [ ] **Step 5: Gate**

```bash
# Both spawn paths resolve the config BEFORE creating the row.
grep -n "overridden_task_config\|create_subagent_session" crates/biorouter/src/agents/subagent_tool.rs \
  | head -6
# Expected: at BOTH :~507 and :~526, overridden_task_config on the SMALLER line.
# The extension narrowing is by tier as well as by name.
awk '/async fn apply_settings_overrides/,/^}/' crates/biorouter/src/agents/subagent_tool.rs \
  | grep -c "classify_extension" ; echo "expect: 1"
# This task adds exactly ONE floor crossing, not two, and Task 7's EXPECTED is
# bumped in the SAME commit.
awk '/async fn apply_settings_overrides/,/^}/' crates/biorouter/src/agents/subagent_tool.rs \
  | grep -c "privacy::floor(" ; echo "expect: 1"
cargo test -p biorouter --lib \
  privacy::tests::floor_is_crossed_only_where_a_capability_establishes_a_classification \
  | grep "test result:" ; echo "expect: 1 passed; 0 failed"
# ⚠ Assert the printed count, not "PASS": a named filter that resolves to
# nothing prints `0 passed` and exits 0 (see "Which test filters are validated").
# Expected with BOTH lines of EXPECTED uncommented. A failure naming
# subagent_tool.rs with count 2 means the extension retain used `floor` instead
# of the shared refusal predicate — read the comment in Step 3 (c) before
# "fixing" the constant.
# And the spawn refusal is a real variant, not an invented constructor.
grep -c "PrivateChildOfPublicParent" crates/biorouter/src/privacy/refusal.rs ; echo "expect: >= 2 (variant + constructor)"
```

**What this catches.** Stamping the tier in `create_subagent_session`'s INSERT without reordering —
which is literally what the design says to do, compiles, and produces an orphan `SubAgent` row on
every refused spawn (durably, and in a detached task on the background path). Test 3 is the only
thing that fails it, and the ordering grep is what makes the fix visible in review.

- [ ] **Step 6: Commit**

```bash
git add crates/biorouter/src/agents/subagent_tool.rs crates/biorouter/src/agents/subagent_task_config.rs \
        crates/biorouter/src/privacy/refusal.rs crates/biorouter/src/privacy/mod.rs
git commit -m "feat(privacy): spawn inheritance - resolve the tier before the row, filter private extensions (#56)"
```

---

### Task 24: The two shipped features the gates break

**C2 — a scheduled job created from a private session becomes permanently, silently broken.**
`scheduler.rs:846-850` builds its provider from `Config::global()` **only**, `:867` binds. Under
Gate A that bind is refused and `run_workflow_job` returns `Err` on every cron tick forever, with no
repair affordance since a new session is minted per run.

**H4 — Agent Drafter's per-turn route restore is refused once the route ratchets.**
`apps.rs:2181-2229` snapshots `prev` (`:2208`), binds the route provider (`:2211`), runs the turn,
then restores `prev` at `:4073` with `let _ = agent.update_provider(prev, &session_id).await;` — the
error **discarded**. Sequence: public app session, route pinned to `versa_azure` → Gate A allows the
bind → Gate B ratchets on that turn → the restore of a public `prev` is now refused, silently, and
the app session is stuck on the route's private provider for every later turn. Design §6.4 asserts
this transient switch "does not raise it"; it is precisely what makes it ratchet.

**Files:**

| Action | Path | Anchor (re-verified at `9558c346`) |
|---|---|---|
| Modify | `crates/biorouter/src/scheduler.rs` | provider construction `:846-850`; bind `:867`; `.reply(` `:897` |
| Modify | `crates/biorouter-server/src/routes/apps.rs` | `apply_route_for_turn` `:2181-2229` (`prev` `:2208`, bind `:2211`); restore `:4073`; `configure_worker_provider` `:1480-1516` (global fallback `:1503-1514`); `ClientFrame::ModelSelect` `:3409-3428` (bind `:3418`, variant `:323`); `provider_class` `:2089`, `LOCAL_PROVIDERS` `:2068`, `INSTITUTIONAL_PROVIDERS` `:2074`, consumers `:2114`/`:2123`, test table `:6447` |

- [ ] **Step 1: Write the failing tests**

```rust
#[tokio::test]
async fn a_scheduled_job_from_a_private_session_still_runs() {
    let creator = private_session_on("versa_azure").await;
    let job = create_schedule_from(&creator).await;
    let run = tick(&job).await;
    assert!(run.is_ok(), "{run:?}");
    assert_eq!(run.unwrap().session.provider_name.as_deref(), Some("versa_azure"));
}

#[tokio::test]
async fn a_route_that_ratchets_an_app_session_does_not_strand_it() {
    let app = public_app_session().await;
    pin_route(&app, "versa_azure").await;
    run_turn(&app).await;                                     // Gate B ratchets to private
    // The restore of the public `prev` must not silently fail and leave the
    // session on the route's private provider forever.
    let after = row(&app.session_id).await;
    assert_eq!(after.privacy_tier, SessionClassification::Private);
    assert_eq!(after.provider_name.as_deref(), Some("versa_azure"),
               "restore was refused, so the route provider must remain — deliberately, not silently");
    assert!(last_app_notice().contains("private"), "the user is told");
}

#[test]
fn provider_class_is_not_inverted_any_more() {
    // The existing table at :6447 never exercises a versa_* name, which is why
    // the inversion has been green: versa_azure and versa_bedrock match neither
    // exact list nor the substrings `local`/`institution`, so they fall to
    // External — while bare `azure`, `bedrock`, `aws_bedrock`, `databricks` and
    // `vertex` are listed Institutional.
    assert!(provider_is_private_for_app("versa_azure"));
    assert!(provider_is_private_for_app("versa_bedrock"));
    assert!(provider_is_private_for_app("llamacpp"));
    assert!(!provider_is_private_for_app("aws_bedrock"));
    assert!(!provider_is_private_for_app("azure_openai"));
    assert!(!provider_is_private_for_app("databricks"));
}

#[tokio::test]
async fn an_unpinned_worker_profile_inherits_the_main_agents_provider() {
    // R5. configure_worker_provider has NO branch that reads the main agent's
    // provider (:1480-1516), so an unpinned profile falls to Config::global()
    // at :1503-1514 and a worker under a versa_azure app runs on the user's
    // commercial default.
    let app = app_session_on("versa_azure").await;
    assert_eq!(worker_provider_for(&app, "researcher").await.get_name(), "versa_azure");
}
```

- [ ] **Step 2: Run** → all four **FAIL** (test 3 fails on the first two assertions).

- [ ] **Step 3: Implement**

(a) **Scheduler**: resolve the provider from the **creating session's** `provider_name` before
falling back to `Config::global()` — which is also what R5 wants — and surface a job-level error in
the schedules UI rather than a silent per-tick `Err`.

(b) **`apply_route_for_turn`**: make the restore ratchet-aware. Attempt it; on a `PrivacyRefusal`,
leave the route provider bound, emit one `ui_notify`-class notice to the app, and log at `warn!`
with the stable event name `app_route_restore_refused`. Do **not** silently discard.

(c) **Replace `provider_class`** with the shared tier. `LOCAL_PROVIDERS`/`INSTITUTIONAL_PROVIDERS`
and the substring tests go; `provider_allowed_for_app` (`:2114`) and `resolve_route` (`:2123`) call
`Provider::tier()` on the constructed instance. Add the four `versa_*`/`aws_bedrock` rows to the
existing table at `:6447` rather than writing a new one — **and rename that test
`provider_class_table` → `provider_tier_table` in the same edit.** The name is not cosmetic: it is
what the gate below asserts on, because `grep -c "fn provider_class"` cannot distinguish a surviving
`fn provider_class` from a surviving `fn provider_class_table`, and a test still called
`provider_class_table` is the tell that the taxonomy was patched rather than replaced.

(d) **`configure_worker_provider`**: read the main agent's provider before the global fallback, and
extend the §3.7 admission check (which today inspects only an explicit pin) to cover the fallback.

`ClientFrame::ModelSelect` (`:3409-3428`, bind at `:3418`) is fixed with **zero new code** — it goes
through `Agent::update_provider`, so Gate A covers it. Note that `GET /apps/{id}/agent` is exempt
from secret-key auth (`auth.rs:52-77`, `is_public_app_get` matching the tail `["agent"]` at `:76`),
which is exactly why it must be covered by the bind gate rather than by a route check. Lock it in
with a test rather than adding one.

- [ ] **Step 4: Run**

```bash
cargo test -p biorouter --lib scheduler
cargo test -p biorouter-server --lib routes::apps
cargo test -p biorouter-mcp --lib agent_drafter::
node scripts/agent-drafter/ui-control-harness.mjs
```

- [ ] **Step 5: Gate**

```bash
# The inverted classifier is gone, not patched. ⚠ THE TEST MUST BE RENAMED TOO.
# `grep -c "fn provider_class"` is 2 today — the fn at :2089 AND the test
# `provider_class_table` at :6447 — so "expect: 0" means the test becomes
# `provider_tier_table`. The first version of this gate paired that zero-count
# with an awk alternation that still BLESSED `fn provider_class_table`; the two
# lines contradicted each other and one of them was always red. There is one
# name now, in both places.
grep -c "LOCAL_PROVIDERS\|INSTITUTIONAL_PROVIDERS" crates/biorouter-server/src/routes/apps.rs
echo "expect: 0 (4 today)"
grep -n "fn provider_class" crates/biorouter-server/src/routes/apps.rs
echo "expect: no output (2 today: the fn :2089 and the test :6447 — BOTH are renamed)"
grep -c "fn provider_tier_table" crates/biorouter-server/src/routes/apps.rs ; echo "expect: 1"
# The table exercises the names that made the inversion green.
awk '/fn provider_tier_table/,/^        }/' crates/biorouter-server/src/routes/apps.rs \
  | grep -c "versa_azure\|versa_bedrock\|aws_bedrock" ; echo "expect: >= 3 (1 today, aws_bedrock only)"
# The restore no longer discards its error.
grep -c "let _ = agent.update_provider" crates/biorouter-server/src/routes/apps.rs ; echo "expect: 0 (2 today)"
```

**What this catches.** For (c), a fix that adds `"versa_azure"` to `INSTITUTIONAL_PROVIDERS` — which
makes the two assertions pass and keeps a second, divergent tier taxonomy in the tree, so the next
private provider is wrong again. The `fn provider_class` zero-count is what forbids it. For (b), the
`let _ =` grep: leaving the discard in place produces a session silently stranded on a provider the
user never chose, and no test that only checks the tier would notice.

- [ ] **Step 6: Commit**

```bash
git add crates/biorouter/src/scheduler.rs crates/biorouter-server/src/routes/apps.rs
git commit -m "fix(privacy): keep scheduled jobs and app routes working under the barrier; replace provider_class (#56)"
```

---

### Task 25: Phase 3 gate

- [ ] **Step 1: Suite, lints, OpenAPI, frontend** — as Task 20 Step 1.

- [ ] **Step 2: The four irreversibility carriers, end to end**

```bash
cargo test -p biorouter --lib -- \
  every_copy_path_carries_the_tier_and_the_provider \
  an_import_with_no_tier_is_private_and_one_with_a_tier_is_only_raised_by_it \
  the_spawn_matrix_holds \
  a_refused_spawn_leaves_no_orphan_row \
  | grep "test result:" ; echo "expect: 4 passed; 0 failed"
# ⚠ The count is the gate. libtest ORs these four names and prints `0 passed`,
# exit 0, if ALL FOUR are misspelled — and `3 passed` if one is. Neither reads
# as a failure. This is the single shape that hid BR-71's most expensive defect.
cargo test -p biorouter --test subagent_delegation
```

- [ ] **Step 3: The ratchet cannot be reversed by anything in the tree**

```bash
# The whole audit surface. Exactly one statement outside the migration may
# lower it, and it does not exist until Task 29.
grep -rn --include='*.rs' "privacy_tier *= *'public'" crates/ | grep -v "DEFAULT 'public'"
echo "expect: no output (Task 29 makes this exactly 1, in privacy/declassify.rs)"
grep -c "privacy_tier = CASE WHEN" crates/biorouter/src/session/session_manager.rs ; echo "expect: 1"
```

- [ ] **Step 4: A real diverge of a real private chat (manual, once)**

```bash
export XDG_CONFIG_HOME=/tmp/privacy-p3-check
# Create a chat, run one turn on versa_azure, confirm the row is private,
# then diverge it through the GUI route and confirm the child is private and
# carries provider_name — which is the path that shipped broken.
curl -s -X POST "http://127.0.0.1:3000/sessions/$SID/diverge" -H 'X-Secret-Key: test' \
  -H 'Content-Type: application/json' -d '{}' | python3 -m json.tool | grep -E "privacy_tier|provider_name"
# Expected: "privacy_tier": "private", "provider_name": "versa_azure"
```

- [ ] **Step 5: Adversarial review of the phase diff; every finding addressed.**

---

# Phase 4 — user-facing

Seven tasks. **§14.5's pairing-aware extension state is a prerequisite of Gate C, not an ergonomic
extra**: Gate C returns `ErrorData` from inside `dispatch_tool_call`, so the refusal never enters
`PermissionCheckResult` and produces **no approval card, no denial record and nothing in the GUI at
all** — only text in the model's context. Without Task 28 the user's entire experience of Gate C is
a tool that silently does not work, which is the "the OMOP tool is broken" outcome §16(5) predicts.

### Task 26: `PrivacyBadge`, and the contrast assertions the design's spec would fail

**Departure D4.** The design's §14.1 tokens do not survive measurement. Measured with the repo's own
`ui/desktop/scripts/lib/theme-tokens.mjs` across all six family×mode scopes (parchment, alma-mater,
roche-limit × light, dark):

| Pair | parchment:light | parchment:dark | alma:light | alma:dark | roche:light | roche:dark |
|---|---|---|---|---|---|---|
| `--text-default` on `--background-muted` (**Private, chosen**) | 14.30 | 13.86 | 14.44 | 12.91 | 15.13 | 13.43 |
| `--text-muted` on `--background-muted` (**Public, chosen**) | 6.20 | 6.66 | 5.50 | 7.26 | 6.25 | 6.25 |
| `--text-default` on `--sidebar` (**dot, chosen**) | 13.01 | 13.86 | 14.02 | 15.41 | 15.53 | 15.29 |
| `--text-subtle` on `--background-medium` (design's Public label, on the row-hover ground) | 4.89 | **3.75** | **4.45** | **4.28** | 4.79 | 4.97 |
| `--border-subtle` vs `--background-muted` (design's Public hairline) | 1.23 | **1.00** | 1.16 | 1.24 | 1.16 | 1.18 |
| `--border-subtle` vs `--background-medium` | 1.14 | 1.38 | **1.00** | 1.05 | 1.08 | 1.05 |
| `--border-strong` vs `--background-muted` (the best border token) | 1.53 | 1.38 | 1.35 | 1.58 | 1.38 | 1.45 |

Three conclusions. `--text-standard` does not exist. The Public hairline would ship **invisible** —
parchment:dark measures exactly 1.00, the same colour — and **no border token in this system reaches
3:1 on any ground**, so an outline pill is not expressible here at all. And a `--text-subtle` label
drops under AA the moment the user hovers a History row in three of the six scopes.

`ui/desktop/scripts/check-contrast.mjs` passes 252 assertions today and looks at **none** of those pairs: its
`TEXT_GROUNDS` (`:70-78`) is `app, canvas, default, muted, sidebar`, and `--background-medium`
appears only in `RING_GROUNDS` (`:83`). Which also means the app's own `Badge` default tone
(`neutral` = `bg-background-medium text-text-muted`) is outside the audit; it happens to pass
(4.75–6.18) and nothing holds it there.

**Files:**

| Action | Path | Anchor (re-verified at `9558c346`) |
|---|---|---|
| Create | `ui/desktop/src/components/ui/PrivacyBadge.tsx` | new |
| Modify | `ui/desktop/src/components/ui/badge.tsx` | `toneClass` `:9-16` (six tones); `Badge` `:26` |
| Modify | `ui/desktop/scripts/check-contrast.mjs` | `TEXT_GROUNDS` `:70-78`; `RING_GROUNDS` `:83`; `assert()` `:55`; `discoverFamilies(css)` `:45` |
| Reference | `ui/desktop/src/components/ui/BuiltInBadge.tsx` | `:3` — the "one badge, many surfaces" precedent to copy in *shape*, not in styling (`bg-background-strong/50 text-text-muted/90` measures **2.90:1** in parchment:dark) |

- [ ] **Step 1: Write the failing tests**

```js
// in scripts/check-contrast.mjs, inside the per-scope loop
  // Issue #56. The two badge fills and the dense-surface dot.
  assert(`${theme}: privacy Private label`, '--text-default', '--background-muted', 4.5, scope);
  assert(`${theme}: privacy Public label`,  '--text-muted',   '--background-muted', 4.5, scope);
  assert(`${theme}: privacy dot on sidebar`, '--text-default', '--sidebar', 3.0, scope);
  assert(`${theme}: privacy dot on tab`,     '--text-default', '--background-default', 3.0, scope);
```

and — the higher-value change, which would have caught the design's spec on its own — start auditing
`--background-medium`, the ground `biorouter-list-row`, `SessionItem` and `ExtensionItem` all paint
on hover, and which `check-contrast.mjs` looks at today only for the focus ring:

```js
  // The row-hover ground (design.md's `biorouter-list-row`). Issue #56 needs it
  // because both privacy pills sit on rows that paint it, and nothing has ever
  // asserted a text ratio against it.
  //
  // ⚠ Two tokens, NOT the TEXT_GROUNDS triple. `--text-subtle` on
  // `--background-medium` is sub-AA in three of the six scopes — parchment:dark
  // 3.75, alma-mater:light 4.45, alma-mater:dark 4.28 (measured with this
  // script's own resolver; see the D4 table above). Those three are a
  // PRE-EXISTING theme gap, not something #56 introduces: the app already
  // paints subtle text on hover rows and CI has never looked. Adding
  // `--background-medium` to TEXT_GROUNDS would therefore turn this task red on
  // arrival with only a theme edit to fix it, which Step 5 forbids. Audit the
  // two tokens this feature actually uses, and leave the third to the a11y
  // backlog that owns it.
  for (const t of ['--text-default', '--text-muted']) {
    assert(`${theme}: ${t.slice(2)} on --background-medium`, t, '--background-medium', 4.5, scope);
  }
```

⚠ **`RING_GROUNDS` stays exactly as it is.** It is
`const RING_GROUNDS = [...TEXT_GROUNDS, '--background-medium'];` (`:83`). Because
`--background-medium` does **not** move into `TEXT_GROUNDS`, there is no duplicate to remove and no
ring assertion changes. (An earlier version of this task moved the ground and then had to strip
`RING_GROUNDS`' copy to avoid six duplicate ring assertions; that whole manoeuvre is gone with the
move.) Do not "tidy" line 83.

**The arithmetic, so the expected total is derived rather than guessed:** 252 today, `+12` from the
new hover-ground block (2 text tokens × 6 family×mode scopes), `+24` from the four badge assertions
(× 6 scopes), `+0` from rings → **288**.

Verify the 252 decomposes as you expect before trusting the delta: per scope the script runs
15 (TEXT_GROUNDS 5 × 3 tokens) + 6 (RING_GROUNDS) + 2 (accent) + 8 (4 statuses × 2) + 2 (borders)
+ 3 (code ground) + 3 (focus) + 3 (sidebar icon) = **42**, and 42 × 6 scopes = 252.

```tsx
// PrivacyBadge.test.tsx
it('renders through the app badge primitive and adds no geometry of its own', () => {
  const { container } = render(<PrivacyBadge tier="private" />);
  const el = container.querySelector('[data-testid="privacy-badge"]')!;
  expect(el).toHaveTextContent('Private');
  expect(el.className).toContain('rounded-sm');          // from Badge, not hand-rolled
  expect(el.querySelector('svg')).not.toBeNull();        // never colour alone: shape + glyph + word
});
it('renders nothing in dense mode for a public session', () => {
  const { queryByTestId } = render(<PrivacyBadge tier="public" dense />);
  expect(queryByTestId('privacy-badge')).toBeNull();     // no dot means public
});
```

- [ ] **Step 2: Run**

```bash
cd ui/desktop && node scripts/check-contrast.mjs ; echo "exit=$?"
```

Expected: **FAIL** — the four new assertions cannot resolve `--text-standard` if the design's spec
was followed, and the `--background-medium` addition surfaces three sub-AA `--text-subtle` pairs.
With the chosen tokens the run is clean; the *point* of adding the assertions first is that the
design's tokens fail them.

- [ ] **Step 3: Implement**

```tsx
// Private is the marked state; Public is the quiet state. A badge on
// absolutely everything trains people to stop seeing badges, which defeats
// R10's actual goal — knowing which tier you are in BEFORE hitting a wall.
//
// Both states use a FILL, not an outline: measured across all six family x mode
// scopes, no border token in this design system reaches 3:1 against
// --background-muted or --background-medium (--border-subtle is 1.00-1.38,
// --border-strong 1.35-1.58), so the design's hairline Public pill would be
// invisible — literally identical colours in parchment:dark.
export function PrivacyBadge({ tier, dense = false, className }: PrivacyBadgeProps) {
  if (dense && tier === 'public') return null;             // no dot means public
  if (dense) {
    return <span data-testid="privacy-badge" data-privacy="private"
                 title="Private — only private models can read this chat"
                 className={cn('h-1.5 w-1.5 rounded-full bg-text-default', className)} />;
  }
  return (
    <Badge data-testid="privacy-badge" data-privacy={tier}
           className={cn('bg-background-muted',
                         tier === 'private' ? 'text-text-default' : 'text-text-muted',
                         className)}>
      {tier === 'private' ? <ShieldIcon className="h-3 w-3" /> : null}
      {tier === 'private' ? 'Private' : 'Public'}
    </Badge>
  );
}
```

- [ ] **Step 4: Run**

```bash
cd ui/desktop && node scripts/check-contrast.mjs && npm run themes -- --check
npx vitest run PrivacyBadge 2>&1 | tail -5
```

Expected: `OK — all 288 contrast assertions pass` (252 + 12 + 24 + 0, per the arithmetic above),
`OK — generated artifacts are current (3 themes)`, and **1 file / 2 tests**. Read a wrong total
rather than "fixing" the theme: **294** means `--background-medium` went into `TEXT_GROUNDS` after
all — the run then shows `3 FAIL` on `--text-subtle` and exits 1, and the only way to green is a
theme edit Step 5 forbids; **276** means the hover-ground block landed outside the per-scope loop
and ran once instead of six times; **264** means the four badge assertions did.

- [ ] **Step 5: Gate**

```bash
cd ui/desktop
# The assertion count is the tell that the new checks actually ran.
node scripts/check-contrast.mjs | tail -1 ; echo "expect: OK — all 288 contrast assertions pass"
# RING_GROUNDS is UNTOUCHED. `--background-medium` never moved into TEXT_GROUNDS,
# so there is no duplicate — and a worker who removes this line anyway silently
# deletes six ring assertions.
grep -c "RING_GROUNDS = \[...TEXT_GROUNDS, '--background-medium'\]" scripts/check-contrast.mjs
echo "expect: 1 — unchanged from today"
# The three sub-AA pairs stayed OUT of the audit, deliberately and by name. If
# this is non-zero the run is red for a reason this task cannot fix.
grep -c "text-subtle.*--background-medium\|'--text-subtle', '--background-medium'" scripts/check-contrast.mjs
echo "expect: 0 — see the ⚠ in Step 1; parchment:dark 3.75, alma:light 4.45, alma:dark 4.28"
# Zero theme work: no generator run, no new token.
npm run themes -- --check && git diff --stat themes/ src/styles/themes.generated.ts
echo "expect: OK, and an empty diffstat"
# The token that does not exist is not used, and the one that does is.
grep -rn "var(--text-standard)" src | wc -l ; echo "expect: 0"
grep -rn -- "--text-standard" src/components themes | wc -l ; echo "expect: 0 (the only tree-wide hit is a comment in src/styles/search.css saying the token does not exist)"
grep -c "text-text-default" src/components/ui/PrivacyBadge.tsx ; echo "expect: >= 1"
# It rides the one primitive and hand-rolls none of Badge's geometry. `rounded-full`
# on the dense dot is NOT Badge geometry and is expected; what must not appear is a
# second copy of the pill's radius, padding or type scale, which is what badge.tsx's
# own doc-comment exists to prevent (and what BuiltInBadge already does wrong —
# `bg-background-strong/50 text-text-muted/90` measures 2.90:1 in parchment:dark).
grep -c "from './badge'" src/components/ui/PrivacyBadge.tsx ; echo "expect: 1"
grep -cE "rounded-sm|px-1\.5|py-0\.5|text-\[11px\]" src/components/ui/PrivacyBadge.tsx
echo "expect: 0 — all four come from Badge"
```

⚠ **A `grep -rn -- "--text-standard" ui/desktop/src | wc -l` returns `1`, not `0`** — the hit is a
comment in `src/styles/search.css:2` that says the token does not exist. A gate written as
"expect: 0" against that path fails on correct code. Scope the grep to `src/components` and
`themes`, and pair the zero-count with a positive count for the real token.

**What this catches.** A worker copying §14.1 verbatim ships a `var(--text-standard)` label that
resolves to nothing and inherits whatever colour it lands on, and a Public pill that is invisible in
parchment:dark. Neither produces an error, neither fails a screenshot review at a glance, and both
pass the current 252 assertions. The `--background-medium` block is what turns that class of gap
into a CI failure for every future chip on a hover row, not just this one.

**What it deliberately does NOT catch, and why the number is 288 rather than 294.** An earlier
version of this task added `--background-medium` to `TEXT_GROUNDS` wholesale, which audits three
tokens rather than two and totals 294. Three of those eighteen assertions **fail**:
`--text-subtle` on `--background-medium` measures 3.75 (parchment:dark), 4.45 (alma-mater:light) and
4.28 (alma-mater:dark) — the same three numbers this task's own D4 table already prints, from the
same resolver. So the gate demanded `all 294 pass` from a run that exits 1 with `3 FAIL`, while
Step 5 forbade the only available fix ("Zero theme work … an empty diffstat"). It was unreachable by
construction, and it was quoted in three places. The pair is a **pre-existing** a11y gap — the app
paints subtle text on hover rows today and CI has never asserted it — so it belongs to the theme
backlog, not to #56. [Open question 16](#open-questions) carries it. Do not close it here by
lowering a threshold: an assertion with a fudged floor is worse than an absent one.

- [ ] **Step 6: Commit**

```bash
git add ui/desktop/src/components/ui/PrivacyBadge.tsx ui/desktop/scripts/check-contrast.mjs \
        ui/desktop/src/components/ui/PrivacyBadge.test.tsx
git commit -m "feat(ui): PrivacyBadge on the shared primitive, with contrast assertions for its grounds (#56)"
```

---

### Task 27: Badges on every session surface

**Files:**

| Action | Path | Anchor (re-verified at `9558c346`) |
|---|---|---|
| Modify | `ui/desktop/src/components/sessions/SessionListView.tsx` | the row's inner `SessionItem` `React.memo` at `:653` — it **shadows** the exported `components/sessions/SessionItem.tsx`; badge both or neither |
| Modify | `ui/desktop/src/components/sessions/SessionItem.tsx` | the exported component |
| Modify | `ui/desktop/src/components/sessions/SessionHistoryView.tsx` | `SessionHeader` `:59`, `title={session.name}` `:276`, metadata row `:285-310` |
| Modify | `ui/desktop/src/components/BaseChat.tsx` | `SessionNamePill` render at `:2136` |
| Modify | `ui/desktop/src/components/SessionNamePill.tsx` | props `:10-16`; `SessionNamePill` `:18`. ⚠ It owns the app's one existing per-session overflow menu (`MoreHorizontal` → rename / diverge, `:1-45`) — **do not put declassify there** (§12.1) |
| Modify | `ui/desktop/src/components/chatGroups/ChatTabStrip.tsx` | ⚠ `.br-tab__dot` at `:306` (`group-hover:hidden`, `aria-label="Running"`) and `:377` is the **running** indicator; CSS `main.css:1388-1395` (7 px, `--text-accent`, pulsing `br-tab-pulse`). Use a distinct class; tab geometry is `min-width: 88px; max-width: 190px; gap: 7px` (`main.css:1178-1212`), so a pill is impossible |
| Modify | `ui/desktop/src/components/BioRouterSidebar/RecentChats.tsx` | rows `:191-212` (`h-8`, `ActiveChatIndicator` `:212`); fed by `useSidebarSessions` → `GET /sessions/sidebar` → `SessionSummary`, which Task 6 gave `privacy_tier` |

- [ ] **Step 1: Write the failing tests — one per surface, and both counts**

```tsx
it.each([
  ['SessionListView',      () => render(<SessionListView sessions={[privateSession]} />)],
  ['SessionItem',          () => render(<SessionItem session={privateSession} />)],
  ['SessionHistoryView',   () => render(<SessionHistoryView session={privateSession} />)],
  ['BaseChat header',      () => render(<BaseChat session={privateSession} />)],
  ['RecentChats',          () => render(<RecentChats sessions={[privateSummary]} />)],
])('%s shows the private badge', (_name, mount) => {
  mount();
  expect(screen.getByTestId('privacy-badge')).toHaveAttribute('data-privacy', 'private');
});

it('a public session is unmarked on the dense surfaces', () => {
  render(<ChatTabStrip tabs={[publicTab]} />);
  expect(screen.queryByTestId('privacy-badge')).toBeNull();
});

it('the privacy dot survives hover and does not collide with the running dot', () => {
  render(<ChatTabStrip tabs={[{ ...privateTab, running: true }]} />);
  expect(screen.getByLabelText('Running')).toBeInTheDocument();
  const dot = screen.getByTestId('privacy-badge');
  expect(dot.className).not.toContain('group-hover:hidden');   // the running dot IS hidden on hover
});
```

- [ ] **Step 2: Run** → **FAIL** on every case.

- [ ] **Step 3: Implement** — thread `privacy_tier` from each surface's session object into
`<PrivacyBadge tier={...} dense={...} />`. `SessionNamePill` gains a `privacyTier` prop; `ChatTab`
(`chatGroupsTypes.ts:6-31`) already carries `sessionId`, so the dot is keyed by session. Use
`.br-tab__privacy-dot` (a new class), do **not** hide it on hover, and place it so both dots can
render at once.

⚠ `ChatGroupsState` (`chatGroupsTypes.ts:52-61`) is `{ version, layout, groups, activeGroupId, seq }`
— there is **no `order` field**, whatever a neighbouring plan says.

- [ ] **Step 4: Run**

```bash
cd ui/desktop && npx vitest run SessionListView SessionItem SessionHistoryView \
  BaseChat RecentChats ChatTabStrip 2>&1 | tail -8
```

⚠ Record the expected **file count and test count** in the PR. A vitest filter that matches nothing
fails alone and **passes in company** — one live term hides any number of dead ones, and a file count
of 5 where 6 were expected means one suite died while vitest still exited 0.

- [ ] **Step 5: Gate**

```bash
cd ui/desktop
# Mounted on every surface, not just defined. This is BR-71 Task 28's exact
# defect: a component that renders correctly in isolation and is never mounted.
grep -rl "PrivacyBadge" src/components | sort
# Expected, exactly these 8 at the end of THIS task (Task 28 adds 5 more):
#   ui/PrivacyBadge.tsx, ui/PrivacyBadge.test.tsx,
#   sessions/SessionListView.tsx, sessions/SessionItem.tsx,
#   sessions/SessionHistoryView.tsx, SessionNamePill.tsx,
#   BioRouterSidebar/RecentChats.tsx, chatGroups/ChatTabStrip.tsx
# BaseChat.tsx is modified (it passes `privacyTier` down) but does not import
# the badge itself — do not "fix" its absence from this list by adding an import.
# The running dot is untouched.
grep -c "br-tab__dot" src/components/chatGroups/ChatTabStrip.tsx ; echo "expect: 2 (the pre-existing indicators at :306 and :377)"
grep -c "br-tab__privacy-dot" src/components/chatGroups/ChatTabStrip.tsx ; echo "expect: 1"
```

**What this catches.** Two wrong implementations. (1) A badge component that is written, tested in
isolation and mounted nowhere — indistinguishable from working code at review time, and the exact
state `components/settings/security/SecurityToggle.tsx:14` is in today (fully written, plausible,
**rendered by nothing**; repo-wide grep returns only its own definition). The file-count gate is what
catches it. (2) Reusing `.br-tab__dot`, which pulses at `--text-accent` and is hidden on hover to
make room for the close button — producing a privacy marker that disappears exactly when the user
points at the tab. The third test and the two class counts catch it.

- [ ] **Step 6: Commit**

```bash
git add ui/desktop/src/components/sessions ui/desktop/src/components/SessionNamePill.tsx \
        ui/desktop/src/components/BaseChat.tsx ui/desktop/src/components/chatGroups/ChatTabStrip.tsx \
        ui/desktop/src/components/BioRouterSidebar/RecentChats.tsx ui/desktop/src/styles/main.css
git commit -m "feat(ui): session privacy badges on every list, header and tab surface (#56)"
```

---

### Task 28: The model surfaces, and the pairing state Gate C needs

**Files:**

| Action | Path | Anchor (re-verified at `9558c346`) |
|---|---|---|
| Modify | `ui/desktop/src/components/settings/providers/ProviderGrid.tsx` | ⚠ it **imports `getOrderedProviderGroups`** (`:14`, called `:178`) **and ignores both `label` and `accentClassName`**, hardcoding "Local Models" `:208`, "Institutional Models" `:218`, "Commercial Models" `:227` with its own semantic dots. Editing `providerOrdering.ts` alone changes nothing visible |
| Modify | `ui/desktop/src/components/settings/providers/providerOrdering.ts` | `label`/`accentClassName` `:25-26`; the three group definitions `:68-81`; `classifyProvider` `:39-47` (Task 5 already switched it onto the backend field) |
| Modify | `ui/desktop/src/components/knowledge/IngestPanel/IngestModelPicker.tsx` | **the only consumer of `label`/`accentClassName`** — list `:77`, render `:221-222`. A third model-selection surface the design's §14.2 table never lists |
| Modify | `ui/desktop/src/components/settings/models/subcomponents/SwitchModelModal.tsx` | the real picker (696 lines), takes `sessionId`; option renderer `renderModelOptionLabel` `:129`; options built `:407-442` |
| Modify | `ui/desktop/src/components/settings/models/bottom_bar/ModelsBottomBar.tsx` | trigger `className="flex h-7 min-w-0 max-w-[120px] …"`, `MAX_INLINE_MODEL_LABEL_CHARS = 24` `:30`; `TooltipContent` `:182`; dropdown header `:185-192`; "Lead/Worker Settings" `:206`; `SwitchModelModal` `:215`; `sessionId` prop `:21`/`:33` |
| Modify | `ui/desktop/src/components/bottom_menu/BottomMenuExtensionSelection.tsx` | takes `sessionId` `:26`/`:29`, fetches per-session extension state `:77` — **this is where the true per-session pairing state belongs** |
| Modify | `ui/desktop/src/components/settings/extensions/subcomponents/ExtensionItem.tsx` | `:50-86`, beside `BuiltInBadge` at `:60` |

⚠ **§14.5's "third state computed against the focused session" is new plumbing, not copy.**
`grep -n "sessionId\|session" ui/desktop/src/components/settings/SettingsView.tsx` and
`.../extensions/ExtensionsSection.tsx` both return **zero** hits: Settings has no session awareness,
and with tabs and splits "the focused session" is undefined once the user has navigated away from
chat. Split the requirement: **Settings** computes the pairing state against the **global default
provider's tier** and says so; **the composer's extension selector** computes the true per-session
state, because it already has `sessionId`.

⚠ **A "Private" pill cannot fit in the composer chip.** `max-w-[120px]` with 24-character truncation.
The chip gets a dot; the word goes in the existing tooltip (`:182`) and the dropdown header
(`:185-192`).

- [ ] **Step 1: Write the failing tests**

```tsx
it('the provider grid headers name the two taxonomies with the same words', () => {
  render(<ProviderGrid providers={all} />);
  expect(screen.getByText(/Private · Local/)).toBeInTheDocument();
  expect(screen.getByText(/Private · Institutional/)).toBeInTheDocument();
  expect(screen.getByText(/Public · Commercial/)).toBeInTheDocument();
  expect(screen.queryByText('Institutional Models')).toBeNull();
});

it('a public model is disabled with its reason in a private chat', () => {
  render(<SwitchModelModal sessionId="s1" session={privateSession} />);
  const row = screen.getByRole('option', { name: /Claude Opus/ });
  expect(row).toHaveAttribute('aria-disabled', 'true');
  expect(row).toHaveTextContent(/private chat/i);
});

it('a private extension is visible-but-disabled in the composer, never omitted', () => {
  // Omission is what produces "the OMOP tool is broken". Gate C is invisible in
  // the GUI by construction — it returns ErrorData from inside
  // dispatch_tool_call, so it never enters PermissionCheckResult and produces
  // no approval card and no denial record.
  render(<BottomMenuExtensionSelection sessionId="s1" />);
  const item = screen.getByText('ucsfomopagent').closest('[role="menuitem"]')!;
  expect(item).toHaveAttribute('aria-disabled', 'true');
  expect(item).toHaveTextContent(/public model/i);
});

it('the Settings extension card states which provider it judged against', () => {
  render(<ExtensionsSection />);
  expect(screen.getByText(/unavailable in new chats \(default model is public\)/i)).toBeInTheDocument();
});
```

- [ ] **Step 2: Run** → **FAIL** on all four.

- [ ] **Step 3: Implement** — relabel in **all three** places (`providerOrdering.ts`'s `label`,
`ProviderGrid`'s hardcoded literals, and `IngestModelPicker`'s `section.label` consumption); disable
public rows in `SwitchModelModal` with the inline reason; render the dot + tooltip in
`ModelsBottomBar`; add the visible-but-disabled state to `BottomMenuExtensionSelection` and the
default-provider-scoped state to `ExtensionItem`.

Two copy notes, both from the design and both load-bearing. Institutional → *"Private because
Biorouter recognises this specific UCSF gateway endpoint"*. Azure/Bedrock → *"Public — Biorouter
can't verify where this account's endpoint points."* The obvious wording ("a direct cloud account,
even if your institution pays for it") is **not accurate as shipped**: `azure.rs:204` gives
`AZURE_OPENAI_ENDPOINT` a default of `https://unified-api.ucsf.edu/general`, the same UCSF gateway
`versa_azure` uses.

- [ ] **Step 4: Run**

```bash
cd ui/desktop && npx vitest run ProviderGrid SwitchModelModal ModelsBottomBar \
  BottomMenuExtensionSelection ExtensionsSection IngestModelPicker 2>&1 | tail -8
npx tsc --noEmit
```

- [ ] **Step 5: Gate**

```bash
cd ui/desktop
# The relabel reached the screen, not just the data.
grep -cE "Local Models|Institutional Models|Commercial Models" src/components/settings/providers/ProviderGrid.tsx
echo "expect: 0"
grep -c "section.label" src/components/knowledge/IngestPanel/IngestModelPicker.tsx ; echo "expect: 1 (still consumed — the relabel is in providerOrdering.ts)"
# Settings did not grow a fake focused-session concept. The zero-count is the
# real half and it is 0 today, so it is a genuine tripwire.
grep -c "sessionId" src/components/settings/extensions/ExtensionsSection.tsx ; echo "expect: 0 (0 today)"
# ⚠ `grep -c "sessionId" BottomMenuExtensionSelection.tsx ; expect: >= 2` was
# VACUOUS: that file already threads a session id 17 times, so it was green
# before and after. What this task adds there is the *tier-aware disabled row*,
# so assert THAT — anchored on the prop the component must now receive and on
# the reason string the test asserts.
grep -c "capabilityTier\|privacyTier" src/components/bottom_menu/BottomMenuExtensionSelection.tsx
echo "expect: >= 1 (0 today)"
grep -ci "public model" src/components/bottom_menu/BottomMenuExtensionSelection.tsx
echo "expect: >= 1 (0 today) — the visible reason, matching Step 1's /public model/i"
grep -c "aria-disabled" src/components/bottom_menu/BottomMenuExtensionSelection.tsx
echo "expect: >= 1 (0 today) — visible-but-disabled, never removed from the list"
# The chip has no pill.
grep -c "PrivacyBadge" src/components/settings/models/bottom_bar/ModelsBottomBar.tsx ; echo "expect: 1 (0 today)"
grep -c "dense" src/components/settings/models/bottom_bar/ModelsBottomBar.tsx ; echo "expect: >= 1 (0 today)"
```

**What this catches.** The obvious edit — changing the three `label` strings at
`providerOrdering.ts:68/74/80` — changes **nothing visible**, because `ProviderGrid` imports the
function and ignores the field. The zero-count on the hardcoded literals is the only gate that
notices, and the `IngestModelPicker` line is there so a worker does not "clean up" the now-unused
field and break the one surface that *does* read it. The `sessionId` counts catch the other wrong
implementation: threading a fabricated "focused session" into Settings, where no such concept exists.

- [ ] **Step 6: Commit**

```bash
git add ui/desktop/src/components/settings ui/desktop/src/components/bottom_menu \
        ui/desktop/src/components/knowledge/IngestPanel/IngestModelPicker.tsx
git commit -m "feat(ui): tier badges and pre-flight states on every model and extension surface (#56)"
```

---

### Task 29: Declassification — the dialog, the route, and the audit

⚠ **The design's stated attachment point does not exist.** §12.1 says "History → the session's own
row → overflow menu". `SessionListView`'s row has **no** overflow menu: it has a `DropdownMenu`
whose trigger is a `NewWindow` icon labelled `Launch options for ${session.name}` (`:792-812`)
containing exactly one item, "Open in new window" (`:806-811`), plus three bare icon buttons — Edit
(`:815`), Export (`:829`), Delete (`:843`) — inside an `sm:opacity-0 sm:group-hover:opacity-100`
cluster (`:788`). Adding a real `MoreHorizontal` menu is part of the task, not a given.

⚠ **A typed confirmation would be the first in the app.**
`grep -rn "toUpperCase() ===\|=== 'DELETE'\|confirmationText\|confirmPhrase" ui/desktop/src/components`
returns **zero** product hits. `ConfirmationModal` (`ui/desktop/src/components/ui/ConfirmationModal.tsx:11`) takes
`message: string` and has no input and no focus control; `ui/desktop/src/components/ui/dialog.tsx` has **no**
`onOpenAutoFocus` handling and renders a close button whenever `showCloseButton = dismissible`
(`:44-45`, `:75`), so Radix's default focus lands on the close button, not Cancel.

**Files:**

| Action | Path | Anchor (re-verified at `9558c346`) |
|---|---|---|
| Create | `crates/biorouter/src/privacy/declassify.rs` | new |
| Create | `ui/desktop/src/components/ui/DangerousConfirmDialog.tsx` | new — used by **both** typed confirmations (§12.4 and §14.6) so they cannot diverge |
| Create | `ui/desktop/src/components/sessions/DeclassifySessionDialog.tsx` | new — shared by both entry points |
| Modify | `crates/biorouter-server/src/routes/session.rs` | new `POST /sessions/{session_id}/declassify`; route table beside `:1013`/`:1029` |
| Modify | `crates/biorouter-server/src/auth.rs` | `is_public_app_get` `:52-77`; `check_token` `:80-126` |
| Modify | `ui/desktop/src/components/sessions/SessionListView.tsx` | the row control cluster `:788-855` |
| Modify | `ui/desktop/src/components/sessions/SessionHistoryView.tsx` | `SessionHeader`'s `actionButtons` slot `:230-266` |
| Reference | `ui/desktop/src/components/settings/app/ResetPanel.tsx` | `:344-387` — the closest precedent: a bespoke `Dialog` that **previews exactly what will be destroyed** before confirming, with a `variant="destructive"` confirm at `:383` |

- [ ] **Step 1: Write the failing tests**

```rust
#[tokio::test]
async fn only_a_user_confirmation_can_lower_the_tier() {
    // UserConfirmation is a ZST whose constructor is invoked in exactly one
    // place: the HTTP handler, after it has matched the typed confirmation. No
    // MCP server, no ToolRouter, no workspace_* handler and no CLI subcommand
    // can construct one. "An agent cannot call this" is enforced by Rust module
    // privacy, not by the route being undocumented.
    let s = private_session_with_reason("mcp:ucsfomopagent").await;
    declassify(&sm, &s.id, UserConfirmation::for_test()).await.unwrap();
    let row = reread(&s.id).await;
    assert_eq!(row.privacy_tier, SessionClassification::Public);
    assert_eq!(row.privacy_reason.as_deref(), Some("declassified_by_user"));
    // A declassified session must never be indistinguishable from one that was
    // always public.
    assert_eq!(audit_rows(&s.id).await.len(), 1);
    assert_eq!(audit_rows(&s.id).await[0].actor_kind, "user");
    // The bound provider is left exactly as it was: a public chat may run a
    // private model, and that direction was never restricted.
    assert_eq!(row.provider_name.as_deref(), Some("versa_azure"));
}

#[tokio::test]
async fn the_route_needs_more_than_the_secret_key() {
    // §9.3 A1: the secret is reachable from any developer-enabled agent shell,
    // so `X-Secret-Key` alone is not a human. Note that a test asserting the
    // route is not in the public-GET exemption list is VACUOUSLY true —
    // is_public_app_get only matches GETs under /apps/{id} with an explicit
    // tail allowlist (auth.rs:52-77) and can never match a POST under /sessions.
    assert_eq!(post_declassify(no_headers()).await.status(), 401);
    assert_eq!(post_declassify(secret_key_only()).await.status(), 403);
    assert_eq!(post_declassify(secret_key_and_capability_token()).await.status(), 200);
}
```

```tsx
it('the phrase gate is real, not decorative', async () => {
  render(<DeclassifySessionDialog session={{ ...s, id: 'abc123def456' }} />);
  const confirm = screen.getByRole('button', { name: /Make public/ });
  await user.type(screen.getByLabelText(/last 6 characters/i), 'ef45');
  expect(confirm).toBeDisabled();
  await user.clear(screen.getByLabelText(/last 6 characters/i));
  await user.type(screen.getByLabelText(/last 6 characters/i), s.name);   // the NAME, not the id
  expect(confirm).toBeDisabled();
  await user.clear(screen.getByLabelText(/last 6 characters/i));
  await user.type(screen.getByLabelText(/last 6 characters/i), 'def456');
  expect(confirm).toBeEnabled();
});

it('Cancel holds initial focus and Enter does not fire the destructive action', async () => {
  const onConfirm = vi.fn();
  render(<DangerousConfirmDialog open phrase="def456" onConfirm={onConfirm} />);
  expect(document.activeElement).toBe(screen.getByRole('button', { name: /Cancel/ }));
  await user.type(screen.getByRole('textbox'), 'def4{Enter}');
  expect(onConfirm).not.toHaveBeenCalled();
});

it('a turn-only session gets the single-click path, an mcp session does not', () => {
  const { rerender } = render(<DeclassifySessionDialog session={{ ...s, privacy_reason: 'turn:versa_azure' }} />);
  expect(screen.queryByRole('textbox')).toBeNull();
  expect(screen.getByText(/undo/i)).toBeInTheDocument();
  rerender(<DeclassifySessionDialog session={{ ...s, privacy_reason: 'mcp:ucsfomopagent' }} />);
  expect(screen.getByRole('textbox')).toBeInTheDocument();
});

it('the action is on the History row and NOT in the chat header', async () => {
  render(<SessionListView sessions={[privateSession]} />);
  await user.click(screen.getByLabelText(/More actions for/));
  expect(screen.getByText(/Make this chat public/)).toBeInTheDocument();

  render(<SessionNamePill name="x" privacyTier="private" onRename={vi.fn()} />);
  expect(screen.queryByText(/Make this chat public/)).toBeNull();
});
```

- [ ] **Step 2: Run** → Rust **COMPILE ERROR** (`unresolved module declassify`); TS **FAIL**.

- [ ] **Step 3: Implement**

```rust
// crates/biorouter/src/privacy/declassify.rs

/// Proof that a human confirmed. A ZST whose constructor is `pub(in …)` — it is
/// invoked in exactly one place, the HTTP handler, after it has matched the
/// typed confirmation.
pub struct UserConfirmation(());

/// The ONLY writer in the tree permitted to lower `privacy_tier`. Every other
/// write goes through the session update builder, whose emission is the
/// monotone `CASE WHEN` and physically cannot lower it; this bypasses the
/// builder with its own UPDATE. A repo-grep gate asserts exactly one statement
/// matching `privacy_tier\s*=\s*'public'` exists outside the migration.
///
/// The audit row is written in the SAME transaction, BEFORE the UPDATE.
pub async fn declassify(
    sm: &SessionManager,
    session_id: &str,
    _ok: UserConfirmation,
) -> Result<()> { … }
```

§12.4's graded confirmation, keyed on `privacy_reason`: `mcp:*` (or inherited from an `mcp:*`
ancestor) → typed confirmation on the **last 6 characters of the session id**, displayed beside the
field; `turn:*` only → **single-click with a 5-second undo**, still audited, still user-only, still
not agent-invocable. Not the session name: `is_default_session_name`
(`session_manager.rs:1821`, ⚠ the design cites a stale `:1614-1632`) shows `"New Session"`,
`"CLI Session"`, `"Session <N>"` and `"New session <N>"` are all live placeholders, and
`fallback_session_name` (`:1721`, ⚠ design cites `:1527`) derives a short title from the first user
message — so a name-typed phrase is either a duplicate string shared by dozens of rows, destroying
the justification (forcing the user to look at *which* conversation), or a sentence to retype. An id
suffix is unique, short, and forces row-identity checking.

`DangerousConfirmDialog` owns the `onOpenAutoFocus` handler that puts focus on Cancel, and binds no
keyboard shortcut to confirm. §14.6's `DISABLE PROTECTION` uses the same component.

- [ ] **Step 4: Run**

```bash
cargo test -p biorouter --lib privacy::declassify
cargo test -p biorouter-server --lib routes::session
just generate-openapi && (cd ui/desktop && npm run generate-api)
cd ui/desktop && npx vitest run DeclassifySessionDialog DangerousConfirmDialog SessionListView SessionNamePill 2>&1 | tail -6
```

- [ ] **Step 5: Gate**

```bash
# The entire audit surface for "can the ratchet be reversed".
grep -rn --include='*.rs' "privacy_tier *= *'public'" crates/ | grep -v "DEFAULT 'public'"
echo "expect: exactly 1 hit, in crates/biorouter/src/privacy/declassify.rs"
# Nothing agent-reachable can construct the proof.
grep -rn "UserConfirmation" crates/ | grep -v "privacy/declassify.rs" | grep -v routes/session.rs
echo "expect: no output"
# One dialog primitive, and it is genuinely shared. At the end of THIS task the
# list is the primitive, its own test, and DeclassifySessionDialog; Task 30 adds
# PrivacyPanel as the fourth. Building a second typed-confirm component instead
# would guarantee the two phrases diverge, which is why this is a file list and
# not a count.
cd ui/desktop && grep -rl "DangerousConfirmDialog" src/components | sort
echo "expect: DangerousConfirmDialog.tsx, DangerousConfirmDialog.test.tsx, DeclassifySessionDialog.tsx"
```

**What this catches.** Three wrong implementations. (1) A phrase field that is rendered but never
gates the button — with no precedent in the app to copy, this is the likely outcome; the three-step
type-and-assert test fails it. (2) Relying on Radix defaults for focus, which lands on the close
button and lets Enter submit; the focus test fails it. (3) Attaching the action to
`SessionNamePill`'s existing overflow menu — the obvious slot, and the one §12.1 forbids; the
paired presence/absence test fails it.

- [ ] **Step 6: Commit**

```bash
git add crates/biorouter/src/privacy/declassify.rs crates/biorouter-server/src/routes/session.rs \
        crates/biorouter-server/src/auth.rs ui/desktop/src/components/ui/DangerousConfirmDialog.tsx \
        ui/desktop/src/components/sessions ui/desktop/openapi.json ui/desktop/src/api
git commit -m "feat(privacy): user-only declassification with a graded confirmation and an audit row (#56)"
```

---

### Task 30: Settings → Privacy — the master toggle, its three hardening measures, and the badge it does not hide

**This task changed shape under DR-15.** The first version shipped a Gate-C-scoped switch and a test
asserting that Gates A and D were *unaffected* by it. The operator has since ruled the other way
([Open question 3](#open-questions) is closed): one master toggle, `BIOROUTER_PRIVACY_TIERS`,
default `on`, and with it off **nothing is refused and nothing is sandboxed**. The old scoping
assertion is therefore inverted, not deleted — Gates A and D must now go quiet with the rest.

`SettingsView.tsx` has exactly **three** tabs today —
`grep -c 'data-testid="settings-.*-tab"'` returns 3 (`settings-models-tab` `:93`,
`settings-chat-tab` `:100`, `settings-app-tab` `:109`).

**Files:**

| Action | Path | Anchor (re-verified at `9558c346`) |
|---|---|---|
| Create | `ui/desktop/src/components/settings/privacy/PrivacyPanel.tsx` | new |
| Modify | `ui/desktop/src/components/settings/SettingsView.tsx` | the three `TabsTrigger`s at `:93`/`:100`/`:109` and their `TabsContent`s |
| Modify | `ui/desktop/src/components/ui/PrivacyBadge.tsx` | **created by Task 26**; this task adds the `enforcementOff` presentation |
| Modify | `crates/biorouter/src/privacy/mod.rs` | `privacy_tiers_enabled()` — a `const fn … { true }` stub since Task 14; this task gives it a body |
| Modify | `crates/biorouter/src/config/base.rs` | `get_param` `:755-773` — resolves task-local override → **env var** → `config.yaml`; the toggle must bypass the env branch |
| Modify | `crates/biorouter-server/src/routes/config_management.rs` | `/config/upsert` — `#[utoipa::path]` at `:176`, route registration at `:895`. ⚠ The file is `config_management.rs`; there is no `routes/config.rs` |
| Reference | `crates/biorouter/src/security/security_inspector.rs` | `:70-95` — the always-on floor no config key can lower, the pattern the read-path hardening copies |

⚠ **Nothing is scoped out of the toggle any more, including the knowledge-base barrier.** The
previous version of this task carried a ⚠ saying Task 10C's barrier was deliberately *not*
opt-outable, on the grounds that a KB carries session contents and has no declassification path
(AR-1). That reasoning survives as a *cost* and is why the confirmation copy below names knowledge
bases explicitly — but it is no longer a carve-out. A user who turns the feature off gets a machine
on which a public model can read a private base, and the dialog says so in those words.

- [ ] **Step 1: Write the failing tests**

The gate that matters is behavioural and it is a **matrix**, because the wrong implementation this
task must reject is *a master toggle wired to some of the gates*. A textual grep cannot separate
"wired to ten" from "wired to three"; ten paired assertions can, and each one names the gate it
covers so a failure points at the missing wiring rather than at "privacy is broken".

```rust
/// DR-15. Ten gates, each asserted in both toggle positions. The `on` column is
/// what every other task already tests, restated here so this test fails when a
/// gate is wired to the toggle but broken, not only when it is unwired.
///
/// ⚠ Do not collapse this into a loop over closures returning `bool`. Each row
/// needs a different fixture, and the value of the test is that a compile error
/// appears when a gate this plan adds later is not represented here.
#[tokio::test]
async fn the_master_toggle_governs_every_gate_in_both_directions() {
    // ---- ON: the shipped default. Every gate refuses. -----------------------
    set_privacy_tiers(true).await;
    let priv_sess = private_session().await;
    assert!(agent.update_provider(public_provider(), &priv_sess.id).await.is_err());   // A  (Task 12)
    assert!(reply_on(public_provider(), &priv_sess.id).await.is_err());                 // B  (Task 13)
    assert!(call_private_tool_via_agent_loop().await.contains("ucsfomopagent"));        // C  (Task 14)
    assert!(assert_extension_reachable_as_public("ucsfomopagent").await.is_err());      // C' (Task 15)
    assert!(!allowed_extension_keys_as_public().await.contains(&"ucsfomopagent".into()));// E (Task 16)
    assert_eq!(search_as(ProviderTier::Public, &db, "cohort").await.results.len(), 0);  // D  (Task 17)
    assert!(kb_search_as_public(&private_kb).await.is_err());                           // KB (Task 10C)
    assert!(ingest_conversation_as(ProviderTier::Public, &priv_sess.id).await.is_err());// G  (Task 11)
    assert!(assert_alt_provider_allowed(public_provider(), &priv_sess).is_err());       // H  (Task 19)
    assert!(shell_requires_read_deny_sandbox_for(ProviderTier::Public));                // DR-14 (Task 14B)

    // ---- OFF: nothing is refused, and nothing is sandboxed. -----------------
    set_privacy_tiers(false).await;
    assert!(agent.update_provider(public_provider(), &priv_sess.id).await.is_ok());
    assert!(reply_on(public_provider(), &priv_sess.id).await.is_ok());
    assert!(!call_private_tool_via_agent_loop().await.contains("private"));
    assert!(assert_extension_reachable_as_public("ucsfomopagent").await.is_ok());
    assert!(allowed_extension_keys_as_public().await.contains(&"ucsfomopagent".into()));
    assert_eq!(search_as(ProviderTier::Public, &db, "cohort").await.results.len(), 1);
    assert!(kb_search_as_public(&private_kb).await.is_ok());
    assert!(ingest_conversation_as(ProviderTier::Public, &priv_sess.id).await.is_ok());
    assert!(assert_alt_provider_allowed(public_provider(), &priv_sess).is_ok());
    assert!(!shell_requires_read_deny_sandbox_for(ProviderTier::Public));
}

/// AR-7, as an assertion rather than a paragraph: with the toggle off the
/// ratchet does not fire, and turning it back on does not go back and fix it.
/// This is the one behaviour a reader is most likely to assume works the other
/// way, so it is pinned rather than described.
#[tokio::test]
async fn nothing_ratchets_while_the_toggle_is_off_and_re_enabling_does_not_backfill() {
    set_privacy_tiers(false).await;
    let (agent, s) = agent_on(private_provider()).await;
    call_tool_via_http("ucsfomopagent__run_query", &s.id).await.unwrap();
    reply_on(private_provider(), &s.id).await.unwrap();
    assert_eq!(reread(&s.id).await.privacy_tier, SessionClassification::Public,
               "DR-4's two triggers must not fire while the feature is off");

    set_privacy_tiers(true).await;
    assert_eq!(reread(&s.id).await.privacy_tier, SessionClassification::Public,
               "re-enabling must not retro-classify; there is no content scan (AR-7)");
}

#[tokio::test]
async fn no_environment_variable_can_turn_protection_off() {
    // The failure mode is an agent disabling its own protection, and
    // Config::get_param's env branch (config/base.rs:755-773) is the easiest
    // lever in the tree. Read straight from the loaded values map instead.
    std::env::set_var("BIOROUTER_PRIVACY_TIERS", "off");
    assert!(privacy_tiers_enabled(), "an env var disabled the whole feature");
}

#[tokio::test]
async fn the_key_cannot_be_flipped_through_config_upsert() {
    let r = post_config_upsert("BIOROUTER_PRIVACY_TIERS", "off").await;
    assert_eq!(r.status(), 403);
    assert!(r.text().await.contains("Settings"));
}
```

```tsx
it('the Privacy tab exists and its toggle is mounted', async () => {
  render(<SettingsView />);
  await user.click(screen.getByTestId('settings-privacy-tab'));
  expect(screen.getByRole('switch', { name: /Privacy tiers/ })).toBeInTheDocument();
});

// DR-15's badge ruling, as a test. The wrong implementation hides the badge —
// which is what "the feature is off" naively suggests — and the user who
// disabled it in March has no on-screen reminder in September.
it('badges stay visible while enforcement is off, and say so', () => {
  render(<PrivacyBadge tier="private" enforcementOff />);
  expect(screen.getByText(/Private/)).toBeInTheDocument();
  expect(screen.getByText(/enforcement off/i)).toBeInTheDocument();
});
```

- [ ] **Step 2: Run** → **FAIL** on all seven.

- [ ] **Step 3: Implement**

**(a) The predicate.** One function, in `crates/biorouter/src/privacy/mod.rs`, replacing the
`const fn … { true }` stub Task 14 introduced:

```rust
/// The master privacy-tier switch (R7, DR-15). `true` — the default — means
/// every gate, the ratchet and the read-deny sandbox are live. `false` means
/// none of them are.
///
/// Read straight from the loaded config values, **never** through
/// `Config::get_param` (`config/base.rs:755-773`), whose middle branch resolves
/// an environment variable. The threat this closes is specific and cheap: the
/// agent has `developer__shell`, so if the value were env-readable then
/// `BIOROUTER_PRIVACY_TIERS=off biorouterd` — or a line in the user's shell
/// profile — is a one-token disable of the control the agent is subject to.
/// The authoritative value is read once at startup into `TIERS_ENABLED` and
/// changed only by `POST /config/upsert`'s gated arm, so this is a relaxed
/// atomic load per call and is safe to put inside a hot gate.
pub fn privacy_tiers_enabled() -> bool {
    TIERS_ENABLED.load(std::sync::atomic::Ordering::Relaxed)
}
```

**(b) The three hardening measures of §10.6**, unchanged in substance and now protecting more:
(1) read the value bypassing `Config::get_param`'s env branch, straight from the loaded values map;
(2) gate the key in `POST /config/upsert` so the flip must come from Settings → Privacy with its
confirmation; (3) hold the authoritative value in daemon memory from startup. **Not SecretGuard**,
which cannot enforce this: `find_denied_path` scans tool-argument strings and `candidate_is_denied`
requires a literal path token that exists on disk, so
`cd ~/.config/biorouter && python3 -c "open('config.yaml','a')…"` evades it, as does any variable
indirection (§9.3 C1; the module's own doc-comment concedes it is "conservative by design").

The check goes **inside** each gate rather than in an `is_enabled()` wrapper, following the
`SensitiveOpsInspector` pattern, so a mid-session change is honoured and the opt-out is one auditable
line per gate rather than an absent gate.

**(c) The panel.** One switch, on by default, in a new Settings → Privacy tab:

> **Privacy tiers** — On
> Chats on private models (Versa, or a local model) stay private: a public model can't read them,
> can't call a private extension, and can't reach your knowledge bases through the shell.

Turning it **off** requires typing `DISABLE PRIVACY TIERS` and shows, verbatim, all four sentences —
the third and fourth are the ones a user cannot reconstruct for themselves and are why this is a
typed confirmation rather than a switch:

> This turns off **every** privacy guardrail on this machine, for every conversation.
> Commercial models will be able to call UCSF clinical extensions, read private chat history, read
> and write your knowledge bases, and read your saved chats, memories and Biorouter apps straight
> off the disk through the shell.
> **While it is off, Biorouter stops recording which conversations touched private material.**
> Turning it back on will protect what is already marked private — but it cannot go back and mark
> anything that happened while it was off.

While off: a persistent amber strip in the settings sidebar, and every privacy badge in the app
renders muted with the suffix **— enforcement off**.

**(d) The badge, which is the part most likely to be got wrong.** DR-15 keeps badges *visible*
while enforcement is off. The two rejected alternatives, and why:

- **Hide them.** It reads as the tidy answer — the feature is off, so its ornament goes — and it is
  the worst one. The badge is the only place the tier is ever stated; removing it makes an
  *unprotected* machine indistinguishable from a machine with no private material on it, at exactly
  the moment (picking a model, pasting a cohort) when the distinction matters. The person who needs
  the reminder is the person who turned it off and forgot.
- **Leave them unchanged.** A pill that reads plain **Private** while nothing enforces it is not
  information, it is a false statement, and it is worse than no badge because the user acts on it.

So the badge stays and changes what it says. The suffix is on the badge itself rather than only in
the settings strip because badges appear on surfaces the strip does not — the session list, the
model chip, the extension rows.

- [ ] **Step 4: Run**

```bash
cargo test -p biorouter --lib privacy
cargo test -p biorouter-server --lib routes::config_management
cd ui/desktop && npx vitest run SettingsView PrivacyPanel PrivacyBadge 2>&1 | tail -5
```

- [ ] **Step 5: Gate**

```bash
cd ui/desktop
grep -c 'data-testid="settings-.*-tab"' src/components/settings/SettingsView.tsx ; echo "expect: 4 (3 today)"
cd ..
# ── The behavioural gate is Step 1's matrix. These are the cheap structural
# ── checks that catch the two failures a matrix test cannot see.

# (1) The toggle is never read through Config::get_param, whose env branch an
#     agent with a shell can set.
awk '/fn privacy_tiers_enabled/,/^}/' crates/biorouter/src/privacy/mod.rs | grep -c "get_param"
echo "expect: 0"

# (2) There is exactly ONE predicate. A second, narrower flag is how DR-9 comes
#     back through the side door, and it would pass Step 1's matrix as long as
#     the master one is wired everywhere the matrix looks.
grep -rn "fn privacy_.*_enabled\|PRIVACY_.*ENFORCEMENT\|privacy_opt_out" --include='*.rs' crates/ \
  | grep -v "fn privacy_tiers_enabled" | grep -v "privacy_tiers_enabled()"
echo "expect: NO OUTPUT — one switch, one name."

# (3) Every gate reads it. Enumerated by ENCLOSING FUNCTION, not by file: three
#     calls in one file is satisfiable by three calls in one function, and a
#     per-file count cannot tell a file with two gates and one check from a file
#     with two gates and two.
#
#     ⚠ This list is the plan's own gate inventory and must grow with it. If a
#     later task adds a gate, it adds a row here AND a row to Step 1's matrix.
#     A gate absent from both is a gate the toggle does not reach.
check() {  # check <file> <fn-name>
  echo -n "$2: "
  awk "/(async )?fn $2\(/,/^    }/" "$1" | grep -c "privacy_tiers_enabled()"
}
check crates/biorouter/src/agents/extension_manager.rs dispatch_tool_call            # Gate C  + DR-14 guard
check crates/biorouter/src/agents/extension_manager.rs assert_extension_reachable    # Gate C'
check crates/biorouter/src/agents/extension_manager.rs allowed_extension_keys        # Gate E
echo "expect: 1 each"

# (4) The badge is not conditionally unmounted. The wrong implementation is
#     `{enforcementOn && <PrivacyBadge …/>}` at a call site — which no badge
#     unit test can see, because the badge itself is fine.
cd ui/desktop
grep -rn "PrivacyBadge" src/ --include='*.tsx' | grep -E "&&\s*<PrivacyBadge|\?\s*<PrivacyBadge"
echo "expect: NO OUTPUT — the badge renders unconditionally and varies its own presentation."
```

**What this catches.** A panel that is written and never mounted — exactly the state
`components/settings/security/SecurityToggle.tsx:14` is in today (declared, plausible, zero consumers
repo-wide); a toggle read through `Config::get_param`, which makes `BIOROUTER_PRIVACY_TIERS=off` in
an agent shell sufficient to disable the feature the agent is subject to; **a master toggle wired to
three gates out of ten**, which is the failure this task exists to prevent and which every textual
gate in the previous version would have reported green; and a badge hidden by its call site rather
than restyled by itself.

- [ ] **Step 6: Commit**

```bash
git add ui/desktop/src/components/settings ui/desktop/src/components/ui/PrivacyBadge.tsx \
        crates/biorouter/src/privacy/mod.rs \
        crates/biorouter/src/config/base.rs crates/biorouter-server/src/routes/config_management.rs
git commit -m "feat(privacy): one master toggle for the whole privacy-tier feature (#56)"
```

---


### Task 31: The CLI is a required R10 surface

Every repair affordance in Phase 4 so far is a GUI card. `biorouter-cli/src/session/builder.rs:479-484`
resolves the provider as `--provider` flag → saved session provider → workflow's
`biorouter_provider` → global default; two of those four can produce a public provider on a private
session, and one is a **shared workflow YAML** pinning `anthropic`, which would now refuse to run in
any private session with nothing explaining why.

**Files:**

| Action | Path | Anchor (re-verified at `9558c346`) |
|---|---|---|
| Modify | `crates/biorouter-cli/src/session/builder.rs` | provider precedence `:479-484`; `providers::create` `:523`; bind `:601` |
| Modify | `crates/biorouter-cli/src/commands/session.rs` | new `declassify <id>` subcommand |
| Modify | `crates/biorouter-cli/src/session/mod.rs` | the Gate B terminal refusal rendering |
| Modify | `crates/biorouter/src/workflow/` | workflow load-time provider check |

- [ ] **Step 1: Write the failing tests**

```rust
#[tokio::test]
async fn a_workflow_pinning_a_public_provider_fails_at_load_not_mid_turn() {
    let s = private_session().await;
    let err = load_workflow_into(&s, yaml_pinning("anthropic")).await.unwrap_err();
    assert!(err.to_string().contains("pins `anthropic`"));
    assert!(err.to_string().contains("private session"));
    assert_eq!(turns_started(), 0);
}

#[tokio::test]
async fn the_cli_prints_the_tier_and_the_exact_re_run_command() {
    let out = run_cli(&["session", "-r", "--id", &private_session_id]).await;
    assert!(out.contains("Private"));
    let refusal = run_cli(&["session", "-r", "--id", &residual_session_id]).await;
    assert!(refusal.contains("--provider versa_azure"));   // the exact re-run
    assert!(refusal.contains("Versa"));                    // available private models
}

#[tokio::test]
async fn declassify_works_by_id_regardless_of_session_type() {
    // list_sessions filters to ('user','scheduled'), so a private Hidden or
    // Terminal session has no GUI declassification surface. Do NOT add a
    // "System sessions" filter to History — on this machine that surfaces 511
    // hidden sessions into a user-facing list. The CLI escape hatch works by id.
    for t in [SessionType::Hidden, SessionType::SubAgent, SessionType::User] {
        let s = private_session_of_type(t).await;
        run_cli_confirming(&["session", "declassify", &s.id]).await.unwrap();
        assert_eq!(reread(&s.id).await.privacy_tier, SessionClassification::Public);
    }
}
```

- [ ] **Step 2: Run** → **FAIL** on all three.

- [ ] **Step 3: Implement** — (a) tier printed at session start; (b) Gate B's terminal refusal lists
the available private models and the exact re-run command; (c) `biorouter session declassify <id>`
running the same graded confirmation at the terminal; (d) the workflow load-time check.

- [ ] **Step 4: Run** → `cargo test -p biorouter-cli` → **PASS**.

- [ ] **Step 5: Gate**

```bash
# The escape hatch is not type-filtered. ⚠ Check the range is NON-EMPTY first:
# `declassify_command` does not exist today, so the awk range is 0 lines and
# `grep -c` on nothing is 0 — a PASS, before the function is written and again
# if the worker names it anything else.
awk '/fn declassify_command/,/^}/' crates/biorouter-cli/src/commands/session.rs | wc -l
echo "expect: > 1 (0 today — the fn does not exist yet). A 0 here makes the next"
echo "  line vacuous, so read this one first."
awk '/fn declassify_command/,/^}/' crates/biorouter-cli/src/commands/session.rs | grep -c "SessionType"
echo "expect: 0 — it works by id"
# And History did not gain a system-sessions filter. (0 today AND 0 under a
# correct implementation — this one is a genuine tripwire, not a measurement:
# the wrong implementation is the only thing that makes it non-zero.)
grep -rn "System sessions\|include_hidden" ui/desktop/src/components/sessions/ ; echo "expect: no output"
```

**What this catches.** Adding a "System sessions" filter to History as the declassification path for
Hidden sessions — which is the obvious fix and which surfaces 511 hidden sessions on this machine
into a user-facing list, a regression traded for an edge case.

- [ ] **Step 6: Commit**

```bash
git add crates/biorouter-cli crates/biorouter/src/workflow
git commit -m "feat(cli): print the tier, teach the repair, and declassify by id (#56)"
```

---

### Task 32: Phase 4 gate

- [ ] **Step 1: Suite, lints, OpenAPI, frontend, contrast, themes**

```bash
cargo test --workspace --no-fail-fast 2>&1 | tail -20
cargo fmt --check && ./scripts/clippy-lint.sh
just generate-openapi && git diff --exit-code ui/desktop/openapi.json
cd ui/desktop && npx tsc --noEmit && npm run lint:check && npm run test:run 2>&1 | tail -8
node scripts/check-contrast.mjs | tail -1     # expect: OK — all 288 contrast assertions pass
npm run themes -- --check                     # expect: OK — generated artifacts are current (3 themes)
```

⚠ **288 — and it must match Task 26's Step 4 exactly, because two of the three numbers this plan has
quoted here were wrong.** 252 measured on `main` today (42 assertions × 6 family×mode scopes),
`+12` from Task 26's hover-ground block (2 text tokens × 6 scopes), `+24` from the four new badge
assertions (× 6 scopes), `+0` from rings — `RING_GROUNDS` is untouched, because
`--background-medium` never moves into `TEXT_GROUNDS`. The first version said **274**, a number its
own Task 26 contradicted. The second said **294**, which is the total for a variant that puts
`--background-medium` into `TEXT_GROUNDS` — and that run does not print `OK` at all: three of its
eighteen new assertions fail AA (`--text-subtle`, measured 3.75 / 4.45 / 4.28) and it exits 1. Both
versions read to a worker as "the phase failed" when the phase had succeeded. If the printed total
is **294**, the ground moved after all; if **276** or **264**, one of the two new blocks landed
outside the per-scope loop and ran once instead of six times.

- [ ] **Step 2: Live GUI verification over CDP — the four surfaces, in a sandbox**

⚠ **Read [`docs/desktop-ui/launching-the-dev-gui.md`](../desktop-ui/launching-the-dev-gui.md) first.**
Five distinct launcher failures produce symptoms that read as application bugs. In particular:
`env -u ELECTRON_RUN_AS_NODE` (agent shells export it, and Electron then exits instantly with no
window and no error); do **not** use `electron-forge start` (it reads stdin, so `< /dev/null` takes
the app down); pass `--config vite.renderer.config.mts` (a bare `npx vite` skips Tailwind and renders
unstyled serif HTML that is fully functional); set `BIOROUTER_NO_HMR=1` (any save under
`ui/desktop/src` full-reloads the renderer and destroys the session under test); and verify with a
**CDP screenshot**, never `screencapture` of the whole screen.

⚠ **Sandbox the config**: launching the dev GUI can wipe `~/.config/biorouter`. Use
`XDG_CONFIG_HOME=/tmp/privacy-p4-check`.

Check, with evidence: (1) a private chat's badge in History, the chat header, the tab and the
sidebar; (2) switching a private chat to a public model shows the Gate A repair card and **no**
success toast; (3) a private extension in the composer's selector is visible-but-disabled with its
reason; (4) the declassification dialog's phrase gate, Cancel focus, and the resulting
"Public — made public by you on …" badge.

- [ ] **Step 3: The badges are mounted, not merely defined**

```bash
cd ui/desktop
# Enumerate rather than count: Task 27 mounted 6 host files, Task 28 mounted 5
# more, and a bare number invites "fixing" a mismatch by deleting a mount.
grep -rl "PrivacyBadge" src/components | sort
# Expected, exactly these 13:
#   ui/badge-adjacent:  ui/PrivacyBadge.tsx, ui/PrivacyBadge.test.tsx
#   Task 27 hosts:      sessions/SessionListView.tsx, sessions/SessionItem.tsx,
#                       sessions/SessionHistoryView.tsx, SessionNamePill.tsx,
#                       BioRouterSidebar/RecentChats.tsx, chatGroups/ChatTabStrip.tsx
#   Task 28 hosts:      settings/providers/ProviderGrid.tsx,
#                       settings/models/subcomponents/SwitchModelModal.tsx,
#                       settings/models/bottom_bar/ModelsBottomBar.tsx,
#                       bottom_menu/BottomMenuExtensionSelection.tsx,
#                       settings/extensions/subcomponents/ExtensionItem.tsx
grep -c 'data-testid="settings-.*-tab"' src/components/settings/SettingsView.tsx ; echo "expect: 4"
```

- [ ] **Step 4: Adversarial review of the phase diff; every finding addressed.**

---

# Phase 5 — the marketplace

Five tasks. Independent of everything above (O11) — enforcement runs off the compiled-in const, so
the website blocks nothing. Sequenced here only because the `--check` gate needs the generated Rust
file Task 8 created.

### Task 33: `registry.json` v2 and the generator's first hard failures

⚠ **The join key is right by luck today.** `id` is derived from the download filename by
`slugFromUrl` (`build-registry.mjs:33-36`), and `spokeagent-0.4.1` is the **only** version-suffixed
id among the 37 (verified). `cdwagent` and `ucsfomopagent` happen to be un-suffixed and happen to
match `manifest.name`, which `BrxtInstallModal.tsx:154` writes as the config name. The day a private
extension ships a version-suffixed download filename it classifies as unlisted ⇒ **PUBLIC**,
silently. `baam.html:3793` already carries a suffix-stripping workaround for the featured shelf; the
app has none, and a suffix-stripping heuristic in a security path is exactly the thing that is right
until it isn't. **The generator emits an explicit `extension_name` and hard-fails when a `private`
entry lacks one.**

⚠ **The generator has no failure mode at all today** — no `throw`, no `process.exit(1)`. Adding hard
failures is new behaviour for this script, not an extension of existing behaviour.

**Files:**

| Action | Path | Anchor (re-verified at `9558c346`) |
|---|---|---|
| Modify | `landing/scripts/build-registry.mjs` | the `data-license` idiom to copy at `:102`; `slugFromUrl` `:33-36`; emitted object `:107-118` (skills at `:130`/`:135`); registry literal `:155-160` (`version: 1` at `:156`); **the two hardcoded paths this task must parameterise** — `readFileSync(join(ROOT,'baam.html'))` `:20` and `writeFileSync(join(ROOT,'registry.json'), out)` `:163` |
| **Create** | `landing/scripts/fixtures/invalid-privacy.html` | new — one `ext-card` with `data-privacy="maybe"` |
| **Create** | `landing/scripts/fixtures/private-no-name.html` | new — one `ext-card` with `data-privacy="private"` and no `data-extension-name` |
| **Create** | `landing/scripts/fixtures/clinical-unannotated.html` | new — one `ext-card` whose `ext-desc` says "de-identified clinical records" and which carries no `data-privacy` attribute at all |
| **Create** | `landing/scripts/fixtures/happy.html` | new — two well-formed `ext-card`s, one public one private-with-name |
| Modify | `landing/registry.json` | version 1 → 2; 37 extensions, 129 skills |
| Modify | `crates/biorouter/src/privacy/registry_private.rs` | now a generator output |
| Modify | `ui/desktop/src/components/baam/registry.fallback.json` | verified in sync at 37/129, by luck — joins the generator's outputs |
| Modify | `ui/desktop/src/components/baam/registry.ts` | `RegistryExtension` `:8-19` |

⚠ **Each fixture needs the wrapper the parser requires, not just a card.** `pickCards` is called on
the output of `sliceById('extensions-section')` (`:96`), which walks `<div>` depth from
`id="extensions-section"`. A fixture that is a bare `<div class="ext-card">` yields an **empty**
scope, zero cards, zero validations and **exit 0** — a fixture that silently proves nothing. Wrap
every fixture in `<div id="extensions-section"> … </div>`, and confirm each one parses by checking
the generator's own stdout line ("`N extensions`") on the happy fixture before relying on the
three failing ones.

⚠ **`--input` needs `--out` beside it, or the gate destroys `landing/registry.json`.** The script
writes its output unconditionally (`:163`). Running it against a two-card fixture would overwrite the
real 37-extension registry with two entries, and the next gate in this very task
(`json.load(open('landing/registry.json'))`) would then compare the const against the wreckage. Add
both flags in Step 3; a fixture run must write to a temp path or to nothing.

- [ ] **Step 1: Write the failing tests — four fixture runs, and the failures must be VALIDATION failures**

```bash
cd /Users/wgu/Desktop/BioRouter-privacy
for f in invalid-privacy private-no-name clinical-unannotated; do
  out=$(node landing/scripts/build-registry.mjs \
          --input landing/scripts/fixtures/$f.html --out /dev/null 2>&1)
  code=$?
  echo "$f exit=$code"
  echo "$out" | grep -q "data-privacy\|data-extension-name" \
    && echo "  ...and it named the rule" || echo "  ...WRONG: not a validation failure"
done
node landing/scripts/build-registry.mjs \
  --input landing/scripts/fixtures/happy.html --out /tmp/happy-registry.json
echo "happy exit=$?"   # expect 0, and stdout says "2 extensions"
```

**expect:** `exit=1` for the three, each followed by "and it named the rule"; `exit=0` for happy.

⚠ **The stdout check is the whole gate, not decoration.** A bare "exit != 0" passes on *any* error:
an unrecognised flag, a missing fixture file, a path resolved against the wrong base, a
`ReferenceError`. All four of those are what a half-finished implementation actually produces, and
all four would have read green. Asserting that the message names the rule that fired is what makes
the run evidence.

- [ ] **Step 2: Run** → **all four fail today, but not in the way the gate wants.**
`build-registry.mjs` has **no argv handling of any kind**: it ignores `--input`/`--out`, reads the
real `landing/baam.html`, writes the real `landing/registry.json`, and exits **0**. So today the
three failure cases print `exit=0` (red, correctly) — and the fourth, `happy`, prints `exit=0` too
while having read the wrong file entirely and clobbered the registry. Restore
`landing/registry.json` with `git checkout` after this step.

- [ ] **Step 3: Implement** — three pieces, in this order. The first two do not exist at all today.

**(a) argv, and the failure mode.** Replace the two hardcoded paths (`:20`, `:163`) and add the
`fail` helper the validations call — the script has no `throw` and no `process.exit(1)` anywhere, so
`fail` is not "an existing idiom to reuse", it is new:

```js
// --- argv, so the validations can be exercised against fixtures -------------
// Both flags are needed. `--input` alone still WRITES to landing/registry.json,
// so a fixture run would overwrite the real 37-extension catalog with the
// fixture's two entries.
const argv = process.argv.slice(2);
const flag = (name, fallback) => {
  const i = argv.indexOf(name);
  return i === -1 ? fallback : argv[i + 1];
};
const INPUT = flag('--input', join(ROOT, 'baam.html'));
const OUTPUT = flag('--out', join(ROOT, 'registry.json'));
const html = readFileSync(INPUT, 'utf8');

// Collect every violation and report them together, then exit non-zero. One
// `throw` per violation would hide the second and third problems behind the
// first, which is how a publisher ends up fixing this file three times.
const violations = [];
const fail = (msg) => violations.push(msg);
```

and, immediately before the `writeFileSync` at `:163`:

```js
if (violations.length) {
  for (const v of violations) console.error(`registry: ${v}`);
  console.error(`registry: ${violations.length} validation failure(s); nothing written`);
  process.exit(1);
}
```

**(b) the three validations**, beside the `data-license` idiom at `:102`:

```js
  const privacy = first(/data-privacy="([^"]+)"/, card) || 'public';
  const extensionName = first(/data-extension-name="([^"]+)"/, card) || '';
  // The DEFAULT matters more than the extraction: an un-annotated card is
  // public by construction, so R11(ii)'s fail-open direction is enforced by the
  // tool rather than by reviewer discipline.
  if (privacy !== 'private' && privacy !== 'public') {
    fail(`${id}: data-privacy must be "private" or "public", got "${privacy}"`);
  }
  if (privacy === 'private' && !extensionName) {
    // No suffix-stripping heuristic. `spokeagent-0.4.1` proves ids and
    // manifest names diverge, and a heuristic in a security path is right
    // until it isn't.
    fail(`${id}: a private extension must declare data-extension-name`);
  }
  // Forces the medcp/msbaseagent revisit AT PUBLISH TIME rather than relying on
  // someone remembering: the private badge is granted by publishing to BAAM.
  if (!/data-privacy=/.test(card)) {
    // ⚠ `.some(k => …)` scopes `k` to the callback, so the message cannot name
    // the keyword from outside it. `.find` binds the match where the message
    // can see it — an earlier draft wrote `fail(`… "${k}" …`)` after a `.some`,
    // which is a ReferenceError under ESM strict mode: the script would exit
    // non-zero with a stack trace, and a gate that only checked "exit != 0"
    // would have read that as the rule firing.
    const hit = CLINICAL_KEYWORDS.find((k) => description.toLowerCase().includes(k));
    if (hit) {
      fail(`${id}: description matches "${hit}" but the card declares no data-privacy`);
    }
  }
```

with `CLINICAL_KEYWORDS = ['patient', 'clinical record', 'ehr', 'phi', 'medical record', 'de-identified clinical']`,
both keys in the emitted object at `:107-118`, `version: 2` at `:156`, and two further outputs:
`crates/biorouter/src/privacy/registry_private.rs` and
`ui/desktop/src/components/baam/registry.fallback.json`.

**(c) the four fixtures**, each wrapped in `<div id="extensions-section"> … </div>` (see the ⚠
above). Keep them minimal — one or two `ext-card`s — so a reviewer can see the single attribute each
one is about.

`RegistryExtension` gains `extension_name?: string` and `privacy?: 'private' | 'public'`, **both
optional** so an old cached document still parses.

- [ ] **Step 4: Run**

```bash
node landing/scripts/build-registry.mjs
git diff --stat landing/registry.json crates/biorouter/src/privacy/registry_private.rs \
                ui/desktop/src/components/baam/registry.fallback.json
cargo test -p biorouter --lib privacy::extensions
```

- [ ] **Step 5: Gate**

```bash
# The four fixtures exist. Without this line the loop below "passes" on a
# missing-file ENOENT, which is the shape the first version of this gate had.
ls landing/scripts/fixtures/{invalid-privacy,private-no-name,clinical-unannotated,happy}.html
echo "expect: four paths, no 'No such file'"
# The three hard failures exist and fire — AS VALIDATIONS, not as crashes.
# `--out /dev/null` is mandatory: without it every run below rewrites the real
# landing/registry.json, which the python check further down then reads.
for f in invalid-privacy private-no-name clinical-unannotated; do
  out=$(node landing/scripts/build-registry.mjs \
          --input landing/scripts/fixtures/$f.html --out /dev/null 2>&1)
  code=$?
  printf '%s exit=%s ' "$f" "$code"
  echo "$out" | grep -q "^registry: .*\(data-privacy\|data-extension-name\)" \
    && echo "OK (named the rule)" \
    || { echo "BROKEN — not a validation failure:"; echo "$out" | head -3; }
done
# expect: exit=1 and "OK (named the rule)" three times. An exit=1 WITHOUT the
# message is a crash — a bad flag, an ENOENT, a ReferenceError — and is exactly
# what a half-finished implementation produces.
# And the happy fixture still parses, so the three failures above are not simply
# "the parser found no cards at all".
node landing/scripts/build-registry.mjs \
  --input landing/scripts/fixtures/happy.html --out /tmp/happy-registry.json
echo "expect: exit 0 and a line reading 'registry.json written: 2 extensions, 0 skills'"
# The real catalog was NOT touched by any of the above.
git diff --quiet -- landing/registry.json && echo "OK: registry.json untouched" || \
  echo "BROKEN: a fixture run wrote over the real catalog — --out is not wired"
# The generated const and the registry agree.
python3 -c "
import json,re
r=json.load(open('landing/registry.json'))
assert r['version']==2, r['version']
want=sorted(e['extension_name'] for e in r['extensions'] if e.get('privacy')=='private')
src=open('crates/biorouter/src/privacy/registry_private.rs').read()
got=sorted(re.findall(r'\"([^\"]+)\"', src.split('PRIVATE_EXTENSIONS')[1]))
assert want==got, (want,got); print('in sync:', got)"
# expect: in sync: ['cdwagent', 'ucsfomopagent']
```

**What this catches.** Asserting only that the happy path still emits 37 entries — which is the
natural test and which passes an implementation where the three `fail()` calls are unreachable
(wrong regex, wrong variable, `||` where `&&` was meant). The three fixture runs are the gate, and
they are the whole reason the clinical-keyword rule is worth having: it is the mechanism that forces
the medcp/msbaseagent revisit at publish time.

⚠ **And it catches the state the first version of this gate was in, which was worse than no gate.**
That version ran `node … --input landing/scripts/fixtures/$f.html >/dev/null 2>&1` and asserted only
"non-zero". Measured: `build-registry.mjs` has no argv handling, `landing/scripts/fixtures/` does not
exist, and Task 33's Files table created neither — so *any* error satisfied it. A worker who never
wrote a fixture, never wired `--input`, and never added a single validation would see three
`exit=1`s from three ENOENTs and record the gate as passed. The `ls`, the message grep and the
`git diff --quiet` are the three lines that make it evidence.

- [ ] **Step 6: Commit**

```bash
git add landing/scripts/build-registry.mjs landing/registry.json \
        crates/biorouter/src/privacy/registry_private.rs \
        ui/desktop/src/components/baam/registry.fallback.json \
        ui/desktop/src/components/baam/registry.ts
git commit -m "feat(marketplace): registry v2 with privacy + extension_name, and the generator's first hard failures (#56)"
```

---

### Task 34: Wire the `--check` — because there is nothing to extend

⚠ **`landing/scripts/check-consistency.mjs` is an orphan.** It accumulates `failures` and exits 1 at
`:110-112` — the right host — and it already has the precedent check
`registry.extensions.length === (baam ext-card count)` at `:105`. But
`grep -rn "check-consistency" --include="*.yml" --include="Justfile" --include="*.json" --include="*.sh" .`
returns **zero** hits: not in any GitHub workflow, not in the `Justfile`, and there is no
`landing/package.json`. `just check-everything` (`Justfile:8-25`) runs **seven** checks —
`cargo fmt --all`, `clippy-lint.sh`, `npm run lint:check`, `check-openapi-schema.sh`,
`check-version-consistency.sh`, `check-brand-consistency.sh`, `check-no-cross-drift.sh` — **none of
which touch `landing/`**. `.github/workflows/deploy-landing.yml` runs **zero** checks — checkout,
configure-pages, upload, deploy. The design's "wired into `just check-everything`, mirroring the
theme-generator precedent" describes wiring that does not exist for this script. **This task creates
the step.**

**Files:**

| Action | Path | Anchor (re-verified at `9558c346`) |
|---|---|---|
| Modify | `landing/scripts/check-consistency.mjs` | `failures` accumulation; `process.exit(1)` `:110-112`; the length check at `:105` |
| Modify | `Justfile` | `check-everything` at `:8-25` (six steps today) |
| Modify | `.github/workflows/deploy-landing.yml` | add a check job before deploy |

- [ ] **Step 1: Write the failing test** — deliberately desync the two outputs and assert the wired
command fails:

```bash
# Running the script by hand proves nothing about the WIRING, which is the
# defect this task exists to fix.
sed -i.bak 's/"ucsfomopagent"//' crates/biorouter/src/privacy/registry_private.rs
just check-everything ; echo "exit=$?"     # expect: non-zero
mv crates/biorouter/src/privacy/registry_private.rs.bak crates/biorouter/src/privacy/registry_private.rs
```

- [ ] **Step 2: Run** → `just check-everything` exits **0** today, whatever the registry says.

- [ ] **Step 3: Implement** — a `--check` mode in `check-consistency.mjs` comparing `registry.json`,
`registry_private.rs` and `registry.fallback.json`, a `check-privacy-registry` recipe in the
`Justfile` called from `check-everything`, and a check job in `deploy-landing.yml`. Mirrors the
theme-generator precedent CLAUDE.md blesses (`npm run themes -- --check` inside `lint:check`).

- [ ] **Step 4: Run** → `just check-everything` → **clean**.

- [ ] **Step 5: Gate**

```bash
# ⚠ NOT one OR'd count. `grep -c "A\|B" ; expect: >= 2` is satisfied by TWO hits
# of B and zero of A — i.e. by a recipe that exists and is never called, or by a
# call to a recipe that does not exist. Both halves get their own assertion, and
# both are 0 today so both baselines are real.
grep -c "^check-privacy-registry:" Justfile ; echo "expect: 1 — the recipe exists (0 today)"
awk '/^check-everything:/,/^$/' Justfile | grep -c "check-privacy-registry"
echo "expect: 1 — and check-everything actually calls it (0 today)"
grep -c "check-privacy-registry\|check-consistency" .github/workflows/deploy-landing.yml
echo "expect: >= 1 — the deploy runs it too (0 today; the workflow runs ZERO checks)"
# The desync test from Step 1, run as a gate — the only one that proves WIRING
# rather than existence:
sed -i.bak 's/"ucsfomopagent"//' crates/biorouter/src/privacy/registry_private.rs
just check-everything >/dev/null 2>&1 ; echo "desync exit=$?  # expect: non-zero"
mv crates/biorouter/src/privacy/registry_private.rs.bak crates/biorouter/src/privacy/registry_private.rs
git diff --quiet -- crates/biorouter/src/privacy/registry_private.rs && echo "OK: restored"
# Note the `--`: without it git reads the path as a possible revision and dies
# with "ambiguous argument" whenever the file is untracked, which is a non-zero
# exit that looks like "the desync survived".
```

**What this catches.** A gate written as "add a line to the existing landing check step" is
**un-runnable** — the step does not exist. And a gate that runs the script by hand passes while the
wiring is still missing, which is precisely the state the tree is in today.

- [ ] **Step 6: Commit**

```bash
git add landing/scripts/check-consistency.mjs Justfile .github/workflows/deploy-landing.yml
git commit -m "ci(marketplace): wire the registry privacy --check into check-everything and the landing deploy (#56)"
```

---

### Task 35: `baam.html` — five components, and the tag row that clips

**Files:**

| Action | Path | Anchor (re-verified at `9558c346`) |
|---|---|---|
| Modify | `landing/baam.html` | `extCardHtml` `:3804` (`rawTags.slice(0, 3)` `:3806`; the card div with `data-tags`/`data-license`/`data-org` `:3824`; `.ext-tags` render `:3833`); `buildExtChips` `:3838`; `renderExtensions` `:3864`; `filterExtensions` `:3909` (the org branch at **`:3924`**, which reads a **literal** `c.dataset.org`); `loadRegistryExtensions` `:3941`; `.ext-tags { … overflow: hidden; max-height: 22px }` `:191`; static CDWAgent card `:471`, UCSFOMOPAgent card `:495` |
| Modify | `landing/shared.css` | `.tag.ucsf` `:413`, `.tag.mcp` `:414`, `.tag.federated` `:415` |

⚠ **`extCardHtml` truncates to three tags** (`rawTags.slice(0, 3)` at `:3806`) and `.ext-tags` is
`max-height: 22px; overflow: hidden`. Prepending a badge makes **four** chips in a clipped
single-line row, so the third real tag disappears on every card. **Pair the prepend with
`slice(0, 2)`.**

⚠ **`filterExtensions` reads the org facet as a literal**, not `c.dataset[f]`. The line at `:3924` is
`const ok = (f === 'org') ? vals.indexOf(c.dataset.org || '') !== -1 : vals.some(v => hay.indexOf(v) !== -1);`
so the edit is `(f === 'org' || f === 'privacy') ? vals.indexOf(c.dataset[f] || '') !== -1 : …`.

⚠ **The static no-JS cards carry only `data-license`** (`:471`) — no `data-org`, no `data-tags`. They
are simultaneously the no-JS view **and the generator's input**, so `data-privacy` must be authored
there *and* emitted by `extCardHtml`. Both, or the facet works in one view and not the other.

- [ ] **Step 1: Write the failing test** — a Playwright or jsdom check over the built page:

```js
test('the privacy facet filters the shelf down to the two private extensions', async ({ page }) => {
  await page.goto('/baam.html');
  await page.click('[data-chip="privacy"][data-value="private"]');
  const visible = await page.$$eval('.ext-card:visible', els => els.map(e => e.dataset.extensionName));
  expect(visible.sort()).toEqual(['cdwagent', 'ucsfomopagent']);
});

test('a card with three real tags still shows all three', async ({ page }) => {
  await page.goto('/baam.html');
  const chips = await page.$$eval('.ext-card[data-extension-name="cdwagent"] .ext-tags > span',
                                  els => els.map(e => e.textContent));
  expect(chips[0]).toBe('Private');
  expect(chips.length).toBeLessThanOrEqual(3);
  // The clip test: every chip must be inside the 22px row.
  const clipped = await page.$$eval('.ext-card .ext-tags > span',
    els => els.filter(e => e.getBoundingClientRect().bottom > e.parentElement.getBoundingClientRect().bottom).length);
  expect(clipped).toBe(0);
});

test('the no-JS view is badged too', async ({ browser }) => {
  const ctx = await browser.newContext({ javaScriptEnabled: false });
  const page = await ctx.newPage();
  await page.goto('/baam.html');
  await expect(page.locator('.ext-card[data-privacy="private"] .tag.private')).toHaveCount(2);
});
```

- [ ] **Step 2: Run** → **FAIL** on all three.

- [ ] **Step 3: Implement** — `data-privacy` + `data-extension-name` on the card div beside
`data-license`; the badge **prepended** to `.ext-tags` with `slice(0, 2)`; a Private/Public facet in
`buildExtChips`; the `filterExtensions` ternary; `data-privacy="private"` plus a visible badge on the
two static cards (`:471` tags row `:486`; `:495` tags row `:510`); and in `shared.css:413-415`:

```css
/* Private uses the NAVY ramp: institutional in tone, visually distinct from
   the coral MCP tag. No new red — the palette has exactly one accent
   (coral #b85a32 text, #cf6d47 bars-only) and no semantic danger colour;
   inventing one breaks the Apple-deck reskin. */
.tag.private { background: rgba(5,32,73,0.07); color: var(--ucsf); }
.tag.public  { background: var(--bg-3);        color: var(--text-3); }
```

Dark overrides go in the same block, since `landing/theme.js` sets the `.dark` class pre-paint.
**Both states are labelled** — a badge shown only on private teaches a visitor nothing about its
absence.

- [ ] **Step 4: Run** → the three tests → **PASS**; then `node landing/scripts/build-registry.mjs`
and confirm the two static cards produce the two `private` registry entries.

- [ ] **Step 5: Gate**

```bash
# The prepend is paired with the slice change, or the third tag clips.
grep -c "rawTags.slice(0, 3)" landing/baam.html ; echo "expect: 0"
grep -c "rawTags.slice(0, 2)" landing/baam.html ; echo "expect: 1"
# The facet reads dataset[f], not a literal.
grep -c "c.dataset.org" landing/baam.html ; echo "expect: 0"
grep -c "c.dataset\[f\]" landing/baam.html ; echo "expect: 1"
# Both views carry the attribute.
grep -c 'data-privacy="private"' landing/baam.html ; echo "expect: 2 (the two static cards)"
grep -c 'data-privacy=\${' landing/baam.html ; echo "expect: 1 (extCardHtml)"
# No new colour.
grep -cE "\.tag\.(private|public)[^{]*\{[^}]*(#[0-9a-fA-F]{3,6}|rgb)" landing/shared.css
echo "expect: 1 — only the navy rgba(5,32,73,0.07), which .tag.ucsf already uses"
```

**What this catches.** Prepending the badge without changing the slice, which silently clips the
third real tag on every card in a 22 px `overflow: hidden` row — invisible in a diff and invisible in
a screenshot of a two-tag card. And editing only the runtime `extCardHtml`, leaving the static no-JS
cards unbadged: they are the generator's input, so the registry would then say `public` for both
private extensions.

- [ ] **Step 6: Commit**

```bash
git add landing/baam.html landing/shared.css
git commit -m "feat(landing): privacy badges and a Private/Public facet on the BAAM shelf (#56)"
```

---

### Task 36: `docs.html`, and the drift nothing would catch

`landing/docs.html:1468-1480` carries a hand-written "Extension agents in the marketplace" table
(`<thead>` Agent / What it connects / Credentials at `:1471`; SPOKEAgent `:1473`, UCSFOMOPAgent
`:1474`, CDWAgent `:1475`, PlaywrightAgent `:1476`, CodeGraphAgent `:1477`, BiorOffice `:1478`).
**Nothing generates or checks it.**

**Files:**

| Action | Path | Anchor |
|---|---|---|
| Modify | `landing/docs.html` | the table at `:1468-1480` |
| Create | `landing/scripts/check-docs-privacy.mjs` | new |
| Modify | `landing/scripts/check-consistency.mjs` | call it |

- [ ] **Step 1: Write the failing test** — set the table's UCSFOMOPAgent row to "Public" and assert
`just check-everything` exits non-zero.

- [ ] **Step 2: Run** → exits **0**.

- [ ] **Step 3: Implement** — add a **Privacy** column to the six-row table, and
`check-docs-privacy.mjs` comparing each row to `registry.json`.

`landing/skills.html` and `landing/index.html` list no extensions and need no change — **say so in
the doc** so a later reviewer does not "fix" it.

- [ ] **Step 4: Run** → `just check-everything` → clean.

- [ ] **Step 5: Gate**

```bash
# ⚠ Anchor inside the TABLE, not the page. `grep -c "Privacy" docs.html` is 0
# today so the baseline is honest, but the word could land anywhere — a nav
# item, a footer link, a sentence — and still score >= 1 with the column
# missing. Assert the header cell and the six data cells instead.
awk '/Extension agents in the marketplace/,/<\/table>/' landing/docs.html \
  | grep -c "<th>Privacy</th>" ; echo "expect: 1 — the column header"
awk '/Extension agents in the marketplace/,/<\/table>/' landing/docs.html \
  | grep -cE ">(Private|Public)<" ; echo "expect: 6 — one per agent row"
grep -c "check-docs-privacy" landing/scripts/check-consistency.mjs ; echo "expect: 1 (0 today)"
# The desync test from Step 1, run as a gate — existence is not wiring.
sed -i.bak 's|<td>Private</td>|<td>Public</td>|' landing/docs.html
just check-everything >/dev/null 2>&1 ; echo "desync exit=$?  # expect: non-zero"
mv landing/docs.html.bak landing/docs.html
git diff --quiet -- landing/docs.html && echo "OK: restored"
```

**What this catches.** The table drifting the day badges ship on BAAM — the failure that has no
detector today because nothing generates it and nothing reads it. And the version of this task that
adds `check-docs-privacy.mjs`, never calls it from `check-consistency.mjs`, and passes a gate that
only asked whether the word "Privacy" appears somewhere in a 1500-line HTML file.

- [ ] **Step 6: Commit**

```bash
git add landing/docs.html landing/scripts/check-docs-privacy.mjs landing/scripts/check-consistency.mjs
git commit -m "docs(landing): a Privacy column on the agents table, checked against the registry (#56)"
```

---

### Task 37: The in-app registry — freshness that raises and never lowers

`main.ts:2855-2866` is a bare `fetch(REGISTRY_URL, { headers })` (`REGISTRY_URL` at `:2832`, preload
bridge at `:355`/`:580`) with **no timeout, no cache and no last-good write**.

Correction to §10.2/§14.2: `live: boolean` is **not** "existing-but-discarded" — both modals already
surface it (`BrowseExtensionsModal.tsx:100`, `BrowseSkillsModal.tsx:151`: "· showing bundled catalog
(offline)"). What is missing is freshness and persistence, and the Extensions **settings** cards
show it nowhere.

**Files:**

| Action | Path | Anchor (re-verified at `9558c346`) |
|---|---|---|
| Modify | `ui/desktop/src/main.ts` | `REGISTRY_URL` `:2832`; `ipcMain.handle('registry:fetch', …)` `:2855-2866` |
| Modify | `ui/desktop/src/components/baam/registry.ts` | `RegistryExtension` `:8-19`; `loadRegistry` `:50` |
| **Create** | `ui/desktop/src/components/baam/registry.test.ts` | new — **`registry.ts` has no test file today**, and the first three tests below have nowhere to live. `BrowseExtensionsModal.test.tsx` is the only test file in `components/baam/` |
| Modify | `ui/desktop/src/components/baam/BrowseExtensionsModal.tsx` | `live` consumption `:23`/`:32`/`:35`/`:100` |
| Modify | `ui/desktop/src/components/BrxtInstallModal.tsx` | the config write `:152-161` — records **no provenance whatsoever** |
| **Create** | `ui/desktop/src/components/BrxtInstallModal.test.tsx` | new — verified absent; the fourth test below `render`s this component and has nowhere else to go |

⚠ **The file path is load-bearing, and its absence is what made this task's gate vacuous.** The
gate ran `npx vitest run registry -t "downgrade is never honoured"`. Measured: `registry` matches
three unrelated suites — `src/terminalSessionRegistry.test.ts`, `components/chatGroups/newTabRegistry.test.ts`
and `components/chatGroups/closeActiveTabRegistry.test.ts` — so vitest finds files (no "no test files
found" bail), the `-t` filter then skips all 28 of their tests, and the run prints
`3 skipped / 28 skipped` and **exits 0**, having executed none of the four tests it exists to
protect. That gate is the only thing standing between a compromised `registry.json` and
`ucsfomopagent` losing its private badge on every machine that fetches it.

- [ ] **Step 1: Write the failing tests** — in `src/components/baam/registry.test.ts`

```tsx
it('a downgrade is never honoured for an entry the compiled set names', async () => {
  // private_set = PRIVATE_EXTENSIONS ∪ private(last_good_fetch). An offline
  // laptop can fail to LEARN a new private badge; it can never LOSE one.
  mockFetch({ extensions: [{ extension_name: 'ucsfomopagent', privacy: 'public' }] });
  const { registry } = await loadRegistry();
  expect(effectivePrivacy(registry, 'ucsfomopagent')).toBe('private');
});
it('an upgrade takes effect on the next successful fetch and persists', async () => {
  mockFetch({ extensions: [{ extension_name: 'labarchivesagent', privacy: 'private' }] });
  await loadRegistry();
  mockFetchFailure();
  const { registry, live } = await loadRegistry();
  expect(live).toBe(false);
  expect(effectivePrivacy(registry, 'labarchivesagent')).toBe('private');
});
it('a hung registry does not hang the modal', async () => {
  mockFetchHang();
  const t0 = Date.now();
  const { live } = await loadRegistry();
  expect(live).toBe(false);
  expect(Date.now() - t0).toBeLessThan(12_000);
});
it('the brxt install modal says the resulting badge out loud', () => {
  render(<BrxtInstallModal manifest={{ name: 'anything' }} />);
  expect(screen.getByText(/always Public/i)).toBeInTheDocument();
  expect(screen.getByText(/including commercial models hosted outside UCSF/i)).toBeInTheDocument();
});
```

- [ ] **Step 2: Run** → **FAIL** on all four.

- [ ] **Step 3: Implement** — a 10 s `AbortController`, a last-good write, the union rule, the
staleness line surfaced on the Extensions settings cards as well as in the modals, and the three
provenance strings of §13.5: *"Private — published on the Biorouter marketplace"*, *"Public —
published on the Biorouter marketplace"*, *"Public — installed from a file, not on the marketplace.
Any model can call it."*

Two naming consequences to write down as **known rather than discovered**: a hand-installed
extension *named* `ucsfomopagent` inherits the private badge (fail-closed, fine); and a genuinely
private extension renamed locally becomes public — already the accepted direction under R11(ii), and
unavoidable because the install records no provenance at all.

- [ ] **Step 4: Run** — name the FILES, never the bare word `registry`:

```bash
cd ui/desktop && npx vitest run \
  src/components/baam/registry.test.ts \
  src/components/baam/BrowseExtensionsModal.test.tsx \
  src/components/BrxtInstallModal.test.tsx 2>&1 | tail -6
```

Expected: `3 test files`, with `registry.test.ts` reporting **3 passed** and
`BrxtInstallModal.test.tsx` **1 passed**. A run that reports `3 skipped` has matched suite *paths*
and filtered out every test; a run that reports `2 test files` means one of the two new files was
never created.

- [ ] **Step 5: Gate**

```bash
cd ui/desktop
grep -c "AbortController" src/main.ts ; echo "expect: >= 1 (0 today)"
# The union rule is a function, not four inline ORs.
grep -rn "effectivePrivacy" src/components/baam/ | wc -l ; echo "expect: >= 2 (definition + consumers; 0 today)"
# A live fetch can never lower a compiled-in badge. ⚠ The FILE PATH, not the
# word `registry`: as a name filter, `registry` matches three unrelated suites
# (terminalSessionRegistry, newTabRegistry, closeActiveTabRegistry), `-t` then
# skips all 28 of their tests, and the run prints green having executed ZERO of
# this task's four. Verified by running it against the tree.
test -f src/components/baam/registry.test.ts || echo "MISSING: the test file was never created"
npx vitest run src/components/baam/registry.test.ts \
  -t "a downgrade is never honoured" 2>&1 | tail -4
echo "expect: '1 passed' — NOT '1 skipped', and NOT '0 passed'. A skip here is"
echo "  a filter that did not match; the -t string must be a substring of the"
echo "  it(...) title exactly as written in Step 1."
# ...and the other two in that file actually run too, so a single live term
# cannot hide them.
npx vitest run src/components/baam/registry.test.ts 2>&1 | tail -4
echo "expect: 1 test file, 3 passed"
test -f src/components/BrxtInstallModal.test.tsx || echo "MISSING: the fourth test has no file"
npx vitest run src/components/BrxtInstallModal.test.tsx 2>&1 | tail -4
echo "expect: 1 test file, 1 passed"
```

**What this catches.** The natural implementation — trusting the live document — which lets a
compromised or merely stale `registry.json` **remove** a private badge from `ucsfomopagent` on every
machine that fetches it. The union rule is one line and the first test is the only thing that
enforces it. And, before that, it catches the state this gate was in: a vitest invocation that
reported success while running none of the four tests, because the file they belong in did not exist
and no Files-table row created it.

- [ ] **Step 6: Commit**

```bash
git add ui/desktop/src/main.ts ui/desktop/src/components/baam ui/desktop/src/components/BrxtInstallModal.tsx
git commit -m "feat(marketplace): last-good registry with a timeout, and a union rule that only raises (#56)"
```

---

# Phase 6 — migration, docs, and the release gate

### Task 38: The backfill, and the day-one notice with computed counts

⚠ **The backfill runs ONCE, from the numbered migration arm — never from `ensure_privacy_schema`.**
The reconcile helper runs on **every startup**, and a repeated `WHERE provider_name IN (…)` would
**re-privatise a session the user has just declassified**, because `declassify_session` deliberately
leaves `provider_name` untouched (§12.6). That is a silent one-way regression of the one user-only
action in the design.

⚠ **Re-measure the counts.** §16's table is stale: the `user` NULL-provider bucket moved from 29 to
**343** in one day, and **175 of those have messages** — 175 real conversations of unknown
provenance that backfill **public**. And History shows fewer rows than the raw counts imply, because
`list_sessions_by_types` INNER JOINs `messages` (`session_manager.rs:4066`), so of 936 would-be
private `user` rows only **498** are visible. The notice must quote the visible number.

**Files:**

| Action | Path | Anchor (re-verified at `9558c346`) |
|---|---|---|
| Modify | `crates/biorouter/src/session/session_manager.rs` | the `17 =>` arm (Task 6) |
| Create | `ui/desktop/src/components/privacy/FirstRunPrivacyNotice.tsx` | new |
| Create | `docs/security/privacy-tiers-migration.md` | new |

- [ ] **Step 1: Write the failing tests**

```rust
#[tokio::test]
async fn the_backfill_cannot_un_declassify() {
    let db = migrated_v16_db_with(&[session_on("versa_azure")]).await;
    open(&db).await;                                              // migration 17 runs, backfills private
    declassify_via_user(&db, "s1").await;
    assert_eq!(row(&db, "s1").await.privacy_tier, SessionClassification::Public);
    assert_eq!(row(&db, "s1").await.provider_name.as_deref(), Some("versa_azure"));  // untouched
    open(&db).await;                                              // second launch
    assert_eq!(row(&db, "s1").await.privacy_tier, SessionClassification::Public,
               "the backfill re-privatised a declassified session");
    assert_eq!(row(&db, "s1").await.privacy_reason.as_deref(), Some("declassified_by_user"));
}

#[tokio::test]
async fn the_backfill_marks_what_the_data_proves_and_nothing_else() {
    let db = migrated_v16_db_with(&[
        session_on("versa_azure"), session_on("versa_bedrock"),
        session_on("llamacpp"), session_on("ollama"),
        session_on("anthropic"), session_with_null_provider(),
    ]).await;
    open(&db).await;
    assert_eq!(private_count(&db).await, 4);
    assert_eq!(public_count(&db).await, 2);      // NULL provider backfills PUBLIC — fail-open, by decision
    for k in ["backfilled_private", "backfilled_public_named",
              "backfilled_unknown_provider", "backfilled_empty"] {
        assert!(logged_at_info(k), "{k} not logged");
    }
}

#[tokio::test]
async fn the_notice_quotes_the_history_visible_count_not_the_raw_one() {
    // list_sessions_by_types INNER JOINs messages, so empty sessions never
    // appear. On the operator's machine that is 498 visible vs 936 raw.
    let db = migrated_v16_db_with(&[
        session_on_with_messages("versa_azure"), session_on_with_messages("versa_azure"),
        session_on("llamacpp"),                                   // empty: invisible in History
        session_on_with_messages("anthropic"), session_on_with_messages("anthropic"),
        session_on_with_messages("anthropic"),
        null_provider_with_messages(), null_provider_with_messages(),
    ]).await;
    open(&db).await;
    let n = notice_counts(&db).await;
    assert_eq!(n.private_visible, 2, "quoted the raw row count instead of the visible one");
    assert_eq!(n.public_named_visible, 3);
    assert_eq!(n.unknown_provider_visible, 2);
}
```

- [ ] **Step 2: Run** → **COMPILE ERROR** on the notice helper, then **FAIL** on the first two.

- [ ] **Step 3: Implement** — in the `17 =>` arm only:

```rust
                // Fails OPEN, by decision. A fail-CLOSED backfill (NULL provider
                // + >= 1 message => private) was rejected: a user who has only
                // ever used a commercial provider would find a large slice of
                // their history marked private on first launch, refused on the
                // model they normally use, with only an irreversible
                // declassification as the exit, one chat at a time.
                //
                // The residual, stated rather than buried: `provider_name`
                // records the LAST provider, not every provider. A session that
                // ran on Versa and was later switched backfills public even
                // though its transcript contains private-model work. There is
                // no transcript scan and there will not be one.
                sqlx::query(
                    "UPDATE sessions
                        SET privacy_tier = 'private',
                            privacy_reason = 'backfill:' || provider_name
                      WHERE provider_name IN ('versa_azure','versa_bedrock','llamacpp','ollama')
                        AND privacy_tier = 'public'",
                ).execute(pool).await?;
```

Log the four counts at `info!`, and add `FirstRunPrivacyNotice` computing its numbers from the user's
own DB (§15.5), shown **before** enforcement begins. Grouped declassification extends to all
`backfill:*` reasons with a review-by-provider list, because a backfill is a **guess made by the
system from the last-used provider**, not a user assertion about content.

`docs/security/privacy-tiers-migration.md` carries the release-note line:

> *"Chats from before this version are marked by the model they were last using. If an older chat
> contains work you want kept private, switch it to a private model — it will be marked private from
> its next turn on."*

- [ ] **Step 4: Run**

```bash
cargo test -p biorouter --lib session::session_manager
cd ui/desktop && npx vitest run FirstRunPrivacyNotice 2>&1 | tail -4
```

- [ ] **Step 5: Gate**

```bash
# Both awk ranges must be NON-EMPTY before their counts mean anything: an awk
# START that never matches yields no output, and `grep -c` on nothing is 0 —
# which is a PASS for the first check and reads as "the backfill is correctly
# absent from the reconcile helper" while proving nothing at all.
awk '/async fn ensure_privacy_schema/,/^    }/' crates/biorouter/src/session/session_manager.rs | wc -l
echo "expect: > 1 — if this is 0, the helper is named something else and the"
echo "  zero-count below is vacuous. The name is ensure_privacy_schema (Task 6);"
echo "  earlier prose in this plan called it reconcile_privacy_schema and a"
echo "  worker who followed THAT would produce exactly this vacuous pass."
awk '/^            17 => \{/,/^            \}/' crates/biorouter/src/session/session_manager.rs | wc -l
echo "expect: > 1 — the arm exists at 12-space indentation, like arms 10..16"
# The backfill is in the numbered arm and NOT in the reconcile helper.
awk '/async fn ensure_privacy_schema/,/^    }/' crates/biorouter/src/session/session_manager.rs \
  | grep -c "UPDATE sessions" ; echo "expect: 0"
awk '/^            17 => \{/,/^            \}/' crates/biorouter/src/session/session_manager.rs \
  | grep -c "UPDATE sessions" ; echo "expect: 1"
# And a live-DB check on a COPY, run twice.
cp ~/.local/share/biorouter/sessions/sessions.db /tmp/p6-a.db
for i in 1 2; do BIOROUTER_SESSIONS_DB=/tmp/p6-a.db cargo run -p biorouter-cli -- sessions list >/dev/null; done
sqlite3 /tmp/p6-a.db "select count(*) from sessions where privacy_tier='private';"
# Record it. Then declassify one row by hand, run again, and assert it stayed public.
```

**What this catches.** Putting the backfill in `ensure_privacy_schema` — which is the obvious way
to dodge the migration-number problem (O10) and which silently reverses the user's one irreversible
action on the next app start. The paired `awk` counts are the gate, and the first test is the
behavioural proof. Separately, quoting the raw `sessions` count in the notice overstates what the
user can see and act on by nearly 2×.

- [ ] **Step 6: Commit**

```bash
git add crates/biorouter/src/session/session_manager.rs \
        ui/desktop/src/components/privacy/FirstRunPrivacyNotice.tsx \
        docs/security/privacy-tiers-migration.md docs/security/README.md
git commit -m "feat(privacy): one-time backfill and a first-run notice with the user's own counts (#56)"
```

---

### Task 39: Docs — user-facing, and the design's status closure

**Files:**

| Action | Path | Anchor |
|---|---|---|
| Modify | `docs/security/privacy-tiers.md` | the `**Status:**` header |
| Modify | `docs/security/data-privacy-and-phi.md` | the provider-choice guidance this now enforces |
| Modify | `docs/security/README.md` | the "Documents in this folder" table at `:20-26` |
| Modify | `docs/agent-loop/subagents.md` | the inheritance behaviour Task 23 gates |
| Modify | `docs/agent-loop/tool-routing.md` | the chatrecall/workspace split Gate D sits inside |
| Modify | `CLAUDE.md` | a short "Privacy tiers" subsection under Architecture |

- [ ] **Step 1: Write the failing check** — `docs/organization.md` and
`docs/contributing/documentation-style.md` are binding. Every file opens with the context header
(`> **What this is.** / **Status:** / **Audience:**`), uses sentence-case headings, kebab-case
filenames, and closes with `## Related documentation`.

- [ ] **Step 2 – 4: Apply, and update the design's status line** to record what shipped and what did
not (per the status-header convention).

- [ ] **Step 5: Gate**

```bash
for f in docs/security/privacy-tiers.md docs/security/privacy-tiers-execution-plan.md \
         docs/security/privacy-tiers-migration.md; do
  head -12 "$f" | grep -q "What this is" || echo "MISSING context header: $f"
  grep -q "^## Related documentation" "$f" || echo "MISSING closer: $f"
done
echo "expect: no output"
# Every new doc is indexed, and each ROW is checked separately. `grep -c
# "privacy-tiers"` is a substring count: it is 2 today (the design row :26 and
# the plan row :27), and the migration doc's row would make it 3 — but so would
# a second mention of an existing doc anywhere on the page, which is not the
# same thing at all.
for d in privacy-tiers.md privacy-tiers-execution-plan.md privacy-tiers-migration.md; do
  echo -n "$d: " ; grep -c "]($d)" docs/security/README.md
done
echo "expect: 1 each — three distinct index rows (today: 1, 1, 0)"
git status --porcelain | grep -E "^\?\?.*\.md$" | grep -v "^?? docs/" ; echo "expect: no output"
```

- [ ] **Step 6: Commit**

```bash
git add docs/ CLAUDE.md
git commit -m "docs(security): user documentation for privacy tiers and the design's status closure (#56)"
```

---

### Task 40: Final release gate

- [ ] **Step 1: The whole tree**

```bash
cargo test --workspace --no-fail-fast 2>&1 | tail -20
cargo fmt --check && ./scripts/clippy-lint.sh
just check-everything
cd ui/desktop && npx tsc --noEmit && npm run lint:check && npm run test:run 2>&1 | tail -8
node scripts/check-contrast.mjs | tail -1
npm run themes -- --check
```

Exactly one expected pre-existing failure — `providers::test_anthropic_provider`, which calls the
live Anthropic API and fails on billing — plus the frontend `SessionListView.test.tsx` isolation
flake. **Verify both on a clean checkout before dismissing them.**

- [ ] **Step 2: The integration targets, by name**

```bash
cargo test -p biorouter --test subagent_delegation
cargo test -p biorouter --test soft_interrupt_agent_loop
cargo test -p biorouter --test conversation_writeback_freshness
cargo test -p biorouter --test conversation_writeback_stress
cargo test -p biorouter-server --test knowledge_routes
cargo test -p biorouter-server --test knowledge_routes_e2e
cargo test -p biorouter-server --test llamacpp_routes
# ⚠ `-p biorouter-mcp` is WRONG here and cargo hard-errors
# ("no test target named `mcp_integration_test` in default-run packages").
# The file is crates/biorouter/tests/mcp_integration_test.rs — verified.
cargo test -p biorouter --test mcp_integration_test
cargo test -p biorouter-mcp --lib knowledge::
cargo test -p biorouter-server --lib routes::apps
node scripts/agent-drafter/ui-control-harness.mjs
```

⚠ **Two** of the `--lib` filters used across this plan resolve to modules that had **no tests at all**
before the task that owns them — `agents::chatrecall_extension` and `session::chat_history_search`,
both measured at zero `#[cfg(test)]` blocks. For those two, a non-zero count at the release gate is
the assertion: a `0 passed` is not "nothing to run", it is a suite that did not land where the filter
looks.

⚠ **The other two are NOT zero, and "assert non-zero" is worthless for them.**
`routes::agent` reports **8 passed** on `main` and `routes::session` reports **20 passed**, from four
pre-existing `#[cfg(test)]` blocks that are not named `tests`. A release gate that only demands
"non-zero" from those two is satisfied by a tree in which #56 added no route tests whatsoever. Run
them and compare against the recorded baselines:

```bash
cargo test -p biorouter-server --lib routes::agent   2>&1 | grep "test result:"
echo "expect: strictly MORE than 8 passed  (8 is the untouched baseline)"
cargo test -p biorouter-server --lib routes::session 2>&1 | grep "test result:"
echo "expect: strictly MORE than 20 passed (20 is the untouched baseline)"
cargo test -p biorouter --lib agents::chatrecall_extension 2>&1 | grep "test result:"
echo "expect: non-zero (0 is the untouched baseline)"
cargo test -p biorouter --lib session::chat_history_search 2>&1 | grep "test result:"
echo "expect: non-zero (0 is the untouched baseline)"
```

- [ ] **Step 2b: Task 4b's filter audit, with an EMPTY deferred set**

```bash
# Every module this plan creates now exists, so nothing may be deferred. Re-run
# Task 4b Steps 1 and 5 with an EMPTY deferred table:
#   : > /tmp/56-filters/deferred.txt   # nothing may be missing at the release gate
# Expect: 0 MISSING, 0 DEFER and 0 UNUSED across all 42 (package, filter) pairs.
# A single DEFER here means a filter this plan has been quoting for forty tasks
# names a module that never came to exist — a gate that has been printing
# `0 passed` and exiting 0 the whole time. That is BR-71's most expensive defect,
# and this line is the last place it can be caught.
```

See [Which test filters are validated, and which are not](#which-test-filters-are-validated-and-which-are-not)
and [Task 4b](#task-4b-resolve-every-test-filter-against-a-real-cargo---list-docs-only).

- [ ] **Step 3: The twelve invariants, as commands**

```bash
cd /Users/wgu/Desktop/BioRouter-privacy
# O5 — the ratchet fires in exactly two places, neither of them the bind. PRINT,
# do not `wc -l`: a bare 2 is also produced by two calls in agent.rs and none in
# extension_manager.rs, which is Gate C's ratchet silently missing. (Task 20
# Step 3 prints; this copy counted. Same invariant, two strengths.)
grep -rn "raise_privacy(" --include='*.rs' crates/ | grep -v session_manager.rs
echo "expect: exactly 2 lines — one in agents/agent.rs (Gate B) and one in"
echo "        agents/extension_manager.rs (Gate C). Read the paths, not the count."
# O7 — one production path into an MCP client (see Task 20 Step 3 for the full
# hit list and why a `grep -vc "cfg(test)"` cannot express this).
grep -c "\.call_tool(" crates/biorouter/src/agents/extension_manager.rs ; echo "expect: 1 (1 today)"
grep -rn "\.call_tool(" --include='*.rs' crates/ | wc -l
echo "expect: 10 — the SAME 10 as at 9558c346, so this is a no-growth tripwire"
echo "        rather than a measurement of #56. Any increase is a new bypass."
# O6 — nothing above filter_tools consults a tier.
awk '/async fn get_all_tools_cached/,/^    }/' crates/biorouter/src/agents/extension_manager.rs \
  | grep -c "capability_tier\|allowed_extension_keys" ; echo "expect: 0"
# The ratchet is irreversible except through one statement.
grep -c "privacy_tier = CASE WHEN" crates/biorouter/src/session/session_manager.rs ; echo "expect: 1"
grep -rn --include='*.rs' "privacy_tier *= *'public'" crates/ | grep -v "DEFAULT 'public'" | wc -l ; echo "expect: 1"
# Gate D is in both builders; Gate C has all nine entry points.
grep -c "s.privacy_tier = 'public'" crates/biorouter/src/session/chat_history_search.rs ; echo "expect: 2"
grep -c "assert_extension_reachable(" crates/biorouter/src/agents/extension_manager.rs ; echo "expect: 9"
# O12 — the knowledge-base barrier at its five choke points, and its ratchet.
grep -c "assert_kb_reachable(" crates/biorouter-mcp/src/knowledge/server.rs ; echo "expect: 2 (CP1)"
grep -c "tier::assert_reachable(" crates/biorouter-mcp/src/knowledge/macros/ingest.rs \
        crates/biorouter-mcp/src/knowledge/macros/query.rs \
        crates/biorouter-mcp/src/knowledge/macros/lint.rs ; echo "expect: 1 each (CP2)"
grep -c "tier::assert_reachable(" crates/biorouter-server/src/routes/apps.rs ; echo "expect: 1 (CP3)"
grep -c "tier::assert_reachable(" crates/biorouter-mcp/src/agent_drafter/mod.rs ; echo "expect: 1 (CP4)"
grep -c "pub fn discover(" crates/biorouter-mcp/src/agent_drafter/catalog.rs ; echo "expect: 1 (CP5)"
grep -rn "Catalog::discover(true)" --include='*.rs' crates/*/src/ ; echo "expect: no output (CP5)"
grep -c "tool_handler" crates/biorouter-mcp/src/knowledge/server.rs ; echo "expect: 0 — CP1 is hand-written"
grep -c "raise_tier(" crates/biorouter-mcp/src/knowledge/server.rs ; echo "expect: 3"
grep -c "raise_tier(" crates/biorouter-server/src/routes/apps.rs ; echo "expect: 1"
cargo test -p biorouter-mcp --lib \
  knowledge::server::tests::every_kb_tool_is_gated_or_exempt_for_a_pinned_reason \
  | grep "test result:" ; echo "expect: 1 passed; 0 failed"
cargo test -p biorouter-mcp --lib \
  knowledge::server::tests::no_exempt_tool_volunteers_a_private_bases_id_to_a_public_caller \
  | grep "test result:" ; echo "expect: 1 passed; 0 failed"
grep -rn "kb_tiers_path" crates/biorouter-mcp/src/knowledge/ | grep -v "fn kb_tiers_path"
echo "expect: tier.rs and service.rs::ensure_tiers_migrated only"
grep -rn "store::\(list_pages\|read_page\|write_page\|search\|search_with_scope\)(" \
  --include='*.rs' crates/ | grep -v "src/knowledge/" | wc -l
echo "expect: 4 — a FIFTH is a new CONTENT surface; see Task 10C Step 5"
# ...and the METADATA tripwire, which the two content detectors cannot express.
# BOTH sweeps, at 9558c346: a growth in either is a new way to hand a model a
# base id or name. One sweep with `grep -v src/knowledge/` is what let the
# pointer tools through a whole review round.
grep -rn "\.list_bases()\|\.session_kb_ids(\|\.selection(" --include='*.rs' crates/*/src/ \
  | grep -v "src/knowledge/" | wc -l
echo "expect: 27 — 18 production / 9 test-module; see Task 10D Step 5 sweep (1)"
grep -rn "\.list_bases()\|\.session_kb_ids(\|\.selection(" --include='*.rs' crates/*/src/ \
  | grep "src/knowledge/" | wc -l
echo "expect: 22 — 5 production / 17 test-module; see Task 10D Step 5 sweep (2)"
# The two id-list error messages omit rather than enumerate (Tasks 10C, 11).
awk '/fn kb_id_or_primary\(/,/^    }/' crates/biorouter-mcp/src/knowledge/server.rs \
  | grep -c "tier::is_private" ; echo "expect: 1"
awk '/fn resolve_target_kb\(/,/^}/' crates/biorouter/src/agents/knowledge_tool.rs \
  | grep -c "is_private" ; echo "expect: 1"
# Gate G is one guard in the shared function, covering all three of its callers.
# PRINT: `| wc -l` cannot tell "three callers pass it" from "one caller passes it
# and two tests construct the struct", and this repo's tests live in the same
# files as the code they test.
grep -rn "caller_capability:" --include='*.rs' crates/ | grep -v conversation_ingest.rs
echo "expect: 3 lines, one each in agents/knowledge_tool.rs (the platform tool),"
echo "        biorouter-server/src/routes/knowledge.rs (the HTTP route) and"
echo "        biorouter-cli/src/commands/knowledge.rs (the CLI). Read the paths."
grep -rn "caller_capability: ProviderTier::Private" --include='*.rs' crates/ ; echo "expect: no output"
# floor() is crossed at exactly its two intended callers, and the audit test
# names them rather than counting — see Task 7 for why a count could not work.
cargo test -p biorouter --lib \
  privacy::tests::floor_is_crossed_only_where_a_capability_establishes_a_classification \
  | grep "test result:" ; echo "expect: 1 passed; 0 failed (never just 'PASS' — 0 passed exits 0)"
# The registry const and registry.json agree, through the wired check.
just check-privacy-registry
# No privacy control is an inspector. ⚠ NOT `grep -rn "PrivacyInspector"`: 0
# today, 0 under every wrong implementation, green both ways. The trait's impl
# set is the checkable form.
diff <(grep -rl "impl ToolInspector for" --include='*.rs' crates/ | sort) <(cat <<'EOF'
crates/biorouter/src/hooks/inspector.rs
crates/biorouter/src/permission/managed_inspector.rs
crates/biorouter/src/permission/permission_inspector.rs
crates/biorouter/src/security/security_inspector.rs
crates/biorouter/src/security/sensitive_ops.rs
crates/biorouter/src/tool_monitor.rs
crates/biorouter/tests/tool_inspection_manager_tests.rs
EOF
) && echo "OK: the impl set is exactly what it was at 9558c346"
# The badges are mounted — the same 13-file enumeration as Task 32 Step 3.
cd ui/desktop && grep -rl "PrivacyBadge" src/components | sort
echo "expect: the 13 files listed in Task 32 Step 3, no more and no fewer"
```

- [ ] **Step 4: Live GUI verification, in a sandbox, with evidence**

Use `XDG_CONFIG_HOME=/tmp/privacy-release-check` and a seeded config (the operator's own has
`cdwagent` and `ucsfomopagent` **disabled**, so nothing would be refused). Follow
`docs/desktop-ui/launching-the-dev-gui.md`; `BIOROUTER_NO_HMR=1`; CDP screenshots only. Record the
four surfaces from Task 32 Step 2 plus a full private→public declassification.

Use the Versa models per standing policy (`versa_azure` / `gpt-5.5-2026-04-24` and `versa_bedrock`
opus 4.8); a local model only when local-model behaviour is itself under test.

- [ ] **Step 5: Adversarial Codex review of the full diff, every finding fixed or rebutted with
evidence — never silently dropped.** Re-review after substantial fixes.

- [ ] **Step 6: Close the loop**

Update `docs/security/privacy-tiers.md`'s `**Status:**` to `Current — implemented`, close issue #56
with a summary naming the five gates the design did not have (G, F1, F2, H, and the knowledge-base
barrier of Tasks 10A–10D) and the eight departures, and open follow-up issues for every unresolved
item in [Open questions](#open-questions).

The closing summary must also **name the accepted costs out loud** — AR-1 (a knowledge base one
private session touched is unreadable from every public chat, with no declassification path), AR-2
(existing knowledge bases migrate public even if a private session fed them) and AR-3 (local memory
is ungated). They are rulings, not omissions, but an issue that closes without stating them leaves
the next reader to rediscover them as bugs. See [Accepted risks](#accepted-risks).

---

## Decisions of record

Settled by the operator. **Do not re-argue any of these in a PR**; if implementation contradicts one,
the implementation is wrong.

| # | Decision |
|---|---|
| **DR-1** | **Private models** are institutionally hosted (`versa_azure`, `versa_bedrock`) and user self-hosted (`llamacpp`, `ollama`). **Public** is everything hosted by an AI company or a large cloud — including `azure_openai`, `aws_bedrock`, `databricks` and `vertex`, whatever their names suggest. |
| **DR-2** | **Two lattices, opposite directions.** CAPABILITY (what a session may DO) = the **least** privileged model bound to it, so a mixed lead/worker config gets public reach. CLASSIFICATION (how sensitive its CONTENTS are) = the **most** sensitive thing it has touched, a permanent ratchet. A session can be classified private while holding only public capability. |
| **DR-3** | **A public model must never reach a private session.** Not once, not read-only, not indirectly. The converse is unrestricted: a private model may read anything. |
| **DR-4** | **The ratchet fires on the first TURN and on a permitted private-extension dispatch — never on the bind.** Binding is not when content appears, and ratcheting there would privatise a chat on a mis-click while still missing `POST /agent/call_tool`, which dispatches straight into the extension manager without touching the reply path. |
| **DR-5** | **Lineage decides write access.** Sessions the caller spawned get full control; everything else is read-only. Lineage is **one hop** — a grandchild is `other`. |
| **DR-6** | **The BAAM registry is the sole grantor of a private badge, and anything not on BAAM is PUBLIC** (fail-open, by decision). The private set is exactly **`ucsfomopagent`** and **`cdwagent`**. Built-ins, platform servers and in-process app servers are public. Skills carry no classification. |
| **DR-7** | **`chatrecall` obeys the barrier** — private models recall from private and public, public models from public only. **Side channels (existence, counts, timing) are explicitly out of scope**: no count padding, no constant-time responses, no decoys. Only content must not cross. |
| **DR-8** | **Declassification is the user's alone** — an explicit deprivatise action in History. Nothing automatic, nothing an agent can invoke. Graded by `privacy_reason`: `mcp:*` gets a typed confirmation, `turn:*`-only gets single-click with undo. |
| **DR-9** | ~~**A global opt-out exists, off by default**, scoped to Gate C (the MCP tool gate) only.~~ **Superseded by DR-15.** The operator has since ruled that the opt-out is a *master* switch over the whole feature, which is the wider of the two readings [Open question 3](#open-questions) recorded. The Gate-C-scoped key is retired rather than kept alongside the master one: two switches whose scopes nest are two things a user must reason about at the moment they are least able to, and the narrower one has no remaining job. |
| **DR-14** | **A public-capability session's arbitrary-execution and path-reading tools run under a mandatory read-deny sandbox, on by default.** The tools that spawn a process (`developer__shell` and its background jobs, `computercontroller__automation_script`, `computer_control`) or resolve a caller-supplied path (`developer`'s file tools) run with **four directories hidden**: the session store, the knowledge roots, the global memory root and the Agent Drafter app root. Everything else on the filesystem stays readable and writable, so ordinary work is untouched — this is **not** a general jail and must not become one. **Private-capability sessions are unaffected.** Where the platform cannot express the exclusion, the fail direction is **closed**: those specific tools are refused with an error naming the two ways out (a private model, or turning privacy tiers off), never run unsandboxed. Tasks 14A, 14B and 14C. |
| **DR-15** | **One master toggle turns the entire privacy-tier feature off**, config key `BIOROUTER_PRIVACY_TIERS`, default `on`. Off means: no bind gate, no turn gate, no dispatch gate, no discovery filter, no `chatrecall` filter, no knowledge-base barrier, no spawn matrix, no classification ratchet, and **no read-deny sandbox** (DR-14) — nothing is refused and nothing is sandboxed. It does **not** delete the columns, the stamps already written, or the audit rows, so turning it back on resumes enforcement over the history that existed when it was turned off. It does **not** hide the badges either: they keep rendering, restyled and suffixed *— enforcement off*, beside a persistent strip. A guardrail that vanishes when disabled cannot be noticed by the person who disabled it six months ago; a badge that still reads plain **Private** while nothing enforces it is a false statement. Neither is acceptable, so the badge stays and changes what it says. |
| **DR-10** | **Fail directions differ by kind, deliberately.** Migration backfill → fail **open** (public). Runtime read of a missing/unparseable column → fail **closed** (private, with `error!`). Import with no tier → fail **closed**. Unknown provider → **Public** (fail-*safe*: less privileged). Unlisted extension → **Public** (fail-open, DR-6). Any gate's lookup failing → refuse, encoded inside `Ok(..)`, never as `Err`. |
| **DR-11** | **`medcp` stays callable by a public model**, and that is the accepted cost of DR-6. It is enabled on the operator's machine with `CLINICAL_RECORDS_*` against a clinical MSSQL backend. The reasoning: a hand-installed extension is the user's own choice, and medcp is a *connector* rather than a data source. **The badge is a statement about provenance, not about the data behind the connector.** |
| **DR-12** | **`spokeagent` is public.** SPOKE holds no patient data; its passcode gates the service, not private content. |
| **DR-13** | **A knowledge base ratchets on ingest**, resolving the either/or design §9.3 B4 refused to defer. A KB takes the tier of the most sensitive session that has ingested into it, and a public-capability session may not read *or write* a private KB. The alternative — declare KBs a designed public sink and warn at ingest — was **rejected**. Two costs come with it and were accepted, not overlooked: a KB one private session touched is unreadable from every public chat including the user's own ordinary work, and existing KBs migrate **public** even if a private session fed them. Both are written out in [Accepted risks](#accepted-risks) (AR-1, AR-2); there is no KB declassification path in v1. A third cost — the *existence* of a private base stays inferable from a guessed id, DR-7's side-channel scope applied consistently — is [AR-5](#ar-5--the-existence-of-a-private-knowledge-base-is-still-inferable). Tasks 10A–10D. |

---

## Open questions

The design's eleven, unchanged in substance and re-stated with what this plan does in the meantime.
**Question 1 is the one place the design reads a requirement in spirit rather than letter and still
needs an operator ruling.** The design's twelfth open item — §9.3 B4's knowledge-base either/or — is
**no longer open**: the operator ruled *ratchet*, and it is implemented in Tasks 10A–10D with its
costs recorded in [Accepted risks](#accepted-risks) (AR-1, AR-2 and AR-5).

| # | Question | What this plan does while it is open |
|---|---|---|
| **1** | **Does a mixed lead/worker composite ratchet the session?** R3 says "switched to a private model even once → private permanently", and a private-lead/public-worker composite *contains* a private model. The design says it does **not** ratchet, because `tier = least` and the transcript has already gone to the public worker, and because ratcheting on `max` would make the bind gate refuse that same composite on the next resume — bricking a working configuration. Using one reduction for both the gate and the ratchet is what makes `capability ≥ classification` provable by induction (Task 7). **This is the single place the letter of a requirement was not followed, and it needs a ruling.** | Implements the design: `LeadWorkerProvider::tier() = least(lead, worker)`, and `floor(Public) = Public` so no ratchet fires. Task 5's composite test and Task 7's induction test both encode this; **a ruling the other way changes both tests and the `tier()` override, and nothing else.** |
| **2** | **Is the spawn-downgrade an approval or a refusal?** R4 permits it, so the design makes it an approval showing the task prompt. But the prompt is written by a private-context model and is the only leak vector, and it is the one control a planted `PermissionRequest` hook could bypass — hooks load from `~/.config/biorouter/config.yaml` and, with `allow_project_hooks`, from `.biorouter/hooks.yaml`, both writable by an agent with `text_editor`. | Task 23 implements the approval, behind `requires_downgrade_confirmation`. Flipping it to a `Deny` is one branch. |
| **3** | ~~**Does the R7 opt-out really stop at Gate C?**~~ **CLOSED — the operator ruled: it stops nowhere.** `BIOROUTER_PRIVACY_TIERS=off` disables every gate, the ratchet and the sandbox (DR-15). The original wording — "opt out of the **entire** protection layer" — is now read literally. | Task 30 implements the master toggle and its Step 1 is a ten-row on/off matrix over every gate. The cost this closure buys is real and is recorded as [AR-7](#ar-7--while-the-tiers-are-off-nothing-is-recorded-and-turning-them-back-on-does-not-reclassify-the-gap): while the toggle is off the ratchet does not run, so sessions that handled private material during that window stay stamped `public` for ever. |
| **4** | **Is the first cross-tier write approval remembered per (caller, target) or per call?** Per-pair-per-session-lifetime was chosen because a confirmation on every steer of a public worker is miserable and would be clicked through. | Task 21 exposes `requires_first_crossing_approval`; the memoisation policy lives with BR-71's inspector. |
| **5** | **Institutional Ollama versus hosted Ollama SaaS.** R1 says self-hosted *or* institution-hosted is private, and config cannot tell a lab GPU box at `OLLAMA_HOST=gpu.lab.ucsf.edu` from a hosted SaaS. **This plan disagrees with the design on the severity**: the design rates "non-loopback stays Private" a false-private and "the one place this design is permissive". It is a live bypass — `ProviderEngine::Ollama` plus a remote `base_url` in one agent-writable JSON file mints a Private-tier provider pointing anywhere. Certainty needs a `BIOROUTER_PRIVATE_HOSTS` allowlist, a new concept deliberately not added. | Task 5 makes **loopback-only** Private and non-loopback Public, and its third test encodes the bypass. A lab GPU box therefore reads Public until an allowlist exists. **This is a real ergonomic regression for lab users and needs a ruling.** |
| **6** | **Should `versa_azure` get its own config keys?** It shares all three `AZURE_OPENAI_*` keys with the public `azure_openai` provider, whose shipped default endpoint (`azure.rs:204`) is the same UCSF gateway. The demotion rule catches the dangerous direction, but it means a user can *lose* their private tier by configuring an unrelated provider. | Task 5 implements the endpoint-host demotion. Separate keys are a follow-up. |
| **7** | **Should the compiled-in private baseline be a signed registry snapshot?** Signing would let a *downgrade* be trusted offline. Today the union rule means an extension can only ever gain a private badge without a fresh fetch — safe, but a genuine reclassification-to-public needs connectivity. | Task 37 implements the union rule and Task 34 gates the const against `registry.json`. Signing is a follow-up. |
| **8** | **Who is "who" in the declassification record?** The app is single-user, so the local OS username is recorded. On a shared lab machine that is right; in a multi-account setup it is not, and there is no user identity in the product to record instead. | Task 29 records the OS user + machine in `classification_audit.actor`, with `actor_kind = 'user'` — a value no other code path can construct. |
| **9** | **Skills (R12) carry no classification, which leaves three gaps.** (a) A skill authored while a private chat was open can embed pasted private text and is then readable by every session and publishable to the marketplace. (b) A skill can instruct the model to call `ucsfomopagent` — harmless in effect because Gate C refuses at dispatch, but the steering is unblocked and produces confusing refusals. (c) BR-71 Task 15 lets one session add skills to another. | v1 mitigation is a line in the skill-creation UI (Task 28's copy pass). Closing (a) needs skills to carry a classification, which contradicts R12. |
| **10** | **`ActiveWorkItem.title` is cross-session content and predates all of this** — derived from a subagent's task prompt and surfaced process-wide with a session id. The visibility rule is applied to it, but it is exposed only via `GET /active_work` for the GUI (the model-facing `subagent_status` is session-scoped), so it may deserve its own fix rather than riding this one. | Task 21 provides `appears_in_list`; wiring `/active_work` to it is a follow-up. |
| **11** | **`POST /agent/call_tool` remains inspector-free.** This design is correct either way because the barrier is in the extension manager, but the route is a standing hazard for every *future* inspector-based control, including BR-71's. | Task 14 fixes its error mapping so a refusal reaches the caller as text rather than a bare 500, and Task 20's gate exercises it explicitly. The route itself is unchanged. |

Nine more this plan surfaced. Twelve and thirteen need a ruling before the phase that touches them;
fourteen, fifteen and sixteen are follow-ups whose *residual* is already accepted (AR-3, AR-1, and —
for sixteen — a pre-existing theme gap this feature neither creates nor is scoped to fix).
Seventeen, eighteen and nineteen came out of DR-14 and each has its residual recorded in
[AR-6](#ar-6--on-a-host-that-cannot-express-the-read-deny-a-public-session-loses-the-shell-and-two-costs-come-with-the-sandbox-itself);
**nineteen must not be actioned before eighteen**, because narrowing the Agent Drafter root is only
safe once the app socket no longer treats an app id as an authenticator.

| # | Question | Blocks |
|---|---|---|
| **12** | **Does `ensure_privacy_schema` co-landing with BR-71 need a merge-order decision?** Both branches add `parent_session_id`; both would take migration 17. The shape-guarded arm plus the unconditional reconcile makes either order safe **in the database**, but the two diffs conflict textually in `session_manager.rs`. Resolution guidance: take either side — the columns are identical — and keep the **shape-guarded** form. | Task 6, and BR-71 Task 1. |
| **13** | **Does `medcp`'s continued reachability need a first-run notice, or is the badge enough?** §13.5 specifies a one-time notice naming any **enabled** extension that is Public and declares clinical-looking credentials. On the operator's machine that names exactly one extension, `medcp`, and nothing else changes. | Task 38's notice copy. Hard-code that expectation into its test fixture. |
| **14** | **How does `memory`'s local store get a tier?** AR-3: `compose_instructions` (`memory/mod.rs:277`) inlines local memories in full (`:310-322`) into every session opened in that directory, including one on a public model, and Task 19 ships only a disclosure. The design's §9.3 B3 names the fix — "classify memory entries and filter `retrieve_all` by the session's capability tier at init" — but the on-disk format carries no provenance (`:387-388` writes a `# {tags}` line and bare lines; `:414-418` reads them back keyed by the *tag string*), and `compose_instructions` runs once at `MemoryServer::new` (`:108`) rather than per turn, so a naive capability filter there freezes across a mid-session model swap — the O6 hazard. A real fix needs per-entry provenance **and** a per-turn recompute. | Nothing in this plan. Open it as a follow-up issue at Task 40 Step 6. |
| **15** | **Does a knowledge base need a declassification path, and does the barrier belong on the GUI's own read routes?** Two halves of the same scope question. (a) AR-1: a session can be declassified (Task 29, user-only, graded, audited) and a KB cannot, so a user who ratchets their only base by accident has no in-product exit. (b) Task 10C gates the four `/knowledge/*` **macro** routes (they run a model) and deliberately leaves the GUI's read routes alone (the Knowledge view is the user, not a model) — a defensible line, but it means the *app* shows a private base that the *agent* in the next tab cannot read, and nobody has decided whether that asymmetry should be visible in the UI. | Nothing in this plan; both are follow-ups. (a) is the one a user will hit first. |
| **17** | **Should Linux get a Landlock read-deny by granting the complement?** Landlock has no deny rule, so hiding a subpath means handling read accesses and granting read to every sibling of every ancestor of every deny root. Task 14A declines it in v1 for three measured reasons, the disqualifying one being that anything created in an enumerated ancestor *after* the ruleset is built is unreadable for that command's lifetime — `cd ~ && mkdir out && echo x > out/f && cat out/f` fails. | Task 14A makes `bubblewrap` the only Linux mechanism that can express the read-deny, and the refusal names `apt install bubblewrap` as the fix. A Landlock complement would remove that dependency; it needs a real ergonomics trial on a populated `$HOME` before it is worth the failure mode. |
| **18** | **Should the per-app agent WebSocket be authenticated by something a shell cannot obtain?** `GET /apps/{id}` and `GET /apps/{id}/agent` are deliberately unauthenticated (`auth.rs:52-78`), and `serve_index` (`apps.rs:168-184`) embeds the socket token in the page it serves, so any loopback client that knows an app id can read the token and drive that app's agent. DR-14 removes the two local sources of app ids (`GET /apps` needs the secret; the app tree is deny root #4) but does not close the path for an id the model already has. | Nothing in this plan; the residual is stated in [AR-6](#ar-6--on-a-host-that-cannot-express-the-read-deny-a-public-session-loses-the-shell-and-two-costs-come-with-the-sandbox-itself) and pinned by Task 14C's `the_unauthenticated_app_surface_does_not_grow_by_accident`. |
| **20** | **Should the daemon's HTTP API authenticate a caller that is on the same machine?** [AR-11](#ar-11--the-daemons-own-api-secret-is-recoverable-so-the-second-door-is-held-by-layer-a-and-not-by-the-environment-strip): the secret is recoverable from the daemon's own environment (`ps -Ewww -p $PPID` on macOS, `/proc/self/environ` in-process on Linux), so `check_token`'s header comparison stops a remote caller and not a local one. Layer A covers the biggest local route, `POST /agent/call_tool`, because that route dispatches through the same choke point. It does **not** cover the routes that return private content without running a tool: `GET /sessions/{id}/export` and the rest of the transcript family, the `/knowledge/*` read routes, `GET /apps/{id}/export`, and `GET /diagnostics/{id}` — which returns a zip of `session.json`, recent `logs/*.jsonl` and a verbatim `config.yaml`, and is the widest single route in the API. | Nothing in this plan. Task 14C states the residual instead of the old "no way to authenticate" claim, and pins the strip so the *remote* half stays closed. Closing the local half needs a per-caller credential the daemon does not hand to its own children — the same shape as [Open question 18](#open-questions), and probably the same fix. |
| **19** | **Should DR-14's Agent Drafter root narrow to `.vault/` plus other sessions' apps?** Denying the whole root means a public-capability chat cannot `cat` its own app's source from the shell, which is a real ergonomic loss for the drafter workflow (AR-6(3)). The whole root is on the list because it is also the only on-disk source of app **ids**, which Open question 18 shows are load-bearing. | Task 14B denies the whole root. Narrowing it is safe only after 18 is closed. |
| **16** | **`--text-subtle` on `--background-medium` is sub-AA in three of the six family×mode scopes, and #56 is not the right owner of the fix.** Measured with `ui/desktop/scripts/lib/theme-tokens.mjs`: parchment:dark **3.75**, alma-mater:light **4.45**, alma-mater:dark **4.28**, against a 4.5 floor. `--background-medium` is the row-hover ground that `biorouter-list-row`, `SessionItem` and `ExtensionItem` all paint, so this affects every subtle label on a hovered row **today** — it is a pre-existing gap, not something the privacy badge introduces, and `check-contrast.mjs` has never asserted it. Task 26 therefore audits only `--text-default` and `--text-muted` on that ground (the two the badge actually uses) and the total is **288**, not 294. Auditing the third token as well makes the run exit 1 with three failures whose only fix is a theme-token edit — precisely the "Zero theme work" Task 26 Step 5 forbids, and a scope the privacy feature has no business taking. | Nothing in this plan. Open it as a theme/a11y follow-up at Task 40 Step 6, alongside the deferred findings from the 2026-07 theme redesign. Do **not** close it by lowering the threshold in `check-contrast.mjs`. |

---

## Related documentation

- [Privacy tiers](privacy-tiers.md) — the design this plan executes, and the specification each task is reviewed against.
- [Data privacy and patient data](data-privacy-and-phi.md) — the provider guidance this system enforces mechanically.
- [Secret storage](secret-storage.md) — the credential model §9.3 A1 turns on; Task 2 pins the strip that closed it and records which of its three fixes are still open.
- [BR-71 execution plan](../agent-loop/designs/br71-execution-plan.md) — the plan this one must land ahead of, and whose Task 1 collides with Task 6's migration.
- [Subagents](../agent-loop/subagents.md) — the inheritance behaviour Task 23 gates.
- [Tool routing](../agent-loop/tool-routing.md) — the chatrecall/workspace split Gate D sits inside.
- [Multi-KB implementation plan](../knowledge-base/multi-kb-implementation-plan.md) — the "one axis, one pointer" visible-set model whose explicit-`kb_id` escape hatch Tasks 10A–10C now qualify.
- [Launching the dev GUI](../desktop-ui/launching-the-dev-gui.md) — required reading before any GUI verification step.
- [Documentation style](../contributing/documentation-style.md) and [documentation organization](../organization.md) — both binding on Task 39.
