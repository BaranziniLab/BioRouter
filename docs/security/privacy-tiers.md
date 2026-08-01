# Privacy tiers

> **What this is.** The design for a privacy-tier system that keeps conversations touched by
> private models or private data sources from ever reaching a model hosted outside the user's
> institution. It classifies models, sessions, MCP extensions and knowledge bases, and enforces the
> boundary at five choke points in the agent loop.
> **Status:** Proposed — **narrowed by operator ruling on 2026-07-30 ([DR-17](privacy-tiers-execution-plan.md#scope-ruling--dr-17-narrows-this-plan-to-the-session-store)); read §1 before
> anything else.** The general filesystem read-deny of §9.5 is **descoped for v1** and this document
> no longer claims it. §17 still needs rulings.
> **Audience:** developers working on the agent loop, `biorouter-server`, the session store, and
> the desktop GUI.

> ⚠ **Every line anchor in this document is historical.**
> They were verified against `main` at `708390d8` and have since moved by roughly **+150
> to +720 lines** — `extension_manager.rs` by +150, `agent.rs` by +719, `session_manager.rs` by
> +200. Do **not** chase a line number from this document; the named **symbol** is the anchor. The
> current positions are tabulated in
> [the execution plan's drift table](privacy-tiers-execution-plan.md#read-this-before-you-chase-a-line-number),
> which was re-verified at `9558c346` and confirmed unchanged at `89c1f026`. Three claims here were
> also false about the tree — §9.3 A1 named a shell-command builder that has never existed, §9.3 B3
> described a global-memory mechanism issue #58 deleted, and §2.3 asserted a uniqueness that a second
> code path contradicts. **All three were corrected in place on 2026-08-01** (execution plan Task 1),
> along with §9.3 B4's ruling, §11.4's missing row, §15.1's migration numbering and §16's counts. The
> anchors added by that pass were verified against the tree on that date; every *other* anchor in this
> document remains historical.
>
> §9.3 B4's forced choice (ratchet knowledge bases, or declare them a public sink) has since been
> **ruled** by the operator: *ratchet*. See the execution plan's
> [Accepted risks](privacy-tiers-execution-plan.md#accepted-risks) for the costs that ruling
> accepts, and its Tasks 10A–10C for the implementation.

---

## 1. Summary

> ⚠ **Scope ruling, 2026-07-30 ([DR-17](privacy-tiers-execution-plan.md#scope-ruling--dr-17-narrows-this-plan-to-the-session-store) and [DR-18](privacy-tiers-execution-plan.md#dr-18--the-knowledge-base-tier-is-user-controllable-and-a-private-session-creates-a-private-base)).** This section was rewritten to match what
> is actually being built. **The general filesystem read-deny of §9.5 is descoped for v1.** What is in
> scope: session logs and histories locked against a public-capability model; a public model that can
> neither raise its own tier nor reach the private-only extensions; knowledge bases as first-class
> tiered objects with a user-controlled tier; and a **disclosure** telling users what a non-private
> model can reach. §9.5, §16(8) and §16(9) still describe the wider control and are marked in place —
> they are retained as the specification a revival would start from, not as work.

Two independent lattices, one column, one predicate, five gates — **and, deliberately, no general
filesystem barrier**.

- **Capability** — what a session may *do* — is the **least** privileged model currently bound to
  it. A mixed lead/worker configuration therefore has public reach, because its transcript already
  goes to the public worker.
- **Classification** — how sensitive a session's contents *are* — is the **most** sensitive thing
  it has ever touched, and is a permanent ratchet in SQL.

A public model must never reach a private session — not once, not read-only, not indirectly. The
converse is unrestricted: a private model may read anything.

The five gates sit on *tool calls*, and **that is where this design's guarantee ends.** A
public-capability session also holds tools that run arbitrary commands and read arbitrary paths, and
the private material is ordinary files on disk. §9.5 specifies a sixth control that would close that
channel — a two-layer read-deny over four directories and one file. **It is descoped for v1 by
operator ruling, and this design does not claim it.**

**So state the boundary plainly, because a reader must not infer a stronger one.** A public model
with shell access **can** still read a private session's derived artifacts, and any file a private
session wrote outside BioRouter's own session store. What the gates stop is narrower and is worth
stating positively:

- **The agent-mediated path.** A public model cannot ask BioRouter for another session's content —
  not through `chatrecall` in either mode, not through cross-session conversation ingest, not through
  the BR-71 workspace tools, not through a private knowledge base.
- **The transcript path.** A private session's history is never sent to a public model on a turn, on a
  summary, on an auto-name, or through a copy, diverge or import.
- **The tier-escalation path.** A public session cannot acquire private capability, cannot attach a
  private extension, and cannot see or call `ucsfomopagent` or `cdwagent` — the *"public models cannot
  spin up private models to help them do their work"* requirement.

**Knowledge bases are first-class, not incidental files.** A base carries a tier, takes it at creation
from the model that created it, ratchets on ingest, and a public-capability session may neither read
nor write a private one. The **user** — never a model — moves a base between private and public
([DR-18](privacy-tiers-execution-plan.md#dr-18--the-knowledge-base-tier-is-user-controllable-and-a-private-session-creates-a-private-base)).

Because the barrier is narrower than the risk, **the risk is disclosed**: a model that is not
HIPAA-compliant, not hosted on-premise and not local can reach what is on the machine, and the
product says so where a user reads it. That disclosure is a shipped requirement (R15), not a caveat —
it is what makes accepting the rest a considered tradeoff rather than an omission.

One **master toggle** turns the enforcement half — gates and ratchet — off for a user who does not
want it (§10.6). It does not turn off the disclosure.

The system is not expressible with what exists today. `provider_class` in
`crates/biorouter-server/src/routes/apps.rs:2089` is the only thing in the tree that resembles it,
and it is **inverted at both ends** (§2.2). Nothing in the session store records a privacy
property, and five distinct code paths can currently attach a public model to a session whose
history came from a private one.

---

## 2. The problem, and why today's code cannot express it

### 2.1 There is no tier anywhere in the backend

| Concept | Today | Verified |
|---|---|---|
| Model tier | Does not exist in Rust. `ProviderMetadata` has exactly eight fields (`name`, `display_name`, `description`, `default_model`, `known_models`, `model_doc_link`, `config_keys`, `allows_unlisted_models`) and no tier. | `crates/biorouter/src/providers/base.rs:145-164` |
| The only tier-shaped list | Lives in the **renderer**: `const INSTITUTIONAL = new Set(['versa_azure','versa_bedrock']); const LOCAL = new Set(['llamacpp','ollama']);` — presentation only, unreachable from the CLI, the daemon or any gate. | `ui/desktop/src/components/settings/providers/providerOrdering.ts:4-5` |
| Session tier | No column. The `sessions` DDL ends `provider_name, model_config_json, diverged_from, external_key, branch_point_msg_uid`. `CURRENT_SCHEMA_VERSION` is 16. | `session_manager.rs:29`, `:1865-1889` |
| Extension tier | No field on `struct Extension` (`config`, `client`, `server_info`, `_temp_dir`, `inprocess`, `_pooled`) and no classification field in the marketplace registry. | `extension_manager.rs:56-72`; `landing/registry.json` (version 1, 37 extensions, 129 skills, keys `id name organization version description tags github download filename license`) |

### 2.2 The one existing approximation is backwards

`provider_class` (`routes/apps.rs:2089`) classifies a provider name into `Local` /
`Institutional` / `External` and gates three real sites (`provider_allowed_for_app`,
`resolve_route`, worker-profile admission). Membership is **exact equality**:

```rust
if p.contains("local") || LOCAL_PROVIDERS.iter().any(|x| p == *x) { return Local; }
if p.contains("institution") || INSTITUTIONAL_PROVIDERS.iter().any(|x| p == *x) { return Institutional; }
External
```

`versa_azure` and `versa_bedrock` match neither list nor either substring, so they fall through to
**`External`** — while `azure`, `azure_openai`, `aws_bedrock`, `bedrock`, `databricks`, `vertex`
are listed **`Institutional`**. Agent Drafter today refuses a Versa route for a sensitive app and
permits a direct Bedrock one. The function's own test table (`:6447`) never exercises a `versa_*`
name, which is why the inversion is green.

### 2.3 Five present-tense paths put private history in front of a public model

Each verified by reading the code, each fixed as a by-product of this design:

1. **`Agent::update_provider` performs no check at all** (`agent.rs:4936-4956`) and swaps the
   in-memory provider *before* it persists. It is nevertheless the **sole writer** of both
   `Agent::provider` and `sessions.provider_name` — a grep for `.provider_name(` outside
   `session_manager.rs` returns exactly one hit, at `agent.rs:4951`. That is what makes a single
   bind gate sufficient.
2. **`chatrecall` LOAD mode has no filter of any kind** (`chatrecall_extension.rs:91-158`). Given
   any session id it calls `get_session(&sid, true)` and emits the session name, working
   directory, message count and six verbatim messages. It does not even carry SEARCH's
   `exclude_session_id` guard. This is **one of two** fully-open cross-session reads in the product
   today. The other is `platform__ingest_conversation`
   (`crates/biorouter/src/agents/knowledge_tool.rs:24-86`), which takes a caller-supplied
   `session_ids` array (`:32-41`), loads each session's full conversation with
   `get_session(sid, true)` (`:49`) and ingests it into a knowledge base — with no lineage,
   ownership or tier check. It is dispatched at `agent.rs:3205`, *before* the extension-manager
   fall-through at `:3339`, so Gate C never sees it; it is not an MCP tool, so Gate E cannot hide
   it; and it never touches `chat_history_search.rs`, so Gate D never sees it. It is advertised
   unconditionally (`agent.rs:3878-3883`, whose own comment reads "The conversation-ingestion tool
   is always available on the platform extension") and its description tells the model outright to
   "Pass `session_ids` to ingest specific (or multiple) sessions instead"
   (`agents/platform_tools.rs:64-65`). Because a knowledge base is a machine-wide tree any session
   may name (§9.3 B4), this is a one-call private→public laundering primitive, and it belongs at or
   above LOAD in §19's order.
3. **Three independent session-copy paths carry the conversation but not the provider.**
   `copy_session` (`session_manager.rs:4138-4168`), `diverge_session` (`:4204-4265` — the primary
   GUI diverge, and it does *not* call `copy_session`) and `import_session` (`:4096-4135`) each
   hand-roll their own update builder. None sets `provider_name` or `model_config`, so a branch of
   a private chat resolves through `restore_provider_from_session`'s `Config::global()` fallback
   (`agent.rs:4963-4978`) and runs private history on the user's default public model, with no
   prompt.
4. **Agent Drafter's `ClientFrame::ModelSelect`** (`routes/apps.rs:3409-3428`) calls
   `create_provider` then `agent.update_provider` with no check whatsoever, over the
   `GET /apps/{id}/agent` WebSocket, which `auth.rs:52-77` exempts from secret-key auth.
5. **`POST /agent/call_tool`** (`routes/agent.rs:1140-1163`) dispatches straight into
   `ExtensionManager::dispatch_tool_call`, skipping `Agent::dispatch_tool_call` and every
   `ToolInspector`. Any control expressed as an inspector is invisible to it.

---

## 3. Settled requirements

| # | Requirement |
|---|---|
| R1 | **Private models** are institutionally hosted (`versa_azure`, `versa_bedrock`) and user self-hosted (`llamacpp`, `ollama`). **Public** is everything hosted by an AI company or a large cloud. |
| R2 | Capability = **least** privileged model in play. Classification = **most** sensitive thing touched. The two are independent. |
| R3 | Classification is a permanent ratchet. |
| R4 | A private session may spawn public children; a public session may never gain private reach. |
| R5 | Children inherit the parent's model and lead/worker mode unless the user says otherwise. |
| R6 | Lineage decides write access: sessions the caller spawned get full control, everything else is read-only. |
| R7 | A global opt-out exists, off by default. It is a **master** switch: with it off there is no gate and no ratchet anywhere (§10.6). ⚠ It does **not** switch off R15's disclosure — with enforcement off the exposure is larger, not smaller. |
| R8 | A public model must never reach a private session. |
| R9 | Only the user can deprivatise a session, from history settings, with a warning. Nothing automatic, nothing agent-invocable. |
| R10 | Badges are visible everywhere — models, sessions, MCP servers. |
| R11 | The BAAM registry is the **sole** source of extension classification. (i) nothing local can grant private; (ii) **anything not on BAAM is public**. Built-ins are public. |
| R12 | Skills carry no classification. |
| R13 | `chatrecall` obeys the barrier. Side channels (existence, counts, timing) are out of scope; only content must not cross. |
| R14 | The registry is trusted (only the Baranzini Lab can publish) and the classification ships on the landing site and in `registry.json`. |
| **R15** | **Users are told what a non-private model can reach.** A model that is not HIPAA-compliant, not hosted on-premise and not local can read what is on the machine; the product says so, in the GUI, in the CLI and in the docs, from one shared copy. Added by [DR-17](privacy-tiers-execution-plan.md#scope-ruling--dr-17-narrows-this-plan-to-the-session-store), which is also what makes its accepted risks acceptable. |
| **R16** | **A knowledge base is a first-class tiered object, and the *user* owns its tier.** It takes a tier at creation from the model that created it, ratchets on ingest, is unreadable and unwritable to a public-capability session when private — and the user, never a model, may publicize or privatize it. Added by [DR-18](privacy-tiers-execution-plan.md#dr-18--the-knowledge-base-tier-is-user-controllable-and-a-private-session-creates-a-private-base). |

⚠ **R8 is unchanged in words and narrowed in reach.** *"A public model must never reach a private
session"* remains the invariant every gate is written against. What [DR-17](privacy-tiers-execution-plan.md#scope-ruling--dr-17-narrows-this-plan-to-the-session-store) descoped is the
**filesystem** channel to that material, not the rule: a public model may not be *handed* a private
session by BioRouter, and it may still read files on the machine it is running on. §1 states the
boundary; R15 is why that is disclosed rather than implied.

---

## 4. The two lattices

They are not two reductions over one set. They are reductions over **different domains**, which is
what makes them provably consistent rather than contradictory.

```
capability(S)     = least over { components of the provider bound to S RIGHT NOW }
                    domain: providers.  Pure function of live state.  NOT STORED.

classification(S) = max   over { floor(capability) at each turn S has ever run,
                                 Private for each private MCP call S made,
                                 the classification inherited at creation }
                    domain: events over time.  Monotone accumulator.  STORED.
```

Capability has no memory. Classification has no reference to live state. The independence is
enforced three ways:

1. **Type level.** Two Rust types that do not interconvert.

   ```rust
   // crates/biorouter/src/privacy/mod.rs

   /// CAPABILITY. Deliberately NO Ord: `max` over this type is always a bug.
   pub enum ProviderTier { Public, Private }
   impl ProviderTier { pub fn least(a: Self, b: Self) -> Self { … } }

   /// CLASSIFICATION. Monotone in time.  Public < Private.
   #[derive(PartialOrd, Ord, …)]
   pub enum Classification { Public, Private }

   /// The ONE crossing. pub(crate); a repo-grep test asserts exactly two callers.
   pub(crate) fn floor(t: ProviderTier) -> Classification { … }
   ```

   There is no `From`/`Into`. A single `Ord`-derived tier would be simpler and would invite
   `max()` where `least()` belongs.

2. **Storage level.** Capability is `Agent::capability_tier()`, a method reading
   `self.provider.lock().await.tier()`. No field, no column, no cache that can go stale.

3. **Scope level.** Of the five gates, exactly one reads both (Gate A/B, the bind). Gate C reads
   capability and `extension.tier`. Gate D reads capability and the *target's* classification,
   never the caller's.

**Invariant, and it should be a property test:** for any sequence of legal binds,
`capability(S) ≥ classification(S)`. The bind admits `P` only if `tier(P) ≥ classification(S)`; the
ratchet then sets `classification := max(old, floor(tier(P))) ≤ floor(tier(P))`. By induction, the
naive failure — "the very next turn feeds private history to a public model" — has no code path.

### 4.1 The mixed configuration, worked through

**Private lead + public worker.** `tier = least(Private, Public) = Public`.

- Capability is **Public**: cannot call a private MCP server, cannot read a private session.
- Classification does **not** ratchet, because `floor(Public) = Public`. The transcript has already
  gone to the public worker, so there is nothing left on that axis to protect — and marking it
  Private would make the bind gate refuse *that same composite* on the next resume, bricking a
  working configuration.

This is the one place the design reads R3 in spirit rather than letter (R3 says "switched to a
private model even once → private permanently", and a composite *contains* a private model). It
needs an explicit ruling — see §17, question 1.

`LeadWorkerProvider::tier()` returning `least(lead, worker)` is the **only** override needed, and
it is the reason `tier()` is an instance method rather than a lookup on `get_name()`:
`get_name()` on a composite returns the **lead's** name (verified,
`providers/lead_worker.rs:332-334`), which would badge a private-lead/public-worker session Private
— the exact inverse of R2.

### 4.2 The residual state

`classification = Private, capability = Public` is representable but unreachable through any legal
bind. It arises only from legacy rows, a pre-fix copy path, an LRU-evicted agent rehydrated with
the process default, or a scheduled job built from global config. **Gate B owns it**, and in that
state both properties hold at once: the session cannot run a turn until repaired or declassified,
*and* it stays invisible to every public reader, because Gate D keys on the target's classification
against the caller's capability, independent of the target's own capability. A corrupted private
session cannot leak while it is broken.

---

## 5. How each thing acquires a tier

### 5.1 Models

One new field and one new method.

```rust
// ProviderMetadata gains a ninth field — Serialize + ToSchema, so it reaches every UI
// surface through `just generate-openapi` → `npm run generate-api`.
pub tier: ProviderTier,

pub trait Provider {
    /// Least-private component of what this instance actually resolved.
    /// DEFAULT = Public: a provider module that forgets to implement it is
    /// least-trusted and can never attach to a private session.
    fn tier(&self) -> ProviderTier { ProviderTier::Public }
}
```

The private set is the list that already exists, moved from the renderer to Rust
(`PRIVATE_PROVIDERS = ["versa_azure", "versa_bedrock", "llamacpp", "ollama"]`). The two frontend
`Set`s are **deleted**; `classifyProvider` switches on the backend field, keeping `PRIORITY_ORDER`
for ordering. One list, one place, drift structurally impossible.

**Unknown ⇒ Public, and that is fail-safe rather than fail-open.** Public is the *less* privileged
tier, so an unrecognised provider gets less reach. This is the opposite fail direction from R11(ii)
and the asymmetry is deliberate: an unknown model is a place data might *go* (restrict it); an
unknown extension is a place data might *come from* (the operator ruled fail-open).

**Only demotions, never promotions.** One demotion rule, closing a verified hazard: `versa_azure`
reads `AZURE_OPENAI_ENDPOINT` / `AZURE_OPENAI_DEPLOYMENT_NAME` / `AZURE_OPENAI_API_VERSION`
(`versa_azure.rs:106-112`) and the public `azure_openai` provider reads the same three keys
(`azure.rs:115-118`). `versa_bedrock` falls back to `AWS_ENDPOINT_URL_BEDROCK_RUNTIME`
(`versa_bedrock.rs:113`), which `bedrock.rs:92` sets **process-globally** with
`std::env::set_var`. So `versa_*` demotes to Public when its resolved endpoint host is not the
compiled-in UCSF gateway (`unified-api.ucsf.edu`), computed at construction when the endpoint is
already resolved.

**Never keyed on a model id.** `us.anthropic.claude-opus-4-8` appears in both
`BEDROCK_KNOWN_MODELS` and `VERSA_BEDROCK_KNOWN_MODELS`. Any model-name badge is wrong by
construction.

**Composites.** `providers::create` intercepts *before* the registry lookup when
`BIOROUTER_LEAD_MODEL` is set (`factory.rs:139-149`), so `create("ollama", …)` can hand back a
composite whose lead is `anthropic`. Tier must always be read off the **constructed instance**.

### 5.2 Sessions

Two columns, added on the line after `diverged_from TEXT,` in the fresh-DB DDL:

```sql
privacy_tier   TEXT NOT NULL DEFAULT 'public',
privacy_reason TEXT
```

`privacy_reason` is audit and UX only, never read by a gate: `turn:versa_azure`,
`mcp:ucsfomopagent`, `inherited:<parent_id>`, `diverged:<parent_id>`, `backfill:<provider>`,
`declassified_by_user`.

**The read fails closed, loudly.** The tree's convention for optional columns is
`row.try_get(…).ok().flatten()`, and BR-71's own plan warns that a missed SELECT "compiles and
silently reads `None`". So:

```rust
privacy_tier: row.try_get::<String,_>("privacy_tier")
    .map(|s| Classification::from_str(&s))
    .unwrap_or_else(|_| { error!("privacy_tier missing from projection"); Classification::Private }),
```

Defaulting to Private paints every session with a Private badge — immediately visible, immediately
fixed, and safe meanwhile. Both `Session`-building projections (`get_session`,
`list_sessions_by_types`) must name the column, each with a unit test asserting a known-public row
does not come back defaulted.

### 5.3 MCP extensions

`tier: ProviderTier` on `struct Extension`, stamped once at admission in `add_extension` (`:532`),
`add_client` (`:737`) and `add_inprocess_server` (`:759`).

On the **record**, not on `ExtensionConfig`, for four reasons each grounded in the tree: it is the
record `get_client_for_tool` already resolves to (`:1033-1040`); `ExtensionConfig` round-trips
through user-writable `config.yaml`, which would make classification locally forgeable and
contradict R11(i); a new field there costs seven match arms plus an OpenAPI cycle; and `pool_key`
carries no session id, so one `ucsfomopagent` child process is shared across sessions — the badge
cannot live on the process.

### 5.4 Knowledge bases

Added by [DR-18](privacy-tiers-execution-plan.md#dr-18--the-knowledge-base-tier-is-user-controllable-and-a-private-session-creates-a-private-base) (R16). A base is not an incidental file — it is *"a piece of biorouter
component"* — so it carries a tier of its own, in a machine-local `<knowledge-root>/.kb-tiers` store
rather than in the session database (`biorouter-mcp` cannot depend on `biorouter`, so it cannot name
`ProviderTier`; the store carries a boolean and the crossing happens one layer up).

Three rules, and the third is the one this design did not have before:

1. **At creation.** A base a **private-capability model** creates is private from birth, before any
   ingest — the tool handler raises it immediately after `create_base` returns. A base a **user**
   creates from the Knowledge view or the CLI starts public: the user is not a model, and inheriting a
   tier from whatever chat happens to be open is the same mis-click hazard §6.1 refuses for sessions.
2. **On ingest.** The base takes the tier of the most sensitive session that has ingested into it, at
   all five choke points, and a public-capability session may neither read nor write a private base.
3. **The user may move it, in both directions, and only the user.** Publicizing is graded — a typed
   confirmation naming how many pages it releases, and the statement that it cannot be undone for
   content already read. Privatizing is one click, because nothing is disclosed by it. The control
   uses the **same** proof-of-user as §12, not a second mechanism, and there is no MCP tool that sets
   a tier. [Task 29A](privacy-tiers-execution-plan.md#task-29a-knowledge-base-publicize--privatize--user-only-graded-audited).

---

## 6. The ratchet

### 6.1 Two triggers, neither of them the bind

Binding a model is not a disclosure. Ratcheting at bind time is wrong in both directions:

- **Ergonomically**, a user who opens the picker, selects Versa and immediately reselects Claude
  has permanently privatised a chat that never sent a byte to a private model, with only an
  irreversible declassification as the exit.
- **Structurally**, it misses a hole. `POST /agent/call_tool` never touches the reply path, so a
  private-bound session could pull clinical data through `ucsfomopagent` — permitted, capability is
  Private — with `privacy_tier` still `public`, then legitimately rebind to a public model.

So classification moves at the two moments content actually appears:

1. **Reply entry (Gate B).** Before a turn runs,
   `privacy_tier := max(privacy_tier, floor(tier(bound provider)))`, reason `turn:<provider>`.
   `Agent::reply` is the single turn entry for every surface — GUI, CLI, subagent, Agent Drafter,
   scheduler, ACP.
2. **Permitted private-extension dispatch (Gate C).** On a successful dispatch to an extension
   whose `tier == Private`, `privacy_tier := max(privacy_tier, Private)`, reason `mcp:<name>`.

`Agent::update_provider` keeps **only** the refusal.

### 6.2 Persisted so it cannot be reversed

The session update builder emits plain `col = ?` through its `add_update!` macro
(`session_manager.rs:2876-2880`). Eight lines above it the same function already contains the
precedent, verbatim (`:2852-2854`):

> `// Additive, atomic accumulation. Emitted as `col = COALESCE(col,0) + ?` so a concurrent turn on the same session cannot lose an update.`

The ratchet copies that shape:

```sql
privacy_tier = CASE WHEN privacy_tier = 'private' THEN 'private' ELSE ? END
```

**This is the load-bearing line of the whole design.** No caller anywhere — a route handler, a CLI
command, a test, a future BR-71 tool, a hand-written query through the builder — can lower the
tier, whatever it passes. The storage layer refuses, not the caller. Concurrency is safe in both
orderings. Auditing "can the ratchet be reversed" is reading one SQL fragment. There is
deliberately no builder setter that accepts `Public`.

### 6.3 Irreversibility across every carrier

| Carrier | Mechanism |
|---|---|
| Restart / resume | Column plus `restore_provider_from_session` passing Gate A; Gate B repairs or refuses |
| Fork / branch / diverge / copy / import | **All four** builders carry `privacy_tier`, `provider_name`, `model_config` — see §9.3, this is a bug fix |
| Spawn | Stamped in `create_subagent_session`'s INSERT, same statement as BR-71 Task 32's `parent_session_id` |
| Scheduled job | The fresh `Scheduled` session inherits the creating session's tier at creation |
| Export / import | Absent `privacy_tier` on import ⇒ **`private`**, not the column default. Read the field only in the raising direction — `max(private_default, imported)` — never as authority to set `public`. |
| DB copy / manual SQL | The column travels with the file; a repo-grep test asserts exactly one statement in the tree matches `privacy_tier\s*=\s*'public'` outside the migration, and that statement is `declassify_session` |

### 6.4 What deliberately does not raise it

- A mixed lead/worker composite (`least = Public`, so `floor(Public) = Public`). Flagged for ruling.
- A private MCP call from a public-capability session — Gate C refuses, so there is no "it
  happened, now ratchet" path.
- Reading a public session into a private one. Upward flow is safe.
- Agent Drafter's transient per-turn switch-and-restore in `apply_route_for_turn`: it writes
  `provider_name` twice per turn, but the ratchet keys on the tier of the provider a turn *runs
  under*, not on the column changing.

---

## 7. The capability matrix

**Inputs.**

- **C** = `capability(caller)` = `least` over the components of the caller's currently-bound
  provider. `Pub` | `Priv`.
- **T** = `target.privacy_tier`, the stored classification. `Pub` | `Priv`.
- **L** = lineage of target relative to caller: `self` · `child`
  (`target.parent_session_id == caller_session_id`) · `other` (includes NULL parent and every
  transitive descendant).

**Lineage is one hop.** A grandchild is `other`: R6 says "sessions the caller *did* spawn", and a
grandchild was spawned by the child. BR-71's `workspace_list { parent_session_id: "<me>" }` filter
already yields exactly the one-hop set, so no recursive CTE and no new "control my subtree" surface
is invented. A leader that needs deeper control asks its child.

**The three rules.**

```
VIS(T)     ⇔  T ≤ C                      // a public caller sees public only
READ       ⇔  VIS                        // any lineage — R6's read-only floor
WRITE      ⇔  VIS ∧ L ∈ {self, child}
BIND(P→T)  ⇔  WRITE ∧ tier(P) ≥ T        // Gate A, evaluated on the target
```

A downgrade write (C=Priv, T=Pub) is **permitted**, with a first-use approval. R4 explicitly
permits a private session to spawn public children, and a rule that lets you spawn a public child
but never send it a prompt makes the permission useless — it would forbid exactly the
private-leader/public-worker arrangement R2 names. But the prompt text *is* private-origin content
crossing into a public model, so the first `workspace_send_prompt` / `workspace_set_tools` from a
given caller into a given public target raises an approval showing the exact payload.

**The matrix.** `✓` allowed · `✓!` allowed, first crossing requires approval showing the payload ·
`✗` refused with a teaching message · `∅` omitted from results entirely. `self` and `child` behave
identically under every rule and are merged; columns D and F prove it.

| BR-71 tool | Class | **A**<br>C=Pub T=Pub<br>self/child | **B**<br>C=Pub T=Pub<br>other | **C**<br>C=Pub T=**Priv**<br>any L | **D**<br>C=Priv T=Pub<br>self/child | **E**<br>C=Priv T=Pub<br>other | **F**<br>C=Priv T=Priv<br>self/child | **G**<br>C=Priv T=Priv<br>other |
|---|---|---|---|---|---|---|---|---|
| `workspace_list` | read | ✓ | ✓ | **∅ row omitted** | ✓ | ✓ | ✓ | ✓ |
| `workspace_read_conversation` | read | ✓ | ✓ | ✗ | ✓ | ✓ | ✓ | ✓ |
| `workspace_watch` | read | ✓ | ✓ | ✗ | ✓ | ✓ | ✓ | ✓ |
| `workspace_open` *(existing session)* | read | ✓ | ✓ | ✗ | ✓ | ✓ | ✓ | ✓ |
| `workspace_send_prompt` *(turn / steer / note)* | write | ✓ | ✗ R6 | ✗ | **✓!** | ✗ R6 | ✓ | ✗ R6 |
| `workspace_set_tools` — extensions / skills / KBs | write | ✓ | ✗ R6 | ✗ | **✓!** | ✗ R6 | ✓ | ✗ R6 |
| `workspace_set_tools` — `add_extensions` naming a **private** extension | write | ✗ target is public-capability | ✗ | ✗ | ✗ | ✗ | ✓ | ✗ R6 |
| `workspace_set_tools` — `{ provider, model }` | bind | ✓ if `tier(P) ≥ Pub` (always) | ✗ R6 | ✗ | **✓!** if `tier(P) ≥ Pub` | ✗ R6 | ✓ **only if `tier(P)=Priv`** | ✗ R6 |
| `workspace_close` | write | ✓ | ✗ R6 | ✗ | ✓ | ✗ R6 | ✓ | ✗ R6 |
| `workspace_spawn_subagent` / `workspace_open { new: … }` | spawn | see §8.2 | | | | | | |

**`workspace_list` omits private rows rather than redacting them.** The operator ruled existence
leaks *acceptable*, not *required*, and omission is strictly simpler: a `workspace_list` row
carries a **title**, and a session title in this product is LLM-generated from the conversation,
i.e. content. Redacting would mean a redaction pass to review; omitting is one `WHERE` clause and
removes the temptation to then call `workspace_read_conversation` on the id.

### 7.1 Non-BR-71 surfaces the same matrix governs

| Surface | Rule |
|---|---|
| `chatrecall` SEARCH | VIS as a SQL predicate (Gate D) |
| `chatrecall` LOAD (`session_id`) | VIS, refused before any text is built |
| Any tool on a private MCP extension | `extension.tier ≤ C` (Gate C) |
| MCP `read_resource`, `read_resource_tool`, `list_resources`, `get_ui_resources` | Same as Gate C |
| MCP `list_prompts`, `list_prompts_from_extension`, `get_prompt` | Same as Gate C |
| Tool *discovery* (`filter_tools`) | Private extensions absent from a public model's tool list |
| `active_work` registry (`ActiveWorkItem.title` is built from a subagent's task prompt) | VIS |
| `GET /sessions`, the History list | **Not** filtered — this is the user's own UI, not a model. Badges instead. **Conditional on §9.1 being fixed;** see the bypass. |
| Agent Drafter route / worker admission | The BIND predicate, replacing `provider_class` |

### 7.2 Delegation is not amplification

A private parent may spawn a public child (R4). The child is Public, sees only public sessions, and
cannot call private extensions — because VIS is evaluated against the **child's own** capability,
never its parent's. A public parent cannot spawn a private child at all, so it can never mint a
private-capability agent to read from; and even if it held one, `workspace_read_conversation` on
that child would be C=Pub, T=Priv → refused.

---

## 8. Inheritance and spawn

### 8.1 R5 is already the implemented behaviour — it needs a gate and a test, not a mechanism

`Agent::dispatch_tool_call` builds the child's config from `self.provider().await` — the parent's
**same `Arc<dyn Provider>`**, not a name and not a config. `apply_settings_overrides`
(`subagent_tool.rs:756-793`) is entered only when
`settings.provider.is_some() || settings.model.is_some() || settings.temperature.is_some()`. With
no settings, the parent's provider instance passes through untouched.

| Thing | Inherited today | Added by this design |
|---|---|---|
| Provider **instance** — hence tier, model, temperature, context limit, and the lead/worker composite | yes, same `Arc` | — |
| `AgentConfig`, including `BioRouterMode`, session manager, permission manager | yes, cloned | — |
| Extension configs — **all of them** | yes | **filtered to `ext.tier ≤ child_tier`** |
| Working directory | yes | — |
| Reasoning effort | no, deliberately (BR-63) | — |
| Knowledge bases | no | — |
| `privacy_tier` | — | **yes, stamped in `create_subagent_session`'s INSERT** |
| `parent_session_id` | — | BR-71 Task 32, moved into the same INSERT |

The extension filter closes a live hole: the parent's full extension set is snapshotted into
`TaskConfig`, `subagent_handler.rs` re-adds every one to the child, and `apply_settings_overrides`
narrows only by **name** (verified: `task_config.extensions.retain(|ext| extension_names.contains(&ext.name()))`),
never by tier — so today a session holding `ucsfomopagent` can spawn a public-model child that
inherits it verbatim.

**"The same worker/leader mode the parent is operating under" has no session-level referent in the
codebase.** `grep -rni '\bleader\b' crates/ --include="*.rs"` returns only process-group leaders.
The clause is satisfied by the `Arc` inheritance itself: because `TaskConfig.provider` is the
parent's *same* `LeadWorkerProvider` instance, a child of a lead/worker parent runs the identical
composite with the identical split, literally rather than by copying settings. Nothing to design,
nothing to persist. One consequence to write down so a future reader does not "fix" it: sharing the
instance also shares its mutable `turn_count` / `failure_count` / `in_fallback_mode`, so a
subagent's turns advance the parent's lead→worker transition. Pre-existing and orthogonal — but
cloning the wrapper to fix it would split the tier computation.

### 8.2 The spawn matrix

| Parent capability | Requested child model | Verdict | Child's `privacy_tier` |
|---|---|---|---|
| Priv | *default — inherit the parent's `Arc`* | ✓ no prompt | `private` (`inherited:<parent>`) |
| Priv | explicit Private | ✓ | `private` |
| Priv | explicit **Public** | **✓! approval** showing the task prompt | **`public`** — not the parent's |
| Pub | *default — inherit* | ✓ | `public` |
| Pub | explicit Public | ✓ | `public` |
| Pub | explicit **Private** | **✗ hard refusal** — R4: a public session may not gain private reach | — |
| any | child would inherit a private extension its own tier cannot call | ✓, extension **dropped** from the child's set, drop reported in the tool result | — |

The downgraded child is created **Public, not inheriting the parent's Private** — otherwise it
would be born in the stuck residual state. It receives only the task prompt, none of the parent's
history, and none of the parent's private extensions. That is why the confirmation shows the
prompt: the prompt is the entire disclosure.

```rust
if settings.provider.is_some() || settings.model.is_some() || settings.temperature.is_some() {
    …existing construction via providers::create…
    let child_tier = task_config.provider.tier();   // the INSTANCE, post-construction
    let parent_cap = parent_provider.tier();        // least() over a composite

    if child_tier == Private && parent_cap == Public {
        return Err(PrivacyRefusal::spawn_upgrade(…));           // R4: hard refusal
    }
    if child_tier == Public && parent_cap == Private {
        task_config.requires_downgrade_confirmation = true;     // R4 permits; disclose
    }
    task_config.privacy_tier = floor(child_tier);
    task_config.extensions.retain(|e| ext_tier(e) <= child_tier);
}
```

**Validated on the constructed instance, not the requested name.** `providers::create` can return
something other than what was asked for (the `BIOROUTER_LEAD_MODEL` intercept), and when only
`settings.model` is given, today's code keeps the parent's `provider_name` and swaps the model
string — and `versa_azure`, `versa_bedrock`, `ollama`, `llamacpp` and every declarative provider
are `allows_unlisted_models`, so an arbitrary model string is accepted. Both are harmless because
the tier is a property of the instance and never of the model id.

### 8.3 Agent Drafter workers violate R5 today

`configure_worker_provider` has no branch that reads the main agent's provider — an unpinned
profile falls through to `Config::global()`, so a worker under a `versa_azure` app runs on the
user's commercial default. Add main-agent inheritance before the global fallback, and extend the
existing §3.7 check (which today inspects only an explicit pin) to cover the fallback path.

---

## 9. Enforcement

### 9.1 Five gates

**Gate A — bind · `Agent::update_provider` (`agent.rs:4936-4956`).** Check `tier(incoming)` against
the target row's `privacy_tier` atomically, in the `WHERE` of one conditional `UPDATE`:

```sql
UPDATE sessions
   SET provider_name = ?, model_config_json = ?, updated_at = datetime('now')
 WHERE id = ?
   AND (privacy_tier = 'public' OR ? /*incoming_is_private*/ = 1)
```

`rows_affected == 0` ⇒ `Err(PrivacyRefusal::PublicModelOnPrivateSession)`. Two properties follow.
**No TOCTOU:** a concurrent ratchet cannot interleave into "private session, public provider
bound". And **the in-memory swap moves after the successful UPDATE**, inverting today's order
(verified: `*current_provider = Some(provider);` at `:4945` precedes the persist at `:4947-4955`),
so a refused swap leaves the chat running on the model it already had.

Refusing at the bind rather than the turn matters because it is the only point that acts before
private history is in front of a public endpoint; refusing at the turn means the decision is
already serialised into `sessions.provider_name` and every later reader sees a public provider on a
private session. The invariant becomes one checkable sentence: **the provider bound to a private
session is always private.**

Every path terminates here, because `update_provider` is the sole writer:

| Path | Site |
|---|---|
| GUI model picker | `ModelAndProviderContext.tsx` `changeModel` → `POST /agent/update_provider` |
| HTTP route | `routes/agent.rs:685-729` |
| Session bootstrap | `ui/desktop/src/utils/providerUtils.ts` `initializeSystem` |
| Resume / restart | `restore_provider_from_session` (`agent.rs:4960-4986`, ends in `self.update_provider`) |
| CLI configure + session builder | `biorouter-cli/src/commands/configure.rs`, `session/builder.rs` |
| Scheduler | `crates/biorouter/src/scheduler.rs:866` |
| Subagent spawn (inherit and override) | `subagent_handler.rs` |
| **Agent Drafter browser `ClientFrame::ModelSelect`** | `routes/apps.rs:3409-3428` — fixed with zero new code |
| Agent Drafter per-turn route + restore, worker profiles | `apps.rs` |
| ACP | `biorouter-acp/src/server.rs` |
| **BR-71 `workspace_set_tools { provider, model }`** | required to call `Agent::update_provider`, not reimplement the persist |

**Gate B — turn · top of `Agent::reply`.** It owns the residual state and is **repair-first**:

1. Load the session row. If `privacy_tier ≤ tier(bound provider)` → ratchet if needed, continue.
2. Else, if `session.provider_name` names a provider whose tier satisfies the classification →
   **rebind from the row and continue silently.** This turns LRU rehydration, the global-config
   fallback and legacy rows into no-ops the user never sees.
3. Else → refuse **this turn** with the repair card.

`Agent::provider()` (`agent.rs:2017-2022`) already returns `Result<Arc<dyn Provider>>` — it returns
`Err(anyhow!("Provider not set"))` today — so it can carry a second, non-DB assertion for the
completion paths that do not pass `reply`, notably `complete_fast` summarisation and session
naming, which read the entire transcript. The assertion compares two enums against a cached
`Agent.session_classification`, written by Gates A and B; `AgentManager` caches one `Arc<Agent>`
per session id, so the cache is sound and re-syncs from the row at every reply entry.

**Gate C — dispatch · `ExtensionManager::dispatch_tool_call` (`extension_manager.rs:1288`).**
Placed immediately beside the BR-23 SecretGuard block, whose in-code comment at `:1351` states
verbatim the reason this location was chosen for exactly this class of rule: *"Enforce it here —
the single choke point every tool call flows through — so no extension can bypass it."*

```rust
let ext_tier = self.extensions.lock().await.get(&client_name).map(|e| e.tier).unwrap_or(Public);
if enforcement_on() && ext_tier == Private && self.capability_tier().await == Public {
    return Err(ErrorData::new(ErrorCode::INVALID_REQUEST, teaching_message(&client_name), None).into());
}
```

By that point the function already has `client_name` from `get_client_for_tool`, `session_id`, and
the manager's `provider: SharedProvider` — **the same `Arc` the Agent swaps** (verified:
`Agent::new` passes `provider.clone()` to `ExtensionManager::new`), so a mid-session model change
is visible on the very next dispatch with zero new plumbing and no TOCTOU window.

**Returning `ErrorData` directly, not routing through an inspector, is deliberate.**
`handle_denied_tools` (`agent.rs:1961-2000`) matches on `inspector_name`, and only the hook
inspector, `"security"` and the repetition inspector get their real reason through — everything
else falls to `DECLINED_RESPONSE` ("The user has declined to run this tool"), which the code itself
calls "actively misleading". Gate C sidesteps that: the message reaches the model verbatim.

**Read off the resolved `Extension` record, never off the tool-name string.** `get_client_for_tool`
(`:1033-1040`) routes by `prefixed_name.starts_with(*key)` over a `HashMap` in nondeterministic
order, and `normalize()` permits `_`, so extensions keyed `a` and `a__b` make `a__b__c` ambiguous.
It mostly fails closed via the `strip_prefix("__")` error, but "mostly fails closed" is not an
argument for a security boundary.

Three call paths converge on this function and only one passes an inspector — which is exactly why
Gate C is not an inspector: the agent loop; `POST /agent/call_tool`; and the `execute_code` JS
bridge (`code_execution_extension.rs:1815`, dispatching inner calls straight to the manager with
the extension name as a first-class value — no static-analysis fudge needed, unlike
`SensitiveOpsInspector`).

**Gate C's siblings.** `dispatch_tool_call` is a complete choke point for *tool calls*, not for
*reaching an MCP server*. Each takes the same one-line check: `read_resource` (`:1116`) and
`read_resource_tool` (`:1043`, the worst — with no `extension_name` it loops **every** installed
extension trying the URI, actively probing private servers on the model's behalf);
`list_resources` (`:1226`); `get_ui_resources` (`:1153`); `list_prompts_from_extension` (`:1428`),
`list_prompts` (`:1458`, fans out over all) and `get_prompt` (`:1505`), since an MCP prompt body is
server-authored text that can carry data.

**Gate D — query · `crates/biorouter/src/session/chat_history_search.rs`.** One SQL literal per
builder. Detailed in §11.

**Gate E — discovery · `filter_tools` (`extension_manager.rs:877-902`).** Not a veto; the reason a
public model never sees a private server's tool names, descriptions or JSON schemas in its system
prompt. It must be here and not in `fetch_all_tools` (`:940`) or `get_all_tools_cached`
(`:904-933`): the cache is keyed on a version counter bumped only by extension add/remove, and
`update_provider` does not bump it, so filtering upstream would freeze one model's allowed set
across a mid-session model swap. Covers `get_prefixed_tools`, `get_prefixed_tools_excluding`, and
therefore `code_execution`'s importable-module catalogue for free.

### 9.2 Why no path escapes

⚠ **Read this table as "every path *through BioRouter*", not "every path".** [DR-17](privacy-tiers-execution-plan.md#scope-ruling--dr-17-narrows-this-plan-to-the-session-store) descoped
the filesystem barrier, so three escapes are **not** covered and are named here rather than left to be
discovered:

| Not covered (accepted, and disclosed under R15) | Why |
|---|---|
| `cat ~/.local/share/biorouter/sessions/sessions.db` from `developer__shell` | There is no read-deny in v1. The session store is an ordinary file to a tool that runs an arbitrary command. |
| Reading any file a private session wrote outside BioRouter's own stores | Working files, outputs, exports and artifacts are not tracked and carry no tier. |
| A local caller that holds the daemon secret calling `GET /sessions/{id}/export` | The secret is recoverable from the daemon's own environment; `check_token` has no principal. §17 carries the fix. |

**What the table below does cover** is the agent-mediated, transcript and tier-escalation paths — the
three §1 names — and each row is exact:

| Would-be escape | Covered by |
|---|---|
| Swap the model from GUI / CLI / HTTP / ACP / scheduler / app page / BR-71 tool | Gate A (the 11-row table above) |
| Reach a turn with a mismatched provider — LRU rehydrate, global fallback, diverge, legacy row | Gate B |
| Summarise or auto-name a private transcript on a public model | Gate B's assertion in `Agent::provider()` |
| Call a private tool from the agent loop | Gate C |
| Call a private tool from `POST /agent/call_tool`, skipping every inspector | Gate C |
| Call a private tool from inside `execute_code`'s sandbox | Gate C |
| Read a private server's **resources** or **prompts** instead of calling a tool | Gate C's siblings |
| Learn a private server's schemas without calling it | Gate E |
| Recall private chat text | Gate D + the LOAD-mode check |
| Read another session via BR-71 | The matrix, in each tool handler |
| Spawn a private child from a public parent | `apply_settings_overrides` |
| Inherit a private extension into a public child | The `TaskConfig` extension filter |
| Diverge / copy / import to launder the transcript | All four builders carry the tier (§9.3) |
| Lower the tier through any DB write | The monotone `CASE WHEN` |
| Reach a private server through an Agent Drafter app | Already impossible — a manifest can only produce `Builtin`/`Platform` configs, so `Builtin { name: "ucsfomopagent" }` fails the `BUILTIN_EXTENSIONS` lookup. **Locked in with a test**, not added. |
| Disable enforcement from the agent | The opt-out is not env-readable and is gated in `/config/upsert` — but see §9.3 C1, SecretGuard alone is not sufficient |

**Two hazards the implementation must respect.** `ToolInspectionManager::inspect_tools` logs and
continues when an inspector returns `Err`, so any privacy check expressed as an inspector must
encode every unknown as `Deny` *inside* `Ok(...)` and never return `Err`. And BR-71 Task 10's own
`Deny` branch still needs its inspector name registered in `handle_denied_tools`
(`agent.rs:1977-2000`) or every cross-session refusal is reported to the model as a user decline
and retried. One line, very easy to omit.

### 9.3 Bypasses the five gates do not cover, found in adversarial review

These are **required work**, not follow-ups. Two of them would let a public model read private
content on day one of shipping the gates as designed.

**A1 (critical) — the daemon's auth secret is in the agent's own environment.** From any
public-model session with `developer`:

```
printenv BIOROUTER_SERVER__SECRET_KEY BIOROUTER_PORT
curl -s -H "X-Secret-Key: $KEY" http://127.0.0.1:$PORT/sessions/<private-id>/export
```

returns the private session's entire transcript as JSON. Verified: `ui/desktop/src/biorouterd.ts:134-135`
puts `BIOROUTER_PORT` and `BIOROUTER_SERVER__SECRET_KEY` into `additionalEnv`, spread into
`processEnv = {...process.env, ...additionalEnv}` at `:149-151`. `developer` is a `Builtin`, so
`shell` runs in-process inside `biorouterd`, and `configure_shell_command`
(`crates/biorouter-mcp/src/developer/shell.rs:395-442`) sets `PATH`, `GIT_EDITOR` and friends.
Stdio extensions inherit too (`extension_manager.rs:846-848`:
`command.args(args).envs(all_envs)`, no `env_clear`). `auth.rs:115-126` is a plain header equality;
rate limiting is keyed on peer IP, which is `127.0.0.1`. `routes/session.rs:769/771/772` expose
`GET /sessions/{id}`, `/export` and `POST /sessions/import`.

This defeats the design specifically: Gate D filters `chatrecall`'s SQL and this route never
touches `chat_history_search.rs`; Gate B guards `Agent::reply` and this is a read. The
`UserConfirmation` ZST (§12) is true and irrelevant — the agent does not construct one, it calls
the handler that does, and `GET /sessions` yields the session name the typed confirmation wants.
It also falsifies the design's premise that `GET /sessions` need not be filtered because "this is
the user's own UI, not a model."

**Fix (1) is half done.** Issue #57 landed the daemon-credential scrub on the shell path: the
builder cited above now ends with `strip_daemon_private_env(&mut command_builder);`
(`shell.rs:433`, with the comment at `:432` — "Last, so nothing set above can re-admit a daemon
credential (issue #57)"), and `developer/background.rs` calls it at `:431`, `:680`, `:766`, `:802`
and `:847`. The helper is `crates/biorouter-sandbox/src/environment.rs:54-79`, keyed on
`is_daemon_private_env_key` (`:36-50`), and its own test asserts `BIOROUTER_SERVER__SECRET_KEY` and
`BIOROUTER_ACP_WS_TOKEN` are stripped while `BIOROUTER_PORT` survives (`:94-103`).
**The stdio MCP extension spawn is still open**: `extension_manager.rs:846-848` does
`command.args(args).envs(all_envs)` with no `strip_daemon_private_env` and no `env_clear`
(`grep -c env_clear` over that file returns 0). The remaining work is one line calling the existing
helper — not a new `.env_remove`.

Three fixes, all needed: (1) call `strip_daemon_private_env` in the stdio spawn as the shell path
already does — a live credential leak today, worth doing regardless
of this design; (2) stop carrying the secret in the environment at all — pass it on a pipe/fd at
startup, or a `0600` file the daemon reads and unlinks (`BIOROUTER_PORT` may stay); (3) bind
declassify to a one-shot capability token minted by the renderer, not to `X-Secret-Key`, or R9's
"only a human" property is documentation rather than mechanism.

**A2 — `sessions.db` is a plain file.** `sqlite3 <db> "select text from messages_fts"` returns
every message of every session, no JSON parsing needed, because `messages_fts` is a **contentful**
FTS5 table (DDL comment at `session_manager.rs:33-47` says exactly why). `DEFAULT_SECRET_PATTERNS`
(`secret_guard.rs:33-45`) covers `**/.env`, `**/secrets.*`, keys and `**/.aws/credentials` — no
session DB. **Correction to the reviewer's version of this finding:** the DB is under
`Paths::data_dir()`, i.e. `~/.local/share/biorouter/sessions/sessions.db` on this machine — *not*
`~/.config/biorouter/`, which holds only `config.yaml` and friends. A pattern written against
`.config` would not match.

**This is bigger than `sessions.db`, and the fix is not a SecretGuard pattern.** The same reasoning
reaches the knowledge tree, the global memory store and the Agent Drafter apps: all are files, all
are readable by any tool that can run a command or open a path, and none of the five gates is on
that path. `developer__shell` executes an arbitrary command (`rmcp_developer.rs:1307`) and is
explicitly *not* jailed by the file tools' containment base (`:1950`); the OS sandbox that could
confine it defaults to **off** (`shell_sandbox/mod.rs:244`);
`computercontroller__automation_script` writes and executes a model-supplied script
(`computercontroller/mod.rs:833`). **So a public-capability model does not need to defeat any gate
in §9.1 — it can read the private material directly.**

Adding `**/sessions.db*` and the data directory to `DEFAULT_SECRET_PATTERNS` was this design's first
answer and is **rejected**, for two reasons that only appear on measurement. That list is an
always-on floor applied to *every* session, so it would hide the user's own chat history and
knowledge base from a **private** session too — a cost nothing in R1–R14 asks for. And it would not
close the read in any case: `candidate_is_denied` (`secret_guard.rs:278-292`) is lexical and
existence-gated, so a computed path or a shell expression walks past it. Stated honestly, it raises
the cost and does not close the read.

**The answer is §9.5** — a read-deny on the tools, conditioned on the session's capability. Note
which half of §9.5 answers which half of the objection: the *capability-conditional* part is what
makes it scoped rather than an always-on floor, and the *in-process barrier at the dispatch choke
point* is what makes it a barrier rather than a cost increase, because it evaluates the argument the
tool was actually given instead of pattern-matching a string. The OS sandbox behind it covers the one
case an in-process check cannot: a shell that constructs the path at runtime, after the daemon has
already handed over the command. Adding the patterns anyway remains worthwhile as defence in depth
for the credential half, but it is not the control.

**B1 — three copy paths, not one.** `copy_session` (`:4138-4168`) is called only by
`diverge_session_for_edit`. The **primary GUI diverge** is `diverge_session` (`:4204-4265`), which
does *not* call it and has its own builder
(`.extension_data().schedule_id().workflow().user_workflow_values().user_provided_name().diverged_from().branch_point_msg_uid()`);
`import_session` (`:4096-4135`) is a third. All three write the conversation via
`replace_conversation` and none carries `provider_name`. Reachable from `routes/session.rs:784`,
the CLI `/diverge`, and `biorouter-cli/src/commands/session.rs`. **Put the carry-over on
`create_session` itself, parameterised**, rather than on each caller's builder — three hand-rolled
builders is three chances to miss one and the fail direction is open. Add a test enumerating every
`create_session` call site.

**B3 (was critical; now a narrower channel) — `memory`'s global store.** Issues #58 and #63 both
landed in this branch's base, and between them they closed the two channels this section was written
about.

*The prompt-injection half (#58).* `MemoryServer::new`
(`crates/biorouter-mcp/src/memory/mod.rs:489`) calls `compose_instructions` (`:524`, defined at
`:632`), and global memories are now **index-only**: the global half reads
`category_names(true)` (`:641`) — which enumerates directory entries and *never opens a body*, so
the bound holds by construction rather than by convention — and contributes only sorted **category
names** under `GLOBAL_INDEX_HEADER` (`:342`), emitted at `:676`. Each name is validated as a label
and rendered as a JSON string literal, i.e. as data, so it cannot forge a prompt line. The listing
is further gated on `GlobalMemoryConsent`: a server with no consent path (`Unavailable`, `:642`)
lists nothing at all.

*The tool-call half (#63).* `retrieve_memories` (`:1200`) now calls
`require_global_consent_path` (called at `:1205`, defined at `:815`) and then refuses
`category == "*" && is_global` outright (`:1221-1234`), backed by a pre-dispatch gate in
`crates/biorouter/src/security/global_memory.rs`. The whole store stays reachable one
user-approved category at a time; it can no longer be drained in one call.

*What remains for #56* is stated in the module's own doc comment (`:627-631`): **the line is drawn
by _store_ — global vs local — not by the sensitivity of the session that wrote the entry.** Local
memories are still inlined in full (`:690-702`), so a sensitive note a private session saved locally
reaches the system prompt of every session later opened in that directory, with no tool call for
Gate C or Gate E to see. The v1 fix is therefore not a `retrieve_all` filter and not a second
consent prompt: it is to refuse `memory__remember_memory { is_global: true }` from a
**private-capability** session, which needs no storage change and is the exact mirror of Gate C.

**B4 — knowledge bases are an unclassified shared sink.** `knowledge` is a built-in ⇒ public.
Storage is a global tree `~/.config/biorouter/knowledge/<kb-id>/`, and any session may name any KB.
Gate C never fires, because both sessions are calling a public extension. **Correction to the
reviewer's version:** the active-KB state is no longer purely global — `paths.rs:66-71` adds
`.active-kb-sessions`, one file per session, with `.active-kb` as the primary fallback. So a public
session does not silently *inherit* the private session's KB by default; it does still reach it by
naming it.

**Ruled (operator, second review round): ratchet.** A knowledge base takes the tier of the most
sensitive session that has ingested into it, and a public-capability session may not read a private
KB. The read side is enforced at the seven entry points that accept an explicit `kb_id` and
therefore bypass the visible-set logic — `kb_search` (`knowledge/server.rs:590-592`),
`kb_search_raw_sources` (`:618-619`), `kb_export` (`:743`), and the four that route through
`kb_id_or_primary`, whose doc comment states the bypass outright ("An explicit `kb_id` always wins
and is never filtered against the session's set", `:308-311`): `kb_list_pages` (`:379`),
`kb_read_page` (`:396`), `kb_get_graph` (`:482`) and `kb_list_history` (`:497`). The tier lives in a
machine-local sidecar beside `.active-kb` and `.hidden-kbs`, not in `manifest.yaml`, because the
manifest travels inside the `.brkb` archive and an imported tier would be attacker-supplied.
Existing knowledge bases migrate **public** (fail-open, DR-10) even if a private session fed them —
an accepted cost, recorded as
[AR-2](privacy-tiers-execution-plan.md#accepted-risks). The way back out of the ratchet is
**user-only**: DR-18 gives the user a publicize/privatize control (Task 29A), which is what resolved
the original AR-1 — a base ratcheted by one private page is *not* unreadable for ever. No model can
invoke it, in either direction.

**B4.1 — the selection itself is content, so a public session sees a filtered view of it and its
pointer can read `null`.** A knowledge base's **id and name are user-authored** and routinely name a
cohort or a study, which is the same reasoning §7 uses to make `workspace_list` *omit* private rows
rather than redact them. That rule has to reach the pointer, not just the bases: `kb_get_active`
takes **no arguments** and returns the whole selection — every visible id, plus the primary and its
deprecated `active_kb` mirror — so one call with nothing to guess enumerates what `kb_list_bases`
omits. So a public-capability session sees `knowledge_bases` with the private ids removed, and
`primary_kb` reads `null` whenever the stored pointer names a base it may not reach. That is
truthful for that session rather than a redaction: it has no write target it can use, and its
KB-less writes correctly fail with the existing "no primary" message. `kb_set_active` refuses a
private target with the sentence a *nonexistent* id gets, byte for byte — a refusal that said
"private" would confirm the base exists in a politer sentence.

**The filter is on the view and never on the store**, and that is a decision rather than an
implementation detail. The stored pointer is what the "one axis, one pointer" repair logic reads;
that logic promotes a primary to the first remaining member of the set and *writes it*. Filtering
the store would therefore make a public model's read silently re-point the user's own primary,
machine-wide and persisted, and the Knowledge view would then show the moved pointer with nothing
having asked for it. One truth on disk; two model-facing tools render a capability-scoped
projection of it.

**B5 — the composite misclassifies the backfill in the leaky direction.** `get_name()` returns the
lead's name (`lead_worker.rs:332-334`) and `update_provider` persists exactly that string. With
`BIOROUTER_LEAD_PROVIDER=anthropic` over a `versa_azure` base, every worker turn runs on the
private gateway while `sessions.provider_name` records `"anthropic"` — so the backfill marks it
public and a public model may read it forever. The mirror case is an annoyance rather than a leak
(lead=`versa_azure`/worker=`anthropic` backfills private, then Gate A permanently refuses the very
composite it was running). Fix: persist the composite truthfully (`provider_components: Vec<String>`
or a `lead:X|worker:Y` encoding) and backfill from it; at minimum treat any `provider_name` that is
unresolvable-as-a-composite as `backfill:unknown` rather than public.

**B6 — Gate E cannot see the extension record as specified, and its prefix rule disagrees with Gate
C's.** `filter_tools` is a **sync** `fn` over `&[Tool]` deriving the prefix as
`tool.name.split("__").next()`; `get_client_for_tool` resolves by `starts_with` over a
`tokio::sync::Mutex<HashMap>`. So (1) a tier lookup inside `filter_tools` requires the function to
become `async` or to take a **precomputed allowed-set** from the async caller — the caching
argument for why it must live there is correct, so pass a `HashSet<&str>`; and (2) for an extension
keyed with an embedded `__` the two rules diverge (`a` vs `a__b`), and while Gate C fails closed,
Gate E could **show a private server's tool names, descriptions and JSON schemas** to a public
model. Schema text is content. Derive both from one resolver.

**C1 — SecretGuard cannot stop `shell` from writing `config.yaml`.** `find_denied_path` scans
tool-argument strings and `candidate_is_denied` requires a literal path token that exists on disk.
`cd ~/.config/biorouter && python3 -c "open('config.yaml','a').write(...)"` evades it, as does any
variable indirection. The module's own doc-comment concedes it is "conservative by design". Do not
rely on it for the opt-out: hold the authoritative value in daemon memory from startup and require
the GUI IPC path to change it, or read it from a trusted file using the `managed/trust.rs`
`verify_trusted` pattern — noting `verify_trusted` is a **no-op on Windows**.

**C2 — a scheduled job created from a private session becomes permanently, silently broken.**
`scheduler.rs:844-866` builds its provider from `Config::global()`, creates a fresh `Scheduled`
session, then calls `agent.update_provider`. Under this design that session inherits the creator's
private tier, Gate A refuses the bind, and `run_workflow_job` returns `Err` on every cron tick
forever — with no repair affordance, since a new session is minted per run. Resolve the scheduled
session's provider from the **creating session's** `provider_name` before falling back to global
(which is also what R5 wants), and surface a job-level error in the schedules UI.

**P6 — CLI plan mode ships the entire conversation to a different provider, outside every gate.**
`biorouter-cli/src/session/mod.rs:786-791` and `:881-885` clone the whole message list and hand it
to `get_reasoner()` (`:2260-2290`), which builds a provider from `BIOROUTER_PLANNER_PROVIDER`
falling back to the global default — never touching `Agent::update_provider`, never entering
`Agent::reply`, never recorded on the session row. A documented first-class feature and a complete
private→public transcript leak. **Hard refusal in `get_reasoner`**, not a warning, with a message
naming `BIOROUTER_PLANNER_PROVIDER`.

### 9.4 Attacks that were tried and genuinely fail

Recorded so they are not re-litigated: `execute_code` inner dispatch (hits the real Gate C choke
point with the extension name as a first-class value); `POST /agent/call_tool` (same reason — this
is the correct placement call); a stale provider inside the extension manager (the `SharedProvider`
`Arc` is the same one `update_provider` writes through); a mixed composite calling a private MCP
server (`least = Public` → refused); a public parent spawning a private child then reading its
output; a private parent's public child reading back up (VIS is evaluated on the child's
capability); `workspace_read_conversation` on an ancestor (lineage widens *write*, never *read*);
`chatrecall` SEARCH after Gate D; a stale registry copy downgrading an extension (the union rule
holds).

---

### 9.5 The sixth control — a private-data read-deny for public capability, on by default

> ⛔ **DESCOPED for v1 by operator ruling ([DR-17](privacy-tiers-execution-plan.md#scope-ruling--dr-17-narrows-this-plan-to-the-session-store)), 2026-07-30 — retained, not deleted.**
> *"We don't have to enforce and encrypt every single step along the way. for now."* Everything in
> §9.5 below specifies the **filesystem** channel and is not being built: Layer A (the argument
> barrier), Layer B (the OS sandbox), the four roots and the `config.yaml` entry. The execution plan's
> Tasks 14A–14F carry the same banner and keep the measured platform analysis, which is the expensive
> part and stays true.
>
> **Two things in this section survive the descoping and must not be read as deferred.** (1) The
> **tool channel** for knowledge bases — §9.5.1's third column, "the roots' doors" — is exactly the
> CP1–CP4 barrier the KB gates implement, and [DR-18](privacy-tiers-execution-plan.md#dr-18--the-knowledge-base-tier-is-user-controllable-and-a-private-session-creates-a-private-base) makes it a requirement (R16) rather than a
> defence-in-depth extra. (2) The structural argument for *why* enumerating file readers cannot work
> is the most durable paragraph here, and any future filesystem control must start from it.
>
> **What is deliberately NOT true in v1:** that a public-capability session's tools cannot reach
> BioRouter's private data on disk. They can. See §1 and R15.

**Ruling (descoped — see the banner above).** When a session's capability is **public**, its tools may
not reach BioRouter's own private data. Private-capability sessions are unaffected: this is not a general jail and must not
become one. Everything outside the named entries stays readable and writable, so ordinary work is
untouched.

#### 9.5.1 It is two channels and three enforcement points

There are two ways a read of a private root reaches the disk, and the difference is who supplies the
path. That is a structural fact about the product, not an implementation detail, so it belongs in the
design.

**The filesystem channel — the *caller* names the path.** A tool argument (`text_editor` with
`path: ~/.config/biorouter/knowledge/p.md`), or a shell command line. The name is visible before
anything runs, so it can be refused before anything runs.

**The tool channel — the *handler* supplies the path.** `agent_drafter__read_app` receives an app id
and a relative path, and the store joins the root itself. `knowledge__kb_read_page` receives a base
id and a page. `memory__retrieve_memories` receives a category. **Nothing in the arguments names a
private root**, so a barrier that inspects arguments finds nothing to refuse and the handler opens
the file anyway. Three successive adversarial reviews found a reader the previous round's control
could not see, and the third found this whole class.

| | **Layer A — the argument barrier** | **Layer B — the OS sandbox** | **The roots' doors** |
|---|---|---|---|
| Channel | filesystem | filesystem | tool |
| Covers | every tool call's arguments, at the daemon's own dispatch choke point | the processes the daemon spawns | every read that goes through a root's own resolver |
| Mechanism | a refusal at `ExtensionManager::dispatch_tool_call`, before the tool's future exists | Seatbelt / bubblewrap wrapping the child | a capability-taking guard inside `ArtifactStore`, the knowledge service's choke points, and the memory funnel |
| Enforced by | BioRouter | the kernel | BioRouter |
| Needs kernel support | **no** | yes | **no** |
| Granularity | the whole root — a raw path cannot be attributed to an object inside it | the whole root | the **object**, where the root has a per-object tier; the whole root where it has none |
| Defeated by | a read whose path the handler supplies | a pre-planted hardlink (both kernels match paths, not inodes); on Linux, a root absent at job start | a reader that does not use the root's resolver — which is why each resolver is private and each leak around it is closed |

**All three are the ruling, and none of them is optional.** An earlier draft of this section carved
the tool channel out of the ruling entirely, on the grounds that `kb_read_page` and `read_app` are
governed by the extension classification instead. That was a redefinition of the ruling rather than
an implementation of it: the ruling says a public session's *tools* may not reach BioRouter's private
data, not "unless the tool owns the directory".

**A public session reaching a *public* object is not an exemption — it is the root's door
answering.** The ruling protects BioRouter's *private* data. A knowledge base classified public, a
session row classified public, an app built in a public chat: none of those is private data, and
refusing them would be a general jail of exactly the kind §9.5's ruling forbids. What is not
acceptable is a root whose contents are **undifferentiated** being handed over because no gate
exists. That was the Agent Drafter root, which had no per-app classification at all, and §9.5.3 says
what it gets instead.

**Why an in-process check has to exist at all.** `computercontroller__cache` reads a caller-supplied path with
`tokio::fs::read_to_string`; `agent_drafter__read_app` reads app bytes with `std::fs::read_to_string`;
`developer__text_editor` opens files directly. **None of them spawns anything — they are the
daemon.** No sandbox the daemon installs on its children can constrain the daemon, so on every
platform, including the two where Layer B works perfectly, those reads are governed by a check in the
code path or by nothing at all. Two successive adversarial reviews found a public tool reading a
private root without spawning a process; the second found one *inside* an extension the first had
already named.

**Why the check must sit at a choke point rather than on a list of tools.** The readers cannot be
found by grep — `xlsx_tool` reads through `umya_spreadsheet`, `pdf_tool` through `lopdf`,
`data_query` through `sqlx`, none of which contains an `fs::` token — and there are 125 tool
declarations in `biorouter-mcp` with 48 path-shaped parameters, a count that both over- and
under-states the real set. **Any control phrased as "the tools that read files" is unmaintainable by
construction. It must be phrased as "every tool call passes through symbol X", and the test for it
must be a tool the production code has never heard of.**

**What the in-process halves buy, and it changes the cost of this feature.** Because Layer A and the
roots' doors hold everywhere — neither needs kernel support — Layer B's platform gaps stop being
feature-killers. Landlock cannot subtract a read and
Windows has no unprivileged confinement — but that no longer means *"a public session cannot read
files on those hosts"*, it means *"a public session cannot spawn a shell on those hosts"*. The
fail-closed refusal narrows from **every file tool** to the five that spawn a child:
`developer__shell` and its background jobs, `computercontroller__automation_script`,
`computer_control`, and `compute_run`/`compute_python`. `text_editor`, `analyze`, `image_processor`,
`cache`, `xlsx_tool`, `pdf_tool`, `docx_tool` and every knowledge / memory / drafter tool keep
working on Windows.

#### 9.5.2 The entries

**Four roots**, and one file:

| Entry | Resolved by | Why |
|---|---|---|
| `<data>/sessions` | `Paths::data_dir()` + `SESSIONS_FOLDER` | `sessions.db` and its FTS mirror — §9.3 A2 |
| `<config>/knowledge` | `knowledge::paths::knowledge_root()` | the tree the KB barrier gates |
| `<config>/memory` | `memory::global_memory_dir()` | the global store §9.3 B3 is about |
| `<config>/agent_drafter` | `agent_drafter::default_root()` | app source, `.vault/`, **and app ids** |
| `<config>/config.yaml` | `Paths::config_dir()` + `CONFIG_YAML_NAME` | **the master switch itself.** §10.6's toggle is loaded from this file at startup, and it is an ordinary non-`SecretGuard` file: a public model can edit it and the next restart has privacy tiers off. Five tools can write it — `text_editor`, `shell`, `cache` **delete**, `kb_export`'s `dest_path` and `export_app`'s `target_dir` — so protecting one of them protects nothing |

Note the two different directories: the session store is under `data_dir`, the other four under
`config_dir`. A deny list written against one prefix misses most of them, and every test that
relocates both with `BIOROUTER_PATH_ROOT` still passes.

Two properties of the entry list, both learned the hard way:

- **The `config.yaml` entry denies reads as well as writes,** because telling a read from a write
  means knowing which argument of which tool is a destination — the per-tool knowledge this design
  has just finished abandoning. The cost is stated in §16: a public chat cannot view `config.yaml`
  through a tool. The user can still open it.
- **The verdict is not existence-gated.** `<config>/memory` is created lazily on first write and does
  not exist on a fresh install, so a containment test that requires the path to exist fails open on
  it — while every test written against a populated fixture passes.

#### 9.5.3 What each enforcement point covers

**Layer A** refuses any tool call whose arguments name a path inside an entry, at the one dispatch
choke point, plus the seven branches the agent short-circuits before that choke point is reached (one
of which, `platform__manage_schedule`, reads an arbitrary `workflow_path` in-process).

**Layer B** wraps the five spawning tools, carrying the same decision as a per-call flag in the MCP
request metadata rather than in session state.

**The roots' doors** refuse a read the handler was about to perform, inside each root's own resolver,
consulting the object's classification where the root has one.

**All three read one capability, sampled once.** The capability is captured at the entry that admits
the tool call — the agent loop, the daemon's `call_tool` route, the code-execution bridge, or the
pre-turn prefetch — and threaded, with the master switch's state captured in the same instant. It is
never re-derived downstream. That matters for a reason worth stating in the design rather than in a
task: the daemon returns a tool call as an **un-awaited future** that then queues behind a global
concurrency limit, so "downstream" can be minutes later. A gate that re-reads the model binding at
that point can let a call admitted under a public model run with private privileges — and a
mid-session model change would then take effect *retroactively*, on work already permitted.

The cost of that choice, stated: **switching a chat's model takes effect on the next tool call, not
on the ones already in flight.** A call admitted as public stays public (a spurious refusal, which
the user resolves by asking again); a call admitted as private stays private for its duration.

**What Layer A does *not* cover, and what does:** the tools that own these roots reach them through
their own resolvers, so nothing in their arguments names a root and Layer A finds nothing to refuse —
`kb_read_page`, `retrieve_memories`, `read_app`, `list_apps`. **That is a gap in Layer A, not an
exemption from the ruling**, and it is closed at each root's own door:

| Root | Its door | What the door consults | If the object is private |
|---|---|---|---|
| `<config>/knowledge` | the knowledge service's four tool-facing choke points (CP1–CP4), plus the two ends that bypass them, `kb_export` and `kb_import` | the base's tier (§9.3 B4's ratchet) | refused, without naming the base |
| `<config>/agent_drafter` | `ArtifactStore`'s private `dir()`, through which every id-keyed read passes — plus its `list()` and its raw-root accessor | **a per-app tier, which this design adds**: an app takes the tier of the most sensitive session that has written to it, exactly as a knowledge base does | refused; and the app's id and title are omitted from `list_apps` rather than reported as hidden |
| `<config>/memory` | `get_memory_file`, the funnel all four memory tools pass through, plus `retrieve_all`'s enumeration | nothing — and it needs nothing, because §9.3 B3 refuses a **private** session's write to the global store, so the global store can only ever hold public content | n/a; the invariant is what makes this door open, and it is load-bearing |
| `<data>/sessions` | the API, not the filesystem: it is a SQLite pool and BioRouter never reads it as files. Gate D (`chatrecall`), Gate G (`ingest_conversation`), and `read_session_blob`, which is scoped to the session's own id | the session row's classification | refused, without naming the session |

**Two consequences worth stating plainly.**

1. **Agent Drafter apps gain a classification they do not have today.** Without one the root is
   undifferentiated, and the only rules available are "all readable from a public chat" (which is
   the hole) or "none readable from a public chat" (which takes a flagship feature out of every chat
   on a commercial model). Every app that exists at migration is classified **public** — there is no
   evidence to classify it otherwise, `Manifest.session_id` is optional and the session it names may
   be gone — so nothing changes on upgrade day and the rule starts applying to what happens next.
2. **The tool channel is finer-grained than the filesystem channel, and a user can see the seam.** A
   public chat may `read_app` its own public app and may not `cat` the same file from the shell,
   because a raw path cannot be attributed to an object inside the root. That asymmetry is a cost of
   having one rule that needs no per-object knowledge; narrowing the filesystem side is a follow-up.

If a tool-channel classification is wrong, the fix is still to change the classification — but "there
is no classification" is not a classification, and that is what this section changes.

#### 9.5.4 What each platform can express, for Layer B

Measured by execution against `crates/biorouter-sandbox` rather than assumed — macOS on a real host,
Linux in a real `bubblewrap 0.8.0` container:

- **macOS (Seatbelt) — yes, directly.** A `(deny file-read* (subpath …))` appended to the base
  profile subtracts the subtree, verified in the production shape where the writable root is `/` and
  is therefore an ancestor of every entry: read, write and `rm` inside the entry all fail while
  writes elsewhere all succeed. A single file uses `literal` rather than `subpath`. Effectively free.
  **Every path must be canonicalized into the profile**: a deny declared with an uncanonicalized
  spelling matches *nothing at all*, in both directions, and the profile still compiles and runs.
- **Linux (bubblewrap) — yes, with `--tmpfs <root> --remount-ro <root>`.** `--tmpfs` after
  `--ro-bind / /` overmounts the directory with an empty tmpfs in the child's own mount namespace —
  but a bare `--tmpfs` is **writable**, so the read-only remount is what makes the policy true as
  stated. A single file is bound read-only from `/dev/null`. Requires `bwrap` installed **and**
  unprivileged user namespaces enabled — and the first does not imply the second: under default
  Docker seccomp `bwrap` is present, executable, and fails on every invocation. **The capability must
  be a live probe, on both platforms, and the probe needs two legs** — one asserting the deny bites,
  one asserting an unrelated read still succeeds — because a host where the sandbox cannot start any
  process passes a one-legged probe.
- **Ordering is load-bearing on Linux and fails open silently.** A `--tmpfs` (or a file's
  `--ro-bind`) emitted *before* the writable `--bind` of its parent is defeated with no error and no
  diagnostic. Three of the four roots live under `$HOME`, which is routinely the session working
  directory and therefore a writable bind root, so this is the common case rather than a corner.
- **Linux (Landlock) — no, and not because of this host.** A Landlock ruleset is a set of *grants*
  with no deny rule and no way to subtract a subpath from a broader grant; the implementation
  deliberately leaves read accesses unhandled so reads stay open. Expressing a deny means granting
  the *complement* — every sibling of every ancestor of every deny root, re-enumerated per command —
  and anything created in one of those ancestors after the ruleset is built becomes unreadable for
  that command's lifetime. Declined; a control whose failure mode is "the file I just wrote cannot
  be read back" is a control that gets switched off.
- **Windows — no.** There is no unprivileged, general-purpose confinement that can wrap an arbitrary
  developer command without breaking it; the sandbox module's own header works through the five
  candidates and why each fails.

**The fail direction is closed, for the five spawning tools.** A public-capability session on a host
where the kernel deny cannot be established does **not** get an unsandboxed arbitrary-execution tool.
Those specific tools are refused, with a deterministic error that names the state, the reason, and
the two ways out — switch this chat to a private model, or turn privacy tiers off for this machine
(§10.6) — and forecloses the retry. On Linux the message names a third fix for the machine itself
(`install bubblewrap`). The refused tools are **not** hidden from the model's tool list: hiding them
makes a model invent workarounds, while a refusal that forecloses the retry makes it stop. Every
other tool keeps working on such a host, because Layer A is not what failed.

**The costs, stated rather than discovered.** On Windows, and on Linux without bubblewrap, a
public-capability chat loses the shell — which for a commercial model on a Windows laptop is the
common configuration, and is a large part of why R7's opt-out is a master switch. A
public-capability chat also cannot read its own history through the `biorouter` CLI (that reads the
session store), cannot `cat` its own drafted app's source (that is the fourth root), and cannot view
`config.yaml` through a tool (that is the fifth entry). All four are the control working as
specified.

#### 9.5.5 The second-order path, stated honestly

The daemon's HTTP API is another read path: `GET /sessions/{id}/export` returns a transcript to
anyone holding `BIOROUTER_SERVER__SECRET_KEY`. An earlier version of this section concluded that the
child-environment strip closed it — *"a sandboxed child knows where the daemon is and cannot
authenticate to it."* **That conclusion is withdrawn.** Measured: on macOS a child reads its parent's
environment with `ps -Ewww -p $PPID`, and the protection people assume (SIP withholding process
environments) applies only to Apple *platform* binaries — a locally compiled binary leaks, and so
does the shipped, notarized, hardened-runtime `biorouterd`. Nor is it `ps`'s setuid bit: a plain
non-setuid `sysctl(KERN_PROCARGS2)` reader recovers it, including under a Seatbelt profile carrying
the deny entries. On Linux the daemon's own `/proc/self/environ` is readable in-process by any tool
that reads a caller-supplied path.

**So: the daemon's API secret is not defensible against a tool running inside the daemon, and no
sandbox this feature installs changes that.** What remains true, and what it costs:

- The strip (`strip_daemon_private_env`, the fix for §9.3 A1, applied last in each command builder)
  is still correct and still required. It keeps the secret out of the child's own environment, where
  a careless `env` dump or log would find it, and it keeps every **remote** caller out.
  `BIOROUTER_PORT` and `BIOROUTER_APP_BASE_URL` are deliberately kept, so locating the daemon is free
  and meant to be.
- **The biggest local route is held by Layer A, not by the secret.** `POST /agent/call_tool` executes
  any tool of any extension with no capability check and no approval prompt — and it dispatches
  through the same choke point Layer A and Gate C sit at. A caller holding the secret therefore gets
  exactly what the chat already had. This is the concrete reason the barrier belongs in the extension
  manager and not one frame up in the agent.
- **What is left exposed** is the set of routes that return private content without running a tool:
  the transcript family, the `/knowledge/*` read routes, `GET /apps/{id}/export`, and
  `GET /diagnostics/{id}` — which returns a zip of `session.json`, recent logs and a verbatim
  `config.yaml`, and is the widest single route in the API. Closing that needs a per-caller
  credential the daemon does not hand to its own children (open question).

One further residual, and it is larger than previously stated: `GET /apps/{id}` and
`GET /apps/{id}/agent` are deliberately unauthenticated, and the served page carries the app's socket
token, so a client that knows an **app id** can drive that app's agent — including any knowledge base
the app's manifest granted it, private ones included. It was previously argued that denying the Agent
Drafter root removed the only local source of app ids. **That is false:** `agent_drafter__list_apps`
is a Public tool that enumerates every id in-process, takes no path argument, and is therefore
untouched by both layers. A public model does not need to already know an app id — it can ask for
one. `GET /apps` still requires the secret, and that is now the only thing standing there.
Authenticating the app socket against a local client is a separate change (open question).

---

## 10. MCP badges

### 10.1 Where the badge is declared

**`landing/registry.json`, and nowhere else** (R11 i). Nothing local can grant private; the `.brxt`
bundle is never read. A bundle *could* self-declare `"privacy": "private"` trivially — `main.ts`
validates only `name`, `display_name`, `description`, `version`, `entry_point`, `repository`,
`env_vars`, and `BrxtInstallModal.tsx:152-161` writes a config recording **no provenance
whatsoever** (no registry id, no source URL, no hash). What stops it is that the resolver never
reads the bundle, and never reads `config.yaml` either.

### 10.2 How the badge reaches the enforcement layer

Enforcement is Rust; the registry fetch is Electron (`main.ts:2832` `REGISTRY_URL`, IPC handler at
`:2855-2866` — a bare `fetch`, no timeout, no cache, no integrity check). The Rust side has no
network path and the CLI and daemon have no Electron, so **a runtime-fetch-only design cannot
enforce anything outside the desktop app.**

`landing/scripts/build-registry.mjs` gains a second output:

```
crates/biorouter/src/privacy/registry_private.rs      (@generated — do not edit)
    pub const PRIVATE_EXTENSIONS: &[&str] = &["cdwagent", "ucsfomopagent"];
```

with a `--check` mode wired into `just check-everything`, mirroring the theme-generator precedent
CLAUDE.md already blesses (`npm run themes -- --check` inside `lint:check`).
`ui/desktop/src/components/baam/registry.fallback.json` — today a hand-maintained copy with no
generator and no CI check, in sync at 37/37 and 129/129 by luck — joins the generator's outputs and
the same gate.

**The matching key is fixed in the generator, not with a heuristic.** Verified in the live registry:
SPOKE's `id` is **`spokeagent-0.4.1`** (derived from the download filename by `slugFromUrl`) while
the installed extension is keyed `spokeagent` from `manifest.name`. `landing/baam.html` already
carries a `featIndex` workaround for this; the app has none. Today harmless — spokeagent is public
— but the identical bug would classify a future version-suffixed *private* extension as unlisted ⇒
public. The generator emits a stable `extension_name` field, the Rust const is keyed on it, and the
build fails if any entry marked `private` lacks one. A suffix-stripping heuristic in a security
path is exactly the thing that is right until it isn't.

**Freshness raises, never lowers.**

```
private_set = PRIVATE_EXTENSIONS ∪ private(last_good_fetch)
```

An upgrade (newly private upstream) takes effect on the next successful fetch and persists. A
downgrade requires a successful live fetch and is never honoured for an entry the compiled-in set
names. An offline laptop can fail to *learn* a new private badge; it can never *lose* one. The
fetch gains a 10 s `AbortController` and writes the last-good copy; `loadRegistry`'s
existing-but-discarded `live: boolean` is surfaced as "catalogue last updated <date>".

### 10.3 The catalogue, classified

| Extension | On BAAM | Tier | Evidence |
|---|---|---|---|
| `ucsfomopagent` | yes, `id: ucsfomopagent` | **PRIVATE** | verified in `landing/registry.json` |
| `cdwagent` | yes, `id: cdwagent` | **PRIVATE** | verified |
| `spokeagent` | yes, `id: spokeagent-0.4.1` | PUBLIC | operator ruling — SPOKE holds no patient data; its passcode gates the service, not private content |
| `medcp` | **no** | PUBLIC | verified absent — all 37 ids scanned |
| `msbaseagent` | **no** | PUBLIC | verified absent |
| the remaining 34 catalogue entries | yes | PUBLIC | no marker |
| built-ins: `developer`, `autovisualiser`, `computercontroller`, `memory`, `tutorial`, `agent_drafter`, `knowledge` | n/a | PUBLIC | R11 |
| platform: `todo`, `chatrecall`, `extensionmanager`, `skills`, `code_execution` | n/a | PUBLIC | R11 |
| in-process app servers: `appcontrol`, `datasql`, `files`, `compute`, `evidence` | n/a | PUBLIC | per-app sandbox |
| anything hand-installed | no | PUBLIC | R11(ii) |

**The private set is exactly two strings.**

`Frontend` extensions are public by construction — `Agent::add_extension` intercepts them before
the ExtensionManager sees them and `add_extension` refuses the variant outright
(`extension_manager.rs:691-693`), so no EM gate can ever see one. Assert it as a test so the
invariant is deliberate rather than incidental.

### 10.4 Accepted risks, stated plainly

**Fail-open on hand-installed extensions is a deliberate bypass, and it is live right now.**
`medcp` is verified enabled in the operator's config with `CLINICAL_RECORDS_BACKEND`,
`CLINICAL_RECORDS_SERVER`, `CLINICAL_RECORDS_DATABASE` and `CLINICAL_RECORDS_USERNAME` in the
plaintext `envs` map and `CLINICAL_RECORDS_PASSWORD` in `env_keys`, against a clinical MSSQL
backend. **A public model can query patient data through it today, and will still be able to after
this design ships.** The operator's reasoning: a hand-installed extension is the user's own choice
and responsibility, and medcp is a *connector* rather than a data source — the same binary may be
pointed at a local SQLite file. The reasoning and the consequence belong in one sentence, and they
are: **the badge is a statement about provenance, not about the data behind the connector.**

**Publishing to BAAM is what grants a private badge.** `medcp` and `msbaseagent` both reach
institutional data and are public *solely because they are unpublished*. Where and by whom that is
revisited: **in the pull request that adds the card to `landing/baam.html`, by the Baranzini Lab
reviewer** — and it is forced rather than remembered, because the `--check` gate makes it
impossible to publish a card without the generated Rust const changing in the same commit, and the
generator hard-fails when a card's description matches a clinical keyword list (`patient`,
`clinical record`, `EHR`, `PHI`, `medical record`, `de-identified clinical`) with no `data-privacy`
attribute present.

**Two naming consequences, known rather than discovered:** a hand-installed extension *named*
`ucsfomopagent` inherits the private badge (fail-closed, fine); and a genuinely private extension
renamed locally becomes public — already the accepted direction under R11(ii), and unavoidable
because the install records no provenance at all.

**A mechanical scan of the 37-entry registry surfaces one candidate the ruling does not cover** and
eight that hit a single signal: `ucsfhpcagent` (UCSF CHPC/SLURM job planning, account inspection,
file transfer) scores on both credential and institutional-data signals; `labarchivesagent` (an
electronic lab notebook — unpublished experimental data), `benchlingagent`, `latchbioagent`,
`opennotebookagent`, `lamindbagent`, `clinicalvariantagent`, `protocolsioagent` and
`ginkgocloudlabagent` hit one. Description prose is a usable but insufficient signal —
`omeroagent` and `dnanexusagent` scored clean despite reaching institutional imaging and genomic
platforms. Not a blocker; a list for the reviewer at publish time.

### 10.5 The v2 option, clearly labelled

**Per-installation credential-derived privacy:** an extension declaring credentials for a clinical
or institutional data source is treated as private *on that machine*. The registry's authority is
untouched — this can only *add* private, never remove it. Two corrections to how it is usually
framed, both verified and both in the design's favour:

1. **It needs no registry-schema change.** `env_keys` and `envs` already exist locally on the
   `Stdio` and `StreamableHttp` variants of `ExtensionConfig`.
2. **It closes `medcp` and does not close `msbaseagent`.** `medcp` puts the identifying keys in the
   plaintext `envs` map and only the password in `env_keys`, so a rule reading `env_keys` alone
   misses it — it must read **both** maps. `msbaseagent` declares no credential keys at all
   (`env_keys: []`, `envs: {MSBASE_LOG_LEVEL}`): no credential-shaped local signal exists for it.
   The rule would also fire on `spokeagent` (`SPOKEAGENT_PASSCODE`) unless the pattern list is
   narrow, which would contradict a settled ruling.

Cost: one hand-maintained pattern list — the one hand-maintained list this design otherwise avoids.
Hence v2, opt-in, and never a correction to the ruling.

### 10.6 The opt-out (R7) — one master toggle

Global, explicit, on by default: `BIOROUTER_PRIVACY_TIERS` (default `on`), and it is a **master**
switch over the whole feature. With it off there is no bind gate, no turn gate, no dispatch gate, no
discovery filter, no `chatrecall` filter and no `chatrecall` LOAD guard, no knowledge-base barrier,
no scoping of the Agent Drafter catalog, no forced export location for a model's `.brkb`, **no
refusal when a public session enables a private extension and no stripping of a private server's
instructions from a public system prompt** (both of which are tool-call-shaped only by analogy and
are the two channels an enumeration is most likely to miss), no spawn matrix, no classification on a
copied session, no visibility predicate, no classification ratchet and **no read-deny at all**
(§9.5) — neither the in-process barrier nor the OS sandbox. Nothing is refused, nothing is
sandboxed, and no path is out of reach.

**The enumeration above is the specification, and it is checked mechanically rather than read.** A
master toggle wired to *some* of the enforcement points is the failure mode here, and it is invisible
to any textual check: the execution plan therefore asserts every point in both toggle positions, and
closes the list at both ends — every place that reads the switch must have a row, and **every place
that refuses on privacy grounds must read the switch**. The second half is what catches an
enforcement point added later that nobody thought to wire, because a new enforcement point is a new
refusal. The one exception is the OS sandbox, which refuses nothing — it hands the kernel a policy —
and so is carried by its own row and its own paired assertion.

**An earlier draft of this section scoped the opt-out to Gate C** — turning off the tool gate
decides what a model may *call*, whereas turning off the session barrier retroactively exposes data
gathered under a private badge — and flagged the scope as open question 3. The operator has ruled
the other way, reading their own words ("opt out of the **entire** protection layer") literally. The
narrower key is retired rather than kept alongside the master one: two switches whose scopes nest
are two things a user must reason about at the moment they are least able to.

**What it does *not* do.** It does not drop the columns, the stamps already written, or the audit
rows, so turning it back on resumes enforcement over the history that existed when it was turned
off. And it does not hide the badges: they keep rendering, restyled and suffixed *— enforcement
off*, beside a persistent strip. Hiding them makes an unprotected machine indistinguishable from a
machine with nothing private on it, at exactly the moment the distinction matters; leaving them
unchanged makes a pill reading plain **Private** a false statement the user acts on.

**The one-way cost, which the confirmation must state.** With the toggle off the ratchet does not
run, so a session that handled private material during that window stays stamped `public` for ever:
there is no content scan, no provenance on the messages, and `privacy_tier` is monotone, so
re-enabling cannot go back and mark it. The alternative — keep ratcheting while the guardrails are
off — was rejected because it silently privatises sessions the user believes are unprotected, and
they would first learn of it as a refusal weeks later.

Three hardening measures, because the failure mode is an agent disabling its own protection:
(1) read it bypassing `Config::get_param`'s env branch, straight from the loaded values map, so no
environment variable can turn protection off; (2) gate the key in `POST /config/upsert` so the flip
must come from Settings → Privacy with its confirmation; (3) hold the authoritative value in daemon
memory from startup — **not** SecretGuard, which cannot enforce this (§9.3 C1). Check *inside* the
gate rather than in an `is_enabled()`, following the `SensitiveOpsInspector` pattern, so a
mid-session change is honoured and the opt-out is one auditable line rather than an absent gate.

Because it is now one predicate read by every gate, the test that matters is a **matrix**: each gate
asserted in both toggle positions. A master toggle wired to three gates out of ten passes every
textual check and is the failure this design is most likely to ship.

---

## 11. `chatrecall`

### 11.1 LOAD is the mode that gets forgotten

`handle_chatrecall` (`chatrecall_extension.rs:78-159`) has two modes and only SEARCH touches the
FTS index. **LOAD has no filter of any kind** — not even the `exclude_session_id` guard SEARCH has.
It takes a caller-supplied `session_id`, calls `get_session(&sid, true)`, and builds:

```
Session: {name} (ID: {sid})
Working Dir: {working_dir}
Total Messages: {total}

--- First Few Messages ---   [first 3, message text verbatim]
--- Last Few Messages ---    [last 3, message text verbatim]
```

The guard goes immediately after `get_session` and **before the header string is built**, so not
even the name or working directory escapes. Ship this first, on its own, ahead of everything else
in this design.

### 11.2 SEARCH — filtered in the query, in both builders

`execute` branches on `fts_available()` (`:108-116`, a `sqlite_master` probe), so a design that
filters only the FTS path leaks on any un-migrated profile. **`sessions` is already joined in
both** — `fetch_rows_fts` at `:135` and `build_sql` at `:211` — so it is one clause each, in the
same position as the existing exclusion:

```rust
if caller_is_public { sql.push_str(" AND s.privacy_tier = 'public'"); }
```

**Written as a SQL literal, not a bind.** Both builders bind conditionally and positionally
(`:154`, `:176`), so an inserted `?` in the wrong position mis-binds silently. The literal is a
compile-time constant of the code path, not user input.

To make an unfiltered search unconstructible, `ChatHistorySearch` takes
`caller_capability: ProviderTier` as a **required constructor parameter** — not a builder setter,
not an `Option`. It threads through three signatures, so a missed call site is a compile error.
`caller_is_public` is `agent.capability_tier() == Public` — the **live provider's** tier, not the
caller session's stored classification. That is the correct operand: the question is who is about
to read this, and the reader is the model. It also means a private session in the residual state
reads as a public caller, which is the safe direction.

### 11.3 Why in the query rather than a partitioned index

1. **`LIMIT ?` is applied by SQLite** (`:150`, `:244`). Post-filtering in Rust would silently
   truncate a public model's results whenever private rows outrank them — fewer public hits than
   exist, non-deterministically, with no error.
2. Private rows are then never in the result set, so `process_rows`, `get_session_totals` and
   `convert_to_results` need no knowledge of tiers. The reviewable surface is two lines of SQL.
3. `messages_fts` is a **contentful** FTS5 table maintained by hand from Rust at every message
   write site (`session_manager.rs:33-47`). Partitioning into `messages_fts_public`/`_private`
   would double all five hand-written write/rebuild/delete sites and — decisively — would make the
   ratchet and declassification **reindex an entire session's text**. With the column on
   `sessions`, both are one `UPDATE` and zero write sites change.

### 11.4 Content versus existence, field by field

| Field | Verdict | Why |
|---|---|---|
| `m.content_json` → `messages[].content` | **CONTENT — withheld** | it *is* the message text, including the `[Tool: …]` renderings |
| `messages[].role`, `messages[].timestamp` | **CONTENT — withheld** | meaningful only attached to a withheld body; returning them buys nothing and invites reconstruction |
| `s.description` → `session_description` | **CONTENT — withheld** | the LLM-generated session title, produced *from the conversation*. A summary of private text, not a label. The field most likely to be mislabelled as metadata, and the one that leaks most per byte. |
| `s.working_dir` → `session_working_dir` | **CONTENT — withheld** | a filesystem path, but in this product it routinely names a cohort, a study or a patient population |
| `ChatRecallResult.last_activity` (`session/chat_history_search.rs:14`, rendered at `chatrecall_extension.rs:219`) | **CONTENT-adjacent — withheld** | it is `max` over *matched* message timestamps (`:347-351`), so it dates the private message containing the search term, not the session. Under §11.4's own rule ("anything derived from a message body is content") it is message-derived. Moot once rows are filtered in SQL, but a reviewer checking the table for completeness must not find a hole. |
| `s.id`, `s.created_at` | existence — may be revealed | acceptable per R13 — **and only safe because LOAD is gated independently.** The two decisions are coupled and must stay coupled. |
| `total_matches`, `results.len()`, `total_messages_in_session` | existence — may be revealed | and nothing is needed: `total_matches` is summed *after* filtering, and `get_session_totals` counts only sessions already in the filtered set |

**The rule a reviewer can apply without re-reading this:** anything derived from a message body is
content; anything the row would have had before its first message is existence.
`session_description` fails that test, which is the point of stating it.

**Side channels are explicitly out of scope** (R13): no count padding, no constant-time responses,
no decoys, no cover traffic. A public model may see a bare "no results", a reduced count, or a
timing difference. The invariant is narrower and checkable: **no snippet, no title, no excerpt, no
working directory, no field drawn from a message body ever reaches a public model** — a property of
the SQL rather than of the rendering code.

### 11.5 Tests

FTS path and LIKE path, each: a public caller with a private chat containing the term gets zero
rows, a private caller gets the row. The `LIMIT` interaction: 10 private rows ranking above 3
public rows with `limit = 5` → a public caller gets all 3 public rows, not 0. LOAD: a public caller
passing a private session id gets a refusal whose text contains no part of the session name or
working directory. `get_session_totals` consistent with the filtered set. A source-level test
asserting `s.privacy_tier` appears in **both** builders.

---

## 12. Declassification

### 12.1 Where it lives

**History → the session's own row → overflow menu → "Make this chat public…"**, shown only when the
chat is private, and the identical control on the session-detail header, sharing one
`DeclassifySessionDialog` so the two entry points cannot diverge. Not in Settings. Not in the chat
header. Not anywhere an agent can reach.

Route: **`POST /sessions/{session_id}/declassify`**. Not under `/config/*`, not a tool, not
reachable from any `workspace_*` handler or MCP server, and explicitly not added to the public-GET
exemption list. **Per §9.3 A1, secret-key auth alone is not sufficient** — bind it to a one-shot
token minted by the renderer over Electron IPC.

**The same mechanism, for knowledge bases.** [DR-18](privacy-tiers-execution-plan.md#dr-18--the-knowledge-base-tier-is-user-controllable-and-a-private-session-creates-a-private-base) adds a second user-only tier change —
publicize / privatize a base (§5.4) — and it reuses this section's proof-of-user, this section's
dialog primitive and this section's "exactly one lowering writer in the tree" rule. **Two mechanisms
for one idea is how the two confirmations diverge**, so a `POST /knowledge/bases/{id}/tier` that
accepts the secret key alone, or a `kb_set_tier` MCP tool, is the wrong implementation of §5.4 and
not a shortcut. [Task 29A](privacy-tiers-execution-plan.md#task-29a-knowledge-base-publicize--privatize--user-only-graded-audited).

### 12.2 Why no agent can invoke it

```rust
// crates/biorouter/src/privacy/declassify.rs
pub struct UserConfirmation(());   // ZST; constructor is pub(in crate::…)

pub async fn declassify(sm: &SessionManager, session_id: &str, ok: UserConfirmation) -> Result<()>
```

`UserConfirmation`'s constructor is invoked in exactly one place — the HTTP handler, after it has
matched the typed confirmation. No MCP server, no `ToolRouter`, no `workspace_*` handler, no CLI
subcommand can construct one. "An agent cannot call this" is enforced by Rust's module privacy
rather than by the route being undocumented.

It is also **the only writer in the tree permitted to lower `privacy_tier`.** Every other write
goes through the session update builder, whose emission is the monotone `CASE WHEN` and physically
cannot lower it; `declassify_session` bypasses the builder with its own `UPDATE`. A repo-grep test
asserts exactly one statement matching `privacy_tier\s*=\s*'public'` exists outside the migration —
the same shape as the existing `scripts/check-version-consistency.sh` guard. That single assertion
is the whole audit surface for "can the ratchet be reversed".

### 12.3 The wording

> ### Make this chat public?
>
> **"{session name}"** is marked **Private**. It has been running on **Versa · versa_azure**, a
> model hosted by UCSF, since **14 Mar 2026**. Its contents may include data from UCSF systems —
> patient records, clinical notes, or other institutional data.
>
> Making it public means **any model can read it from now on** — including commercial models hosted
> outside UCSF such as Anthropic, OpenAI and Google. That covers its full message history, the
> files and query results in it, and anything a private extension returned. It will also start
> appearing in chat-recall searches run by those models.
>
> By continuing you are asserting that nothing in this conversation needs to stay inside UCSF.
>
> **This is permanent and cannot be undone.**
>
> **[ Cancel ]   [ Make public — permanent ]**

If the chat is private by inheritance, one extra line appears: *This chat inherited its private
badge from **"OMOP cohort characterisation"**. Making this one public does not change that chat.*

Destructive button styling using the app's existing destructive treatment (no new colour). `Cancel`
holds initial focus. No keyboard shortcut binds the confirm button.

### 12.4 The confirmation — graded by what was actually touched

This is where the practitioner review changed the design, and it is the single most important
ergonomic decision in it. `privacy_reason` already distinguishes the two cases; use it:

| Reason | Control | Rationale |
|---|---|---|
| `mcp:*`, or inherited from an `mcp:*` ancestor | Typed confirmation + audit row + permanent record | A private data source was actually reached. |
| `turn:*` only, never any `mcp:` event | **Single-click "Make public"** with a 5-second undo, still audited, still user-only, still not agent-invocable | Nothing from a private data source entered this session. The only thing that happened is that text was *sent to* a UCSF-hosted endpoint, which is not a disclosure of UCSF data. |

This preserves every invariant (no automatic downgrade, only a human can lower it, monotone in SQL,
one writer) and removes the friction from the large majority of affected rows — see §16.

**When a typed confirmation is required, confirm on the last 6 characters of the session id**,
displayed immediately beside the field — **not** the session name. `is_default_session_name`
(`session_manager.rs:1614-1632`) shows `"New Session"`, `"CLI Session"`, `"Session <N>"` and
`"New session <N>"` are all live placeholders, and the naming pass is best-effort, so failures
leave sessions stuck on them; `fallback_session_name` (`:1527`) produces up-to-60-character titles
built from the first words of the user's message. A name-typed phrase is therefore either a
duplicate string shared by dozens of rows — destroying the justification, which is forcing the user
to look at *which* conversation — or a sentence to retype. An id suffix is unique, short, and
forces row-identity checking.

### 12.5 What is recorded

**An append-only audit table**, written in the same transaction, *before* the `UPDATE`:

```sql
CREATE TABLE classification_audit (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  session_id              TEXT NOT NULL,
  from_classification     TEXT NOT NULL,
  to_classification       TEXT NOT NULL,
  reason                  TEXT NOT NULL,   -- 'declassified_by_user'
  actor                   TEXT NOT NULL,   -- OS user + machine
  actor_kind              TEXT NOT NULL,   -- always 'user'; no other value is constructible
  occurred_at             TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  app_version             TEXT NOT NULL,
  provider_name_at_change TEXT,
  privacy_reason_before   TEXT,
  message_count_at_change INTEGER
);
```

**And a transcript record**, following the BR-71 Task 32 pattern of a
`user_visible: true / agent_visible: false` message written into the session's own conversation:

> *Made public by **wgu** on 27 Jul 2026 at 14:12 UTC. Before this change the chat was **private**
> (private since 14 Mar 2026 — first turn on Versa · versa_azure). All models can read this chat
> from now on.*

paired with the ratchet's own record, written when the tier is first raised:

> *This chat became private on 14 Mar 2026 on its first turn using Versa · versa_azure.*

Both are `agent_visible: false`, so the model never sees them and cannot be steered by them. The
transcript record travels with the session on export; the audit table survives session deletion.
They answer different questions — "what happened to this chat" and "what has ever been declassified
on this machine". A `warn!` with the stable event name `session_declassified` is emitted alongside,
matching the convention already used for `catastrophic_command_blocked` and `command_policy_deny`.

The session row keeps `privacy_reason = 'declassified_by_user'`, so History shows the badge as
**"Public — made public by you on 27 Jul 2026"** permanently. A declassified session must never be
indistinguishable from one that was always public.

### 12.6 After declassification, and bulk

The chat is genuinely public: its bound provider is left exactly as it was (a public chat may run a
private model — that direction was never restricted), nothing is re-indexed, nothing is rewritten.
If a private model later runs a turn on it, the ratchet fires again and the user may declassify
again. That is deliberate — declassification is an assertion about the contents *as they stood*,
not a permanent exemption.

**No general bulk declassification.** One exception, extended from the original design per §16:
sessions whose reason is `backfill:*` get one grouped dialog with a review-by-provider list,
because a backfill is a **guess made by the system from the last-used provider**, not a user
assertion about content — and `provider_name` records only the *last* provider, so the guess is
wrong in both directions. The per-session flow then applies only to sessions this build actually
observed going private.

---

## 13. Marketplace, registry and landing site

### 13.1 `landing/registry.json` — schema change

Two new fields per extension; registry `version` goes 1 → 2.

```json
{
  "id": "spokeagent-0.4.1",
  "extension_name": "spokeagent",     // NEW — stable join key, == the bundle's manifest.name
  "privacy": "public",                // NEW — "private" | "public", default "public"
  "name": "SPOKEAgent", …
}
```

`RegistryExtension` (`ui/desktop/src/components/baam/registry.ts:8-19`) gains both, `privacy`
optional with a `'public'` default so an old cached document still parses.

### 13.2 `landing/scripts/build-registry.mjs` — generator change

The generator scrapes `landing/baam.html` with regexes and already has the idiom to copy
(`const license = first(/data-license="([^"]+)"/, card) || 'Apache-2.0';` at `:102`). Add:

```js
const privacy        = first(/data-privacy="([^"]+)"/, card) || 'public';
const extension_name = slugFromUrl(download).replace(/-v?\d+(\.\d+)*$/, '');
```

plus both keys in the emitted object, plus **hard build failures** (the generator currently never
fails): if `privacy` is neither value; if `privacy === 'private'` and `extension_name` is empty; and
if a card's description matches the clinical keyword list with **no** `data-privacy` attribute
present at all. That last one is the mechanism that forces the medcp/msbaseagent revisit at publish
time rather than relying on someone remembering.

**The default matters more than the extraction.** Defaulting to `'public'` means an un-annotated
card is public by construction, so R11(ii)'s fail-open direction is enforced by the tool rather
than by reviewer discipline.

Two further outputs, both new: `crates/biorouter/src/privacy/registry_private.rs` and
`ui/desktop/src/components/baam/registry.fallback.json`, both joining the `--check` mode.

### 13.3 `landing/baam.html` — five components

The shelf is rendered client-side (`loadRegistryExtensions` at `:3941` fetches `registry.json` with
`cache:'no-store'`, then `renderExtensions` at `:3864` empties the static grid), so both the
template and the fallback change.

1. **`extCardHtml` (`:3804`)** — add `data-privacy="${privacy}"` on the card div beside the
   existing `data-license`, and **prepend** the badge to the `.ext-tags` row. Prepending matters:
   that row is `max-height: 22px; overflow: hidden` (`:191`), so an appended badge would be clipped
   on cards with many tags. **Both states are labelled** — `<span class="tag private">Private</span>`
   and `<span class="tag public">Public</span>` — because a badge shown only on private teaches a
   visitor nothing about its absence.
2. **`buildExtChips` (`:3838`)** — a Private/Public facet next to the existing UCSF/Community org
   chips.
3. **`filterExtensions` (`:3909`)** — one clause. It already special-cases a dataset-attribute
   facet for `org`; extend the ternary to `(f === 'org' || f === 'privacy')` against
   `c.dataset[f]`. **This is what makes "tell at a glance which are private before installing" real
   rather than aspirational** — a visitor can filter the shelf down to the two private extensions.
4. **The static no-JS cards (from `:471`)** — simultaneously the no-JS view *and the generator's
   input*, so this is where classification is authored. `data-privacy="private"` +
   `data-extension-name` on the CDWAgent card (`:471`) and the UCSFOMOPAgent card (`:495`), plus
   the visible badge in each `.ext-tags` row (`:486`, `:510`). The other 35 rely on the generator
   default.
5. **`landing/shared.css:406-415`** — a `.tag.private` variant beside the existing `.tag.ucsf` and
   `.tag.mcp`:

   ```css
   .tag.private { background: rgba(5,32,73,0.07); color: var(--ucsf); }
   .tag.public  { background: var(--bg-3);        color: var(--text-3); }
   ```

   Private uses the **navy** ramp: institutional in tone, visually distinct from the coral MCP tag.
   **No new red.** The palette has exactly one accent (coral `#b85a32` text, `#cf6d47` bars-only)
   and no semantic danger colour; inventing one breaks the Apple-deck reskin. Dark overrides go in
   the same block, since `landing/theme.js` sets the `.dark` class pre-paint.

### 13.4 Other landing surfaces

`landing/docs.html:1468-1478` carries a hand-written "Extension agents in the marketplace" table
listing six agents with an Agent / What it connects / Credentials header. It gains a **Privacy**
column, plus a new `landing/scripts/check-docs-privacy.mjs` in the site check step comparing it to
`registry.json` — nothing generates this table today, so nothing would catch the drift the day BAAM
ships badges. `landing/skills.html` and `landing/index.html` list no extensions and need no change;
say so in the doc so a later reviewer does not "fix" it.

### 13.5 How a user learns a hand-installed extension is public

The fail direction is settled; these make the consequences legible **before** the user trusts it
with sensitive work.

- **Extensions settings** shows a badge plus provenance on every card, in three distinct strings:
  *"Private — published on the Biorouter marketplace"*, *"Public — published on the Biorouter
  marketplace"*, *"Public — installed from a file, not on the marketplace. Any model can call it."*
- **The `.brxt` install modal** shows the resulting badge above the Install button with:
  *"Extensions installed from a file are always Public. Any model, including commercial models
  hosted outside UCSF, will be able to call this extension."* The manual "Add stdio extension" form
  carries the same line.
- **A one-time notice on first launch after upgrade** names any **enabled** extension that is Public
  and declares clinical-looking credentials. On this machine it names `medcp`. It informs; it does
  not block.
- **Gate C's refusal message** names the marketplace as the grantor, so the mental model is taught
  by the system rather than by documentation.

---

## 14. User experience

### 14.1 The badge

**Private is the marked state; Public is the quiet state.** A badge on absolutely everything trains
people to stop seeing badges, which defeats R10's actual goal — knowing which tier you are in
*before* hitting a wall.

- **Private** — filled pill: `--background-muted` fill, `--text-standard` label, small
  shield-outline glyph, the word "Private". It has weight.
- **Public** — hairline pill: 1 px `--border-subtle`, `--text-subtle` label, no glyph, the word
  "Public". Present and readable (R10 satisfied literally) but recessive.
- In the two dense surfaces (tab bar, narrow session-list rows) only Private renders, as a 6 px
  `--text-standard` dot with the full label in the tooltip. No dot means public.

Never colour alone: shape plus glyph plus word.

**Zero theme work.** A theme is one file (`ui/desktop/themes/<id>.theme.mjs`) and
`npm run themes -- --check` runs inside `lint:check`. The badge uses **only existing semantic
tokens** and adds no new token to any theme file, so it renders correctly across Parchment, Alma
Mater and Roche Limit in both modes with no generator run and cannot fail `check-contrast.mjs`.

### 14.2 Every surface

| Surface | What shows | Where |
|---|---|---|
| Provider grid / settings | Tier badge per provider; the Local / Institutional / Commercial headers stay, now driven by the backend field | `settings/providers/ProviderGrid.tsx`, `providerOrdering.ts` |
| Model picker | Badge per model row; **public rows disabled with an inline reason in a private chat** | `ModelAndProviderContext.tsx` consumers |
| Composer model chip | Badge beside the model name — the most-looked-at spot in the app | `components/bottom_menu/` |
| Chat header | Session badge beside the title | BaseChat header |
| Tab bar | Private dot only | BR-71 Tasks 22-27 |
| Session list / History | Session badge, a "Private only" filter chip, "Made public by you on …" after a declassification | History view |
| Extensions settings | Badge + provenance line, **and a third state computed against the focused session** (§14.5) | extensions settings |
| BAAM Browse (in-app) | Badge per entry, Private/Public facet, and the `live: false` staleness line | `components/baam/` |
| `.brxt` install modal | The resulting badge, above the Install button | `BrxtInstallModal.tsx` |
| **Knowledge view + KB palette** | Base tier chip, and the publicize / privatize control (§5.4) | `components/knowledge/` |
| **The non-private-model disclosure (R15)** | Once, blocking, on the first public bind in an install; then permanently as the Commercial section's line, the model chip's tooltip, and the Settings → Privacy statement | [Task 30A](privacy-tiers-execution-plan.md#task-30a-the-non-private-model-disclosure) |
| `workspace_list` rows and the GUI workspace panel | Badge per row | BR-71 Tasks 12 / 22-27 |
| Landing `baam.html`, `docs.html` | `.tag.private` on the navy ramp; Privacy column | `landing/` |

**Pre-flight, not post-refusal.** In the model picker a public model in a private chat is rendered
disabled with the reason inline; a private extension under a public model is *visible but disabled*
with its reason, so the user knows it exists and why.

### 14.3 Three prerequisites the UX section is built on and which do not exist

These are named tasks, not assumptions. Without them the first user interaction with this feature
is a green success toast over a refused operation.

**P2 — the refusal is currently swallowed and replaced with a success toast.**
`ui/desktop/src/components/ModelAndProviderContext.tsx:235-286` is the one and only model-switch
path. Verified: `updateAgentProvider` is called **without `throwOnError`**, so the generated
`@hey-api` client returns `{error}` rather than throwing; a Gate A refusal would be discarded,
execution continues, `setConfigProvider` rewrites the global default to the refused provider, the
chip flips, and a green toast says the switch succeeded — while the session is still bound to
Versa. That is worse than a wall: the user believes they are on a public model and are not, which
inverts the design's central promise at the most-used surface in the app. Fix: add
`throwOnError: true`; move `setConfigProvider` to *after* a successful session bind; add a `catch`
arm keyed on the privacy code that renders the repair card.

**P3 — the structured 409 does not exist; the route 500s everything.**
`routes/agent.rs:721-727` maps every `update_provider` error to
`(StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to update provider: {}", e))`, and the
declared `responses(...)` block at `:677-683` lists 400/401/424/500 — no 409. So a typed
`PrivacyRefusal` mapped to 409 with a JSON body
(`{code:"privacy_barrier", session_classification, provider_tier, available_private_providers}`),
the `responses` entry, and `just generate-openapi && npm run generate-api` must ship in the same
commit as Gate A, or Gate A ships as "Internal server error".

**P5 — the model chip is global, not per-session.** `ModelsBottomBar.tsx:38-45` reads
`currentModel`/`currentProvider` from the app-wide `ModelAndProviderContext`. The per-session
alternative is already dead code: `CurrentModelContext` is created at `BaseChat.tsx:100` and
exported as `useCurrentModelInfo` at `:101`, and **`CurrentModelContext.Provider` is never rendered
anywhere in the tree** — a grep across `ui/desktop/src` returns exactly those two declarations plus
the two consumers in `ModelsBottomBar`. So `currentModelInfo` is always `null` (the file even
carries the comment *"Since currentModelInfo.mode is not working, let's determine mode
differently"*), and the lead/worker label logic at `:117-121` is already partly inert. Open two
sessions on different providers — which BR-71's tabs make routine — and both chips show the same
string. A tier badge attached to that chip would be confidently wrong for every session not on the
global default. Drive the chip from the focused session (`GET /sessions/{id}` already returns
`provider_name` and `model_config`) or delete the dead context.

**P4, related — switching a model in one chat rewrites the global default.** The same function
follows the per-session bind with `setConfigProvider`, which writes `BIOROUTER_PROVIDER`/
`BIOROUTER_MODEL` app-wide; every new chat then starts on that provider via
`restore_provider_from_session`'s fallback. So "pick Versa once in a scratch chat" privatises not
one session but **every session created afterwards**, each ratcheting on its first turn. Decouple:
offer "Also make this my default for new chats" as an explicit checkbox. If that is too large a
behaviour change, the new-chat flow must render the tier of the inherited default *before* the
first turn with a one-click "start this chat on a public model instead".

### 14.4 Refusals that teach

Each names what happened, which two tiers collided, why the boundary exists, and the shortest way
forward.

**Gate A — model switch refused** (rendered from the 409 of P3):

> **Can't switch this chat to Claude Opus.** This chat is **private** — it has been running on
> Versa (versa_azure) since 14 Mar. Private chats only run on private models, so their contents
> never reach a model hosted outside UCSF.
> Available private models: **Versa · GPT-5.5**, **Versa · Claude Opus**, **Llama Server ·
> qwen3.5-4b**.
> To use a public model here, make this chat public first (History → this chat → Make public). That
> permanently exposes its contents and can't be undone.

**Gate B — turn refused after repair failed:**

> **This chat can't run right now.** It's marked private, but it's currently set to Claude Opus
> (public). This usually happens after branching a chat or restarting the app.
> **[ Switch to Versa · Claude Opus ]  [ Choose another private model ]  [ Make this chat public… ]**

The first button is the one-click fix: the wall is a repair prompt, not a dead end.

**Gate C — private extension, public model** (model-facing, returned verbatim as `ErrorData`):

> `ucsfomopagent` is a private extension: it reaches UCSF clinical data, so only private models may
> call it. This session is running on `anthropic` (public). Ask the user to switch this chat to a
> private model — Settings → Models, or the model chip in the composer — and then try again. This
> is a data-protection boundary set by the Biorouter marketplace, not something to work around: do
> not retry with a different tool name, through code execution, or through a resource read.

Three load-bearing properties: **deterministic** (identical on retry, so the model stops looping);
**names the human action** rather than implying the model can fix it; and **explicitly forecloses
workarounds**, following the register established by issue #42's operator-disabled extension gate.

**Never leak content in a refusal.** Refusals go into the model's context, so they name the tool and
the tier only — never a session title (LLM-generated content) and never a working directory.

### 14.5 Two low-cost changes that remove whole classes of surprise

**Relabel the provider groups so the two taxonomies are literally the same words in the same
place.** `providerOrdering.ts:64-82` labels three groups Local (green) / Institutional (indigo) /
Commercial (amber), and `PRIORITY_ORDER` puts `azure_openai: 0` and `aws_bedrock: 1` at the *top*
of the Commercial group. A UCSF user whose Azure OpenAI account is provisioned and paid for by UCSF
IT will read "my institution's Azure" as institutional; it is **public** under this design.
Conversely `ollama` pointed at a hosted SaaS badges **private**. Relabel to **"Private · Local"**,
**"Private · Institutional"**, **"Public · Commercial"**, with one line of card copy each:
Institutional → *"Private because Biorouter recognises this specific UCSF gateway endpoint"*;
Azure/Bedrock → *"Public — Biorouter can't verify where this account's endpoint points."*

> **Note on the Azure copy.** The obvious wording — *"Public: a direct cloud account, even if your
> institution pays for it"* — is not accurate as shipped. `azure.rs:200-205` gives
> `AZURE_OPENAI_ENDPOINT` a **default of `https://unified-api.ucsf.edu/general`**, the same UCSF
> gateway `versa_azure` uses (`versa_azure.rs:23`). A name-keyed tier therefore calls
> `azure_openai` Public even when it in fact resolves to the UCSF gateway — a conservative,
> fail-safe error, but the copy must not claim something the configuration contradicts.

**Show the pairing, not just the extension.** `~/.config/biorouter/config.yaml` enables extensions
**globally** with a single `enabled:` flag; there is no per-session enablement. Under Gate E a user
who enables `ucsfomopagent` sees **Enabled** in Settings while the tool is simply *absent* from
every public-model chat. A static badge describes a property of the extension; what the user needs
is a property of the *pairing*. The Extensions card renders a third state computed against the
focused session — **"Enabled · unavailable in this chat (public model)"** with the one-click switch
— and the composer's extension selector shows private extensions greyed with the same reason rather
than omitting them. Omission is what produces "the OMOP tool is broken".

### 14.6 The opt-out surface, and warned-rather-than-walled

**Settings → Privacy**, one switch, **on** by default:

> **Privacy tiers** — On
> Chats on private models (Versa, or a local model) stay private: a public model can't read them,
> can't call a private extension, and can't reach your knowledge bases through the shell.

Turning it off requires typing `DISABLE PRIVACY TIERS` and shows all four sentences — the third and
fourth are the ones a user cannot reconstruct for themselves, and are why this is a typed
confirmation rather than a switch:

> This turns off **every** privacy guardrail on this machine, for every conversation.
> Commercial models will be able to call UCSF clinical extensions, read private chat history, read
> and write your knowledge bases, and read your saved chats, memories and Biorouter apps straight
> off the disk through the shell.
> **While it is off, Biorouter stops recording which conversations touched private material.**
> Turning it back on will protect what is already marked private — but it cannot go back and mark
> anything that happened while it was off.

While off, a persistent amber strip sits in the settings sidebar and **every** privacy badge in the
app renders muted with the suffix **"— enforcement off"** — on the session list, the model chip and
the extension rows, not only in Settings.

`POST /config/set_provider` and the `/config/upsert` paths used by `ProviderGuard.tsx` and the
onboarding cards change the *default for future sessions*, not any existing one. Those surfaces show
the tier of what is being set and, if any private session exists, add: *"Existing private
conversations will keep their current model."*

**`LeadWorkerSettings` pre-flights before writing `BIOROUTER_LEAD_MODEL`** — *"Setting a public lead
model will make N private conversations unrunnable until you change it back."* — with the count
computed live, and the resulting composite badged in the chip as **Public**, not as the lead's name.
This control is one `DropdownMenuItem` away from the composer's model chip
(`ModelsBottomBar.tsx:202-208`).

### 14.7 The CLI is a required R10 surface

Every repair affordance above is a GUI card. `biorouter-cli/src/session/builder.rs:479-483` resolves
the provider as `--provider` flag → saved session provider → workflow's `biorouter_provider` →
global default; two of those four can produce a public provider on a private session, and one is a
**shared workflow YAML** pinning `anthropic`, which would now refuse to run in any private session
with nothing explaining why. Minimum CLI surface: (a) `biorouter session -r` prints the session's
tier at start; (b) Gate B's terminal refusal prints the available private models and the exact
re-run command; (c) `biorouter session declassify <id>` runs the same confirmation at the terminal
— which also gives Hidden and Terminal sessions a declassification path (§15.4); (d) a workflow
pinning a public provider fails at *load* with "this workflow pins `anthropic`, which cannot run in
a private session", not mid-turn.

### 14.8 Discoverability

One first-run tip the first time a private model is selected — *"Chats on private models stay
private. Public models can't read them, ever."* — dismissible, shown once. No tour, no banner, no
persistent nag. The badges carry the rest.

---

## 15. Migration

### 15.1 Schema

`privacy_tier TEXT NOT NULL DEFAULT 'public'` and `privacy_reason TEXT`, plus
`classification_audit`, all in the same migration.

**The migration number is not load-bearing, and must not become load-bearing** (execution plan
O10). `main` is at `CURRENT_SCHEMA_VERSION = 16`. The BR-71 worktree
(`feat/br71-workspace-control`) already has **17** with a written, working
`17 => ALTER TABLE sessions ADD COLUMN parent_session_id TEXT`. Whoever merges second silently
re-uses a number, and a database that already ran the other branch's 17 skips the second feature's
arm entirely — the exact incident `run_migrations`' own comment records for v11–v14. So this work
ships a **shape-guarded numbered arm plus an unconditional `ensure_privacy_schema`**, following the
`ensure_session_incarnation_schema` precedent (called from `reconcile_loop_schema`, itself invoked
*after* the version loop). With that, merge order is free in both directions and neither branch has
to wait on the other.

### 15.2 Backfill — fails open, by decision

```sql
UPDATE sessions SET privacy_tier = 'private', privacy_reason = 'backfill:' || provider_name
 WHERE provider_name IN ('versa_azure','versa_bedrock','llamacpp','ollama');
```

A fail-*closed* backfill (NULL provider + ≥1 message ⇒ private) was rejected: a user who has only
ever used a commercial provider would find a large slice of their history marked private on first
launch, refused on the model they normally use, with only an irreversible declassification as the
exit, one chat at a time. The column default is `'public'` and the backfill catches what the data
can prove.

Four counts logged at `info!` so the size of the annoyance is measurable on day one:
`backfilled_private`, `backfilled_public_named`, `backfilled_unknown_provider`, `backfilled_empty`.

**The residual, stated plainly rather than buried:** `provider_name` records the *last* provider,
not every provider. A session that ran on Versa and was later switched to a public model backfills
as public even though its transcript contains private-model work. There is no transcript scan and
there will not be one — it would be slow, heuristic, and wrong in both directions. Mitigation is the
badge plus one release-note line:

> *"Chats from before this version are marked by the model they were last using. If an older chat
> contains work you want kept private, switch it to a private model — it will be marked private from
> its next turn on."*

See §15.5 and §16 for what the backfill actually does to a real machine on day one.

### 15.3 The fail directions, and why they differ

| Situation | Direction | Reasoning |
|---|---|---|
| Migration backfill | fail **open** (public) | one-time, deliberate, user-visible; the alternative bricks a history the user cannot un-brick |
| Runtime read — column missing from a projection, unparseable value | fail **closed** (private, with `error!`) | a bug, not a decision. Every session shows a Private badge: immediately visible, immediately fixed, safe meanwhile. The tolerant `try_get(…).ok().flatten()` convention would silently read public, and `branch_point_msg_uid` already being absent from `list_sessions_by_types`'s projection is the live proof that a projection gets missed. |
| Import of a session with no `privacy_tier` | fail **closed** (private) | an imported transcript of unknown provenance is treated as sensitive; unlike migration, there is no local evidence to reason from |
| Unknown provider | Public | fail-**safe**, not fail-open: Public is the *less* privileged tier |
| Unlisted extension | Public | fail-open, **operator ruling R11(ii)**, isolated to one function `classify_extension(name)` with one const and one comment naming the ruling, so reversing it later is a one-line change rather than an audit |
| Any gate's lookup fails | refuse | encoded as a refusal inside `Ok(...)`, never as `Err` |
| NULL `parent_session_id` | `other` ⇒ read-only | safe for R6 |

### 15.4 Sessions, configs and extensions

**Sessions in flight when the app updates:** none survive. The daemon restarts, agents are rebuilt,
and every session resolves its provider through `restore_provider_from_session` on resume, which now
passes Gate A. Where the backfilled tier and the stored provider disagree, Gate B's repair path
rebinds from the row silently if it can.

**Existing configs:** no migration. No key renamed, no extension entry rewritten, `config.yaml`
untouched. The opt-out key is absent, which means the default (`on`). One sharp edge to pre-empt: a
user with `BIOROUTER_LEAD_MODEL` set to a public lead over a private worker now holds a **Public**
composite, so their private sessions become unrunnable until they change it — hence the pre-flight
warning in §14.6.

**Existing installed extensions:** no migration; the tier is resolved at admission from
`compiled_baseline ∪ last_good_fetch`. On first run after upgrade there is no stored fetch, so the
compiled baseline governs — `ucsfomopagent` and `cdwagent` are Private immediately, offline, with no
network. Nothing is uninstalled or disabled.

**Sessions outside History:** `list_sessions` filters to `session_type IN ('user','scheduled')`
(`session_manager.rs:3537`), so a private Hidden or Terminal session would be enforced forever with
no GUI declassification surface. **Do not add a "System sessions" filter to History** — on this
machine that would surface 511 hidden sessions (459 public, 52 private) into a user-facing list, a
regression traded for an edge case. Use the CLI escape hatch instead (`biorouter session declassify
<id>`, §14.7), which works by id regardless of `session_type`.

**Rollback:** downgrading the app leaves the columns in place and ignored, and
`classification_audit` inert. Nothing is moved, re-indexed or rewritten. The one-way door is the
backfill; rolling forward re-runs nothing, since the migration is versioned.

### 15.5 Day one must be shown, not discovered

1. The first-run notice states the **actual counts, computed from the user's own DB**, over the same
   population History shows — user + scheduled sessions with at least one message (§16) — for example
   *"642 of your 2,587 conversations are now marked private because they last ran on Versa or a
   local model."* Those are this machine's real figures on 2026-08-01; compute, never hardcode.
2. Grouped declassification extends to **all** `backfill:*` reasons, not only `backfill:unknown`,
   with a review-by-provider list (§12.6).
3. Run the backfill and show the counts **before** enforcement begins. One launch of "here is what
   will change" beats a week of unexplained refusals.

### 15.6 Migration test gates

Fresh DB at 17 → column exists, defaults public. DB at 16 with sessions on `versa_azure`,
`anthropic`, NULL → private, public, public. DB at 16 with `messages_fts` absent → the LIKE path is
filtered too. Round-trip: `privacy_tier` survives, and an attempt to lower it through the builder is
a no-op. `copy_session`, `diverge_session` and `import_session` of a private session each yield a
private branch carrying `provider_name`. Import with no tier yields private.

---

## 16. The honest cost

**Private is the default state on a real machine, not the exception.** Measured directly from
`~/.local/share/biorouter/sessions/sessions.db` (aggregate `provider_name` counts only, no message
content):

| session_type | would backfill private | public | NULL provider | total |
|---|---:|---:|---:|---:|
| **user** | **963** | 588 | 2,831 | **4,382** |
| scheduled | 121 | 76 | 0 | 197 |
| hidden | 52 | 668 | 0 | 720 |
| sub_agent | 42 | 33 | 0 | 75 |

**963 of the 1,551 user conversations whose provider is known — 62.1% — go private on first
launch.** 875 of those are `versa_azure` (733) or `versa_bedrock` (142); 88 are `llamacpp` (48) or
`ollama` (40).

> **Re-measured 2026-08-01, four days after the design was written, and the numbers moved a lot.**
> The **NULL-provider** bucket for `user` sessions went from 29 to **2,831** — two orders of
> magnitude — so the fail-open residual is far larger than first reported. Of those 2,831,
> **1,509 have at least one message**: 1,509 real conversations of unknown provenance that backfill
> **public**. Separately, History shows fewer rows than the raw counts imply, because
> `list_sessions_by_types` uses `INNER JOIN messages m ON s.id = m.session_id`
> (`session_manager.rs:4279`), so empty sessions never appear. The number the first-run notice must
> quote is user+scheduled **with at least one message** — **2,587** on this machine today.
> Re-measure at implementation time; this moved by a factor of three in four days.

And it is not an accident of one machine. `ProviderGuard.tsx:177-186` orders the onboarding cards
**Llama Server → Ollama → Institutional → Commercial** — three of the four first-run cards are
private-tier — and CLAUDE.md states the ordering is deliberate. A new UCSF user's default,
zero-setup path produces private sessions. **Every friction below should be re-costed at that
multiplier before it ships.**

Where this annoys someone who has done nothing wrong:

1. **You can never move a private conversation to a public model.** Not with a warning. A researcher
   who used Versa once and now wants Opus on the same thread must stay on Versa, start a new chat,
   or declassify. This is the design's central cost and it is deliberate. The
   ratchet-at-first-turn correction (§6.1) removes the mis-click version, and the graded
   confirmation (§12.4) removes the friction from the `turn:`-only majority.
2. **Turning on lead/worker with a public lead locks you out of your private chats.** `least()`
   makes the composite Public, so one global setting one click from the composer affects unrelated
   conversations, which reads as a bug unless the panel pre-flight-warns. At a 71% private base rate
   this is not a footnote.
3. **Right after upgrade, an orchestration loses control of its existing children.**
   `parent_session_id` only exists going forward, so every pre-upgrade subagent is `other` and
   read-only to its own parent. No clean mitigation; it resolves as new work is spawned.
4. **A remote llama.cpp or Ollama badges Private with no way to say otherwise** — the one place this
   design over-trusts. See §17.
5. **A private extension simply vanishes from a public model's tool list** (Gate E). Without the
   pairing-aware state in §14.5 this reads as "the OMOP tool is broken".
6. **Declassification is one chat at a time** for `mcp:`-reason sessions.
7. **A shared workflow pinning a public provider stops working in private sessions** with nothing
   explaining why, unless §14.7(d) ships.
8. ~~**A public chat cannot view `config.yaml` through a tool.**~~ **Not paid — [DR-17](privacy-tiers-execution-plan.md#scope-ruling--dr-17-narrows-this-plan-to-the-session-store) descopes
   §9.5.** A public chat reads `config.yaml` like any other file, so "why isn't my extension loading"
   debugging stays where it is. The corollary is the one §9.5.2 warned about and this design now
   accepts: **a master switch a public model can read and edit is not a switch**, so the toggle's
   integrity rests on nothing but the file's own permissions.
9. ~~**On Windows and on Linux without bubblewrap, a public chat loses the five tools that spawn a
   child process.**~~ **Not paid — [DR-17](privacy-tiers-execution-plan.md#scope-ruling--dr-17-narrows-this-plan-to-the-session-store) descopes §9.5.** `developer__shell` and its four
   siblings keep working for a public-capability chat on every platform. This was the single largest
   usability price in the design.
10. **The barrier is narrower than a reader of an earlier draft would expect**, and that is now the
   honest cost. A public model with a shell can read the session database and anything a private chat
   left on disk. R15's disclosure is the whole of the mitigation, which is why it is a requirement
   with a task and a gate rather than a paragraph in §14.
11. **A knowledge base a private chat touched is unreadable from public chats until the user
   publicizes it** — one click, but a click they must discover. [DR-18](privacy-tiers-execution-plan.md#dr-18--the-knowledge-base-tier-is-user-controllable-and-a-private-session-creates-a-private-base) adds the control; the
   confirmation names how many pages it releases, and releasing cannot be undone for content already
   read.

If these are not budgeted, the honest prediction is that the first user with 900 private chats and
a commercial subscription tries to turn the feature off — and discovers the R7 opt-out covers only
Gate C, i.e. not the part annoying them. They file a bug instead. (R7 is a master switch now; the
prediction stands for whatever the next narrowest reading of it turns out to be.)

---

## 17. Open questions needing an operator ruling

1. **Does a mixed lead/worker composite ratchet the session?** R3 says "switched to a private model
   even once → private permanently", and a private-lead/public-worker composite *contains* a private
   model. This design says it does **not** ratchet, because `tier = least` and the transcript has
   already gone to the public worker, and because ratcheting on `max` would make the bind gate
   refuse that same composite on the next resume. Using one reduction for both the gate and the
   ratchet is what makes `capability ≥ classification` provable by induction. **This is the single
   place the letter of a requirement was not followed.**
2. **Is the spawn-downgrade an approval or a refusal?** R4 permits it, so it is an approval showing
   the task prompt. But the prompt is written by a private-context model and is the only leak
   vector, and it is the one control a planted `PermissionRequest` hook could bypass — hooks load
   from `~/.config/biorouter/config.yaml` and, with `allow_project_hooks`, from
   `.biorouter/hooks.yaml` in the working directory, both writable by an agent with `text_editor`.
   An operator wanting zero risk makes it a `Deny`.
3. ~~**Does the R7 opt-out really stop at Gate C?**~~ **RULED — it stops nowhere.**
   `BIOROUTER_PRIVACY_TIERS=off` disables every gate, the ratchet and both read-deny layers
   (§10.6). The cost is that nothing is recorded while it is off and re-enabling does not
   reclassify the gap; the typed confirmation states it.
4. **Is the first cross-tier write approval remembered per (caller, target) or per call?**
   Per-pair-per-session-lifetime was chosen because a confirmation on every steer of a public worker
   is miserable and would be clicked through.
5. **Institutional Ollama versus hosted Ollama SaaS.** R1 says self-hosted *or* institution-hosted
   is private, and config cannot tell a lab GPU box at `OLLAMA_HOST=gpu.lab.ucsf.edu` from a hosted
   SaaS. "Non-loopback stays Private, the badge names the resolved host" is a **false-private** — the
   one place this design is permissive rather than restrictive. Certainty needs a
   `BIOROUTER_PRIVATE_HOSTS` allowlist, a new concept deliberately not added (it would need its own
   protection and settings UI).
6. **Should `versa_azure` get its own config keys?** It shares all three `AZURE_OPENAI_*` keys with
   the public `azure_openai` provider, whose shipped default endpoint is the same UCSF gateway. The
   demotion rule catches the dangerous direction, but it means a user can *lose* their private tier
   by configuring an unrelated provider.
7. **Should the compiled-in private baseline be a signed registry snapshot?** Signing would let a
   *downgrade* be trusted offline. Today the union rule means an extension can only ever gain a
   private badge without a fresh fetch — safe, but a genuine reclassification-to-public needs
   connectivity.
8. **Who is "who" in the declassification record?** The app is single-user, so the local OS username
   is recorded. On a shared lab machine that is right; in a multi-account setup it is not, and there
   is no user identity in the product to record instead.
9. **Skills (R12) carry no classification, which leaves three gaps.** (a) A skill authored while a
   private chat was open can embed pasted private text and is then readable by every session and
   publishable to the marketplace. (b) A skill can instruct the model to call `ucsfomopagent` —
   harmless in effect because Gate C refuses at dispatch, but the steering is unblocked and produces
   confusing refusals. (c) BR-71 Task 15 lets one session add skills to another. The v1 mitigation
   is a line in the skill-creation UI; closing (a) needs skills to carry a classification, which
   contradicts R12.
10. **`ActiveWorkItem.title` is cross-session content and predates all of this** — derived from a
    subagent's task prompt and surfaced process-wide with a session id. The visibility rule is
    applied to it, but it is exposed only via `GET /active_work` for the GUI (the model-facing
    `workspace_read_conversation` / `workspace_watch` are session-scoped), so it may deserve its own
    fix rather than riding this one.
11. **`POST /agent/call_tool` remains inspector-free.** This design is correct either way because
    the barrier is in the extension manager, but the route is a standing hazard for every *future*
    inspector-based control, including BR-71's.
12. **Should the daemon's HTTP API authenticate a caller that is on the same machine?** §9.5.5: the
    API secret is recoverable from the daemon's own environment, so the header check stops a remote
    caller and not a local one. §9.5's barrier covers the largest local route because that route
    dispatches through the same choke point; it does not cover the routes that return private
    content without running a tool, of which `GET /diagnostics/{id}` is the widest. A per-caller
    credential the daemon does not hand to its own children is the shape of the fix, and it is
    probably the same fix as the app socket's.
13. **Should the Agent Drafter app socket be authenticated by something a local client cannot
    obtain?** §9.5.5: `agent_drafter__list_apps` hands a public model every app id, and an app id is
    all the unauthenticated `GET /apps/{id}/` page needs to yield that app's socket token. The
    alternative — reclassifying `agent_drafter` as Private in §10.3 — closes it at Gate C and takes
    the whole drafter workflow out of every public chat, which is a much larger behaviour change and
    needs a ruling rather than an implementation decision.

---

## 18. Relationship to BR-71

BR-71 ([issue #30](https://github.com/BaranziniLab/biorouter/issues/30),
[design](docs/agent-loop/designs/agent-workspace-control.md),
[44-task execution plan](docs/agent-loop/designs/br71-execution-plan.md)) is the plan of record for
agent workspace control and glass-box subagents. Its design doc's status line reads *"Current —
proposal only; nothing below is implemented."*

**This work must land before BR-71's cross-session tools ship.** Task 15's `workspace_set_tools
{ provider, model }` changes **another session's** provider and, per the plan's own table, takes
effect next turn — a first-class, agent-callable path to attach a public model to a private session.

### 18.1 Prerequisites, each of which justifies itself without BR-71

| # | Item | Standalone justification |
|---|---|---|
| P1 | `ProviderTier` / `Classification`, `ProviderMetadata.tier`, `Provider::tier()`, `LeadWorkerProvider::tier() = least` | foundation |
| P2 | **Gate A** + the monotone `CASE WHEN` | also fixes `ClientFrame::ModelSelect`, verified today to swap a session's provider from a browser page with no check of any kind |
| P3 | **Gate B** + the `Agent::provider()` assertion | LRU rehydration and the `restore_provider_from_session` global fallback are present-tense downgrades |
| P4 | **Gate C** + its resource/prompt siblings, and **Gate E** | R7 standalone; covers `/agent/call_tool` and `execute_code`, which no inspector reaches |
| P5 | **Gate D** — both chatrecall builders + the LOAD-mode check | today's most direct cross-session read path; LOAD has verified zero filtering |
| P6 | `create_session` carries `privacy_tier` + `provider_name` + `model_config` for all three copy paths | a live laundering path, verified |
| P7 | The generator's second and third outputs + `--check` | the badge cannot exist in Rust without it |
| P8 | A1, B3 from §9.3 | the two findings that would let a public model read private content on day one |

### 18.2 Additions to named BR-71 tasks

**Task 1 (`sessions.parent_session_id`, migration 17).** Add `privacy_tier` + `privacy_reason` in
the same DDL, the same `ALTER TABLE` arm and the same backfill pass, plus `classification_audit`.
Task 1's Step 3 already enumerates every touchpoint — `CURRENT_SCHEMA_VERSION`, the `Session`
struct, the fresh-DB DDL after `diverged_from TEXT,`, the migration arm, the row mapper, the INSERT
column list, the builder field/setter/emission, and both explicit SELECT projections. This
duplicates that checklist exactly, and its warning that a missed SELECT "compiles and silently reads
`None`" is precisely why this design's reader is fail-closed.

**Tasks 4 / 12 (`workspace_list`).** Projection includes `privacy_tier`; rows carry the badge; a
public caller's list **omits** private rows. Task 4 already amends this projection for
`parent_session_id` and `session_type`.

**Task 10 (`WorkspaceMutationInspector`).** Three additions, one of which changes a verdict class:

- The plan already specifies a confirmation for a provider switch, reason *"switches this
  conversation to provider '{provider}', which sends its whole history to that provider's
  endpoint"* (`br71-execution-plan.md:4829`) — the exact sharp edge arriving in the exact code path.
  **When the target is private and the provider public, `RequireApproval` must become `Deny`**: a
  confirmation is the wrong control for a boundary the user may cross only through R9. The
  precedence machinery makes it free — `apply_inspection_results_to_permissions` removes a `Deny`'d
  request from both `approved` and `needs_approval`, and `Allow` is explicitly a no-op, so `Deny`
  beats Auto mode and any stored always-allow with no new machinery.
- The spawn-downgrade confirmation and the first cross-tier write confirmation ride **this**
  inspector, not a new one.
- **Register the inspector's name in `handle_denied_tools`.** Without that one arm, every
  cross-session privacy refusal tells the model the user declined, which is false and invites an
  identical retry. Needed only for Task 10's `Deny`s — Gate C bypasses this machinery entirely.

**Task 15 (`workspace_set_tools`).** Three constraints, all resolved off lookups the task already
performs: `{ provider, model }` **must call `Agent::update_provider`** rather than reimplement the
persist; `add_extensions` naming a private extension gains the tier refusal beside the issue-#42
operator-disabled gate the plan already wires in at `get_extension_entry_by_name`; and lineage gates
the whole tool to `self`/`child`, with a private→public invocation raising the first-crossing
approval.

**Tasks 17 / 19 / 13 / 14 / 16 / 24 (the tool surface).** The matrix in §7 covers `workspace_list`
(12), `workspace_read_conversation` (13), `workspace_send_prompt` (14), `workspace_set_tools` (15),
`workspace_close` (16), `workspace_watch` (17), the merged `subagent` (19) and `workspace_open` (24).

**Task 32 (spawn-context persistence).** Stamp `parent_session_id` **inside
`create_subagent_session`'s INSERT** rather than after `override_system_prompt`, and stamp
`privacy_tier` in the same statement. Today the child row is created with only
`(working_dir, name, SessionType::SubAgent)` and its provider lands later — and for a *background*
subagent the whole stretch runs in a detached `tokio::spawn`, so a daemon kill leaves a durable
`SubAgent` row with no provider and no parent. One INSERT closes both windows.

**Task 36 (the subagent guard).** The existing refusal (a `SessionType::SubAgent` session may not
call the subagent tool) is the shape and the location; the lineage and tier checks belong beside it.

**Tasks 22-28 (GUI).** Tab-bar dots, workspace-row badges, provenance chips, set-tools toasts.

### 18.3 What this asks BR-71 to change: nothing structural

Not its tool surface, schemas, UI, task ordering or worktree strategy. Every intersection above is
one column in a DDL edit already happening, one branch in an inspector already being written, or a
constraint on which existing function a handler calls. That is deliberate: **a policy that forces a
re-plan of an approved, about-to-be-built feature does not get built.**

### 18.4 Independent and parallelisable

- The whole `landing/` change (§13). Ships on its own cadence via `deploy-landing.yml`; enforcement
  runs off the compiled-in const, so the website blocks nothing.
- The last-good-fetch persistence and fetch timeout in `main.ts`; surfacing `live: false`.
- Replacing `provider_class` in `routes/apps.rs` with the shared tier function (a genuine bug fix —
  ship it separately so it is not gated on this design landing).
- `configure_worker_provider`'s missing parent-inheritance and `AgentManager::get_or_create_agent`'s
  default-provider fallback.
- The knowledge-macro `ModelRef` gate and the prompt-hook provider check (v1 emits a load-time
  warning; the hard skip is v1.1). Both run against session content on an unrecorded provider:
  `crates/biorouter/src/hooks/mod.rs` `resolve_prompt_provider`,
  `crates/biorouter/src/agents/knowledge_tool.rs` and `routes/knowledge.rs`.
- The `UserConfirmation` ZST and the repo-grep tests — belt over braces; `privacy_reason` plus a
  `warn!` gives auditability on day one.
- `classification_audit` as a table (the same reasoning).

---

## 19. Suggested implementation order

1. **`chatrecall` LOAD-mode guard, and the `platform__ingest_conversation` guard beside it.** One of
   these is five lines; both are fully-open cross-session reads in the product today (§2.3 item 2),
   and `ingest_conversation` is the worse of the pair because it *writes what it read* into a
   machine-wide knowledge base. Ship the two together, on their own, ahead of everything.
2. **A1's remaining half (the stdio-spawn scrub, then the secret off the environment entirely)** and
   **B3 (refuse a global `remember_memory` from a private-capability session)**. The shell half of A1
   and both of B3's original channels have already shipped as #57, #58 and #63; what is left of A1 is
   still a live leak independent of this design.
3. **P1 + P2 + P3 (types, Gate A, Gate B)** together with **the typed 409 and the `throwOnError`
   fix**, in one commit. Gate A without them ships as "Internal server error" over a green success
   toast.
4. **B1** — carry-over on `create_session`, with the call-site enumeration test.
5. **Gate C + siblings + Gate E** (with B6's precomputed allowed-set and shared prefix resolver),
   plus the generated `registry_private.rs` and its `--check`.
6. **Gate D** — both builders, with the required-constructor-parameter threading.
7. **Migration + backfill + the day-one notice** (§15.5).
8. **Badges and the per-session model chip (P5)**, then declassification.
9. **C2, P6, the CLI surface**, then the landing site.

---

## 20. Claims verified against the tree, and corrections made

Every anchor below was verified at `708390d8` and has since moved; see
[the execution plan's drift table](privacy-tiers-execution-plan.md#read-this-before-you-chase-a-line-number).
**The bare line numbers this section used to carry have been removed rather than re-verified**, because
a stale number here is worse than none — it reads as a citation and is not one. What survives is the
record of *what* was checked, anchored on the symbol, which is the only part that was ever durable.

Verified by reading the code at `main` (708390d8) before writing: `CURRENT_SCHEMA_VERSION = 16`; the
eight-field `ProviderMetadata`; `providerOrdering.ts`'s two `Set`s; `update_provider`'s
swap-before-persist and its status as the **sole** writer of both `Agent::provider` and
`sessions.provider_name`; `restore_provider_from_session`'s `Config::global()` fallback;
`chatrecall` LOAD's complete absence of filtering; both `chat_history_search` builders' existing
`INNER JOIN sessions s` and SQLite-applied `LIMIT ?`; every `extension_manager.rs` symbol
(`filter_tools`, `get_all_tools_cached`, `get_client_for_tool`, `read_resource_tool`,
`read_resource`, `get_ui_resources`, `list_resources`, `dispatch_tool_call`, the SecretGuard
comment, `list_prompts_from_extension`, `list_prompts`, `get_prompt`, `add_extension`, `add_client`,
`add_inprocess_server`, the `Extension` struct's six fields, the `Frontend` refusal);
`copy_session`/`diverge_session`/`import_session` each hand-rolling a builder that omits
`provider_name`; `provider_class`'s exact-equality inversion; `routes/agent.rs`'s 500-only error
mapping and its `call_tool` inspector bypass; `ClientFrame::ModelSelect`'s unchecked
`update_provider`; `ModelAndProviderContext.tsx`'s missing `throwOnError` and its global
`setConfigProvider` write; `CurrentModelContext` never being rendered; `memory/mod.rs`'s
global-memory system-prompt injection (**since deleted by #58 — see §9.3 B3**);
`DEFAULT_SECRET_PATTERNS`; the secret key in
`biorouterd.ts`'s `additionalEnv` and the absence of `env_clear` in both `shell.rs` (**since scrubbed
by #57**) and the stdio spawn (**still open**);
`LeadWorkerProvider::get_name()` returning the lead's name; `factory.rs`'s
`BIOROUTER_LEAD_MODEL` intercept; the `COALESCE` accumulation precedent in `session_manager.rs`;
the `messages_fts` contentful DDL; `registry.json` (version 1, 37 extensions, 129 skills, ten keys,
no classification field, `spokeagent-0.4.1` the only version-suffixed id, `medcp` and `msbaseagent`
absent); `medcp` enabled locally with `CLINICAL_RECORDS_*`; `baam.html`'s render functions and
`.ext-tags` clipping; `shared.css`'s tag variants; `build-registry.mjs`'s `data-license` idiom;
`docs.html`'s hand-written table; `ProviderGuard.tsx`'s onboarding card order; `scheduler.rs`'s
global-config provider; the CLI plan-mode `get_reasoner` path; `apply_settings_overrides`'s
name-only extension narrowing; BR-71's 44-task execution plan including Task 1's migration 17, Task
10's confirmation reason string, and Task 17's `workspace_watch`; and the session-provider aggregate
counts (**re-measured 2026-08-01 — see §16**).

**Corrections made to the input material:**

| Claim as received | Correction |
|---|---|
| The session DB is at `~/.config/biorouter/sessions/sessions.db`, so add `**/.config/biorouter/**` to the secret floor | It resolves through `Paths::data_dir()` — `~/.local/share/biorouter/sessions/sessions.db` on this machine. A `.config`-scoped pattern would not match. Corrected in §9.3 A2. |
| The public `azure` provider shares three config keys with `versa_azure` | The registry name is `azure_openai`, and its shipped `AZURE_OPENAI_ENDPOINT` **default is the UCSF gateway itself** (`azure.rs:204`). The demotion rule is unchanged, but the proposed UX copy ("a direct cloud account, even if your institution pays for it") would be inaccurate. Corrected in §14.5. |
| The knowledge active-KB is a global file, so a public session inherits a private session's KB by default | `paths.rs:66-71` adds `.active-kb-sessions`, one file per session, with `.active-kb` as the primary fallback. The KB-as-shared-sink attack stands (any session may name any KB); the "inherits by default" framing does not. Corrected in §9.3 B4. |
| The design's fix list covers `copy_session` | `diverge_session` (`:4204`) is the primary GUI diverge path and does **not** call `copy_session`; `import_session` (`:4096`) is a third. Corrected in §9.3 B1, and the fix moved onto `create_session`. |
| History would gain a "System sessions" filter so Hidden sessions have a declassification path | That surfaces 511 hidden sessions on this machine into a user-facing list. Replaced with the CLI `declassify <id>` escape hatch. §15.4. |
| Hidden sessions: 435 public / 52 private (487 total) | Re-measured: 459 public / 52 private (511 total). Re-measured again 2026-08-01: 668 public / 52 private (720 total) — the point is that this bucket grows continuously, not that any one figure is right. §16. |
| `provider_class` at `routes/apps.rs:2061-2098` | The function is at `:2089`; `:2061` is the start of its doc comment. |
| `filterExtensions` at `baam.html:3906` | `:3909`. |
| BR-71 is approved and about to be built | Its design doc's own status line reads *"Current — proposal only; nothing below is implemented"*, and issue #30 is open. The dependency argument is unaffected; the wording is. §18. |
| The BR-71 design doc lists the workspace tool surface | The design doc does not mention `workspace_watch`; the **execution plan** does (Task 17), and it is the authoritative task list. Both are cited. |

Nothing in the input material was found to be wrong in a way that changes a gate placement or a
matrix cell.

---

## Related documentation

- [Agent workspace control and glass-box subagents (BR-71)](docs/agent-loop/designs/agent-workspace-control.md) — the feature this must land ahead of.
- [BR-71 execution plan](docs/agent-loop/designs/br71-execution-plan.md) — the 44-task plan whose Tasks 1, 4, 10, 12–17, 19, 22–28, 32 and 36 this design amends.
- [Secret storage](docs/security/secret-storage.md) — the credential model the §9.3 A1 fix touches.
- [Tool routing](docs/agent-loop/tool-routing.md) — the chatrecall/workspace split Gate D sits inside.
- [Subagents](docs/agent-loop/subagents.md) — the inheritance behaviour §8 gates.
