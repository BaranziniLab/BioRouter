# Privacy tiers — implementation plan

> **What this is.** The task-by-task execution plan for the privacy-tier capability system
> designed in [`privacy-tiers.md`](privacy-tiers.md) ([issue #56](https://github.com/BaranziniLab/biorouter/issues/56)):
> forty-three tasks in seven phases — forty numbered, plus **10A, 10B and 10C**, the knowledge-base
> tier the operator ruled on after the first adversarial review — each with a Files table, a failing
> test, complete implementation code, a run step, a gate that fails a plausible wrong implementation,
> and one commit.
> **Status:** Proposed — ready to execute. The design's rulings are settled (see
> [Decisions of record](#decisions-of-record)); the costs the operator knowingly accepted are in
> [Accepted risks](#accepted-risks); fifteen questions remain open (see
> [Open questions](#open-questions)) — the design's eleven plus four this plan surfaced — and none
> of them blocks Phase 0–3.
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
> to a symbol; the contrast total is **294** in both places that quote it; and the hand-traced type
> errors S1–S9 are corrected. Two further defects this pass found on its own are also fixed: the
> `privacy::` test count in Task 7's gate was 4 where the code produces 5, and
> `POST /knowledge/bases/{id}/ingest-conversation` is a **third** fully-open cross-session read that
> the first version's Gate G did not cover. See
> [Which test filters are validated, and which are not](#which-test-filters-are-validated-and-which-are-not).

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

BR-71 names five; this plan has twelve, and each one has a failure mode behind it.

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
the two: the phase opens with Task 10 (LOAD), then Tasks 10A–10C (the sink), then Task 11 (ingest).
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
**shape-guarded numbered arm plus an unconditional `reconcile_privacy_schema`**, following the
`ensure_session_incarnation_schema` precedent (`:2782-2789`, called from `reconcile_loop_schema`
`:2354`, itself called at `:2349` *after* the version loop). With that, merge order is free in both
directions.

**O11 — The whole of `landing/` (Phase 5) is independent and may ship on any cadence.**
Enforcement runs off the compiled-in const, so the website blocks nothing. It is sequenced last
because its `--check` gate needs the generated Rust file to exist, not because anything waits on it.

**O12 — The knowledge-base tier store precedes the KB ratchet, which precedes the KB read barrier,
and all three precede Gate G.**
Task 10A (store + caller-capability channel + migration) → Task 10B (the ratchet on every write) →
Task 10C (the read barrier) → Task 11 (Gate G). Reversing 10B and 10C ships a barrier that refuses
nothing, because on a freshly-migrated machine **every** KB is public until a private session writes
to one — so a read gate landing first is green everywhere and proves nothing, and its own tests have
to fabricate a tier the tree cannot yet produce. And Task 11's second test asserts the ratchet, so it
cannot be written before 10B exists. Nothing here depends on `sessions.privacy_tier`: a KB's tier
lives in its own machine-local store (`<knowledge-root>/.kb-tiers`), because
`crates/biorouter-mcp` **cannot depend on `crates/biorouter`** — the dependency runs the other way
(`extension_manager.rs:1512` uses `biorouter_mcp::secret_guard`), which is the same constraint that
made the knowledge macros take a `Box<dyn Completer>` instead of a `Provider`.

---

## Departures from the design

Eight, each forced by a measurement.

| # | Design says | This plan does | Why |
|---|---|---|---|
| D1 | §11.1/§19: the chatrecall LOAD guard is "five lines, ship it first, ahead of everything else in this design" | Ships it as the **first gate**, after the tier model | The guard compares the caller's capability with the target's classification. Neither exists before Phase 1. "First" is honoured within the gates. |
| D2 | §9.3 B1: "put the carry-over on `create_session` itself, parameterised" | Introduces one `create_derived_session` helper that the three copy paths share | `grep -rn --include='*.rs' "\.create_session(" crates/` returns **104** call sites. Parameterising a 3-arg function with 104 callers to fix three of them is a worse trade than collapsing the three hand-rolled builders into one. Task 22 keeps the design's enumeration test. |
| D3 | §5.1: `Classification` as the stored enum name | `SessionClassification` | `crates/biorouter/src/security/classification_client.rs` already defines `ClassificationClient` / `ClassificationRequest` / `ClassificationResponse` (an unrelated HuggingFace text classifier) in the same crate. |
| D4 | §14.1: Private pill = `--background-muted` fill + `--text-standard` label; Public pill = 1 px `--border-subtle` hairline + `--text-subtle` label | Private = `bg-background-muted text-text-default`; Public = `bg-background-muted text-text-muted` | `--text-standard` **does not exist** (`grep -rn "var(--text-standard)" ui/desktop/src` → 0; the only textual hit is a comment in `search.css:2` saying so). And no border token in the system reaches 3:1 on a pill's real ground: measured with the repo's own `ui/desktop/scripts/lib/theme-tokens.mjs`, `--border-subtle` vs `--background-muted` is **1.00–1.24** across all six family×mode scopes (parchment:dark is exactly 1.00 — identical colours). An outline pill is not expressible here. Full measurements in Task 26. |
| D5 | §15.1: "added by the same `ALTER TABLE sessions ADD COLUMN` arm BR-71 Task 1 uses" | Shape-guarded arm 17 **plus** an unconditional `reconcile_privacy_schema` | O10. |
| D6 | §18.4: the prompt-hook provider check is "v1 emits a load-time warning; the hard skip is v1.1" | Hard refusal in v1, in the same task as the CLI plan-mode refusal | The Stop hook's payload is `crate::agents::goal::transcript_tail(&conversation)` (`agent.rs:5495-5496`) — a real transcript excerpt — shipped to an arbitrary endpoint resolved by `HooksManager::resolve_prompt_provider` (`hooks/mod.rs:690`) and sent by `run_prompt_hook` (`hooks/prompt_runner.rs:57`). It is structurally identical to P6 and carries the same content. |
| D7 | §9.3 B4: "Ratchet a KB's classification on ingest … **or** state plainly that KBs are a designed public sink" | Ratchets (operator ruling), **and** enumerates the read side rather than stating it abstractly: seven explicit-`kb_id` entry points, not just `kb_search` | The design says "a public-capability session may not read a private KB" without naming where that is enforced. `kb_id_or_primary`'s own doc comment (`knowledge/server.rs:308-311`) says "An explicit `kb_id` always wins and is **never filtered** against the session's set", and four tools route through it (`kb_list_pages` `:379`, `kb_read_page` `:396`, `kb_get_graph` `:482`, `kb_list_history` `:497`) on top of `kb_search`'s own branch at `:590-592`, `kb_search_raw_sources`' at `:618-619` and `kb_export` at `:743`. Gating only `kb_search` leaves six live doors, which is the Task 15 failure mode one module over. |
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
  `kb_get_graph`, `kb_list_history`, `kb_search_raw_sources` and `kb_export`, **including for
  material that had nothing to do with the private work**. The KB does not un-ratchet. There is no
  per-page tier and there will not be one in v1: pages are markdown in a git tree, and per-page
  classification is a storage redesign.
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

---

## Which test filters are validated, and which are not

The adversarial verifier could not run a single `cargo test` filter — the `privacy::` modules do not
exist yet, so `cargo test -- --list` cannot resolve them even after a build — and named this "the
single biggest hole in my own coverage", because BR-71's most expensive defect was *a filter that
names a nested module by the wrong path, prints `0 passed`, and exits 0*. This section closes as much
of that hole as is closable before any code exists.

**How each filter was checked.** For every `cargo test` line in this plan, the module path it implies
was resolved against the tree: for an existing module, that the file exists at the path the filter
spells **and** that it contains a `#[cfg(test)] mod tests`; for a module this plan creates, that the
task's Files table puts the file where the filter's path implies.

**Four filters name a module that has no test module today, so they print `0 passed` and exit 0 until
the task that owns them lands.** This is not a defect in the filter — it is the reason each of those
tasks must state a *pre-count of zero* and assert the exact post-count:

| Filter | Module today | Task |
|---|---|---|
| `cargo test -p biorouter --lib agents::chatrecall_extension` | `chatrecall_extension.rs` has **no** `mod tests` | 10, 17 |
| `cargo test -p biorouter --lib session::chat_history_search` | `chat_history_search.rs` has **no** `mod tests` | 17 |
| `cargo test -p biorouter-server --lib routes::agent` | `routes/agent.rs` has **no** `mod tests` | 12, 14 |
| `cargo test -p biorouter-server --lib routes::session` | `routes/session.rs` has **no** `mod tests` | 22, 29 |

Ten of the twelve `crates/biorouter-server/src/routes/*.rs` files with tests were checked; `apps.rs`
and `config_management.rs` (the two this plan filters on besides the four above) both have one.

**Two syntax rules, both verified analytically.**
`cargo test --lib A B` is a hard error (`unexpected argument 'B' found`) — cargo takes exactly one
`TESTNAME` positional, so multiple filters must go after `--`, where libtest ORs them. And a libtest
filter that matches nothing prints `0 passed` and **exits 0**, which is why every gate in this plan
asserts a *count*, never an exit code.

⚠ **`cargo test -p <pkg> --lib <MODULE> -- name1 name2` does not do what it looks like.** Cargo passes
its own `TESTNAME` positional to libtest as *another* OR'd filter, so the module runs in full and the
names after `--` add nothing. Task 6 Step 2 carried this shape and has been corrected to drop the
positional. If you want exactly N named tests, pass **only** names after `--`.

**What remains unvalidated, and why.** Nothing here was *executed*. The filters were resolved
statically against file paths and `mod tests` presence, which catches the BR-71 defect class (a path
that resolves to nothing) but not two others: (a) a test that exists under a *different* nesting than
its file suggests — e.g. a helper `mod` inside `mod tests` — and (b) an expected pass-count that is
right for the module today and wrong after another task adds tests to the same module. Every gate in
this plan that quotes a pass count is therefore paired with either a named-test filter or a
pre/post delta the task records itself. **The first worker to run `cargo test -- --list` should paste
the real module paths into a PR comment**; that is the only thing that closes (a) completely.

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

### Task 2: Scrub daemon credentials from the stdio MCP extension spawn

The remaining half of §9.3 A1, and a live credential leak independent of this design. Any stdio MCP
extension the user installs inherits `BIOROUTER_SERVER__SECRET_KEY` from the daemon's environment,
which is plain header equality at `auth.rs:115-126` against a loopback-bound API that exposes
`GET /sessions/{id}/export`.

**Files:**

| Action | Path | Anchor (re-verified at `9558c346`) |
|---|---|---|
| Modify | `crates/biorouter/src/agents/extension_manager.rs` | the stdio spawn `Command::new(cmd).configure(...)` at `:738-750`; `command.args(args).envs(all_envs)` at `:748-750` |
| Modify | `crates/biorouter/Cargo.toml` | add `biorouter-sandbox` if absent (check first: `grep -n biorouter-sandbox crates/biorouter/Cargo.toml`) |
| Reference | `crates/biorouter-sandbox/src/environment.rs` | `strip_daemon_private_env` `:54-79`; `is_daemon_private_env_key` `:36-50`; its own test `:94-103` |

- [ ] **Step 1: Write the failing test**

Add to `#[cfg(test)] mod tests` in `extension_manager.rs`:

```rust
#[test]
fn the_stdio_spawn_environment_carries_no_daemon_credential() {
    // The daemon puts BIOROUTER_SERVER__SECRET_KEY into every child's
    // environment (ui/desktop/src/biorouterd.ts additionalEnv). auth.rs is a
    // plain header equality and the API is loopback-bound, so any stdio MCP
    // server that inherits it can read every session's transcript through
    // GET /sessions/{id}/export. BIOROUTER_PORT is deliberately NOT stripped:
    // exported apps need it (see biorouter-sandbox environment.rs:94-103).
    let mut env: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    env.insert("BIOROUTER_SERVER__SECRET_KEY".into(), "s3cret".into());
    env.insert("BIOROUTER_ACP_WS_TOKEN".into(), "tok".into());
    env.insert("BIOROUTER_PORT".into(), "3000".into());
    env.insert("MY_EXTENSION_KEY".into(), "keep-me".into());

    let scrubbed = super::scrub_daemon_env(env);

    assert!(!scrubbed.contains_key("BIOROUTER_SERVER__SECRET_KEY"));
    assert!(!scrubbed.contains_key("BIOROUTER_ACP_WS_TOKEN"));
    assert_eq!(scrubbed.get("BIOROUTER_PORT").map(String::as_str), Some("3000"));
    assert_eq!(scrubbed.get("MY_EXTENSION_KEY").map(String::as_str), Some("keep-me"));
}
```

- [ ] **Step 2: Run it**

```bash
cargo test -p biorouter --lib agents::extension_manager::tests::the_stdio_spawn_environment_carries_no_daemon_credential
```

Expected: **COMPILE ERROR** — `cannot find function scrub_daemon_env in this scope`.

- [ ] **Step 3: Implement**

In `extension_manager.rs`, above the stdio spawn:

```rust
/// Remove the daemon's own credentials from an environment about to be handed
/// to a child process. Issue #57 did this for the Developer shell
/// (`configure_shell_command` -> `strip_daemon_private_env`); the stdio MCP
/// spawn was missed, so every installed extension inherited
/// `BIOROUTER_SERVER__SECRET_KEY` and could read any session through
/// `GET /sessions/{id}/export`. Reuses the same key predicate so the two paths
/// can never diverge — `BIOROUTER_PORT` is intentionally preserved.
fn scrub_daemon_env(
    mut envs: std::collections::HashMap<String, String>,
) -> std::collections::HashMap<String, String> {
    envs.retain(|k, _| !biorouter_sandbox::environment::is_daemon_private_env_key(k));
    envs
}
```

and at the spawn (`:748-750`), change

```rust
        command.args(args).envs(all_envs);
```

to

```rust
        command.args(args).envs(scrub_daemon_env(all_envs));
```

If `is_daemon_private_env_key` is not `pub`, make it `pub` in
`crates/biorouter-sandbox/src/environment.rs` in the same commit; do **not** re-implement the key
list here.

- [ ] **Step 4: Run**

```bash
cargo test -p biorouter --lib agents::extension_manager
cargo test -p biorouter-sandbox --lib environment
```

Expected: **PASS**. Record the pre-task count for `agents::extension_manager` before Step 3 and
assert the post-task count is exactly `pre + 1`.

- [ ] **Step 5: Gate**

```bash
# The one call site exists...
grep -c "scrub_daemon_env(all_envs)" crates/biorouter/src/agents/extension_manager.rs ; echo "expect: 1"
# ...and the unscrubbed form is gone.
grep -c "\.envs(all_envs)" crates/biorouter/src/agents/extension_manager.rs ; echo "expect: 0"
# The key list is not duplicated — asserted on the HELPER, not on the file.
awk '/fn scrub_daemon_env/,/^}/' crates/biorouter/src/agents/extension_manager.rs \
  | grep -c "BIOROUTER_" ; echo "expect: 0 — the helper names no key; it delegates to the predicate"
awk '/fn scrub_daemon_env/,/^}/' crates/biorouter/src/agents/extension_manager.rs \
  | grep -c "is_daemon_private_env_key" ; echo "expect: 1"
# And the file-wide count, which exists only to catch a key literal that escaped
# the helper into the spawn itself.
grep -c "BIOROUTER_SERVER__SECRET_KEY" crates/biorouter/src/agents/extension_manager.rs
echo "expect: 2 — both in Step 1's test (the env.insert and the assert!), and nowhere else"
```

⚠ **`expect: 1 (the test only)` was wrong** and would have failed a correct implementation: Step 1's
test names the key **twice**, once in `env.insert(..)` and once in
`assert!(!scrubbed.contains_key(..))`. Counting occurrences of a literal in a whole file is exactly
the fragile shape this plan warns about elsewhere; the two `awk` gates above are the ones that
actually express "the key list is not duplicated", because they are scoped to the helper. Keep the
file-wide count as a tripwire, not as the gate, and update its expected number if you add or remove
an assertion in the test.

**What this catches.** The wrong implementation writes `.env_remove("BIOROUTER_SERVER__SECRET_KEY")`
on the `Command`, which (a) misses `BIOROUTER_ACP_WS_TOKEN` and every future key and (b) leaves a
second copy of the key list that will drift from `biorouter-sandbox`'s. The two helper-scoped `awk`
gates are what catch it: an inline key list puts a `BIOROUTER_` literal inside `scrub_daemon_env`,
and delegating to the shared predicate is the only way to score 0/1. A test asserting only "the
secret is absent" passes the `.env_remove` version.

- [ ] **Step 6: Commit**

```bash
git add crates/biorouter/src/agents/extension_manager.rs crates/biorouter-sandbox/src/environment.rs
git commit -m "fix(extensions): strip daemon credentials from the stdio MCP spawn (#56, completes #57)"
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

```bash
just debug-server &      # BIOROUTER_SERVER__SECRET_KEY=test, port 3000
# Add a trivial stdio extension whose command prints its own environment, then:
curl -s -X POST http://127.0.0.1:3000/agent/call_tool -H 'X-Secret-Key: test' \
  -H 'Content-Type: application/json' \
  -d '{"session_id":"'"$SID"'","name":"envprobe__printenv","arguments":{}}' | grep -c BIOROUTER_SERVER__SECRET_KEY
# Expected: 0. Before this task it is 1.
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
# The tier is never keyed on a name in the enforcement path.
grep -rn 'PRIVATE_PROVIDERS' crates/biorouter/src/providers/ | grep -v '_test' ; echo "expect: no output"
# The two renderer Sets are gone, and nothing reintroduces them.
grep -c "new Set(\['versa_azure'" ui/desktop/src/components/settings/providers/providerOrdering.ts ; echo "expect: 0"
grep -c "new Set(\['llamacpp'" ui/desktop/src/components/settings/providers/providerOrdering.ts ; echo "expect: 0"
# Six tier() implementations, and the gate ENUMERATES them rather than counting:
# one trait default plus five overrides. A bare count invites "fixing" a
# mismatch by deleting an override, which is the one direction that leaks.
grep -rln "fn tier(&self)" crates/biorouter/src/providers/ | sort
# expect exactly these six, no more and no fewer:
#   crates/biorouter/src/providers/base.rs          (the trait default = Public)
#   crates/biorouter/src/providers/lead_worker.rs   (least of its two halves)
#   crates/biorouter/src/providers/llamacpp.rs      (loopback-only)
#   crates/biorouter/src/providers/ollama.rs        (loopback-only)
#   crates/biorouter/src/providers/versa_azure.rs   (UCSF-gateway-host-only)
#   crates/biorouter/src/providers/versa_bedrock.rs (UCSF-gateway-host-only)
grep -rn "fn tier(&self)" crates/biorouter/src/providers/ | wc -l ; echo "expect: 6"
```

⚠ **`expect: 5` was wrong** in the first version of this plan and would have failed a correct
implementation: the trait default in `base.rs` matches `fn tier(&self)` too, so one default plus five
overrides is **6**. Verified 0 in the tree today, so there is no pre-existing offset absorbing the
difference. The file enumeration above is the real gate; the count is the tripwire.

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
# Exactly one function decides an extension's tier.
grep -rn --include='*.rs' "PRIVATE_EXTENSIONS" crates/ | grep -v registry_private.rs | grep -v _test
# expect: exactly 1 hit, in privacy/extensions.rs
# The three admission points each stamp it.
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
cargo test -p biorouter --lib -- \
  the_reconcile_adds_the_columns_even_when_the_version_says_it_already_ran \
  a_fresh_database_defaults_every_session_public
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

Fourteen tasks — eleven numbered plus **10A, 10B and 10C**. The design names five gates; this phase
ships **ten**, because adversarial review of the tree found five live paths the five do not cover
(Tasks 11, 18 and 19) and the second review round added the knowledge-base barrier under an operator
ruling (Tasks 10A–10C; see [Accepted risks](#accepted-risks)). Order inside the phase is O8 (the two
fully-open reads first), then O12 (the KB tier, its ratchet, its barrier), then O3/O4 (bind, then
turn), then the extension gates.

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
# The guard precedes the header construction, not follows it.
awk '/fn handle_chatrecall/,/fn [a-z_]+\(.*SEARCH/' \
  crates/biorouter/src/agents/chatrecall_extension.rs \
  | grep -n "visible_to\|Working Dir:" | head -4
# Expected: the `visible_to` line comes BEFORE the "Working Dir:" line.
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

**Files:**

| Action | Path | Anchor (re-verified at `9558c346`) |
|---|---|---|
| Create | `crates/biorouter-mcp/src/knowledge/tier.rs` | new |
| Modify | `crates/biorouter-mcp/src/knowledge/mod.rs` | the `pub mod` list |
| Modify | `crates/biorouter-mcp/src/knowledge/paths.rs` | add `kb_tiers_path` beside `primary_kb_path` `:62-64`, `primary_kb_sessions_dir` `:69-71`, `hidden_kbs_path` `:73-75`, `hidden_kb_sessions_dir` `:77-79`; `knowledge_root` `:43-45`; `kb_root` `:47-49`; `validate_kb_id` `:3-20` |
| Modify | `crates/biorouter-mcp/src/knowledge/service.rs` | `KnowledgeService::new` `:404`, `new_default` `:411`, `root()` `:415`; `create_base` `:447`; `import_brkb` `:506`; `list_bases` `:523`; `delete_base` `:657`; `lock_root()` — the existing root-level lock the hidden-list setters take (`set_hidden_persisted` `:1193-1198`) |
| Modify | `crates/biorouter-mcp/src/knowledge/server.rs` | `SESSION_ID_META_KEY` `:18`; `session_id_from_context` `:222-224`; `session_id` `:226-228`; `KnowledgeServer::new` `:214` |
| Modify | `crates/biorouter/src/agents/mcp_client.rs` | `McpMeta` `:136-144` (two fields today), `McpMeta::new` `:146-152`, `with_progress_token` `:156-159`, `inject_into_extensions` `:161-172` |
| Modify | `crates/biorouter/src/agents/extension_manager.rs` | the sole production `McpMeta::new(&session_id)` at `:1557`, inside `dispatch_tool_call`'s spawned future (`:1544-1570`) |
| Reference | `crates/biorouter-mcp/src/knowledge/manifest.rs` | `save` `:17-24` — the tmp-then-`rename` idiom to copy; and the reason the tier does **not** go in `Manifest` (`types.rs:58-66`): the manifest travels inside the `.brkb` archive |

⚠ **Four design decisions, each with a reason a reviewer will otherwise ask about.**

1. **A `bool`, not a third enum.** `crates/biorouter-mcp` **cannot** depend on `crates/biorouter` —
   the dependency runs the other way (`extension_manager.rs:1512` uses
   `biorouter_mcp::secret_guard`), which is the same constraint that made the knowledge macros take
   a `Box<dyn Completer>` instead of a `Provider`. So `ProviderTier` is not nameable here. The
   choices are a duplicate enum that must be kept in sync by discipline, or one boolean named
   `caller_is_private`. This plan takes the boolean, and the precedent is in this plan already:
   Task 12's `bind_provider_if_allowed(.., incoming_is_private: bool)`. Because `floor(Private) =
   Private` and `floor(Public) = Public`, the boolean *is* the crossing — which is why Task 7's
   `floor` caller set does not grow here.
2. **A sidecar, not `manifest.yaml`.** The manifest is inside the KB's git tree and is carried by
   `export_brkb`/`import_brkb` (`service.rs:495`/`:506`), so a tier stored there is
   **attacker-supplied on import** — the exact shape Task 22 refuses for session imports. The
   sidecar sits beside `.active-kb` and `.hidden-kbs`, which are already machine-local, already
   outside every KB's repo, and already excluded from the archive.
3. **Fail directions, and they differ on purpose** (DR-10's pattern, one module over). Migration →
   **public** (fail open; AR-2). A kb id with no entry in an *existing* store → **private** (fail
   closed: a directory that appeared without going through `create_base` or `import_brkb` has
   unknown provenance). An absent capability meta key → the caller is **Public** (fail closed for
   reads, which is what Task 10C consumes it for).
4. **The capability meta key goes to built-in servers only.** `McpMeta::new` already ships the
   session id to *every* MCP server including third-party stdio ones; the capability tier
   deliberately does not follow that precedent, because "this user is on an institutional model" is
   a fact about the user's configuration and a third-party server has no business learning it. The
   injection is conditioned on `biorouter_mcp::BUILTIN_EXTENSIONS` membership
   (`crates/biorouter-mcp/src/lib.rs:96`, 7 entries).

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

    ensure_migrated(&root).unwrap();
    assert_eq!(tier_of(&root, "default"), Public);
    assert_eq!(tier_of(&root, "omop"), Public);

    raise(&root, "omop", /* caller_is_private */ true).unwrap();
    ensure_migrated(&root).unwrap();                 // second launch
    assert_eq!(tier_of(&root, "omop"), Private, "the migration re-ran and lowered a tier");
}

#[test]
fn a_base_that_never_went_through_create_or_import_reads_private() {
    // Fail-closed, and it is the difference between "known public" and
    // "unknown". A store that listed only the private ids could not tell them
    // apart, which is why the file is a map and not a list like `.hidden-kbs`.
    let root = tempdir_with_bases(&["default"]);
    ensure_migrated(&root).unwrap();
    std::fs::create_dir_all(root.join("dropped-in-by-hand")).unwrap();
    assert_eq!(tier_of(&root, "dropped-in-by-hand"), Private);
    assert_eq!(tier_of(&root, "default"), Public);
}

#[test]
fn raise_is_monotone_and_registers_an_absent_base_at_the_callers_tier() {
    let root = tempdir_with_bases(&[]);
    ensure_migrated(&root).unwrap();

    raise(&root, "fresh", false).unwrap();           // created from a public chat
    assert_eq!(tier_of(&root, "fresh"), Public);
    raise(&root, "fresh", true).unwrap();            // a private chat writes to it
    assert_eq!(tier_of(&root, "fresh"), Private);
    raise(&root, "fresh", false).unwrap();           // and a public chat writes again
    assert_eq!(tier_of(&root, "fresh"), Private, "a public write lowered the tier");

    raise(&root, "born-private", true).unwrap();
    assert_eq!(tier_of(&root, "born-private"), Private);
}

#[test]
fn deleting_a_base_forgets_its_tier_so_the_id_can_be_reused() {
    // Otherwise `kb_create_base("omop")` from a public chat, after a private
    // `omop` was deleted, silently inherits Private and the user cannot see why.
    let root = tempdir_with_bases(&["omop"]);
    ensure_migrated(&root).unwrap();
    raise(&root, "omop", true).unwrap();
    forget(&root, "omop").unwrap();
    raise(&root, "omop", false).unwrap();
    assert_eq!(tier_of(&root, "omop"), Public);
}

#[test]
fn the_store_is_written_atomically_and_never_leaves_a_tmp_file() {
    // manifest.rs:17-24's idiom. A torn write here reads as "no entry", which
    // fails CLOSED and locks the user out of their own knowledge base.
    let root = tempdir_with_bases(&["default"]);
    ensure_migrated(&root).unwrap();
    raise(&root, "default", true).unwrap();
    let names: Vec<_> = std::fs::read_dir(&root).unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string()).collect();
    assert!(names.iter().any(|n| n == ".kb-tiers"));
    assert!(!names.iter().any(|n| n.ends_with(".tmp")));
}
```

```rust
// crates/biorouter/src/agents/mcp_client.rs, in its existing #[cfg(test)] mod tests

#[test]
fn the_capability_tier_rides_the_same_meta_object_as_the_session_id() {
    let meta = McpMeta::new("sess-1").with_capability_private(true);
    let ext = meta.inject_into_extensions(Extensions::default());
    let m = ext.get::<Meta>().unwrap();
    assert_eq!(m.0.get("biorouter-session-id").and_then(|v| v.as_str()), Some("sess-1"));
    assert_eq!(m.0.get("biorouter-capability-tier").and_then(|v| v.as_str()), Some("private"));
}

#[tokio::test]
async fn a_third_party_extension_never_learns_the_capability_tier() {
    // Decision 4. The session id already goes everywhere; this does not.
    let em = manager_with(stdio_ext("some-third-party"), builtin_ext("knowledge")).await;
    bind_private_provider(&em).await;
    assert_eq!(meta_seen_by(&em, "some-third-party__ping").await
                   .get("biorouter-capability-tier"), None);
    assert_eq!(meta_seen_by(&em, "knowledge__kb_list_bases").await
                   .get("biorouter-capability-tier").and_then(|v| v.as_str()), Some("private"));
}
```

- [ ] **Step 2: Run**

```bash
cargo test -p biorouter-mcp --lib knowledge::tier
cargo test -p biorouter --lib agents::mcp_client
```

Expected: **COMPILE ERROR** — `unresolved module tier`, and `no method with_capability_private`.

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

(b) `tier.rs` — the whole store, ~90 lines, no I/O outside `root`:

```rust
//! The knowledge-base privacy tier (issue #56, design §9.3 B4).
//!
//! A knowledge base takes the tier of the most sensitive session that has
//! ingested into it. This module owns the store and the monotone raise; Task 10B
//! calls `raise` from every content-bearing write and Task 10C reads `is_private`
//! at every entry point that names a base.
//!
//! A `bool` rather than an enum because `biorouter-mcp` cannot depend on
//! `biorouter`, where `ProviderTier` lives — see the task's decision (1).
//! `caller_is_private == true` is exactly `floor(Private) == Private`.

const SCHEMA: u32 = 1;

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct Store {
    schema: u32,
    /// kb id -> "public" | "private". An id ABSENT from a store that exists is
    /// unknown provenance and reads PRIVATE; the whole file being absent means
    /// the migration has not run yet.
    bases: std::collections::BTreeMap<String, String>,
}

/// One-time migration. Every base that exists when this first runs becomes
/// PUBLIC (fail-open, DR-10 and AR-2). Guarded by the file's absence, exactly
/// as `ensure_privacy_schema` is guarded by `table_has_column`: re-running it on
/// every startup would re-add a base whose entry a later `forget` removed, and
/// would race the ratchet.
pub fn ensure_migrated(root: &std::path::Path) -> anyhow::Result<()> { … }

/// PRIVATE unless the store says otherwise. Fail-closed on: no entry, an
/// unparseable file, an unreadable file. Each of those logs at `error!` and
/// paints the base with a badge the user will report on day one — the same
/// trade `SessionClassification::from_stored` makes for the same reason.
pub fn is_private(root: &std::path::Path, kb_id: &str) -> bool { … }

/// Monotone. Registers `kb_id` at the caller's tier if absent, raises it to
/// private if the caller is private, and can never lower it — the file-store
/// twin of the `privacy_tier = CASE WHEN` fragment in `session_manager.rs`.
pub fn raise(root: &std::path::Path, kb_id: &str, caller_is_private: bool) -> anyhow::Result<()> { … }

/// Drop the entry when the base is deleted, so a later base reusing the id is
/// classified by its own creator rather than by a base that no longer exists.
pub fn forget(root: &std::path::Path, kb_id: &str) -> anyhow::Result<()> { … }
```

Write through the same tmp-then-`rename` idiom as `manifest::save` (`manifest.rs:17-24`), and take
`KnowledgeService::lock_root()` around every read-modify-write, which is the lock
`set_hidden_persisted` (`service.rs:1193-1198`) already uses for the sibling sidecar.

⚠ **Known residual, stated rather than discovered.** `lock_root()` is in-process. Two Biorouter
processes (the desktop app and a terminal `biorouter`) raising two different bases at the same instant
can still lose one edit, and the lost edit could be a *raise*. This is the same read-modify-write
hazard `set_hidden_persisted`'s own doc comment already documents for `.hidden-kbs`, it predates this
work, and closing it needs an OS advisory lock the tree does not have anywhere. Do not silently widen
the scope to fix it; open a follow-up.

(c) Call `ensure_migrated` from `KnowledgeService::new` (`service.rs:404`), so both `new_default`
(`:411`) and every test root get it. Then **register** the tier at the two points a base comes into
existence — `create_base` (`:447`) and `import_brkb` (`:506`) — and **forget** it at `delete_base`
(`:657`). Registration belongs in the service rather than in the MCP server because both surfaces
reach it: `kb_create_base` (`server.rs:357`) and `POST /knowledge/bases` (`routes/knowledge.rs:354`)
call the same function, and a base that exists with no entry reads *private* by decision (3), which
would lock the user out of a base they just made from the Knowledge view.

All three take `caller_is_private: bool` as a **required** parameter, so every call site is a compile
error rather than an omission. The two HTTP callers pass `false` — the Knowledge view is the user
typing, with no model attached — and the two MCP callers pass the meta-derived value from (f). That
asymmetry is the same one Task 10C draws for reads, and for the same reason.

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
`biorouter-capability-tier` = `"private"` / `"public"` into the **same** `Meta` object
`inject_into_extensions` already builds (`:161-172`) — never `params.meta`, for the wire-collision
reason the existing comment at `:164-166` gives.

(e) In `dispatch_tool_call`, at the sole `McpMeta::new(&session_id)` (`:1557`):

```rust
            let mut meta = McpMeta::new(&session_id);
            if let Some(token) = progress_token {
                meta = meta.with_progress_token(token);
            }
            // Issue #56. Built-ins only — see (d). `caller_is_builtin` is
            // computed OUTSIDE this future, because `capability_tier()` awaits
            // the provider mutex and this block owns no `&self`.
            if let Some(is_private) = caller_capability_for_builtin {
                meta = meta.with_capability_private(is_private);
            }
```

with `let caller_capability_for_builtin = if biorouter_mcp::BUILTIN_EXTENSIONS.contains_key(client_name.as_str())
{ Some(self.capability_tier().await.is_private()) } else { None };` resolved **before** the
`async move` block, beside the tier lookup Task 14 adds at the same seam.

(f) `KnowledgeServer` reads it, mirroring `session_id_from_context` (`:222-228`):

```rust
const CAPABILITY_TIER_META_KEY: &str = "biorouter-capability-tier";

/// The caller's capability, PUBLIC unless the meta says private.
///
/// Absent means one of: an older daemon, a non-built-in transport, or a direct
/// unit-test construction. All three are "unknown", and unknown must be the
/// restrictive answer for the reads Task 10C gates.
fn caller_is_private(context: Option<&RequestContext<RoleServer>>) -> bool {
    context
        .and_then(|c| c.meta.0.get(CAPABILITY_TIER_META_KEY))
        .and_then(|v| v.as_str())
        == Some("private")
}
```

- [ ] **Step 4: Run**

```bash
cargo test -p biorouter-mcp --lib knowledge::tier
cargo test -p biorouter-mcp --lib knowledge::          # ~122 today; assert pre + 5
cargo test -p biorouter --lib agents::mcp_client
cargo test -p biorouter-server --test knowledge_routes # ~19 today; must be unchanged
```

Expected: **PASS**. `knowledge::` is the count that matters — this task adds a module to it, and the
per-module filter `knowledge::tier` proves the new tests are in the module the filter names rather
than somewhere that happens to compile.

- [ ] **Step 5: Gate**

```bash
cd /Users/wgu/Desktop/BioRouter-privacy
# The tier is not in the manifest, so it cannot ride a .brkb archive.
grep -c "tier\|privacy" crates/biorouter-mcp/src/knowledge/types.rs ; echo "expect: 0"
grep -rn "kb_tiers_path" crates/biorouter-mcp/src/knowledge/ | grep -v "fn kb_tiers_path"
echo "expect: only tier.rs — nothing else may open the file directly"
# Migration is one-shot, not a per-startup repair. (Task 38 makes the identical
# distinction for sessions, and for the identical reason.)
awk '/pub fn ensure_migrated/,/^}/' crates/biorouter-mcp/src/knowledge/tier.rs \
  | grep -c "kb_tiers_path(root).exists()" ; echo "expect: 1 — the absence guard"
# The capability key is built-ins-only.
awk '/let caller_capability_for_builtin/,/;$/' crates/biorouter/src/agents/extension_manager.rs \
  | grep -c "BUILTIN_EXTENSIONS" ; echo "expect: 1"
# Registration is in the SERVICE, so both surfaces get it from one place.
grep -c "tier::raise(" crates/biorouter-mcp/src/knowledge/service.rs ; echo "expect: 2 (create_base, import_brkb)"
grep -c "tier::forget(" crates/biorouter-mcp/src/knowledge/service.rs ; echo "expect: 1 (delete_base)"
grep -c "tier::raise(" crates/biorouter-mcp/src/knowledge/server.rs ; echo "expect: 0 until Task 10B"
# And nothing ENFORCES anything yet: this task registers and migrates, nothing more.
grep -rn "tier::is_private(" crates/ ; echo "expect: no output until Task 10C"
```

**What this catches.** Three wrong implementations. (1) Putting the tier on `Manifest` — the obvious
place, one field, no new file — which makes it travel inside `.brkb` and hands an importer authority
over the badge; the `types.rs` zero-count is the only cheap gate for it. (2) A migration that runs on
every startup "to pick up new bases", which silently lowers a base the day after a private session
raised it; test 1's second `ensure_migrated` is what fails it, and no grep would. (3) A store shaped
like `.hidden-kbs` — a list of private ids — which cannot distinguish *known public* from *unknown*,
so a directory dropped into the knowledge root reads public; test 2 fails it.

- [ ] **Step 6: Commit**

```bash
git add crates/biorouter-mcp/src/knowledge/tier.rs crates/biorouter-mcp/src/knowledge/mod.rs \
        crates/biorouter-mcp/src/knowledge/paths.rs crates/biorouter-mcp/src/knowledge/service.rs \
        crates/biorouter-mcp/src/knowledge/server.rs crates/biorouter/src/agents/mcp_client.rs \
        crates/biorouter/src/agents/extension_manager.rs
git commit -m "feat(knowledge): a per-knowledge-base privacy tier, its store and its migration (#56)"
```

---

### Task 10B: The knowledge-base ratchet — every write a model makes stamps the caller

The ratchet half of the ruling: *a KB takes the tier of the most sensitive session that has ingested
into it.* Nothing refuses anything yet; that is Task 10C.

**Files:**

| Action | Path | Anchor (re-verified at `9558c346`) |
|---|---|---|
| Modify | `crates/biorouter-mcp/src/knowledge/server.rs` | the **three** content-bearing writes the service does not already cover — `kb_write_page` `:409-448`, `kb_add_raw_source` `:454-470`, `kb_append_log` `:650-661` — plus passing the caller's tier into `kb_create_base` `:356-367` and `kb_import` `:764-773`, whose registration Task 10A put in the service. ⚠ **None of these five takes a `RequestContext` today**, unlike the read tools, so each gains `context: RequestContext<RoleServer>` — an rmcp `#[tool]` signature change, and the compile error that forces the plumbing |
| Modify | `crates/biorouter-server/src/routes/knowledge.rs` | the **three** macro routes that write with a caller-supplied model — `ingest` `:1122`, `ingest_conversation` `:1187`, `lint` `:1325` — via `build_completer` `:899-914`. (`query_kb` `:1269` reads; it gets Task 10C's barrier, not a raise) |
| Reference | `crates/biorouter/src/knowledge/conversation_ingest.rs` | `ingest_conversation` `:184-187` (`ConversationIngestArgs` `:172-180`) — Task 11 adds its `caller_capability`; the KB raise rides the same value |
| Reference | `crates/biorouter-server/src/routes/knowledge.rs` | the plain write routes that get **no** raise — `write_page` `:561-581` (which calls `store::write_page` directly, not a service method), `add_raw_source` `:1415`, `create_base` `:354`, `import_brkb` `:1552`, `restore_state` `:882` |

⚠ **Two exclusion lists, and neither is an oversight.**

*Not ratcheted, because no content enters:* `kb_restore_state`, `kb_begin_txn`, `kb_commit_txn`,
`kb_abort_txn`. They move or discard content that is already in the base. Ratcheting on
`kb_abort_txn` — a *discard* — would let a session privatise a base by opening and abandoning a
transaction, a denial-of-service on the user's own knowledge base with no disclosure to justify it.
Task 10C's barrier still covers all four, and that is the control that matters for them.

*Not ratcheted, because no model is involved:* the plain `/knowledge/*` write routes. Those are the
user typing in the Knowledge view — the same scope line Task 10C draws for reads. There is no
service-level write choke point to hang a raise on anyway (`routes/knowledge.rs:571` calls
`store::write_page` directly), so putting one there would mean inventing one, and it would classify a
base by *the user's own editing* rather than by what a model saw. If a base needs privatising because
of what the user pasted into it, that is a user action and it wants a UI control, not a silent
ratchet — [Open question 15](#open-questions).

- [ ] **Step 1: Write the failing tests**

```rust
#[tokio::test]
async fn a_private_session_writing_one_page_ratchets_the_whole_base() {
    // THE test for the ruling, and the one that makes AR-1's cost visible in
    // CI: one page from one private chat privatises the machine-wide base.
    let root = migrated_root_with_public_base("default");
    let srv = knowledge_server_at(&root);

    srv.kb_write_page(params(json!({ "kb_id": "default", "path": "knowledge/omop.md",
                                     "content": "n=412 T2D patients", "commit_message": "x" })),
                      ctx_with_capability(Private)).await.unwrap();

    assert!(tier::is_private(&root, "default"));
}

#[tokio::test]
async fn a_public_session_writing_never_lowers_a_ratcheted_base() {
    let root = migrated_root_with_public_base("default");
    let srv = knowledge_server_at(&root);
    tier::raise(&root, "default", true).unwrap();

    // Task 10C has not landed, so this write still SUCCEEDS. What must not
    // happen is the tier moving.
    srv.kb_append_log(params(json!({ "kb_id": "default", "kind": "note", "summary": "hi" })),
                      ctx_with_capability(Public)).await.unwrap();
    assert!(tier::is_private(&root, "default"), "a public write lowered the tier");
}

#[tokio::test]
async fn every_write_a_model_makes_ratchets_and_the_plumbing_ones_do_not() {
    // Parameterised over the five, plus the four that must NOT. A test on
    // kb_write_page alone passes an implementation that misses kb_add_raw_source
    // — which is the tool the GUI ingest panel and the `ingest` macro actually
    // call, so the whole ingest path would launder silently.
    for probe in RATCHETING_WRITES {          // write_page, add_raw_source, append_log
                                              // (create_base and import register in the
                                              // service, Task 10A, and are probed here too)
        let root = migrated_root_with_public_base("default");
        (probe.run)(&root, Private).await.unwrap();
        assert!(tier::is_private(&root, "default"), "{} did not ratchet", probe.name);
    }
    for probe in NON_RATCHETING_WRITES {      // restore_state, begin_txn, commit_txn, abort_txn
        let root = migrated_root_with_public_base("default");
        (probe.run)(&root, Private).await.unwrap();
        assert!(!tier::is_private(&root, "default"), "{} ratcheted; see the ⚠ above", probe.name);
    }
}

#[tokio::test]
async fn a_base_created_from_a_private_chat_is_born_private() {
    let root = migrated_root_with_public_base("default");
    let srv = knowledge_server_at(&root);
    srv.kb_create_base(params(json!({ "id": "omop", "name": "OMOP" })),
                       ctx_with_capability(Private)).await.unwrap();
    assert!(tier::is_private(&root, "omop"));
    assert!(!tier::is_private(&root, "default"), "creating one base moved another");
}

#[tokio::test]
async fn an_http_macro_run_on_a_private_model_ratchets_the_base_it_writes() {
    // The GUI Knowledge view's own ingest. `build_completer` (:899-914) builds an
    // arbitrary provider from a caller-supplied ModelRef; the base it writes into
    // takes that provider's tier, exactly as an MCP write takes the session's.
    let root = migrated_root_with_public_base("default");
    post_ingest(&root, "default", model_ref("versa_azure", "gpt-5.5-2026-04-24")).await;
    assert!(tier::is_private(&root, "default"));
}
```

- [ ] **Step 2: Run** → **COMPILE ERROR** on the five `#[tool]` signatures (`ctx_with_capability`
      has nowhere to go), then **FAIL** on the ratchet assertions.

- [ ] **Step 3: Implement** — one call, five sites, and the raise is the *first* statement after the
      per-KB lock so a panic in the write cannot leave content in a base whose tier never moved:

```rust
        let _lock = self.service.lock_kb(&p.kb_id).await.map_err(into_err)?;
        // Issue #56, design §9.3 B4 as ruled. The base takes the tier of the
        // most sensitive session that has ingested into it. BEFORE the write:
        // a raise that only happens on success leaves content in a base whose
        // tier never moved if the write panics or the process dies mid-commit,
        // and the failure direction of an over-raise is a badge the user can
        // see, while the failure direction of an under-raise is silent.
        crate::knowledge::tier::raise(
            self.service.root(),
            &p.kb_id,
            Self::caller_is_private(Some(&context)),
        )
        .map_err(into_err)?;
```

For the HTTP macro routes, raise from the route handler using the **constructed provider's** tier —
not the requested model name — because `providers::create` can hand back something else
(`factory.rs:142-146`). `build_completer` (`:899-914`) already constructs it; return the tier
alongside the completer rather than re-deriving it.

- [ ] **Step 4: Run**

```bash
cargo test -p biorouter-mcp --lib knowledge::
cargo test -p biorouter-server --test knowledge_routes
cargo test -p biorouter --lib knowledge::conversation_ingest
```

- [ ] **Step 5: Gate**

```bash
# Eight raise sites in total, and the tree-wide count is the sum of three
# per-file counts rather than one repo grep — a repo-wide number would not say
# which surface lost its raise.
grep -c "tier::raise(" crates/biorouter-mcp/src/knowledge/server.rs  ; echo "expect: 3 (write_page, add_raw_source, append_log)"
grep -c "tier::raise(" crates/biorouter-mcp/src/knowledge/service.rs ; echo "expect: 2 (create_base, import_brkb — from Task 10A)"
grep -c "tier::raise(" crates/biorouter-server/src/routes/knowledge.rs ; echo "expect: 3 (ingest, ingest-conversation, lint)"
# The four plumbing tools have none.
for fn in kb_restore_state kb_begin_txn kb_commit_txn kb_abort_txn; do
  echo -n "$fn: "
  awk "/pub async fn $fn/,/^    }/" crates/biorouter-mcp/src/knowledge/server.rs | grep -c "tier::raise"
done
echo "expect: 0 each — see the ⚠ in this task, they are excluded deliberately"
# Nor do the plain HTTP write routes (the user typing in the Knowledge view).
for h in write_page create_base import_brkb restore_state; do
  echo -n "$h: "
  awk "/pub async fn $h/,/^}/" crates/biorouter-server/src/routes/knowledge.rs | grep -c "tier::raise"
done
echo "expect: 0 each"
# The raise precedes the write in every one of the three server sites.
for fn in kb_write_page kb_add_raw_source kb_append_log; do
  echo -n "$fn: "
  awk "/pub async fn $fn/,/^    }/" crates/biorouter-mcp/src/knowledge/server.rs \
    | grep -n "tier::raise\|write_page(\|add_raw_source(\|append(" | head -2
done
# Expected for each: tier::raise on the SMALLER line number.
# The HTTP macro routes ratchet from the CONSTRUCTED provider, not the requested name.
awk '/async fn build_completer/,/^}/' crates/biorouter-server/src/routes/knowledge.rs \
  | grep -c "tier()" ; echo "expect: 1"
grep -c "model.provider" crates/biorouter-server/src/routes/knowledge.rs
echo "expect: 1 — only the providers::create call itself; the tier is never keyed on the name"
```

**What this catches.** Three wrong implementations. (1) Ratcheting only in `kb_write_page`, the tool
whose name says "write" — which misses `kb_add_raw_source`, the one the GUI ingest panel and the
`ingest` macro actually call, so the entire GUI path launders silently. The parameterised test is the
only thing that fails it. (2) Ratcheting on the *success* return, which the `kb_import` path makes
observable: a 400 MB archive that fails halfway has already written pages. (3) Keying the HTTP
ratchet on `body.model.provider` — the string the caller supplied — rather than on the instance
`providers::create` returned, which the `BIOROUTER_LEAD_MODEL` intercept can make different.

- [ ] **Step 6: Commit**

```bash
git add crates/biorouter-mcp/src/knowledge/ crates/biorouter-server/src/routes/knowledge.rs
git commit -m "feat(knowledge): ratchet a knowledge base to the tier of the sessions that ingest into it (#56)"
```

---

### Task 10C: The knowledge-base barrier — the explicit-`kb_id` branch and its fifteen siblings

The read half of the ruling, and the task the verifier's finding is really about: **`kb_search`'s
explicit-`kb_id` branch bypasses the visible-set logic entirely.** `kb_search` at
`knowledge/server.rs:590-592` joins `kb_root(self.service.root(), &kb_id)` directly and searches it;
only the `else` at `:602-604` goes through `search_visible_bases` (`:258-286`). Six more read paths
do the same thing, four of them through `kb_id_or_primary`, whose doc comment (`:308-311`) states the
bypass as a feature: *"An explicit `kb_id` always wins and is never filtered against the session's
set — that is how a hidden base (Soul) stays reachable."* Hiding is a *tidiness* control and that
sentence is correct for it. The privacy tier is not a tidiness control, and the same code path must
now answer both questions differently.

**Files:**

| Action | Path | Anchor (re-verified at `9558c346`) |
|---|---|---|
| Modify | `crates/biorouter-mcp/src/knowledge/server.rs` | **reads (7):** `kb_search` `:579-606` (explicit branch `:590-601`), `kb_search_raw_sources` `:612-644` (explicit branch `:618-634`), `kb_export` `:737-758`, and the four through `kb_id_or_primary` `:312-342` — `kb_list_pages` `:373-384`, `kb_read_page` `:390-400`, `kb_get_graph` `:476-485`, `kb_list_history` `:491-503`. **writes (9):** `kb_create_base` `:357`, `kb_write_page` `:409`, `kb_add_raw_source` `:454`, `kb_restore_state` `:509`, `kb_begin_txn` `:527`, `kb_commit_txn` `:543`, `kb_abort_txn` `:562`, `kb_append_log` `:650`, `kb_import` `:764`. **the list:** `kb_list_bases` `:348-354` → `visible_bases_for_context` `:251-256` |
| Modify | `crates/biorouter-server/src/routes/knowledge.rs` | the four macro routes only — `ingest` `:1122`, `ingest_conversation` `:1187`, `query_kb` `:1269`, `lint` `:1325`; `build_completer` `:899-914` |

⚠ **`kb_set_active` and `kb_get_active` are NOT gated, deliberately.** They move and report a
*pointer*, and the pointer is a bare kb id the session already had to know to pass. Refusing there
would break the "one axis, one pointer" repair logic in `CLAUDE.md` (a hidden primary promotes to the
lexicographically first remaining base) for reasons that have nothing to do with privacy. A public
session may point at a private base and will then be refused on every read.

⚠ **The `/knowledge/*` HTTP routes the GUI uses are NOT gated, and this is the load-bearing scope
decision of the task.** DR-3 says *a public model* must never reach a private session. The Knowledge
view is the **user**, not a model: `GET /knowledge/bases/{id}/page`, `/pages`, `/graph`, `/history`,
`/preview`, `/export` are that user reading their own knowledge base in their own app, and a barrier
there would lock a user out of their own notes with no model involved anywhere. The four macro routes
**are** gated, because those run a model. If you find yourself adding a check to `get_page_body`
(`:817`) or `list_pages` (`:517`), stop: that is a different product decision and it is
[Open question 15](#open-questions).

- [ ] **Step 1: Write the failing tests**

```rust
#[tokio::test]
async fn the_explicit_kb_id_branch_is_not_a_way_around_the_barrier() {
    // The finding, exactly. Before this task the `kb_id`-carrying branch at
    // :590-592 searches any base on the machine, and `search_visible_bases`
    // — the only code that consults the session's set — is in the `else`.
    let root = migrated_root_with_public_base("default");
    tier::raise(&root, "default", true).unwrap();
    seed_page(&root, "default", "knowledge/omop.md", "SENTINEL-COHORT-N-412");
    let srv = knowledge_server_at(&root);

    let out = srv.kb_search(params(json!({ "kb_id": "default", "query": "cohort" })),
                            ctx_with_capability(Public)).await.unwrap();
    let text = text_of(&out);
    assert!(text.contains("private"), "must say why: {text}");
    assert!(!text.contains("SENTINEL-COHORT-N-412"), "leaked a snippet: {text}");
    assert!(!text.contains("knowledge/omop.md"), "leaked a page path: {text}");
}

#[tokio::test]
async fn no_entry_point_that_names_a_base_reaches_a_private_one_under_a_public_model() {
    // Parameterised over all SIXTEEN. Gating kb_search alone leaves six read
    // doors — and `kb_export` is the worst of them, because it writes the entire
    // base to an attacker-named path on disk in one call (:744-752).
    let root = migrated_root_with_public_base("omop");
    tier::raise(&root, "omop", true).unwrap();
    seed_page(&root, "omop", "knowledge/x.md", "SENTINEL-BODY");
    let srv = knowledge_server_at(&root);

    for probe in KB_ENTRY_POINTS {          // 7 reads + 9 writes, one closure each
        let outcome = (probe.run)(&srv, "omop", Public).await;
        assert!(outcome.is_refusal(), "{} was not refused", probe.name);
        assert!(!outcome.text().contains("SENTINEL-BODY"), "{} leaked a body", probe.name);
        assert_eq!(outcome.bytes_written(), 0, "{} wrote anyway", probe.name);
    }
    for probe in KB_ENTRY_POINTS {
        assert!((probe.run)(&srv, "omop", Private).await.is_ok(), "{} refused a private caller", probe.name);
    }
}

#[tokio::test]
async fn a_kb_less_search_still_serves_the_public_bases_it_can_see() {
    // The fan-out shape Task 15 gets wrong in the extension manager: a single
    // up-front refusal turns `search_visible_bases` into all-or-nothing, so one
    // private base in the session's set costs the user every other base.
    let root = migrated_root_with_bases(&["default", "omop"]);
    tier::raise(&root, "omop", true).unwrap();
    seed_page(&root, "default", "knowledge/a.md", "public-hit cohort");
    seed_page(&root, "omop",    "knowledge/b.md", "private-hit cohort");
    let srv = knowledge_server_at(&root);

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
    let root = migrated_root_with_bases(&["default", "omop"]);
    tier::raise(&root, "omop", true).unwrap();
    let srv = knowledge_server_at(&root);
    let ids = base_ids(&srv, Public).await;
    assert_eq!(ids, vec!["default"]);
    assert_eq!(base_ids(&srv, Private).await, vec!["default", "omop"]);
}

#[tokio::test]
async fn a_public_model_macro_cannot_run_against_a_private_base_over_http() {
    let root = migrated_root_with_public_base("omop");
    tier::raise(&root, "omop", true).unwrap();
    let r = post_query(&root, "omop", model_ref("anthropic", "claude-opus-4-8")).await;
    assert_eq!(r.status(), 409);
    assert!(r.text().await.contains("private"));
    // And the GUI's own read routes are untouched: the user is not a model.
    assert_eq!(get_page_body(&root, "omop", "knowledge/x.md").await.status(), 200);
}
```

- [ ] **Step 2: Run** → **FAIL** on all sixteen probes and on the list test; the last test's 409 half
      fails and its 200 half passes.

- [ ] **Step 3: Implement** — one helper, sixteen call sites, and the refusal is a **constant**:

```rust
    /// The KB twin of `ExtensionManager::assert_extension_reachable`. `Err` is
    /// the refusal; `Ok(())` permits. Reads the base's stored tier, never the
    /// session's set — hiding and privacy are different questions and
    /// `kb_id_or_primary` (:312) answers only the first.
    fn assert_kb_reachable(&self, kb_id: &str, caller_private: bool) -> Result<(), ErrorData> {
        if caller_private || !crate::knowledge::tier::is_private(self.service.root(), kb_id) {
            return Ok(());
        }
        Err(ErrorData::new(ErrorCode::INVALID_REQUEST, KB_PRIVATE_REFUSAL.to_string(), None))
    }
```

```rust
/// Names no base, no page and no snippet. Constant, so a model that retries sees
/// the same string and stops rather than looping (the same rule Task 14's
/// `privacy_refusal` follows, and for the same reason).
const KB_PRIVATE_REFUSAL: &str = "\
This knowledge base is private: a session running an institutional or self-hosted model has \
ingested into it, so only a private model may read or write it. This session is running on a \
public model. Ask the user to switch this chat to a private model — Settings > Models, or the \
model chip in the composer — and try again. Do not retry with a different knowledge base id, \
through kb_export, or through a raw-source search; the boundary is the same everywhere.";
```

Placement rules, and each one has a wrong version that compiles:

- The seven reads and nine writes call it **immediately after the id is resolved and before any
  filesystem read of the base**. For the four that route through `kb_id_or_primary`, that is right
  after that call — not inside it, because `kb_id_or_primary` is also how a *write* resolves its
  target and a shared refusal there would report a read error on a write.
- `kb_search`'s and `kb_search_raw_sources`' explicit branches call it before `search_with_scope`;
  their `else` branches do **not** — `search_visible_bases` filters instead (next bullet).
- `search_visible_bases` (`:258-286`) filters **inside** its per-base loop at `:266`, so a private
  base is skipped and the public ones still answer. A guard before the loop is the all-or-nothing bug
  test 3 exists to catch.
- `visible_bases_for_session` (`:240-249`) gains the same filter beside its `hidden.contains` retain
  at `:247`, which is what makes `kb_list_bases` omit rather than redact.
- The four HTTP macro routes check **before** `build_completer` runs, so an unknown model and a
  private base produce the privacy 409 rather than a 400 about the model.

- [ ] **Step 4: Run**

```bash
cargo test -p biorouter-mcp --lib knowledge::
cargo test -p biorouter-server --test knowledge_routes
cargo test -p biorouter-server --test knowledge_routes_e2e
```

- [ ] **Step 5: Gate**

```bash
# All sixteen, plus the definition.
grep -c "assert_kb_reachable(" crates/biorouter-mcp/src/knowledge/server.rs
echo "expect: 17 = 1 definition + 7 reads + 9 writes"
# The two fan-out sites filter INSIDE their loop, not before it.
for fn in search_visible_bases visible_bases_for_session; do
  echo -n "$fn: "
  awk "/fn $fn/,/^    }/" crates/biorouter-mcp/src/knowledge/server.rs \
    | grep -n "for base\|retain\|tier::is_private" | head -3
done
# Expected: the loop/retain line BEFORE (or containing) the tier check.
# The explicit-kb_id branch is closed — this is the finding, as a command.
awk '/pub async fn kb_search\(/,/^    }/' crates/biorouter-mcp/src/knowledge/server.rs \
  | grep -n "assert_kb_reachable\|search_with_scope" | head -2
# Expected: assert_kb_reachable on the SMALLER line number.
# The pointer tools and the GUI read routes are untouched, deliberately.
awk '/pub async fn kb_set_active/,/^    }/' crates/biorouter-mcp/src/knowledge/server.rs \
  | grep -c "assert_kb_reachable" ; echo "expect: 0"
for h in get_page_body list_pages get_graph list_history preview_state export_brkb; do
  echo -n "$h: "
  awk "/pub async fn $h/,/^}/" crates/biorouter-server/src/routes/knowledge.rs | grep -c "tier::is_private"
done
echo "expect: 0 each — the Knowledge view is the user, not a model (see the ⚠ above)"
# The refusal names nothing.
grep -c "kb_id" <(awk '/const KB_PRIVATE_REFUSAL/,/;$/' crates/biorouter-mcp/src/knowledge/server.rs)
echo "expect: 0"
```

**What this catches.** Four wrong implementations. (1) Gating `kb_search` only — literally what the
finding names — which leaves `kb_read_page`, `kb_list_pages`, `kb_get_graph`, `kb_list_history`,
`kb_search_raw_sources` and `kb_export` open, and `kb_export` writes the whole base to disk in one
call. The sixteen-probe test is the only thing that fails it. (2) Putting the check inside
`kb_id_or_primary`, which looks like the one choke point and is not — `kb_search`,
`kb_search_raw_sources`, `kb_export` and all nine writes take `kb_id` directly and never call it.
(3) A single up-front guard in `search_visible_bases`, turning a KB-less search into all-or-nothing
so one private base costs the user every other base. (4) Filtering hits *after*
`search_with_scope` returns rather than skipping the base — which reads the private base's index off
disk, and is the same post-filter mistake Gate D's `LIMIT` test exists to catch one crate over.

- [ ] **Step 6: Commit**

```bash
git add crates/biorouter-mcp/src/knowledge/server.rs crates/biorouter-server/src/routes/knowledge.rs
git commit -m "feat(knowledge): refuse a public model on a private knowledge base at all sixteen entry points (#56)"
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

**Files:**

| Action | Path | Anchor (re-verified at `9558c346`) |
|---|---|---|
| Modify | `crates/biorouter/src/knowledge/conversation_ingest.rs` | `ConversationIngestArgs` `:172-180` (7 fields today, ending `cancel`); `ingest_conversation` `:184-187`; the empty/undigestible early returns `:188-194`; `render_conversations(&args.sessions)` `:191` — the guard goes **before** it, so no transcript is rendered for a session that is about to be refused |
| Modify | `crates/biorouter/src/agents/knowledge_tool.rs` | `handle_ingest_conversation` `:24-86`; `session_ids` parse `:32-41`; the load loop `:48-49` (`get_session(sid, true)`); the `ingest_conversation(` call at `:61` |
| Modify | `crates/biorouter-server/src/routes/knowledge.rs` | `ingest_conversation` `:1187-1258`; the `session_ids` load loop `:1202-1212`; the `ConversationIngestArgs` literal `:1224-1232` |
| Modify | `crates/biorouter-cli/src/commands/knowledge.rs` | `handle_ingest_conversation` `:500`; the `ingest_conversation(` call at `:571` |
| Reference | `crates/biorouter/src/agents/agent.rs` | dispatch `:2660`; advertisement `:3131` (`ingest_conversation_tool()`), with the surrounding comment "The conversation-ingestion tool is always available on the platform extension" |
| Reference | `crates/biorouter/src/agents/platform_tools.rs` | `PLATFORM_INGEST_CONVERSATION_TOOL_NAME` `:5`; `ingest_conversation_tool()` `:51`; the description telling the model to "Pass `session_ids`" at `:63-65` |

⚠ **`Agent` has no `capability_tier()`** — Task 10 put that method on `ExtensionManager`, whose
`provider` field is private. `handle_ingest_conversation` is `impl Agent` (`knowledge_tool.rs:23-24`),
so it resolves its own capability with `self.provider().await.map(|p| p.tier()).unwrap_or(ProviderTier::Public)`
— fail-closed to Public, and `Agent::provider()` is the accessor Task 13 has already hardened.

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
```

- [ ] **Step 2: Run** → **COMPILE ERROR** at all three `ConversationIngestArgs` literals (missing
      field `caller_capability`), then **FAIL** on the refusal and ratchet assertions.

- [ ] **Step 3: Implement**

(a) `ConversationIngestArgs` gains a **required, non-`Option`** field:

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

carrying `refused.len()` into `IngestResult` so each caller can report it, and naming **only** the
count and the reason:

```rust
/// Names no session, no title and no working directory — §11.4 classifies all
/// three as content, and a session title in this product is LLM-generated from
/// the conversation itself.
const REFUSED_ALL_PRIVATE: &str = "\
Those chats are private: they were created under a model hosted inside the institution, so only a \
private model may read them. This session is running on a public model. Ask the user to switch this \
chat to a private model and try again.";
```

(c) The three call sites each pass their own capability: the platform tool from
`self.provider().await.map(|p| p.tier()).unwrap_or(ProviderTier::Public)`; the HTTP route from the
provider `build_completer` constructed (the **instance**, not `body.model.provider` — `factory.rs:142-146`
can hand back something else); the CLI from the session's bound provider.

(d) The HTTP route maps a full refusal to **409**, not 500 — the same typed status Gate A uses
(Task 12), for the same reason: a barrier that surfaces as an internal error teaches the caller to
retry.

- [ ] **Step 4: Run**

```bash
cargo test -p biorouter --lib -- agents::knowledge_tool knowledge::conversation_ingest
cargo test -p biorouter-server --test knowledge_routes
cargo test -p biorouter-cli --lib commands::knowledge
```

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
# All three callers pass the field (a missed one would not compile, but a caller
# that hardcodes Private compiles fine and is the real risk).
grep -rn "caller_capability:" --include='*.rs' crates/ | grep -v conversation_ingest.rs
echo "expect: 3 — and NONE of them may read 'caller_capability: ProviderTier::Private'"
grep -rn "caller_capability: ProviderTier::Private" --include='*.rs' crates/ ; echo "expect: no output"
# The refusal names no session.
grep -c "session.name" crates/biorouter/src/agents/knowledge_tool.rs ; echo "expect: 0"
```

**What this catches.** Four wrong implementations. (1) The check placed before the loop, on
`session_ids[0]` or on the current session — which admits every other element of a caller-supplied
array; the `partition` shape makes that shape unwritable. (2) A refusal that returns an error but has
already called `kb_write_page`; the byte-equality assertion is the only thing that fails it.
(3) Guarding the platform tool and calling it done — leaving `POST /knowledge/bases/{id}/ingest-conversation`
as an unguarded copy; the required field and the cross-file `grep` are what fail it. (4) The
plausible-looking fix of hardcoding `caller_capability: ProviderTier::Private` at the HTTP or CLI
call site to make it compile — which reads as "this caller is trusted" and is exactly wrong for the
route that needs the check most. The last grep is the only gate that sees it.

- [ ] **Step 6: Commit**

```bash
git add crates/biorouter/src/knowledge/conversation_ingest.rs \
        crates/biorouter/src/agents/knowledge_tool.rs \
        crates/biorouter-server/src/routes/knowledge.rs \
        crates/biorouter-cli/src/commands/knowledge.rs
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
async fn a_concurrent_ratchet_cannot_interleave_into_a_bad_state() {
    // The check and the write are one conditional UPDATE, so there is no TOCTOU
    // window. Race a bind against a ratchet 200 times and assert the invariant
    // holds in every outcome: if the row is private, its provider is private.
    for _ in 0..200 {
        let (agent, s) = agent_on(private_provider()).await;
        let a = tokio::spawn({ let a = agent.clone(); let id = s.id.clone();
                               async move { a.update_provider(public_provider(), &id).await } });
        let b = tokio::spawn(ratchet_to_private_owned(s.id.clone()));
        let _ = tokio::join!(a, b);
        let row = reread(&s.id).await;
        if row.privacy_tier.is_private() {
            assert_eq!(row.provider_name.as_deref(), Some("versa_azure"));
        }
    }
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
        // refused model. The invariant this establishes is one sentence — the
        // provider bound to a private session is always private — and it is
        // what lets every later reader trust `sessions.provider_name`.
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

        let mut current_provider = self.provider.lock().await;
        *current_provider = Some(provider);
        Ok(())
    }
```

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

⚠ **`cargo test -p biorouter-server --lib routes::agent` prints `0 passed` and exits 0 today**:
`crates/biorouter-server/src/routes/agent.rs` has **no `#[cfg(test)] mod tests`**. Ten of the
route modules have one, so the filter shape is right and the module is simply empty. This task
creates it. Assert a count, never "no failures" — a zero here after Step 3 means the route tests
landed in a different file.

- [ ] **Step 5: Gate**

```bash
# Persist precedes swap. Both line numbers come from the same function, so a
# reordering shows as an inversion here.
awk '/pub async fn update_provider/,/^    }/' crates/biorouter/src/agents/agent.rs \
  | grep -n "bind_provider_if_allowed\|\*current_provider = Some" 
# Expected: bind_provider_if_allowed on the SMALLER line number.
# The route no longer collapses every error to 500.
grep -c "privacy_barrier" crates/biorouter-server/src/routes/agent.rs ; echo "expect: >= 1"
grep -c '"409"' ui/desktop/openapi.json ; echo "expect: >= 1"
# The client throws.
awk '/const changeModel/,/^  \);/' ui/desktop/src/components/ModelAndProviderContext.tsx \
  | grep -c "throwOnError" ; echo "expect: 2 (updateAgentProvider AND setConfigProvider)"
```

**What this catches.** The wrong implementation adds the check to `Agent::update_provider` as an
`if` **before** the existing body, leaving the swap-then-persist order at `:5663-5666` intact. It
passes any test that only asserts `Err`, and it leaves the live agent running on the refused public
model — the precise inverse of the design's promise. The first assertion of Step 1's first test and
the `awk` ordering gate are what fail it. Separately, shipping Gate A without (c) and (d) reproduces
the shipped bug exactly: a refusal rendered as a green success toast.

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
awk 'NR>=3258 && NR<=3400' crates/biorouter/src/agents/agent.rs \
  | grep -n "ElicitationResponse\|bind_allowed\|restore_goal"
# Expected order: ElicitationResponse ... bind_allowed ... restore_goal.
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
cargo test -p biorouter --lib privacy::tests::floor_is_crossed_only_where_a_capability_establishes_a_classification
# Expected: PASS with agent.rs uncommented in EXPECTED. A failure here naming
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
        if privacy_enforcement_enabled() {
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

`privacy_enforcement_enabled()` reads the opt-out **inside** the gate, not through an
`is_enabled()` wrapper, following the `SensitiveOpsInspector` pattern, so a mid-session change is
honoured and the opt-out is one auditable line rather than an absent gate. Task 30 implements it;
until then it is `const fn … { true }`.

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
grep -rn "\.call_tool(" --include='*.rs' crates/ | grep -v "^crates/biorouter/src/agents/extension_manager.rs:15"
echo "expect: only #[cfg(test)] hits (skills_extension.rs tests from :798, code_execution_extension.rs tests from :2115)"
# The gate reads the RESOLVED RECORD, not the tool-name string. Asserted as two
# anchored patterns, positive and negative, rather than as a count over an awk
# range: the range `/pub async fn dispatch_tool_call/,/SecretGuard/` is 67 lines
# and spans the prefix-strip at :1471-1481, so `grep -c prefixed_name` over it
# returns 3 TODAY, before a line of #56 exists. That gate could never pass and
# never measured what its comment said.
grep -c "\.get(&client_name)" crates/biorouter/src/agents/extension_manager.rs ; echo "expect: >= 1"
grep -cE "privacy_refusal\(&(prefixed_name|tool_name)|classify_extension\(&(prefixed_name|tool_name)" \
  crates/biorouter/src/agents/extension_manager.rs
echo "expect: 0 — the tier is never resolved from the tool-name string"
# Gate C is not an inspector.
grep -rn "PrivacyInspector" crates/ ; echo "expect: no output"
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
            Some(e) if privacy_enforcement_enabled() => Err(e),
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
# The three fan-out sites guard INSIDE their loop, not before it.
for fn in read_resource_tool get_ui_resources list_prompts; do
  echo -n "$fn: "
  awk "/pub async fn $fn/,/^    }/" crates/biorouter/src/agents/extension_manager.rs \
    | grep -n "for \|FuturesUnordered\|assert_extension_reachable" | head -3
done
# Expected for each: the loop/FuturesUnordered line BEFORE assert_extension_reachable.
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
        let enforce = privacy_enforcement_enabled();
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
# One helper, three sites — and no site builds a provider around it.
grep -rn "assert_alt_provider_allowed(" --include='*.rs' crates/ | grep -v "privacy/alt_provider.rs" | wc -l
echo "expect: 3"
for f in crates/biorouter-cli/src/session/mod.rs crates/biorouter/src/hooks/mod.rs \
         crates/biorouter/src/agents/knowledge_tool.rs; do
  echo -n "$f: "; grep -c "assert_alt_provider_allowed" "$f"
done
echo "expect: 1 each"
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
cargo test -p biorouter-mcp --test mcp_integration_test
cargo test -p biorouter-mcp --lib knowledge::
```

Task 13 edits `Agent::reply`'s prologue, which is where a reordering shows up in
`conversation_writeback_freshness`'s three #59 ordering tests. Tasks 10A–10C touch every knowledge
entry point, which is what the last two targets cover.

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
grep -rn "PrivacyInspector\|privacy.*impl ToolInspector" crates/ ; echo "expect: no output"
# O12 — the knowledge-base barrier, all sixteen entry points plus the definition,
# and the ratchet across its three surfaces (3 MCP writes + 2 service
# registrations + 3 HTTP macro routes = 8).
grep -c "assert_kb_reachable(" crates/biorouter-mcp/src/knowledge/server.rs ; echo "expect: 17"
grep -c "tier::raise(" crates/biorouter-mcp/src/knowledge/server.rs ; echo "expect: 3"
grep -c "tier::raise(" crates/biorouter-mcp/src/knowledge/service.rs ; echo "expect: 2"
grep -c "tier::raise(" crates/biorouter-server/src/routes/knowledge.rs ; echo "expect: 3"
# Only tier.rs opens the store, and the tier never reached the manifest.
grep -rn "kb_tiers_path" crates/biorouter-mcp/src/knowledge/ | grep -v "fn kb_tiers_path"
echo "expect: only tier.rs"
grep -c "tier\|privacy" crates/biorouter-mcp/src/knowledge/types.rs ; echo "expect: 0"
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
# Nobody re-derives the matrix.
grep -rn --include='*.rs' "privacy_tier.*==.*Private" crates/ | grep -v "privacy/" | grep -v _test
echo "expect: no output — every consumer calls may_read/may_write/appears_in_list"
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
| Reference | `crates/biorouter-server/src/routes/session.rs` | `POST /sessions/{id}/diverge` at `:1029`. ⚠ **`routes/session.rs` has no `#[cfg(test)] mod tests` today**, so Step 4's `cargo test -p biorouter-server --lib routes::session` prints `0 passed` and exits 0 before this task; it creates the module |
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
cargo test -p biorouter --lib -- every_copy_path_carries_the_tier_and_the_provider 2>&1 | tail -3
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
cargo test -p biorouter --lib privacy::tests::floor_is_crossed_only_where_a_capability_establishes_a_classification
# Expected: PASS with BOTH lines of EXPECTED uncommented. A failure naming
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
existing table at `:6447` rather than writing a new one.

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
# The inverted classifier is gone, not patched.
grep -c "LOCAL_PROVIDERS\|INSTITUTIONAL_PROVIDERS" crates/biorouter-server/src/routes/apps.rs ; echo "expect: 0"
grep -c "fn provider_class" crates/biorouter-server/src/routes/apps.rs ; echo "expect: 0"
# The table exercises the names that made the inversion green.
awk '/fn provider_class_table|fn provider_tier_table/,/^        }/' crates/biorouter-server/src/routes/apps.rs \
  | grep -c "versa_azure\|versa_bedrock\|aws_bedrock" ; echo "expect: >= 3"
# The restore no longer discards its error.
grep -c "let _ = agent.update_provider" crates/biorouter-server/src/routes/apps.rs ; echo "expect: 0"
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
  a_refused_spawn_leaves_no_orphan_row
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

and — the higher-value change, which would have caught the design's spec on its own — add
`--background-medium` to `TEXT_GROUNDS`, the ground `biorouter-list-row`, `SessionItem` and
`ExtensionItem` all paint on hover.

⚠ **`RING_GROUNDS` must lose its own copy in the same edit.** It is
`const RING_GROUNDS = [...TEXT_GROUNDS, '--background-medium'];` (`:83`), so moving
`--background-medium` into `TEXT_GROUNDS` without changing that line lists it **twice** and silently
adds six duplicate ring assertions. Change it to `const RING_GROUNDS = [...TEXT_GROUNDS];`.

**The arithmetic, so the expected total is derived rather than guessed:** 252 today, `+18` from the
new text ground (3 text tokens × 6 family×mode scopes), `+24` from the four badge assertions
(× 6 scopes), `+0` from rings once the duplicate is removed → **294**.

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

Expected: `OK — all 294 contrast assertions pass` (252 + 18 + 24 + 0, per the arithmetic above),
`OK — generated artifacts are current (3 themes)`, and **1 file / 2 tests**. If the printed total is
**300**, `RING_GROUNDS` still carries its duplicate; if it is **270**, the four badge assertions
landed outside the per-scope loop and ran once instead of six times.

- [ ] **Step 5: Gate**

```bash
cd ui/desktop
# The assertion count is the tell that the new checks actually ran.
node scripts/check-contrast.mjs | tail -1 ; echo "expect: OK — all 294 contrast assertions pass"
grep -c "RING_GROUNDS = \[...TEXT_GROUNDS, '--background-medium'\]" scripts/check-contrast.mjs
echo "expect: 0 — the duplicate ground was removed, not left to inflate the count"
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
pass the current 252 assertions. The `--background-medium` addition is what turns the whole class of
gap into a CI failure for every future chip, not just this one.

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
# Settings did not grow a fake focused-session concept.
grep -c "sessionId" src/components/settings/extensions/ExtensionsSection.tsx ; echo "expect: 0"
grep -c "sessionId" src/components/bottom_menu/BottomMenuExtensionSelection.tsx ; echo "expect: >= 2"
# The chip has no pill.
grep -c "PrivacyBadge" src/components/settings/models/bottom_bar/ModelsBottomBar.tsx ; echo "expect: 1"
grep -c "dense" src/components/settings/models/bottom_bar/ModelsBottomBar.tsx ; echo "expect: >= 1"
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

### Task 30: Settings → Privacy, the opt-out, and its three hardening measures

`SettingsView.tsx` has exactly **three** tabs today —
`grep -c 'data-testid="settings-.*-tab"'` returns 3 (`settings-models-tab` `:93`,
`settings-chat-tab` `:100`, `settings-app-tab` `:109`).

**Files:**

| Action | Path | Anchor (re-verified at `9558c346`) |
|---|---|---|
| Create | `ui/desktop/src/components/settings/privacy/PrivacyPanel.tsx` | new |
| Modify | `ui/desktop/src/components/settings/SettingsView.tsx` | the three `TabsTrigger`s at `:93`/`:100`/`:109` and their `TabsContent`s |
| Modify | `crates/biorouter/src/config/base.rs` | `get_param` `:755-773` — resolves task-local override → **env var** → `config.yaml`; the opt-out must bypass the env branch |
| Modify | `crates/biorouter-server/src/routes/config_management.rs` | `/config/upsert` — `#[utoipa::path]` at `:176`, route registration at `:895`. ⚠ The file is `config_management.rs`; there is no `routes/config.rs` |
| Reference | `crates/biorouter/src/security/security_inspector.rs` | `:70-95` — the always-on floor no config key can lower, the pattern the opt-out's scope copies |

**R7 is scoped to Gate C only** (design §10.6), and open question 3 records that the operator may
have meant more. The reasoning to give the user: turning off the tool gate decides *what a model may
call*; turning off the session barrier would retroactively expose data already gathered under a
private badge, and that decision has its own deliberate flow (R9).

⚠ **The knowledge-base barrier (Task 10C) is NOT under the opt-out**, and Step 5's gate asserts it.
A KB carries session *contents* — the same category as Gate D's chat history, not the same as Gate
C's tool permissions — and AR-1 records that it has no declassification path, so an opt-out there
would be a one-way door with no way back. If the operator later widens R7 (open question 3), widening
it to the KB needs its own ruling and a KB declassification path first.

- [ ] **Step 1: Write the failing tests**

```rust
#[tokio::test]
async fn no_environment_variable_can_turn_protection_off() {
    // The failure mode is an agent disabling its own protection, and
    // Config::get_param's env branch (config/base.rs:755-773) is the easiest
    // lever in the tree. Read straight from the loaded values map instead.
    std::env::set_var("BIOROUTER_PRIVACY_MCP_ENFORCEMENT", "off");
    assert!(privacy_enforcement_enabled(), "an env var disabled the gate");
}

#[tokio::test]
async fn the_key_cannot_be_flipped_through_config_upsert() {
    let r = post_config_upsert("BIOROUTER_PRIVACY_MCP_ENFORCEMENT", "off").await;
    assert_eq!(r.status(), 403);
    assert!(r.text().await.contains("Settings"));
}

#[tokio::test]
async fn turning_it_off_changes_gate_c_and_nothing_else() {
    set_enforcement(false).await;
    assert!(call_private_tool_via_agent_loop().await.is_ok());     // Gate C off
    // The session barrier is untouched: R3/R4/R6/R13 carry no "by default".
    assert!(agent.update_provider(public_provider(), &private_session.id).await.is_err()); // Gate A
    assert_eq!(search_as(ProviderTier::Public, &db, "cohort").await.results.len(), 1);     // Gate D
}
```

```tsx
it('the Privacy tab exists and its toggle is mounted', async () => {
  render(<SettingsView />);
  await user.click(screen.getByTestId('settings-privacy-tab'));
  expect(screen.getByRole('switch', { name: /Private extension protection/ })).toBeInTheDocument();
});
```

- [ ] **Step 2: Run** → **FAIL** on all four.

- [ ] **Step 3: Implement** — the three hardening measures of §10.6: (1) read the value bypassing
`Config::get_param`'s env branch, straight from the loaded values map; (2) gate the key in
`POST /config/upsert` so the flip must come from Settings → Privacy with its confirmation; (3) hold
the authoritative value in daemon memory from startup. **Not SecretGuard**, which cannot enforce
this: `find_denied_path` scans tool-argument strings and `candidate_is_denied` requires a literal
path token that exists on disk, so `cd ~/.config/biorouter && python3 -c "open('config.yaml','a')…"`
evades it, as does any variable indirection (§9.3 C1; the module's own doc-comment concedes it is
"conservative by design").

The check goes **inside** the gate rather than in an `is_enabled()` wrapper, following the
`SensitiveOpsInspector` pattern, so a mid-session change is honoured and the opt-out is one auditable
line rather than an absent gate.

- [ ] **Step 4: Run**

```bash
cargo test -p biorouter --lib privacy
cargo test -p biorouter-server --lib routes::config_management
cd ui/desktop && npx vitest run SettingsView PrivacyPanel 2>&1 | tail -5
```

- [ ] **Step 5: Gate**

```bash
cd ui/desktop
grep -c 'data-testid="settings-.*-tab"' src/components/settings/SettingsView.tsx ; echo "expect: 4 (3 today)"
cd .. 
# One auditable line, read inside the gate. THREE production sites — and the
# `-v` filter must exclude this task's own test, which calls the function inside
# an assert!() and is not a gate site. Enumerate the files rather than counting
# a repo-wide grep: the first version of this gate said "exactly 3" against a
# pattern that also matched Test 1's `assert!(privacy_enforcement_enabled(), ..)`,
# so a correct implementation returned 4 and read red.
grep -rn "privacy_enforcement_enabled()" --include='*.rs' crates/ \
  | grep -v "fn privacy_enforcement_enabled" \
  | grep -v "assert!(privacy_enforcement_enabled"
# expect: exactly 3 lines, all in crates/biorouter/src/agents/extension_manager.rs —
#   dispatch_tool_call (Gate C), assert_extension_reachable (Gate C's siblings),
#   allowed_extension_keys (Gate E).
grep -rln "privacy_enforcement_enabled()" --include='*.rs' crates/ | sort
# expect: privacy/mod.rs (the definition + this task's test) and
#         agents/extension_manager.rs (the three gate sites). Nothing else.
# DR-9 scoping, as a command: the opt-out is Gate C's, so it must NOT appear in
# any other gate's file — including the knowledge-base barrier, which is a
# session-content control like Gate D and is deliberately not opt-outable.
for f in crates/biorouter/src/agents/agent.rs crates/biorouter/src/session/chat_history_search.rs \
         crates/biorouter-mcp/src/knowledge/server.rs crates/biorouter/src/agents/subagent_tool.rs; do
  echo -n "$f: "; grep -c "privacy_enforcement_enabled" "$f"
done
echo "expect: 0 each"
# The opt-out never reaches Config::get_param.
awk '/fn privacy_enforcement_enabled/,/^}/' crates/biorouter/src/privacy/mod.rs | grep -c "get_param"
echo "expect: 0"
```

**What this catches.** A panel that is written and never mounted — exactly the state
`components/settings/security/SecurityToggle.tsx:14` is in today (declared, plausible, zero
consumers repo-wide). A gate that only checks the component exists reproduces it; the tab-count
plus the mount test is what catches it. And an opt-out read through `Config::get_param`, which makes
`BIOROUTER_PRIVACY_MCP_ENFORCEMENT=off` in an agent shell sufficient to disable the gate the agent is
subject to.

- [ ] **Step 6: Commit**

```bash
git add ui/desktop/src/components/settings crates/biorouter/src/privacy/mod.rs \
        crates/biorouter/src/config/base.rs crates/biorouter-server/src/routes/config_management.rs
git commit -m "feat(privacy): Settings > Privacy opt-out, scoped to Gate C and not env-readable (#56)"
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
# The escape hatch is not type-filtered.
awk '/fn declassify_command/,/^}/' crates/biorouter-cli/src/commands/session.rs | grep -c "SessionType"
echo "expect: 0 — it works by id"
# And History did not gain a system-sessions filter.
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
node scripts/check-contrast.mjs | tail -1     # expect: OK — all 294 contrast assertions pass
npm run themes -- --check                     # expect: OK — generated artifacts are current (3 themes)
```

⚠ **294, not 274.** Task 26 derives the total twice and both derivations agree: 252 measured on
`main` today, `+18` from adding `--background-medium` to `TEXT_GROUNDS` (3 text tokens × 6
family×mode scopes), `+24` from the four new badge assertions (× 6 scopes), `+0` from rings once
`RING_GROUNDS` loses its now-duplicate copy of `--background-medium`. The first version of this plan
said **274** here — a phase gate quoting a number its own Task 26 contradicts, which a worker meeting
it reads as "the phase failed". If the printed total is **300**, `RING_GROUNDS` still carries the
duplicate; if it is **270**, the four badge assertions landed outside the per-scope loop.

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
| Modify | `landing/scripts/build-registry.mjs` | the `data-license` idiom to copy at `:102`; `slugFromUrl` `:33-36`; emitted object `:107-118` (skills at `:130`/`:135`); registry literal `:155-160` (`version: 1` at `:156`) |
| Modify | `landing/registry.json` | version 1 → 2; 37 extensions, 129 skills |
| Modify | `crates/biorouter/src/privacy/registry_private.rs` | now a generator output |
| Modify | `ui/desktop/src/components/baam/registry.fallback.json` | verified in sync at 37/129, by luck — joins the generator's outputs |
| Modify | `ui/desktop/src/components/baam/registry.ts` | `RegistryExtension` `:8-19` |

- [ ] **Step 1: Write the failing tests — three fixture runs, each expecting a non-zero exit**

```bash
# landing/scripts/build-registry.test.mjs (or a shell fixture harness)
node build-registry.mjs --input fixtures/invalid-privacy.html      # data-privacy="maybe"
echo "expect: exit != 0"
node build-registry.mjs --input fixtures/private-no-name.html      # private, no data-extension-name
echo "expect: exit != 0"
node build-registry.mjs --input fixtures/clinical-unannotated.html # description says "patient records", no data-privacy at all
echo "expect: exit != 0"
node build-registry.mjs --input fixtures/happy.html                # exit 0, 37 extensions
echo "expect: exit 0"
```

- [ ] **Step 2: Run** → all four exit **0** today. The first three are the failures.

- [ ] **Step 3: Implement** — beside the `data-license` idiom at `:102`:

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
  if (!/data-privacy=/.test(card) && CLINICAL_KEYWORDS.some(k => description.toLowerCase().includes(k))) {
    fail(`${id}: description matches "${k}" but the card declares no data-privacy`);
  }
```

with `CLINICAL_KEYWORDS = ['patient', 'clinical record', 'ehr', 'phi', 'medical record', 'de-identified clinical']`,
both keys in the emitted object at `:107-118`, `version: 2` at `:156`, and two further outputs:
`crates/biorouter/src/privacy/registry_private.rs` and
`ui/desktop/src/components/baam/registry.fallback.json`.

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
# The three hard failures exist and fire.
for f in invalid-privacy private-no-name clinical-unannotated; do
  node landing/scripts/build-registry.mjs --input landing/scripts/fixtures/$f.html >/dev/null 2>&1
  echo "$f exit=$?"   # expect: non-zero for all three
done
# The generated const and the registry agree.
python3 -c "
import json,re
r=json.load(open('landing/registry.json'))
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
grep -c "check-privacy-registry\|check-consistency" Justfile ; echo "expect: >= 2 (the recipe + its call from check-everything)"
grep -c "check-consistency\|check-privacy" .github/workflows/deploy-landing.yml ; echo "expect: >= 1"
# The desync test from Step 1, run as a gate.
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
grep -c "Privacy" landing/docs.html ; echo "expect: >= 1"
grep -c "check-docs-privacy" landing/scripts/check-consistency.mjs ; echo "expect: 1"
# The desync test from Step 1, run as a gate.
```

**What this catches.** The table drifting the day badges ship on BAAM — the failure that has no
detector today because nothing generates it and nothing reads it.

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
| Modify | `ui/desktop/src/components/baam/BrowseExtensionsModal.tsx` | `live` consumption `:23`/`:32`/`:35`/`:100` |
| Modify | `ui/desktop/src/components/BrxtInstallModal.tsx` | the config write `:152-161` — records **no provenance whatsoever** |

- [ ] **Step 1: Write the failing tests**

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

- [ ] **Step 4: Run** → `cd ui/desktop && npx vitest run registry BrowseExtensionsModal BrxtInstallModal`.

- [ ] **Step 5: Gate**

```bash
cd ui/desktop
grep -c "AbortController" src/main.ts ; echo "expect: >= 1"
# The union rule is a function, not four inline ORs.
grep -rn "effectivePrivacy" src/components/baam/ | wc -l ; echo "expect: >= 2 (definition + consumers)"
# A live fetch can never lower a compiled-in badge.
npx vitest run registry -t "downgrade is never honoured" 2>&1 | tail -3
```

**What this catches.** The natural implementation — trusting the live document — which lets a
compromised or merely stale `registry.json` **remove** a private badge from `ucsfomopagent` on every
machine that fetches it. The union rule is one line and the first test is the only thing that
enforces it.

- [ ] **Step 6: Commit**

```bash
git add ui/desktop/src/main.ts ui/desktop/src/components/baam ui/desktop/src/components/BrxtInstallModal.tsx
git commit -m "feat(marketplace): last-good registry with a timeout, and a union rule that only raises (#56)"
```

---

# Phase 6 — migration, docs, and the release gate

### Task 38: The backfill, and the day-one notice with computed counts

⚠ **The backfill runs ONCE, from the numbered migration arm — never from `reconcile_privacy_schema`.**
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

**What this catches.** Putting the backfill in `reconcile_privacy_schema` — which is the obvious way
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
# Every new doc is indexed. docs/ is the ONLY documentation folder.
grep -c "privacy-tiers" docs/security/README.md ; echo "expect: 3"
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
cargo test -p biorouter-mcp --test mcp_integration_test
cargo test -p biorouter-mcp --lib knowledge::
cargo test -p biorouter-server --lib routes::apps
node scripts/agent-drafter/ui-control-harness.mjs
```

⚠ Four of the `--lib` filters used across this plan resolve to modules that had **no tests before
the task that owns them** — `agents::chatrecall_extension`, `session::chat_history_search`,
`routes::agent`, `routes::session`. At the release gate they must all report a non-zero count. A
`0 passed` here is not "nothing to run"; it is a suite that did not land where the filter looks. See
[Which test filters are validated, and which are not](#which-test-filters-are-validated-and-which-are-not).

- [ ] **Step 3: The twelve invariants, as commands**

```bash
cd /Users/wgu/Desktop/BioRouter-privacy
# O5 — the ratchet fires in exactly two places, neither of them the bind.
grep -rn "raise_privacy(" --include='*.rs' crates/ | grep -v session_manager.rs | wc -l ; echo "expect: 2"
# O7 — one production path into an MCP client (see Task 20 Step 3 for the full
# hit list and why a `grep -vc "cfg(test)"` cannot express this).
grep -c "\.call_tool(" crates/biorouter/src/agents/extension_manager.rs ; echo "expect: 1"
grep -rn "\.call_tool(" --include='*.rs' crates/ | wc -l ; echo "expect: 10 — any increase is a new bypass, read the diff"
# O6 — nothing above filter_tools consults a tier.
awk '/async fn get_all_tools_cached/,/^    }/' crates/biorouter/src/agents/extension_manager.rs \
  | grep -c "capability_tier\|allowed_extension_keys" ; echo "expect: 0"
# The ratchet is irreversible except through one statement.
grep -c "privacy_tier = CASE WHEN" crates/biorouter/src/session/session_manager.rs ; echo "expect: 1"
grep -rn --include='*.rs' "privacy_tier *= *'public'" crates/ | grep -v "DEFAULT 'public'" | wc -l ; echo "expect: 1"
# Gate D is in both builders; Gate C has all nine entry points.
grep -c "s.privacy_tier = 'public'" crates/biorouter/src/session/chat_history_search.rs ; echo "expect: 2"
grep -c "assert_extension_reachable(" crates/biorouter/src/agents/extension_manager.rs ; echo "expect: 9"
# O12 — the knowledge-base barrier and its ratchet.
grep -c "assert_kb_reachable(" crates/biorouter-mcp/src/knowledge/server.rs ; echo "expect: 17"
grep -c "tier::raise(" crates/biorouter-mcp/src/knowledge/server.rs ; echo "expect: 3"
grep -c "tier::raise(" crates/biorouter-mcp/src/knowledge/service.rs ; echo "expect: 2"
grep -c "tier::raise(" crates/biorouter-server/src/routes/knowledge.rs ; echo "expect: 3"
grep -rn "kb_tiers_path" crates/biorouter-mcp/src/knowledge/ | grep -v "fn kb_tiers_path"
echo "expect: only tier.rs — one reader and one writer of the store"
# Gate G is one guard in the shared function, covering all three of its callers.
grep -rn "caller_capability:" --include='*.rs' crates/ | grep -v conversation_ingest.rs | wc -l
echo "expect: 3 — the platform tool, the HTTP route, the CLI"
grep -rn "caller_capability: ProviderTier::Private" --include='*.rs' crates/ ; echo "expect: no output"
# floor() is crossed at exactly its two intended callers, and the audit test
# names them rather than counting — see Task 7 for why a count could not work.
cargo test -p biorouter --lib privacy::tests::floor_is_crossed_only_where_a_capability_establishes_a_classification
# The registry const and registry.json agree, through the wired check.
just check-privacy-registry
# No privacy control is an inspector, and none returns Err.
grep -rn "PrivacyInspector" crates/ ; echo "expect: no output"
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
barrier of Tasks 10A–10C) and the eight departures, and open follow-up issues for every unresolved
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
| **DR-9** | **A global opt-out exists, off by default**, scoped to Gate C (the MCP tool gate) only. Open question 3 records that this scoping may not be what the operator meant. |
| **DR-10** | **Fail directions differ by kind, deliberately.** Migration backfill → fail **open** (public). Runtime read of a missing/unparseable column → fail **closed** (private, with `error!`). Import with no tier → fail **closed**. Unknown provider → **Public** (fail-*safe*: less privileged). Unlisted extension → **Public** (fail-open, DR-6). Any gate's lookup failing → refuse, encoded inside `Ok(..)`, never as `Err`. |
| **DR-11** | **`medcp` stays callable by a public model**, and that is the accepted cost of DR-6. It is enabled on the operator's machine with `CLINICAL_RECORDS_*` against a clinical MSSQL backend. The reasoning: a hand-installed extension is the user's own choice, and medcp is a *connector* rather than a data source. **The badge is a statement about provenance, not about the data behind the connector.** |
| **DR-12** | **`spokeagent` is public.** SPOKE holds no patient data; its passcode gates the service, not private content. |
| **DR-13** | **A knowledge base ratchets on ingest**, resolving the either/or design §9.3 B4 refused to defer. A KB takes the tier of the most sensitive session that has ingested into it, and a public-capability session may not read *or write* a private KB. The alternative — declare KBs a designed public sink and warn at ingest — was **rejected**. Two costs come with it and were accepted, not overlooked: a KB one private session touched is unreadable from every public chat including the user's own ordinary work, and existing KBs migrate **public** even if a private session fed them. Both are written out in [Accepted risks](#accepted-risks) (AR-1, AR-2); there is no KB declassification path in v1. Tasks 10A–10C. |

---

## Open questions

The design's eleven, unchanged in substance and re-stated with what this plan does in the meantime.
**Question 1 is the one place the design reads a requirement in spirit rather than letter and still
needs an operator ruling.** The design's twelfth open item — §9.3 B4's knowledge-base either/or — is
**no longer open**: the operator ruled *ratchet*, and it is implemented in Tasks 10A–10C with its
costs recorded in [Accepted risks](#accepted-risks).

| # | Question | What this plan does while it is open |
|---|---|---|
| **1** | **Does a mixed lead/worker composite ratchet the session?** R3 says "switched to a private model even once → private permanently", and a private-lead/public-worker composite *contains* a private model. The design says it does **not** ratchet, because `tier = least` and the transcript has already gone to the public worker, and because ratcheting on `max` would make the bind gate refuse that same composite on the next resume — bricking a working configuration. Using one reduction for both the gate and the ratchet is what makes `capability ≥ classification` provable by induction (Task 7). **This is the single place the letter of a requirement was not followed, and it needs a ruling.** | Implements the design: `LeadWorkerProvider::tier() = least(lead, worker)`, and `floor(Public) = Public` so no ratchet fires. Task 5's composite test and Task 7's induction test both encode this; **a ruling the other way changes both tests and the `tier()` override, and nothing else.** |
| **2** | **Is the spawn-downgrade an approval or a refusal?** R4 permits it, so the design makes it an approval showing the task prompt. But the prompt is written by a private-context model and is the only leak vector, and it is the one control a planted `PermissionRequest` hook could bypass — hooks load from `~/.config/biorouter/config.yaml` and, with `allow_project_hooks`, from `.biorouter/hooks.yaml`, both writable by an agent with `text_editor`. | Task 23 implements the approval, behind `requires_downgrade_confirmation`. Flipping it to a `Deny` is one branch. |
| **3** | **Does the R7 opt-out really stop at Gate C?** The operator wrote "opt out of the **entire** protection layer", but R3/R4/R6/R13 are stated as invariants without a "by default". Scoping it to the MCP gate is a materially different design from scoping it to everything. | Task 30 scopes it to Gate C and asserts (test 3) that Gates A and D are unaffected. Widening it means deleting that assertion, not adding code. |
| **4** | **Is the first cross-tier write approval remembered per (caller, target) or per call?** Per-pair-per-session-lifetime was chosen because a confirmation on every steer of a public worker is miserable and would be clicked through. | Task 21 exposes `requires_first_crossing_approval`; the memoisation policy lives with BR-71's inspector. |
| **5** | **Institutional Ollama versus hosted Ollama SaaS.** R1 says self-hosted *or* institution-hosted is private, and config cannot tell a lab GPU box at `OLLAMA_HOST=gpu.lab.ucsf.edu` from a hosted SaaS. **This plan disagrees with the design on the severity**: the design rates "non-loopback stays Private" a false-private and "the one place this design is permissive". It is a live bypass — `ProviderEngine::Ollama` plus a remote `base_url` in one agent-writable JSON file mints a Private-tier provider pointing anywhere. Certainty needs a `BIOROUTER_PRIVATE_HOSTS` allowlist, a new concept deliberately not added. | Task 5 makes **loopback-only** Private and non-loopback Public, and its third test encodes the bypass. A lab GPU box therefore reads Public until an allowlist exists. **This is a real ergonomic regression for lab users and needs a ruling.** |
| **6** | **Should `versa_azure` get its own config keys?** It shares all three `AZURE_OPENAI_*` keys with the public `azure_openai` provider, whose shipped default endpoint (`azure.rs:204`) is the same UCSF gateway. The demotion rule catches the dangerous direction, but it means a user can *lose* their private tier by configuring an unrelated provider. | Task 5 implements the endpoint-host demotion. Separate keys are a follow-up. |
| **7** | **Should the compiled-in private baseline be a signed registry snapshot?** Signing would let a *downgrade* be trusted offline. Today the union rule means an extension can only ever gain a private badge without a fresh fetch — safe, but a genuine reclassification-to-public needs connectivity. | Task 37 implements the union rule and Task 34 gates the const against `registry.json`. Signing is a follow-up. |
| **8** | **Who is "who" in the declassification record?** The app is single-user, so the local OS username is recorded. On a shared lab machine that is right; in a multi-account setup it is not, and there is no user identity in the product to record instead. | Task 29 records the OS user + machine in `classification_audit.actor`, with `actor_kind = 'user'` — a value no other code path can construct. |
| **9** | **Skills (R12) carry no classification, which leaves three gaps.** (a) A skill authored while a private chat was open can embed pasted private text and is then readable by every session and publishable to the marketplace. (b) A skill can instruct the model to call `ucsfomopagent` — harmless in effect because Gate C refuses at dispatch, but the steering is unblocked and produces confusing refusals. (c) BR-71 Task 15 lets one session add skills to another. | v1 mitigation is a line in the skill-creation UI (Task 28's copy pass). Closing (a) needs skills to carry a classification, which contradicts R12. |
| **10** | **`ActiveWorkItem.title` is cross-session content and predates all of this** — derived from a subagent's task prompt and surfaced process-wide with a session id. The visibility rule is applied to it, but it is exposed only via `GET /active_work` for the GUI (the model-facing `subagent_status` is session-scoped), so it may deserve its own fix rather than riding this one. | Task 21 provides `appears_in_list`; wiring `/active_work` to it is a follow-up. |
| **11** | **`POST /agent/call_tool` remains inspector-free.** This design is correct either way because the barrier is in the extension manager, but the route is a standing hazard for every *future* inspector-based control, including BR-71's. | Task 14 fixes its error mapping so a refusal reaches the caller as text rather than a bare 500, and Task 20's gate exercises it explicitly. The route itself is unchanged. |

Four more this plan surfaced. Twelve and thirteen need a ruling before the phase that touches them;
fourteen and fifteen are follow-ups whose *residual* is already accepted (AR-3, AR-1) and whose
*fix* is not scheduled.

| # | Question | Blocks |
|---|---|---|
| **12** | **Does `ensure_privacy_schema` co-landing with BR-71 need a merge-order decision?** Both branches add `parent_session_id`; both would take migration 17. The shape-guarded arm plus the unconditional reconcile makes either order safe **in the database**, but the two diffs conflict textually in `session_manager.rs`. Resolution guidance: take either side — the columns are identical — and keep the **shape-guarded** form. | Task 6, and BR-71 Task 1. |
| **13** | **Does `medcp`'s continued reachability need a first-run notice, or is the badge enough?** §13.5 specifies a one-time notice naming any **enabled** extension that is Public and declares clinical-looking credentials. On the operator's machine that names exactly one extension, `medcp`, and nothing else changes. | Task 38's notice copy. Hard-code that expectation into its test fixture. |
| **14** | **How does `memory`'s local store get a tier?** AR-3: `compose_instructions` (`memory/mod.rs:277`) inlines local memories in full (`:310-322`) into every session opened in that directory, including one on a public model, and Task 19 ships only a disclosure. The design's §9.3 B3 names the fix — "classify memory entries and filter `retrieve_all` by the session's capability tier at init" — but the on-disk format carries no provenance (`:387-388` writes a `# {tags}` line and bare lines; `:414-418` reads them back keyed by the *tag string*), and `compose_instructions` runs once at `MemoryServer::new` (`:108`) rather than per turn, so a naive capability filter there freezes across a mid-session model swap — the O6 hazard. A real fix needs per-entry provenance **and** a per-turn recompute. | Nothing in this plan. Open it as a follow-up issue at Task 40 Step 6. |
| **15** | **Does a knowledge base need a declassification path, and does the barrier belong on the GUI's own read routes?** Two halves of the same scope question. (a) AR-1: a session can be declassified (Task 29, user-only, graded, audited) and a KB cannot, so a user who ratchets their only base by accident has no in-product exit. (b) Task 10C gates the four `/knowledge/*` **macro** routes (they run a model) and deliberately leaves the GUI's read routes alone (the Knowledge view is the user, not a model) — a defensible line, but it means the *app* shows a private base that the *agent* in the next tab cannot read, and nobody has decided whether that asymmetry should be visible in the UI. | Nothing in this plan; both are follow-ups. (a) is the one a user will hit first. |

---

## Related documentation

- [Privacy tiers](privacy-tiers.md) — the design this plan executes, and the specification each task is reviewed against.
- [Data privacy and patient data](data-privacy-and-phi.md) — the provider guidance this system enforces mechanically.
- [Secret storage](secret-storage.md) — the credential model Task 2's daemon-credential scrub touches.
- [BR-71 execution plan](../agent-loop/designs/br71-execution-plan.md) — the plan this one must land ahead of, and whose Task 1 collides with Task 6's migration.
- [Subagents](../agent-loop/subagents.md) — the inheritance behaviour Task 23 gates.
- [Tool routing](../agent-loop/tool-routing.md) — the chatrecall/workspace split Gate D sits inside.
- [Multi-KB implementation plan](../knowledge-base/multi-kb-implementation-plan.md) — the "one axis, one pointer" visible-set model whose explicit-`kb_id` escape hatch Tasks 10A–10C now qualify.
- [Launching the dev GUI](../desktop-ui/launching-the-dev-gui.md) — required reading before any GUI verification step.
- [Documentation style](../contributing/documentation-style.md) and [documentation organization](../organization.md) — both binding on Task 39.
