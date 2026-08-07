# Six-surface privacy audit — the complete synthesis

> **What this is.** The full cross-surface synthesis of the six-agent privacy-tiers audit run on
> 2026-08-06 against branch `feat/privacy-tiers`, re-run on 2026-08-07 with the two payloads the
> first synthesis never received, and re-checked finding by finding against the tree as it stands
> at `bbcfdb06`.
> **Status:** Historical record — audit completed 2026-08-07. The campaign it feeds (issue #56) was
> still in flight on `feat/privacy-tiers` at the time of writing, and *What has already been fixed*
> states what had landed by then.
> **Audience:** maintainers and agents working on issue #56, and anyone deciding whether the
> privacy boundary is adequate to ship.

Six auditors read six surfaces of the privacy-tiers work and returned six findings sets. The
synthesiser was handed **four**. The desktop-renderer payload was truncated mid-sentence inside its
first gap finding, and two surfaces — the renderer's gap list and the innocent-mistake ranking —
never reached it at all. The resulting report said so in its own first paragraph and is correct as
far as it goes; it is simply missing a third of its input, including the one surface that answers
the operator's actual question. This document is the synthesis with all six payloads, and with
every finding re-checked against the current tree, because sixteen of them had been fixed in the
twenty-one commits that landed between the audit and this re-run — including the one the earlier
synthesis said it would block a release on.

Read *The innocent-mistake ranking* first if you want the operator's question answered. Read *What
has already been fixed* first if you are about to act on the earlier synthesis, because much of it
is now closed.

## Identifiers used here

- `C-nn` — a finding **closed** since the audit ran, verified in the tree.
- `O-nn` — a finding **still open**, verified in the tree.
- `N-nn` — a finding **new**, produced by one of the fixes rather than found by the audit.
- Gate letters (`A`, `B`, `C`, `E`, `F1`, `F2`, `G`, `H`), decision records (`DR-nn`), rules
  (`R4`, `R6`, `R11`) and section numbers (`§7`, `§8.2`, `§10.2`) are the campaign's own scheme and
  are defined in [`docs/security/privacy-tiers.md`](../../security/privacy-tiers.md) and its
  execution plan. **A bare `§n` in this document always names a section of that design, never a
  section of this page** — this page's own sections are referred to by name. Three documents with
  three numbering schemes is how the earlier synthesis's `§7` came to mean two different things.

Every line reference in this document is to the **committed** content of `feat/privacy-tiers` at
commit `bbcfdb06`, read on 2026-08-07. Where the code and the campaign's own prose disagree, the
code is reported.

> ⚠ **The worktree was mid-merge when this was written, and that changed how it had to be read.**
> `/Users/wgu/Desktop/BioRouter-privacy` held an unfinished merge (`MERGE_HEAD` `af2b8f3b`) with
> seven conflicted files, five of which this document cites — including
> `SessionListView.tsx`, `routes/session.rs`, `ModelsBottomBar.tsx` and `ChatGroupsShell.tsx`,
> which carry the evidence for `N-01`, `N-02`, `O-14` and `O-17`. A file with conflict markers in
> it is not a state of the program: half its lines belong to one side of a merge and half to the
> other, and a line number means nothing. Every claim resting on those five files was therefore
> re-verified against `git show bbcfdb06:<path>` rather than the file on disk, and all of them
> hold with the cited line numbers intact. **A later reader re-checking this document against a
> working tree may see different content for those files, and that is the merge, not a
> correction.**

## The innocent-mistake ranking

This is the payload the first synthesis never saw, and it is the one the operator asked for: **how
does a well-meaning researcher send PHI to a commercial model while trying to do their job?**
Ranked by likelihood × harm for a non-malicious user and a cooperative agent — not by cleverness,
and not by how interesting the mechanism is. Deliberate circumvention is out of scope; so is the
filesystem residual the operator has already accepted, except where a path is *automatic* rather
than requiring the public model to choose to read something.

### Rank 1 — PHI arrives through a tool that classifies nothing

The private set is two compile-time strings: `PRIVATE_EXTENSIONS = ["cdwagent", "ucsfomopagent"]`
(`crates/biorouter/src/privacy/registry_private.rs:42`). Everything else is Public, and R11(ii)
rules it so deliberately. The consequence is not a gap in the machinery; it is the machinery's
declared boundary:

- a cohort CSV opened with `developer__text_editor`;
- a lab-built MCP server installed from a `.brxt` or hand-written into `config.yaml` — the tree's
  own test names the live instance, `medcp`, enabled on the operator's machine against a clinical
  MSSQL backend and asserted Public;
- an internal portal fetched by `computercontroller`.

None of these raises the session's classification, so the tier never ratchets, so the next turn
goes to whatever model is bound and every later chat-recall, ingest and workspace read treats the
transcript as public. **This is the largest real-world path by which PHI reaches a commercial
model, it needs no agent misbehaviour and no user error, and it is a ruling rather than a defect.**
It deserves re-confirmation now that the rest of the machinery is complete, because every other
control in the feature is downstream of a classification this path never produces.

There is also no local escape hatch. §10.2's second union term — *"∪ private(last good registry
fetch)"* — is implemented only in TypeScript (`ui/desktop/src/components/baam/privateSet.ts`), and
R11(i) forbids any local route to mark an extension private. A lab with an in-house PHI connector
cannot protect it short of publishing on BAAM and shipping a new app build.

### Rank 2 — the Knowledge ingest panel decides where the documents go, before any tier exists

`grep -n 'tier' ui/desktop/src/components/knowledge/IngestPanel/*.tsx` still returns nothing. The
flow is the feature's own happy path: *New knowledge base → my cohort → drag in the clinical notes
PDFs → Digest.* `resolveIngestModel` preselects the app-configured model, which for most users is a
commercial one, and the digestion sub-agent sends every raw document to it page by page. A
brand-new base is Public, so it stays readable by every later public chat.

The barrier itself is real — once a base is private, `assert_reachable` refuses reads, writes and
lint-autofixes across 24 call sites — but the raise happens **on ingest**, which is one ingest too
late. Worse, the only tier surface in the view is the base's own badge, which reads *Public* and
therefore reads as reassuring rather than as a warning. `IngestWarnings.tsx` exists but is about
file validation, not tier.

### Rank 3 — nothing looks at the content on the way in

The system is provider-side only. `GuardrailStage` is declared with five variants and **none is
ever constructed or matched** anywhere in the tree; `PreFlight` is documented as *"before the model
sees the user input (e.g. PII masking of the prompt)"* and is the missing entry point. The one
PII/PHI detector wired to the main loop scans tool **results** on their way into the model, defaults
to annotate-only, and never consults the provider tier — `guard_tool_result(output,
tool_output_guardrail)` at `crates/biorouter/src/agents/agent.rs:2825` takes a mode, not a
`CallCapability`.

Two ordinary paths follow. *(a)* The user pastes a de-identification-pending cohort table into
whatever chat is in front of them; a Public chat carries no badge at all, by design, so the only
signal was a modal seen once at install. *(b)* The user says "read patients.csv and summarise the
outliers" in a public chat; the detector fires on the MRNs, prepends a framing note, and the whole
file goes to the commercial model anyway. The disclosure copy is honest that (b) is not stopped.
What is worth stating is that the detector which *noticed* is right there and is tier-blind.

### Rank 4 — the composer names the wrong model, and now asserts the wrong institution

`ModelsBottomBar` takes its model name and provider from `useModelAndProvider()`, whose
`currentModel`/`currentProvider` are module-level state seeded from the global `BIOROUTER_MODEL` /
`BIOROUTER_PROVIDER` keys. The per-chat escape hatch is dead: `CurrentModelContext` is created at
`ui/desktop/src/components/BaseChat.tsx:106` and **no `.Provider` for it exists anywhere in the
tree**, so `useCurrentModelInfo()` always returns `null`.

Two chats open — A private on Versa, B public on GPT-5. The user switches models in A; `changeModel`
binds session A *and* rewrites the global default. They switch to tab B. B's chip now reads the
Versa model name and carries no "hosted outside your institution" line, because the global provider
is private. B's daemon-side agent is still bound to GPT-5 off its own session row. Everything on
screen says Versa. The user pastes a cohort description into B.

The global-chip behaviour predates the privacy work. What is new is that a privacy disclosure was
conditioned on it — and, since the audit, **so was DR-26's affiliation badge**: the same effect now
reads `getProviders()` for `currentProvider` and renders an institution pill from it. The chip's
dense privacy dot and `privacyLine` *are* per-chat, so one 120-pixel control mixes two correct
per-chat markers with two incorrect per-app ones.

### Rank 5 — a cooperative agent walks around the refusal it was just given

In a private chat with Workspace Control enabled, `workspace_open { new: { prompt } }` creates a
session through `WorkspaceServices::start_session`, which takes no capability, binds the machine
default provider, and stamps no classification — the row is born Public — and then runs the model's
own text on it as a detached turn (`crates/biorouter/src/agents/workspace_extension.rs:1188-1252`).
The extension dimension of that call **is** now gated (see C-04); the model dimension is not.

The identical intent expressed as `subagent { settings: { provider: "openai" } }` is refused
outright by DR-19. Same extension, same capability, one door bolted and one open — and
`spawn_downgrade`'s refusal text still advises *"start a new chat on it and give it the task
directly,"* which is a description of the open door. A model that reads its refusals carefully is
**more** likely to find the bypass than one that gives up, which inverts the intent of teaching
refusals.

The sibling path, `workspace_send_prompt` into a pre-existing public chat, is permitted for the same
reason: `refuse_unless_visible` short-circuits `Ok` for any private caller before it reads the
target row, because `may_read(Private, Public)` is true. Likelihood is bounded by Workspace Control
being off by default; harm is a permanent public-classified transcript containing private-origin
text.

### Rank 6 — editing a message drops the institution

`create_derived_session` carries `privacy_tier` through `raise_privacy` and never copies
`session_affiliations` (`crates/biorouter/src/session/session_manager.rs:6621-6653`); `import_session`
does the same. Editing a message forks a session — the most ordinary gesture in the product — and
the fork reads as *"no institution has touched this,"* which `owners_compatible` treats as
compatible with **every** model. Three live consumers read the emptied column: chat-recall load,
the chat-recall search SQL, and conversation ingest. One lattice is monotone across a fork; the
other silently resets.

### Rank 7 — the diagnostics bundle still carries the named private transcript

The cross-session sweep is fixed and `config.yaml` is now redacted (C-02). What remains is narrower
and still innocent: `GET /diagnostics/{session_id}` (`crates/biorouter-server/src/routes/status.rs:35`)
and `biorouter session diagnostics` (`crates/biorouter-cli/src/commands/session.rs:800`) both call
`generate_diagnostics` with **no reach gate of any kind**, and the bundle contains `session.json` —
a full export of the session named. The user hits a bug *in the private chat*, saves
`diagnostics_<id>.zip`, and attaches it to a GitHub issue. This remains the only path in the whole
set where a private transcript leaves the machine to a third party with the user believing they
sent a bug report.

### Rank 8 — the GUI badges Private over an extension the daemon treats as Public

The renderer derives privacy from the live BAAM catalogue; the daemon's private set is the two
compile-time strings above. After any catalogue update that tags a new connector private, Settings →
Extensions shows a Private badge on an extension a public model may enable and call.
`extensionPrivacy.ts` calls this, in its own words, *"a promise the daemon is not yet keeping."* A
user who reads the badge concludes the gates apply. They do not.

### Rank 9 — the master switch has no renderer half, so turning it off strands the user

`usePrivacyTiersEnabled` still has exactly one consumer, `PrivacyBadge`. The two composer
pre-flights that *state a policy reason* — `extensionPairingRefused` and
`SwitchModelModal.blockedReasonFor` — judge unconditionally. A user turns privacy tiers off in
Settings → Privacy (typing the phrase, reading the four paragraphs) precisely because something is
blocked, returns to their chat, and finds every public model still greyed out with *"Unavailable —
this is a private chat, so only private models may run in it."* The daemon would now permit the
bind. Not a leak; it is the control that teaches users the reason lines are unreliable, which is
what makes the leak-shaped ones ignorable.

### Rank 10 — the chat disagrees with itself about whether it is private

`useSessionPrivacyTiers` reads a cached session list whose only production invalidator,
`notifySessionListChanged`, is called by `useDiverge.ts` alone. `ChatInput` re-reads on
`message-stream-finished`; the header pill and the tab dot do not. A chat that starts public and
ratchets Private mid-life — the ordinary case — carries a Private dot on the composer and nothing
on the tab or the pill above the transcript until something reloads the window. Staleness is always
in the under-marking direction, so nothing is wrongly permitted; the cost is that the pill, the
surface the design elsewhere treats as the answer to "what is in this chat?", answers late.

### Rank 11 — the backfill classified from the last provider bound

`BACKFILL_PRIVATE_PROVIDERS` is complete for the runtime, and the migration now runs on the
fresh-database branch too (C-10). What remains is that `provider_name` records only the **last**
provider bound to a row. A chat full of private-model work whose final act was switching to Claude
for one wording question backfills Public, with no provenance and no notice specific to it — and
nothing anywhere surfaces the set *"chats that ran on a private provider at some point but were
backfilled public."* DR-10's fail-open is defensible; the absence of a way to find the rows is what
keeps this on the list.

### Rank 12 — the composer's knowledge chip says nothing

`BottomMenuKnowledgeSelection.tsx` contains zero tier-shaped code in 151 lines. A user in a public
chat opens the knowledge chip and switches on a private KB; nothing marks it private, nothing says
it will not work here, and every subsequent `kb_search` fails with a refusal the model relays as
prose. The extension chip six pixels away answers exactly this question for extensions.

### What did *not* make the ranking, and why

`workspace_watch` as an activity oracle, `manage_extensions {disable}` as availability loss, the
`grant_needs_system_authentication` dead duplicate, and `search_available_extensions`' existence
disclosure are all real and all either closed (see *What has already been fixed*) or too narrow to
reach a well-meaning user. The
`session export` family has moved: it is no longer a leak (C-07) and is now a broken feature
(N-01).

## What has already been fixed

Twenty-one commits landed on `feat/privacy-tiers` between `f83d86df` (the tree the audit read) and
`bbcfdb06` (the tree this synthesis read). **Sixteen findings are closed**, against twenty-three
still open across the two sections that follow, and three newly created. The closed set includes
every finding the earlier synthesis ranked in its top four, and its single named release blocker.
What remains open clusters in three places: §7's unimplemented write row, the renderer, and the one
accepted ruling Rank 1 of the ranking above is about. Each row
below was re-verified by reading the current source, not by trusting a commit subject.

| # | Finding as reported | Closed by | Verified at |
|---|---|---|---|
| C-01 | `platform__manage_schedule {session_content}` returned any named session's full transcript with no check of any kind, callable by subagents | The `§7` READ predicate now runs through one shared adapter, `privacy::visibility::refuse_unless_readable`, ahead of the transcript load; the dispatch arm passes `cap` | `agents/schedule_tool.rs:69, 480-505`; `privacy/visibility.rs:168` |
| C-01b | `manage_schedule {sessions}` listed every run session's name, working dir and message count | Filtered by `appears_in_list(cap.tier(), session.privacy_tier)` before rendering | `agents/schedule_tool.rs` (list arm) |
| C-02 | Diagnostics bundle swept the last ten `llm_request.*.jsonl` regardless of origin, plus a verbatim `config.yaml` | `log_belongs_to_session` filters the sweep to the named session; `redact_config_yaml` replaces credential-shaped values | `session/diagnostics.rs:344-360, 273, 373` |
| C-03 | `RequestLog` wrote full payloads unconditionally, private providers included | `PayloadPolicy::for_tier` → `MetadataOnly` for a private provider, decided once at `start_with_tier` and never re-read mid-stream | `providers/utils.rs:499-531, 552-556` |
| C-04 | Gate F1 bypassed by `workspace_set_tools` and by `workspace_open {new:{extensions}}` | `refuse_gated_extension_enable` / `refuse_gated_new_session_extensions`, before the daemon lookup and before `create_session` | `agents/workspace_extension.rs:1197, 2110-2119` |
| C-05 | Project-local memory from a private chat was inlined in full into every later session's system prompt | A `MemoryCapability` axis plus `local_withheld_notice`: private-origin local memories are withheld and their count is stated | `biorouter-mcp/src/memory/mod.rs:357-366, 461-473` |
| C-06 | `session export` ungated on CLI and HTTP | HTTP: `session_reach` before the load. CLI: `authorize_export_at_terminal` → `SessionManager::authorize_export`, with a source-scan test asserting the gate precedes the read | `routes/session.rs:752-777`; `biorouter-cli/src/commands/session.rs:374-452, 989-999` |
| C-07 | Legacy-JSONL sessions imported Public and skipped arm 19's backfill on a fresh database | The backfill now runs on the fresh-schema branch, and a failure walks the schema counter back so the next launch retries instead of leaving rows public on a DB that reports itself migrated | `session/session_manager.rs:2907-2945` |
| C-08 | Master switch off → a Private parent minted a permanently `public` child | The child's stamp is `privacy_tier.max(parent_classification(...))`, so the ratchet is monotone across the toggle | `agents/subagent_tool.rs:1038` |
| C-09 | `search_available_extensions` named *and described* private connectors to a public model | The method takes `admitted: CallCapability`; both the enabled listing and the config-disabled listing are filtered through Gate E's own verdict | `agents/extension_manager.rs:2913-2935` |
| C-10 | `manage_extensions {action: disable}` returned before any privacy decision, so a public chat could unload the clinical connector | `assert_extension_manageable`, which is `assert_extension_reachable` verbatim, so discovery and management answer with one function | `agents/extension_manager_extension.rs:274-290` |
| C-11 | `workspace_watch` and `workspace_close` received no capability and checked nothing | Both dispatch arms now pass `cap`; both call `refuse_unless_visible` | `agents/workspace_extension.rs:3018-3022, 2344, 2440` |
| C-12 | `workspace_set_tools` performed no privacy check at all | Now receives `cap` and calls `refuse_unless_visible` on the target | `agents/workspace_extension.rs:3019, 1821+` |
| C-13 | DR-26's cross-affiliation warning was `tracing::warn!` and nothing else at the user's own enable surface | `/agent/add_extension` returns the notice in its 200 body, composed by the same `Agent::cross_affiliation_notice` that `/agent/update_provider` returns; the renderer consumes it | `routes/agent.rs:1099-1145`; `ui/desktop/src/utils/crossAffiliationNotice.ts` |
| C-14 | `kb_is_out_of_reach` asked the tier axis where `assert_kb_reachable` asked two | Both axes ride one `CallerIdentity`, and `kb_is_out_of_reach` is now literally `assert_kb_reachable(...).is_err()` — there is no narrower thing left to pass | `biorouter-mcp/src/knowledge/server.rs:36-52, 425-427` |
| C-15 | `docs/security/privacy-tiers.md`'s "Did not ship" ledger claimed `visibility.rs` had no production caller and named the wrong two tools as the whole exposure | Rewritten against the tree on 2026-08-06, symbol by symbol, including an explicit note that the entry had been false | `docs/security/privacy-tiers.md:108-180` |
| C-16 | The anti-regression test for the §7 matrix asserted callers for two predicates out of five and was cited as evidence the matrix was wired | Superseded for most guards by `crates/biorouter/tests/privacy_guard_wiring.rs` — a 1 284-line census with three statuses (`Wired`, `WiredThrough`, `Unwired(reason)`) that **requires a stated reason** for every zero, so an unwired guard is a recorded decision rather than an invisible one | `crates/biorouter/tests/privacy_guard_wiring.rs` |

Two further changes arrived that the audit did not ask for and that are worth knowing: **DR-31**
added an affiliation comparison to the spawn gate, which had compared tiers only
(`subagent_tool.rs:1656`), and the CLI was given a real `CallCapability` so session reach is gated
on capability rather than on being human — which fixed an asymmetry where the CLI was refused for a
private chat while the desktop app was admitted for the *same* chat running the *same* public model.

## What is still open in the daemon

| # | Finding | Evidence at `bbcfdb06` |
|---|---|---|
| O-01 | **The private→public downgrade write is permitted and silent.** `workspace_send_prompt` enforces VIS only; `refuse_unless_visible` short-circuits `Ok` for any private caller before reading the target row. The design's `✓!` first-crossing approval never fires. | `agents/workspace_extension.rs:1534-1538` (the handler's own comment says so) |
| O-02 | **`workspace_open {new}` implements none of §8.2's spawn matrix on the model dimension.** `open_new_session` creates the row through `start_session`, which takes no capability and binds the machine default, then optionally runs the model's own prompt text on it. | `agents/workspace_extension.rs:1188-1252`; `biorouter-server/src/workspace/services.rs:170-236` |
| O-03 | **`may_write`, `lineage_of` and `requires_first_crossing_approval` still have zero production callers.** §7's entire write row and R6's lineage floor (columns B, E, G) are unenforced: a public caller may steer, re-tool and close a public sibling it did not spawn. They are now *declared* `Unwired` in the census with stated reasons — which converts an invisible defect into a recorded one but does not close it. | `privacy/visibility.rs:97, 112, 138`; `tests/privacy_guard_wiring.rs:234-278` |
| O-04 | **`session_affiliations` is not carried by `create_derived_session` or `import_session`.** Carrying it is one `UPDATE` and is monotone in the safe direction. | `session/session_manager.rs:6621-6653`, `:6540-6580` |
| O-05 | **The diagnostics bundle is still ungated for the session it names**, on both the HTTP route and the CLI subcommand. | `biorouter-server/src/routes/status.rs:35-56`; `biorouter-cli/src/commands/session.rs:800-830` |
| O-06 | **`start_session` still discards the operator's `enabled: false`.** It resolves names with `get_extension_by_name`, which is `get_extension_entry_by_name(name).map(…)` onto `entry.config`, dropping the flag — the exact defect `resolve_added_extensions` carries a 14-line warning about, ~500 lines away in the same feature. The tier and affiliation half is now gated; the operator's own flag is not. | `biorouter-server/src/workspace/services.rs:184`; `biorouter/src/config/extensions.rs:138-140` |
| O-07 | **The backfill reads only the last-bound provider**, and nothing surfaces the rows it mis-classified. | `session/session_manager.rs:3912`, `backfill_update_sql` |
| O-08 | **The private extension set is two compile-time strings, with no local override and no runtime refresh.** §10.2's second union term exists only in TypeScript. Because Gate C's ratchet is conditioned on the extension being classified private, an untagged private connector also never classifies the chat. | `privacy/registry_private.rs:42`; `ui/desktop/src/components/baam/privateSet.ts` |
| O-09 | **`GuardrailStage` is dead and the tool-result detector is tier-blind.** No input-side PII stage exists. | `guardrails/mod.rs:20-32`; `agents/agent.rs:2825` |
| O-10 | **`provenance::record` has zero Rust production callers.** The live writer is TypeScript and fires only when a `registrySource.registryId` is present, so DR-23's rename defence covers desktop Browse installs and nothing else. | `privacy/provenance.rs:295` |
| O-11 | **`grant_needs_system_authentication` remains a dead duplicate** whose doc claims a caller that does not exist; `strict_mode_authorization` re-spells the rule inline. Now declared `Unwired` in the census. | `privacy/mixing.rs:442`; `tests/privacy_guard_wiring.rs:652` |
| O-12 | **An accepted cross-affiliation grant clears only the tool-call door.** `assert_extension_reachable` passes no session and reads no grant, so the same model refused at `read_resource` after a successful Approve sees a refusal with no accept control under it. The code names the cause: a missing argument, not a decision. | `agents/extension_manager.rs:2089-2101` |
| O-13 | **`GET /sessions` and `GET /sessions/sidebar` still enumerate wholesale, and `GET /knowledge/active` is still open.** One unproven request returns every private chat on the machine, titled, with its working directory. This does not weaken the reach gate — none of those rows carries a transcript — but it undercuts the reason `SESSION_OUT_OF_REACH` is worded as one sentence for two answers, since the per-id oracle is closed and the bulk one is not. | `biorouter-server/src/routes/session_reach.rs:36-52` (the module's own residual list) |

## What is still open in the desktop renderer

This is the surface the first synthesis had only as a truncated *holds* list. Its gap findings, folded
in and re-verified:

| # | Finding | Direction | Evidence at `bbcfdb06` |
|---|---|---|---|
| O-14 | **The composer's model chip shows the app-global model, not the chat's bound model** — and the privacy disclosure, and now DR-26's affiliation badge, hang off it. `CurrentModelContext` has no `.Provider` anywhere in the tree. | under-marking | `components/BaseChat.tsx:106-107`; `settings/models/bottom_bar/ModelsBottomBar.tsx:213-245` |
| O-15 | **The master switch has no renderer half in either composer pre-flight.** `usePrivacyTiersEnabled` has one consumer, `PrivacyBadge`. `extensionPairingRefused` and `SwitchModelModal.blockedReasonFor` judge unconditionally and state a policy reason for doing so. | over-marking | `components/ConfigContext.tsx:483-487`; `settings/extensions/extensionPrivacy.ts:191`; `settings/models/subcomponents/SwitchModelModal.tsx:~282` |
| O-16 | **Extension pairing is keyed on the session's classification; Gate C is keyed on the bound provider's tier.** These agree only after the first completed turn — and, with the master switch off, never, because the ratchet sits inside `if privacy_enforced`. A new chat on Versa shows the clinical connector disabled with "Unavailable in this chat (public model)". | over-marking | `bottom_menu/BottomMenuExtensionSelection.tsx:249, 419, 450`; `agents/agent.rs:4807` |
| O-17 | **The chat header pill and the tab dot are snapshots and never re-read; the composer dot does.** Three chat-side tier surfaces disagree about the same chat. | under-marking | `chatGroups/ChatGroupsShell.tsx:86-138`; `hooks/useDiverge.ts:97` (the sole invalidator); `components/ChatInput.tsx:308-329` |
| O-18 | **The composer's knowledge-base chip carries no tier badge and no pairing state**, unlike the extension chip beside it. The Knowledge view's palette *does* badge, and has a full tier control with a typed-phrase confirmation. | silent | `bottom_menu/BottomMenuKnowledgeSelection.tsx` (zero tier-shaped code); cf. `knowledge/KBSelector/KBSelectorPalette.tsx` |
| O-19 | **DR-27's mixing policy has a reader and no writer, and no surface states it.** `readMixingMode()` is called from exactly one place, the accept card's effect. `PrivacyPanel.tsx` contains no occurrence of "mixing", "strict" or "open". A site that wants `strict` or `open` must hand-craft a config upsert. | silent | `utils/crossAffiliation.ts:202-214`; `settings/privacy/PrivacyPanel.tsx` |
| O-20 | **A cross-affiliation mismatch on a knowledge base has no accept control at all.** `assert_reachable` composes a refusal whose only remedy is "change the model", because the grant is keyed on an extension and lives in a database that crate cannot reach. Shaped like the extension refusal, but with no button — which reads as the button being broken rather than absent. | silent | `biorouter-mcp/src/knowledge/tier.rs:483-497` |
| O-21 | **The Knowledge ingest panel has zero tier awareness.** See Rank 2 above; this is the renderer half of it. | silent | `knowledge/IngestPanel/*` |
| O-22 | **The carefully-worded `SESSION_OUT_OF_REACH` refusal never reaches a human.** Every renderer path that can receive it discards the text for a generic retry message. Now materially wider than when it was written, because the export route joined the gated list (see N-01). | silent | `sessions/SessionsView.tsx:31-32`; `components/ChatInput.tsx:320-322` |
| O-23 | **The CLI session banner asserts enforcement that may be switched off.** `tier_row(Private)` prints *"Private — only a model hosted inside the institution may run here"* with no read of the master switch, while `PrivacyBadge` goes to considerable lengths to append "— enforcement off" and documents the rejected alternative as *"not information, it is a false statement, and it is worse than no badge because the user acts on it."* | over-claiming | `biorouter-cli/src/session/privacy.rs:130-137`, called unconditionally at `session/output.rs:1408` |

One renderer finding has been **resolved into a ruling** rather than fixed: `SwitchModelModal` now
states the selected provider's affiliation before the switch, and records in its own source why the
tier is deliberately *not* badged there — the only tier it has is the type-level `metadata.tier`, and
a Private pill hung on that would read Private for an `ollama` re-pointed off this machine.

## New findings, produced by the fixes

These are not in any of the six payloads. They exist because of what landed between the audit and
this re-run, and they are the reason a synthesis has to re-read the tree rather than merge reports.

**N-01 — the GUI's Export button is now a silent no-op on a private chat.** `GET
/sessions/{id}/export` correctly joined the gated list and calls `session_reach` before the load
(`routes/session.rs:767-772`). `session_reach` admits a caller two ways: a capability that covers
the target, stated as a provider name in `CALLER_PROVIDER_HEADER`, or the user-action proof.
`SessionListView.tsx:659` calls `exportSession({ path, throwOnError: true })` with **neither** —
the renderer's global `client.setConfig` sets only `Content-Type` and `X-Secret-Key`
(`renderer.tsx:465-471`), and `userActionHeaders()` is attached per request, which this call site
does not do. `handleExportSession` has no `catch`. So a user clicking Export on a private row gets a
403, an unhandled promise rejection, no file, no toast and no error. The leak was closed and the
feature was broken in the same motion, and the failure is invisible — which is exactly the shape
O-22 describes.

**N-02 — export now has two decisions, and only one door calls the shared one.**
`SessionManager::authorize_export` is a real shared gate: it returns `ExportDecision::{Unrestricted,
CapabilityRequired, SystemAuthenticationRequired, Authorized, SessionNotFound}`, re-reads the row
between passes, and raises the OS authentication prompt last. It has **exactly one production
caller** — the CLI. The HTTP route, and therefore the desktop app, uses `session_reach` instead. One
capability, two doors, two different rules, two different user experiences: at the terminal a
private export costs a Touch ID prompt and succeeds; in the GUI it silently fails. This is the
"capability guarded at three doors of four" shape the campaign has closed four times elsewhere,
re-appearing in the fix for the finding that named it.

**N-03 — the anti-regression scan lost its anchor.** `visibility::tests::the_matrix_has_production_callers`
is a source scan of `workspace_extension.rs`. The decision body has since moved into
`visibility.rs::refuse_unless_readable`, so `may_read(` no longer appears in the production half of
the file the scan reads. The scan is green; it is green for the wrong reason. `privacy-tiers.md`
records this honestly at lines 155-161, and `privacy_guard_wiring.rs` supersedes it for most guards
— but the scan itself still exists and still passes, so a reader who finds it first is told the
matrix is wired.

## What the four-payload synthesis could not see

Stated explicitly, because the earlier report is on the record and someone will read it:

1. **It had no innocent-mistake ranking**, so its own ranked table ordered by leak severity across surfaces
   rather than by the likelihood of a well-meaning researcher hitting it. The two orders differ
   substantially. In particular that table's top item, `manage_schedule {session_content}` — the
   one finding it said it would block a release on — is now closed, while the item that would rank
   first on the operator's question, the unclassified-connector fail-open (Rank 1), appears nowhere
   in its ranked table because it is a ruling rather than a defect.
2. **It had no renderer gap list**, so nine of the ten renderer findings above are absent from it
   entirely. It listed `cross_affiliation_warnings` having zero renderer callers, correctly, but had
   no way to see that the master switch, the extension pairing state, the KB chip, the ingest panel,
   the mixing policy and the stale pill were all in the same condition.
3. **It missed the ingest panel and the guardrails entirely** — Ranks 2 and 3 above, both of which
   are automatic paths that need no agent decision and no user error.
4. **It could not see that four of its own eight "unwired guard" rows had a different status by the
   time it was written**, because it verified against the tree it was given rather than the tree as
   it stood.

Its structural findings — the ratchet is sound; the gates share one predicate rather than four
spellings; the capability is sampled once and carried; the spawn path is tight — hold, and were
re-confirmed here.

## Decisions only the operator can settle

**D1 — is R11(ii) still the ruling?** Everything in Rank 1 follows from *"anything not on BAAM is
Public."* The rest of the machinery is now complete enough that this is the dominant residual, and
it is the answer to the question that was asked. Options: accept and re-state it prominently in the
user-facing disclosure; add a Rust reader for §10.2's second union term; or add an operator-only
local route to tag an extension private (which R11(i) currently forbids, for the reason that a
public model could otherwise edit `config.yaml`).

**D2 — does the private→public write ship silently, or get refused like the spawn?** The design says
`✓!`; the code says `✓`; `requires_first_crossing_approval` has no consumer and none can be built on
a tool-call path without a user channel. Two consistent options: refuse the downgrade write outright
exactly as `spawn_downgrade` already does (one branch, no new UI, and it lets three dead predicates
be deleted rather than a fifth correct-but-uncalled guard shipped), or record the ruling that v1
permits it silently and correct §7. If you take the first, amend `spawn_downgrade`'s advice text — it
currently points at the door you would be closing.

**D3 — is `workspace_open {new}` a spawn?** §7 files it under "see §8.2" and the code applies none of
§8.2. Inheriting the caller's tier makes it a spawn subject to DR-19; keeping it a public-session
constructor requires wiring the approval on `send_prompt`. Today it does neither, and it is the
route DR-19's own refusal text names.

**D4 — do `create_derived_session` and `import_session` carry `session_affiliations`?** One `UPDATE`,
monotone in the safe direction. Listed as a decision only because it changes behaviour for existing
forked rows.

**D5 — should the GUI be able to export a private chat at all, and by which decision?** N-01 and N-02
have to be settled together. Either the renderer sends the user-action proof and the two doors keep
two rules, or `export_session` calls `SessionManager::authorize_export` like the CLI does and the
GUI grows the system-authentication prompt. Doing nothing leaves a button that fails silently.

**D6 — should the composer's pre-flights go quiet when the master switch is off?** Going quiet
restores the user's route out; staying loud keeps the warning visible on the configuration where
exposure is largest. `PrivacyBadge` chose "stay visible, say enforcement is off" — the same treatment
needs a third state on both pre-flights, not a boolean.

**D7 — is a first-ingest-into-a-public-KB warning in scope for v1?** The current behaviour means the
sensitivity of a base is decided by whichever session ingests first, and for the Knowledge view's own
dropzone that is the app default model, chosen for reasons unrelated to the documents being dropped.

## Method and limits

Six auditors read six surfaces against branch `feat/privacy-tiers` on 2026-08-06: data-flow paths
into a model's context; subagent spawn and the Workspace Control extension; extension calls and the
four gates; session-store writes and every path out of the store; the desktop renderer; and the
innocent-mistake question. Their results are one `{"type":"result", …}` record each in the
workflow journal at
`~/.claude/projects/-Users-wgu-Desktop-BioRouter/967f3aca-…/subagents/workflows/wf_97db4c3e-832/journal.jsonl`.
The first synthesis (record 13) received four of the six.

This document merges all six and then re-checks every finding by reading the working tree at
`bbcfdb06`. **No finding is reported here on the strength of a payload alone.** Findings the audits
raised that could not be re-confirmed in the current tree are recorded as closed above, with the
mechanism that closed them, not silently dropped.

Three limits are worth stating. First, this is a **reading** of the tree, not an execution of it: no
test was run and no build was made as part of this synthesis, so a finding whose disposition depends
on runtime behaviour (the macOS `LAContext` prompt actually appearing from a daemon spawned by
Electron, for instance) is reported as the earlier audit left it. Second, `session_reach`'s own
residual list is a snapshot of an enumeration rather than a proof of completeness, and O-13 inherits
that limit — the routes named there are the ones someone enumerated, and the module says so.

Third, and most important for anyone re-checking this: **the worktree was mid-merge**, as the
warning under *Identifiers used here* records. That was discovered late, after the findings were
written, and it invalidated the first pass over five cited files — the working copies contained
conflict markers, so the "current state" they appeared to show was two states interleaved. The
affected claims were re-derived from committed content and survived unchanged, but the general
lesson is worth keeping: an audit that reads a working tree is reading whatever another agent
happens to have left there, and `git status` is part of the method, not a formality before it.

## Related documentation

- [`docs/security/privacy-tiers.md`](../../security/privacy-tiers.md) — the design this audit was
  read against, including the §7 capability matrix and the "Did not ship" ledger that C-15 rewrote.
- [`docs/security/privacy-tiers-execution-plan.md`](../../security/privacy-tiers-execution-plan.md) —
  the decision records (`DR-nn`) cited throughout, including DR-15's master switch, DR-19's spawn
  refusal, DR-26's affiliation axis and DR-27's mixing policy.
- [`docs/security/privacy-tiers-implementation-brief.md`](../../security/privacy-tiers-implementation-brief.md) —
  the accepted-residual list, which O-05 and the export findings both bear on.
- [`docs/security/data-privacy-and-phi.md`](../../security/data-privacy-and-phi.md) — the
  user-facing statement of what BioRouter does and does not stop, which Ranks 1 and 3 are the
  engineering backing for.
- [`docs/security/institutional-affiliation.md`](../../security/institutional-affiliation.md) — the
  DR-26 axis that O-04, O-12 and O-20 all sit on.
- [`docs/history/workspace-control/README.md`](../workspace-control/README.md) — the BR-71 workspace
  control record, whose tools are the subject of O-01, O-02, O-03 and O-06.
