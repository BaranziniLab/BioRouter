# Privacy tiers — implementation brief

> **What this is.** The entry point for implementing [issue #56](https://github.com/BaranziniLab/biorouter/issues/56).
> It tells you where to work, which rulings are closed, which approaches are already dead and why,
> how to stage the work, and what verification actually proves. It does **not** replace
> [`privacy-tiers.md`](privacy-tiers.md) (the design) or
> [`privacy-tiers-execution-plan.md`](privacy-tiers-execution-plan.md) (the fifty-one task units) —
> it tells you how to use them and where they are known to be wrong.
> **Status:** Proposed — the plan it fronts has failed four independent adversarial review rounds and
> is expected to change again during implementation. Read [the brief is subject to change](#the-brief-is-subject-to-change)
> before anything else.
> **Audience:** the engineer or agent implementing #56, and whoever reviews that work.

Privacy tiers give every model, session, extension and knowledge base one of two tiers. A session's
**capability** is the least-privileged model bound to it; its **classification** is the most
sensitive thing it has touched, ratcheted permanently. The single invariant is that a public-capability
model must never reach private material — not once, not read-only, not indirectly. That invariant is
easy to state and has proved genuinely hard to enforce: the design and the plan have been rewritten
ten times, and four independent review rounds each found real defects the previous round's authors
and reviewers had missed. This brief exists so that the fifth round does not repeat the first four.

---

## The brief is subject to change

In the operator's own words: **"the plan can be subject to change if problems arise or things that
can compromise the security arise."**

That is not a disclaimer. It is a working instruction, and the history says you will use it. Four
rounds of review have found four architecturally distinct bypasses, each one invisible to the round
before it. There is no reason to believe the fifth round's implementation is the first that will not
turn one up. Treat "I have found something that weakens the barrier" as an **expected** event with a
defined procedure, not as a failure to be worked around quietly.

### The amendment protocol

If implementation uncovers a bypass, an unsound assumption, an enforcement point the plan does not
cover, or anything else that would weaken the barrier:

1. **Stop.** Do not narrow the design to fit what you have already built, and do not carve out an
   exception so the current task can close. Every one of the four failures below was, at the moment
   of discovery, expressible as a small carve-out.
2. **Write the finding down** in the plan, in the task it belongs to, with the *measurement* that
   established it — a `file:line` in the real tree, a command and its output, or a test that fails.
   A finding without a measurement will be re-argued.
3. **Raise it** to the operator, naming which settled ruling it touches and what the options are.
4. **Do not silently narrow a settled ruling.** The rulings in
   [the settled rulings](#the-settled-rulings) are the operator's, not the plan's. If the tree cannot
   express one, that is a finding to raise, not a specification to soften.

Two precedents, both real and both in this document's history:

- The design's §9.5.3 once carried a carve-out reading *"what Layer A does not cover, deliberately:
  `kb_read_page`, `retrieve_memories`, `read_app`, `list_apps`."* That sentence **redefined DR-14
  rather than implementing it**, and the ninth round rejected it. The right response was Task 14E (a
  guard in each root's own resolver), not a shorter promise.
- The tenth-round revision note contains the sentence *"⚠ **Defined* was as far as that fix went;
  nothing emitted the field until the tenth round below."* A field was added to satisfy a type
  assertion, the gate went green, and the control it existed for was dead on both kernels for a full
  round. Every type assertion passed.

**What must never be silently narrowed:** DR-1 through DR-16, and the two structural rulings that came
out of the review rounds — *the barrier sits at a choke point, never on an enumeration* (see
[the killed-approaches register](#the-killed-approaches-register)), and *the capability is sampled once
at the outermost entry and threaded* (O15).

---

## How to read the material

Read in this order. Do not start in the execution plan.

| # | Document | Why, and what to take from it |
|---|---|---|
| 1 | **This brief** | Where to work, what is settled, what is dead. |
| 2 | [`privacy-tiers.md`](privacy-tiers.md) (2,257 lines) | The *what* and *why*. §3 (settled requirements R1–R14), §4 (the two lattices), §7 (the capability matrix), §9 (enforcement), §9.5 (the read-deny). ⚠ **Every Rust line number in the design is stale** — it was verified against `main` at `708390d8`. Read it for the model, never for an anchor. |
| 3 | [`privacy-tiers-execution-plan.md`](privacy-tiers-execution-plan.md) (18,160 lines) | The *how*. Read its preamble in full — "Read this before you chase a line number", "Non-negotiable orderings" (O1–O16), "Departures from the design" (D1–D9), "Accepted risks" (AR-1–AR-15), "Which test filters are validated", "Decisions of record" (DR-1–DR-16), "Open questions" (1–24). Then read only the task units for the stage you are on. |

**The execution plan is detailed and it has known-wrong code in it.** Three separate review rounds
found prescribed snippets that cannot compile, and the plan itself has recorded some of the same
symbols as absent elsewhere on its own pages. Measured examples, all from round 3:

- Task 14D called `Agent::capability_tier()` and `Agent::working_dir()` — neither exists, and the plan
  had already recorded at its own line ~7005 that `Agent` has no capability method.
- The Developer re-check read `self.private_data` on `DeveloperServer`, which has no such field, in a
  handler (`text_editor`) that receives no `RequestContext` to derive one from.
- `expand_tilde(candidate)` was called by a module that neither defines nor imports it; the only such
  function is private to `crates/biorouter/src/security/policy/command.rs`.
- `Config::all_values().get(…)` does not compile — `all_values()` returns
  `Result<HashMap<…>, ConfigError>`.
- `tempfile` was prescribed for production `seatbelt.rs` while being a dev-dependency only.
- Task 12 put a method on `SessionStorage` and called it on `SessionManager`, which exposes only
  `storage()`.

Those six are repaired. **One is not, and it was found while writing this brief** — it is the live
example of what the rest of this section is about:

> **The contrast gate is already vacuous.** Tasks 26 and 32 expect
> `node scripts/check-contrast.mjs` to print `OK — all 288 contrast assertions pass` *after* #56 lands,
> from a stated baseline of 252 plus Task 26's 36 new assertions. Measured on `main` today, with no #56
> code anywhere: it prints **`OK — all 288 contrast assertions pass`**. `check-contrast.mjs` gained
> **+73 lines** between the plan's anchor `9558c346` and `main` (`grep -c privacy` over it → **0**), so
> the plan's post-#56 total is now the *pre*-#56 baseline. That gate passes today, and it would still
> pass if Task 26's 36 assertions never landed. **Re-measure the baseline, correct both tasks, and
> record the correction** — if Task 26's arithmetic holds (252 → 288 pre-existing, +12 hover-ground,
> +24 badge) the number to assert is **324**.

The lesson is the general one, and it applies to snippets and to numbers nobody has checked yet:

> **The compiler is the truth. The plan is a strong hypothesis.** Where a prescribed snippet does not
> compile, fix it against the real signature — and **record the deviation in the task**, in the same
> commit. A silent fix means the next reviewer re-derives the same problem, and it also hides the case
> where the snippet was wrong because the *design* was wrong.

Same rule for line numbers. **The named symbol is the anchor; the line number is a hint.** The plan's
anchors are against `9558c346`; `main` has moved. Measured drift on files the plan anchors into,
`9558c346` → current `main`:

| File | Change |
|---|---|
| `crates/biorouter-mcp/src/developer/rmcp_developer.rs` | +892 lines (issues #64/#67/#68, the file-tool jail) |
| `crates/biorouter-mcp/src/memory/mod.rs` | +615 |
| `crates/biorouter-mcp/src/knowledge/macros/ingest.rs` | +442 |
| `crates/biorouter-mcp/src/developer/shell.rs` | +416 |
| `crates/biorouter-mcp/src/knowledge/macros/query.rs` | +128 |

Two concrete near misses that already exist: `MemoryRouter::get_memory_file` is `memory/mod.rs:434` on
`main` and `:336` in the plan; Landlock's write-only handling is `shell_sandbox/linux.rs:415`
(`AccessFs::from_write(abi)`) and `:399` in the plan. A near miss reads as a hit — that is how BR-71
lost time, and it is why every Files table in the plan states its base.

---

## Where to work

### Base: `main`

All three documents are on `main`. Confirm you have them before writing a line:

```bash
cd /Users/wgu/Desktop/BioRouter
git log --oneline -5 -- docs/security/privacy-tiers-execution-plan.md
# The most recent entry must be 0d37998e ("land DR-16, the user-only tier raise, on main").
git rev-parse --abbrev-ref HEAD          # main
grep -c "DR-16" docs/security/privacy-tiers-execution-plan.md   # 4 — if 0, your checkout is stale
```

A stale checkout is the cheapest possible way to lose a day, and this feature has already lost a
ruling to one (below).

### Do **not** use `feat/privacy-tiers`, and do not use `/Users/wgu/Desktop/BioRouter-privacy`

The branch name and the worktree both look like the obvious place to work. They are not.

- The branch carried the **documents only**. `git diff --name-only main...feat/privacy-tiers` shows no
  production code that belongs to #56 — the non-doc files in that diff are `main`'s own commits the
  branch does not have.
- It is **stale in both directions**, measured: `git rev-list --left-right --count main...feat/privacy-tiers`
  → `63 1`. `main` is 63 commits ahead. The branch's copy of `privacy-tiers.md` is 1,881 lines against
  `main`'s 2,257 — it is missing the whole two-layer read-deny rewrite (`35bf782e`, `6d6a7eca`,
  `72dc9de2`).
- Its one unmerged commit was a **settled operator ruling that never reached main**: `500e9b1d`
  recorded DR-16 (the user-only tier raise) and the merge that landed 18,000 lines of plan did not
  carry it. It has now been cherry-picked onto `main` as `0d37998e`. Nothing else on the branch is
  wanted.

Continuing on that branch would fork the docs — you would be editing an execution plan whose design
document is 376 lines behind the one everyone else reads.

### One fresh branch and worktree per stage

Stages stay independently reviewable and revertible. Branch off `main`, in a worktree:

```bash
cd /Users/wgu/Desktop/BioRouter
git worktree add -b feat/privacy-tiers-stage1 ../BioRouter-privacy-s1 main
# later, each off the CURRENT main, after the previous stage merged:
git worktree add -b feat/privacy-tiers-stage2 ../BioRouter-privacy-s2 main
git worktree add -b feat/privacy-tiers-stage3 ../BioRouter-privacy-s3 main
git worktree add -b feat/privacy-tiers-stage4 ../BioRouter-privacy-s4 main
```

A stage merges to `main` **only after its own verification passes** — its phase gate, the full suite,
and an adversarial review of its diff. Conventional commits; **no `Co-Authored-By` trailers**, CI
rejects them. Do not push.

### Two hazards measured in this campaign

**Concurrent `cargo` across worktrees will OOM-kill builds mid-run, and a killed build reads like a
test failure.** There are nine worktrees on this machine already. Keep `CARGO_BUILD_JOBS` modest
(`CARGO_BUILD_JOBS=4` is a reasonable ceiling when anything else is compiling) and do not run several
stages' suites at once. If a suite dies with a signal rather than a failing assertion, suspect the
OOM killer before suspecting your code.

**BR-71 (issue #30) is being implemented concurrently on `feat/br71-workspace-control` and collides
with Stage 2.** Measured: that branch is 88 commits ahead of `main` and its diff touches
`crates/biorouter/src/agents/agent.rs`, `crates/biorouter/src/agents/extension_manager.rs`,
`crates/biorouter/src/agents/extension.rs`, `crates/biorouter-server/src/routes/session.rs` and the
whole new `crates/biorouter-server/src/workspace/` tree. Stage 2 threads a `CallCapability` through
`Agent::dispatch_tool_call` and `ExtensionManager::dispatch_tool_call` — the two functions BR-71
rewrites most. **Rebasing on `main` after BR-71 merges is cheaper than resolving that conflict twice.**
Two related facts already recorded: BR-71 also takes migration 17 for `parent_session_id` (O10 and
open question 12 make either merge order safe *in the database*, but the two diffs conflict textually
in `session_manager.rs`), and O16 sequences Task 14E relative to 14B.

---

## The settled rulings

Decided by the operator. **Not open for re-litigation in a PR.** If the implementation contradicts one,
the implementation is wrong; if the *tree* cannot express one, that is a finding to raise under
[the amendment protocol](#the-amendment-protocol), not a licence to narrow it.

The operative sentence of each is quoted verbatim below. The full row — the reasoning, the costs
accepted and the alternatives rejected — is in the plan's [Decisions of record](privacy-tiers-execution-plan.md#decisions-of-record);
read the full row before implementing against it.

| # | The ruling, verbatim |
|---|---|
| **DR-1** | "**Private models** are institutionally hosted (`versa_azure`, `versa_bedrock`) and user self-hosted (`llamacpp`, `ollama`). **Public** is everything hosted by an AI company or a large cloud — including `azure_openai`, `aws_bedrock`, `databricks` and `vertex`, whatever their names suggest." |
| **DR-2** | "**Two lattices, opposite directions.** CAPABILITY (what a session may DO) = the **least** privileged model bound to it… CLASSIFICATION (how sensitive its CONTENTS are) = the **most** sensitive thing it has touched, a permanent ratchet." |
| **DR-3** | "**A public model must never reach a private session.** Not once, not read-only, not indirectly. The converse is unrestricted: a private model may read anything." |
| **DR-4** | "**The ratchet fires on the first TURN and on a permitted private-extension dispatch — never on the bind.**" |
| **DR-5** | "**Lineage decides write access.** Sessions the caller spawned get full control; everything else is read-only. Lineage is **one hop** — a grandchild is `other`." |
| **DR-6** | "**The BAAM registry is the sole grantor of a private badge, and anything not on BAAM is PUBLIC** (fail-open, by decision). The private set is exactly **`ucsfomopagent`** and **`cdwagent`**." |
| **DR-7** | "**`chatrecall` obeys the barrier**… **Side channels (existence, counts, timing) are explicitly out of scope**: no count padding, no constant-time responses, no decoys. Only content must not cross." |
| **DR-8** | "**Declassification is the user's alone** — an explicit deprivatise action in History. Nothing automatic, nothing an agent can invoke." |
| **DR-9** | Superseded by DR-15. The Gate-C-scoped opt-out is **retired**, not kept alongside the master switch. |
| **DR-10** | "**Fail directions differ by kind, deliberately.** Migration backfill → fail **open**. Runtime read of a missing/unparseable column → fail **closed**. Import with no tier → fail **closed**. Unknown provider → **Public**. Unlisted extension → **Public**. Any gate's lookup failing → refuse, encoded inside `Ok(..)`, never as `Err`." |
| **DR-11** | "**`medcp` stays callable by a public model**, and that is the accepted cost of DR-6." |
| **DR-12** | "**`spokeagent` is public.** SPOKE holds no patient data." |
| **DR-13** | "**A knowledge base ratchets on ingest**… A KB takes the tier of the most sensitive session that has ingested into it, and a public-capability session may not read *or write* a private KB." |
| **DR-14** | "**A public-capability session's tools may not reach Biorouter's own private data, on by default, and the control is TWO layers.**… The entries are the four roots the operator named — the session store, the knowledge roots, the global memory root and the Agent Drafter app root — plus one file, `<config>/config.yaml`… Everything else on the filesystem stays readable and writable — this is **not** a general jail and must not become one. **Private-capability sessions are unaffected.**" |
| **DR-15** | "**One master toggle turns the entire privacy-tier feature off**, config key `BIOROUTER_PRIVACY_TIERS`, default `on`." ⚠ Session copy is **not** on the list of things it disables; the badges do **not** disappear, they restyle and suffix *— enforcement off*. |
| **DR-16** | "**Raising a session's capability to Private is the user's act alone. A model may never do it.**" ⚠ **DR-16 has no task written for it.** The ruling names a `Task 18A` that does not exist in the plan. See [Stage 4](#stage-4--the-master-toggle-the-user-only-tier-raise-and-the-ui). |

Two further rulings are structural rather than policy, came out of the review rounds, and carry the
same weight:

- **The barrier sits at a choke point, never on an enumeration.** Phrase every gate as *"every tool
  call passes through symbol X"*, never as *"the list of tools that read files"*. See the register
  below for why.
- **The capability is sampled once at the outermost entry and threaded** (O15). Four production entries
  reach a tool call — the agent loop, `POST /agent/call_tool`, the `execute_code` bridge, and
  `Agent::call_prefetch_tool`. Each captures one `CallCapability` (provider tier *and* master toggle,
  in one instant) and everything downstream takes it as a parameter.

---

## The killed-approaches register

This is the most valuable section in the brief. Everything below was tried, shipped past its own
green test suite, and then killed by a reviewer. **Do not re-propose any of it.**

### The one lesson: every enforcement design that enumerates gets defeated one level down

Enumeration has now lost **four** times, at four different levels of abstraction, and each loss was
invisible to the level above it:

| Round | The enumeration | How it was defeated | Evidence |
|---|---|---|---|
| **1** | Enumerate **tools** — classify the tools that touch private data | Missed the arbitrary-execution builtins entirely. `developer__shell` runs any command, the shell is explicitly not jailed by the file tools' base, and the OS sandbox that could confine it defaults to **Off**. A public model never has to defeat a tool gate; it reads `sessions.db` — which carries a *contentful* FTS mirror of every message by design — straight off disk. | `shell_sandbox/mod.rs:271` (`_ => SandboxMode::Off`), `session_manager.rs:29`, `secret_guard.rs:33` (`DEFAULT_SECRET_PATTERNS` covers credentials, not the data roots) |
| **2** | Enumerate a **tool list** — guard `developer` and `computercontroller` | The OS sandbox cannot constrain tools that read files **in-process** inside `biorouterd`. `computercontroller__cache` accepts an arbitrary path and reads it with `tokio::fs::read_to_string`; the Agent Drafter readers do the same. **They *are* the daemon.** No sandbox the daemon installs on its children can constrain the daemon. Round 1 named the two servers; round 2 found `cache` *inside* one of them. | `computercontroller/mod.rs:1482`, `agent_drafter/store.rs:637`, `developer/text_editor.rs:641` |
| **3** | Enumerate **argument shapes** — guard any path-shaped argument at the choke point | Handlers compute their own paths. `read_app` receives an app id and a *relative* path; `ArtifactStore` supplies the denied root via `self.root.join(id)`. `export_app` reads the app root implicitly and writes to a caller-named destination — a copy primitive. And **only the Developer server receives the session cwd**, so every other built-in resolves a relative path against the *daemon's* cwd while the guard resolved it against the session's. All three pass the plan's own "surprise tool" test. | `agent_drafter/store.rs:447` (`fn dir`), `crates/biorouter-mcp/src/lib.rs:49`, `:77` |
| **4** | The tier is **advisory anyway** | A caller could raise its **own** session to Private with one credential-free `POST /agent/update_provider {provider:"llamacpp"}`. The rule that would stop it forbids *"switch this chat to a private model"* — which is step 1 of the two-ways-out message in **every refusal this feature ships**. | `routes/agent.rs`; recorded as AR-15, ruled on as **DR-16** |

Why a mechanical fix is impossible, measured: **125 `#[tool(…)]` declarations in
`crates/biorouter-mcp/src`**, and a `grep` for `fs::`/`File::open` structurally cannot find the
readers — `computercontroller__xlsx_tool` goes through `umya_spreadsheet`, `pdf_tool` through `lopdf`,
`datasql__data_query` through `sqlx`, and none of those lines contains an `fs::` token. A mechanical
extractor written for that survey silently dropped **the entire developer server**, because
`rmcp_developer.rs` contains an inner `#[cfg(test)]` and the extractor stopped there.

**What survives:** the choke point. `ExtensionManager::dispatch_tool_call`
(`extension_manager.rs:1438`) is where BR-23's SecretGuard argument scan has run for months; its own
comment calls it *"the single choke point every tool call flows through."* Measured on `main` today:
`grep -rn "\.call_tool(" --include='*.rs' crates/ | wc -l` → **10** total, of which exactly **one** is
a production dispatch into an MCP client, `extension_manager.rs:1562`, inside that function. That
count is a no-growth tripwire, not a measurement of #56 — any increase is a new bypass.

### Individually killed approaches

| Approach | Why it is dead | Evidence |
|---|---|---|
| **The OS sandbox as *the* mechanism** (Layer B alone) | In-process readers are the daemon. Layer A (in-process, at the choke point) is **primary**; Layer B is defence in depth for spawned children only. This reframing also *shrinks* the cost: on a platform that cannot express the kernel deny, a public session loses the five **spawning** tools, not every file tool. | Plan §"DR-14 is two layers" |
| **Add the data roots to `DEFAULT_SECRET_PATTERNS`** (design §9.3 A2) | Two disqualifiers. It is **unconditional** — an always-on floor applied to every session, so it would hide the user's own KB and chat history from a **private** session too. And it would not close the read anyway: `candidate_is_denied` is lexical and existence-gated, so `sqlite3 "$(printf '%s' ~/.local/share/…/sessions.db)"` walks past it. | `secret_guard.rs:33`, and the design's own admission that "this raises the cost, it does not close the read" |
| **A shared `Arc<RwLock<Vec<PathBuf>>>` guard set before dispatch** | Cross-call mutable state, and dispatch deliberately permits overlapping calls (it returns the tool work as a boxed future). A public dispatch sets the guard; a provider swap plus a private dispatch clears it before the first builtin reads it; the public call then runs unrestricted. Replaced by: Layer A reads Gate C's own **local** and returns before the boxed future exists; Layer B carries a per-call boolean in `_meta`. | `extension_manager.rs:1531`, `:1544` |
| **Gate C as a `ToolInspector`** | Invisible to three of the four production paths that reach the extension manager (`agent.rs`, `routes/agent.rs:1162`, `code_execution_extension.rs`, `Agent::call_prefetch_tool` — the last runs *before* the turn). | O7 |
| **Gate C in `Agent::dispatch_tool_call` instead of the manager's** | Same reason. The agent loop is one of four callers. | O7 |
| **Ratcheting at bind time** | Privatises a chat on a mis-click *and* still misses `POST /agent/call_tool`, which dispatches straight into the extension manager without touching the reply path. | DR-4, O5 |
| **Landlock for the Linux read-deny** | Landlock has no deny rule; the current backend handles **write** accesses only, leaving reads open. Granting the complement was tried and declined: anything created in an enumerated ancestor *after* the ruleset is built is unreadable for that command's lifetime — `cd ~ && mkdir out && echo x > out/f && cat out/f` fails. `bubblewrap` is the only Linux mechanism that can express it. | `shell_sandbox/linux.rs:415` (`AccessFs::from_write(abi)`), open question 17 |
| **`seatbelt::available()` as the capability probe** | It checks only that `/usr/bin/sandbox-exec` exists. Measured on this host: the file exists, and `sandbox-exec -p '(version 1)(allow default)' /bin/true` exits **71** with `sandbox_apply: Operation not permitted`. The probe must be a real negative-plus-positive **execution** test. | `seatbelt.rs:168` |
| **The bubblewrap `--tmpfs` write-deny claim, and its `rm -f` test** | `--tmpfs` is **writable**, so "neither readable nor writable" was false; and the test asserted that `rm -f` of a now-hidden file *fails*, when `rm -f` on an absent path exits 0. The prescribed implementation could not pass its own test on Linux under any correct implementation. | Plan Task 14A |
| **Exempting `kb_get_active` / `kb_set_active` because "the caller already knows the id"** | False for a **no-argument** tool. `kb_get_active` serialised the whole selection — every visible base id plus the primary pointer — and the completeness test named it as exempt, i.e. blessed it. Fix: filter the **view**, never the store (`repair_decision` writes `next_ids.first()`, so filtering the store would re-point the user's primary as a side effect of a *read*). | `knowledge/server.rs:665`, `:691`, `:721`; `knowledge/service.rs:1635` |
| **Sending all three `handle_kb_frame` call sites to `agent.provider()`** | The mid-turn site sits *after* `turn_agent` is bound, and `turn_agent` can be a worker. A private-main/public-worker pair was evaluated as private. It compiles because both agents are in scope. Only the mid-turn site takes `turn_agent`. | `routes/apps.rs:3541` (bind), `:3847` (mid-turn frame) |
| **Reading capability at `capability_report`'s existing position** | That position precedes `configure_main_provider`, so it reads the provider the session held *before* the app's own model was bound — wrong in both directions. The call moves **below** the bind. `configure_worker_agent` has the ordering right and the check missing. | `routes/apps.rs:1257` (report), `:1259` (bind), `:1553-1561` (worker) |
| **"The model has no way to authenticate to the daemon"** | False on both platforms. The daemon secret is recoverable from the daemon's own environment (`ps -Ewww -p $PPID` on macOS, `/proc/self/environ` in-process on Linux). Electron puts it there. Recorded as **AR-11**. | `auth.rs:80` (`check_token` — one machine-wide shared secret, byte-compared), `ui/desktop/src/biorouterd.ts:299` |
| **"Only disk and authenticated `GET /apps` enumerate app ids"** | False. `agent_drafter__list_apps` is a tool on a **public** extension that takes no path argument, so neither Layer A nor a filesystem deny can see it. Withdrawn in Task 14C; open questions 18 and 19 were corrected. | `agent_drafter/mod.rs:2636` → `store.rs:606` |
| **"A caller with the secret gets exactly what the chat gets"** | False. `CallToolRequest` carries a caller-supplied `session_id`; the route resolves *that* agent and dispatches through its manager, and `GET /sessions` enumerates ids. What the plan does instead: the route **declares Public** (`CallCapability::public_enforced()`), so naming a private session's id buys nothing. That is a **narrowing, not a closure** — it does not close "issue any tool call at all with the daemon secret" (AR-11), which is issue #47. | `routes/agent.rs:207-211`, `:1145`, `:1162`; `routes/session.rs:240` |
| **"Blocking `/config/upsert` makes the toggle Settings-only"** | False. `config.yaml` is an ordinary non-SecretGuard file that five tools can write; a public model edits it and the next daemon restart loads privacy tiers off. It joins DR-14's entries as a **file**. The HTTP channel to the same file is open question 24 and still unruled. | `secret_guard.rs:33`, `config_management.rs:174` |
| **A gate that prints a count and exits 0** | `grep -c '^MISSING' …; echo "expect: 0"` exits 0 whatever the count. **A gate a human must interpret is not a gate** — and this was the defect in the very task whose subject is *a command that reports success while doing nothing*. Every assertion must run through a `want`-style helper and the block must end `( exit "$rc" )`. | Plan Task 4b |
| **Gating a `cargo test` filter on "PASS"** | libtest prints `0 passed` and **exits 0** when a filter matches nothing. Every filter gate must assert a *count*, and where the module already has tests, a `pre + N` delta. `routes::agent` is **8** and `routes::session` is **20** on an untouched tree — "expect non-zero" is satisfied by a tree in which #56 added no route tests at all. | Plan §"Which test filters are validated" |
| **Keying a deferral allowlist on a filter *name*** | Excuses that filter in **every** package. `-p biorouter-mcp --lib privacy::refusal` names nothing and never will (`privacy/refusal.rs` is created in `biorouter`), yet it was reported DEFER because a filter of the same name was expected in a different crate. Key on the `(package, filter)` **pair**, and evidence every deferred row — searched over the plan **minus the table**, because otherwise each row witnesses itself inside its own heredoc. | Plan Task 4b |
| **Forbidding only the *trusting* literal in a provenance gate** | Hardcoding `Public` / `false` under-ratchets private callers and passes. Both directions must be forbidden, and each production caller needs a **behavioural** private/public matrix — a structural-only check is defeated by `caller_is_private: provider_name == Some("ollama")`, which keys on the requested name when `providers::create` can return something else. | Plan Tasks 10B, 11 |
| **A race test as N unconstrained spawns** | 200 spawns on a `current_thread` runtime under a conditional assertion that was **false for a correct implementation**. Replaced by forced interleavings behind `#[cfg(test)]` seams, with the seam placed *inside* the helper between the read and the write — a seam outside and before it passes a `SELECT` + unconditional `UPDATE` by refusing for the wrong reason. | Plan Task 12 |

---

## Staging

Four stages. Value lands early; the part that has failed review four times is isolated at the end of
the enforcement work, where it can be re-planned without unwinding anything else.

Within a stage, follow the plan's task units in order and honour the **non-negotiable orderings**
(O1–O16) — each one has a measured failure mode behind it, and O12/O13/O15/O16 are the ones that
decide whether a commit even compiles.

### Stage 1 — the parts Codex has confirmed sound

**Delivers:** the tier model and the knowledge-base barrier, which is the largest genuinely-settled
block of work in the plan.

**Task units:** Phase 0 (Tasks 1–3), Phase 1 (Tasks 4, 4b, 5–9), and Phase 2's KB block (Tasks 10,
10A, 10B, 10C, 10D) plus Task 11 (Gate G).

**Why these first:** three review rounds independently confirmed them sound as specifications, and one
reviewer tried to reconstruct the pointer leak past the replacement tests and reported it could not.
Specifically confirmed:

- **CP1 is valid.** The hand-written `<KnowledgeServer as ServerHandler>::call_tool` is a faithful
  replacement of both rmcp-macro-generated methods: `ToolCallContext::new` is `pub`,
  `ToolRouter::call` is `pub`, and `CallToolRequestParams.arguments` is present before the tool body
  runs. A future incompatible rmcp change produces a **compile failure**, not a silent bypass.
  (`rmcp 0.14.0`, locked.)
- **The KB pointer filtering (Task 10C / finding 2.2) is closed.** All three pointer fields filtered,
  the stored primary preserved, private and nonexistent targets made indistinguishable, error
  candidates filtered, private callers still working.
- **CP3 (finding 2.3) is closed.** Both mixed main/worker directions are driven through the real
  socket, and only the mid-turn site is attributed to `turn_agent`.
- **CP5 (finding 2.4) is closed.** Both global/manifest-provider inversions are exercised through
  `configure_agent` itself, plus the worker-specific grant.
- **The #57 child-environment strip is sound** for every spawn path the design relies on — foreground
  and background shells, `automation_script`, all three computer-control platforms, stdio/inline
  extensions, and the Agent Drafter smoke/esbuild children.

**Depends on:** nothing outside `main`.

**Stop and re-plan if:** a sixth KB/content surface appears that none of CP1–CP5 covers (the review
sweep found none, but the sweep was over `KnowledgeService`, `store::*`, `kb_root`, `list_bases` and
`session_kb_ids` — a surface reached another way would be a new class); or if the ratchet's
`pre + N` arithmetic cannot be made to hold because Task 4b's measured baselines have drifted by more
than the tasks in this stage add.

### Stage 2 — capability threading

**Delivers:** `CallCapability`, sampled **once** at each of the four outermost entries and threaded
through Agent dispatch, the ExtensionManager policy, Gate C, built-in metadata and Layer B metadata.

**Task units:** Task 10's `CallCapability` half (it is written up there for ordering reasons), plus the
threading work O15 describes, plus Tasks 12–13 (Gates A and B) and 14 (Gate C).

**Why it is its own stage:** round 3 found the capability was sampled **four** times, not three, and the
master toggle three or four more. One call could pass Gate C with tiers ON and build an empty path
policy with tiers OFF. The attack is concrete: call A starts Public; the Agent-level policy samples
Public and passes because no literal denied path appears; the provider is swapped to Private across an
intervening `await`; the manager samples again, sees Private, and hands Layer B
`private_data_deny=false`. A's public-authored shell then runs unsandboxed.

**The shape that makes this checkable:** `capability_tier()` is **deleted as a shape**, not relocated.
The gate is a whole-tree count of absence (`grep -rn "capability_tier(" crates/` → 0), because that is
the only kind of gate that fails "threaded it but kept the sampler". `McpMeta` must be built **above**
the `async move` in `ExtensionManager::dispatch_tool_call` — inside it is execution time, on the far
side of an unbounded dispatch queue. `PrivatePathPolicy::for_call` must **not** re-read
`privacy_tiers_enabled()`.

**Depends on:** Stage 1's tier model (O1). **Conflicts with BR-71** — this is the stage to rebase.

**Stop and re-plan if:** a fifth production entry to a tool call turns up (the four are the agent loop,
`POST /agent/call_tool`, the `execute_code` bridge, and `Agent::call_prefetch_tool`); or if threading
the capability requires changing a signature that BR-71 is concurrently rewriting in an incompatible
way — in that case, rebase first and re-derive, do not thread twice.

### Stage 3 — the filesystem barrier

**Delivers:** Layer A (the in-process path barrier at the dispatch choke point **and a guard in each
root's own resolver**) and Layer B (the OS read-deny sandbox, spawned children only).

**Task units:** 14A (Layer B mechanism), 14B (Layer A arguments), 14C (the daemon's own HTTP API),
14D (the readers that never reach the choke point), 14E (**the roots' own doors** — this is the one
that answers round 3), 14F (`export_app`'s write target).

**Be honest about this stage: it is the one that has failed review four times.** All four entries in
the enumeration table above are Stage 3's subject. Budget for a re-plan; do not budget for it going
green first time.

**The two structural commitments, which are what round 3 bought:**

- **Layer A is not one check.** It is a check at the choke point (for caller-supplied paths) **plus a
  guard in each root's own resolver** (for handler-supplied paths). The roots' doors, measured:
  `ArtifactStore::dir` is already private (`agent_drafter/store.rs:447`);
  `MemoryRouter::get_memory_file` is private (`memory/mod.rs:434` on `main`); the session store is a
  sqlx pool nobody outside `session/` should hold. **Knowledge has no such door** —
  `resolve_readable_path` (`knowledge/store.rs:121`) has **3** call sites against roughly 40 direct
  filesystem reads in the same module, and `KnowledgeService::root()` is `pub`
  (`service.rs:415`) because `routes/knowledge.rs` legitimately joins off it at 7 sites. So CP1–CP4
  are the knowledge root's door, and a grep is what stops an eighth reader appearing beside them
  (open question 22, accepted).
- **The coverage gate must be phrased as a choke point, not a list.** The gate that makes "every tool
  call" checkable registers an in-process server **at test time** and asserts its invented
  `read_thing(path)` tool is refused. No list-based implementation passes it. A "surprise tool" test
  that supplies the full **absolute** denied path does not count — both implicit-root defects
  (`read_app`, `export_app`) pass that one.

**Depends on:** Stage 2 (Layer A reads the threaded `CallCapability`; Layer B carries it in `_meta`).
O16: Task 14E lands with or after 14B and before the Phase 2 gate; 14F depends on 14E.

**Known-open at the end of this stage, and accepted:** AR-9 (the symlink swap window — closed at the
open on both platforms via `O_RESOLVE_BENEATH`/`O_NOFOLLOW_ANY` on macOS and `openat2(RESOLVE_BENEATH)`
on Linux, both measured refusing a symlink traversal and a `..` escape; a **pre-planted hardlink**
still defeats Layer B, because both kernels match paths, not inodes); AR-10 (on Linux a deny root that
does not exist when a job starts stays visible to that job for its whole life); AR-6 (on a host that
cannot express the kernel deny a public session loses the five **spawning** tools).

**Stop and re-plan if:** a fifth *class* of read appears — anything that is neither a caller-supplied
path at the choke point nor a handler-supplied path at a root's resolver. That is the shape of all
four previous failures and it is the signal that the architecture, not the list, is wrong.

### Stage 4 — the master toggle, the user-only tier raise, and the UI

**Delivers:** DR-15's master toggle, DR-16's user-only tier raise, the badges, declassification, the
CLI surface, the marketplace, and the migration.

**Task units:** Phase 3 (21–25), Phase 4 (26–32), Phase 5 (33–37), Phase 6 (38–40) — **plus a task
that does not exist yet**.

⚠ **DR-16 is a settled ruling with no task written for it.** The ruling names `Task 18A: the two HTTP
channels that raise a session's own tier, and the user-proof neither of them has`. That task must be
written before it is implemented, and writing it requires resolving two recorded open questions:

- **Open question 23** — what should a daemon that was handed no user-action key do? Four shipping
  launch paths supply none (`just run-server`, `just debug-server`, a hand-run `biorouterd agent`, any
  headless deployment). Failing closed there means a human at the keyboard cannot bind a private model
  over HTTP on the dev path this repo uses daily. An env-var escape hatch is **not** an alternative —
  AR-11 measured the daemon's environment to be recoverable, so a key delivered that way is a key the
  model has.
- **Open question 24** — does DR-16 extend to `POST /config/set_provider` and `POST /config/upsert`,
  which write `BIOROUTER_PROVIDER`, after which `restore_provider_from_session` falls back to it and a
  new session comes up private-capability with no `update_provider` call at all? DR-14 makes
  `config.yaml` a deny entry on the *filesystem* channel precisely because *"a master switch a public
  model can edit is not a switch"*; `/config/upsert` is the **HTTP** channel to the same file and no
  layer of DR-14 sees it.

**The toggle's own hazard:** DR-15's matrix covers **twenty** rows — nineteen enforcement points plus
the session-copy invariant — and an earlier version covered ten. Gate F's two channels, the spawn
matrix, Layer A's insertion points, the catalog, the export location and the visibility predicate could
all stay armed with the feature "off". It is closed at both ends by two inventory diffs, the second of
which starts from the **refusals**, so a gate nobody wired still fails. A crate-graph fact the plan
found late and that constrains the implementation: **`biorouter-mcp` cannot see `biorouter`**, so the
enabled-flag atomic lives in `biorouter-mcp` with a `biorouter` re-export.

**Depends on:** Stages 1–3 for everything it renders and everything it disables.

**Stop and re-plan if:** open questions 23 or 24 cannot be answered without an operator ruling — they
cannot, which is why they are questions. Raise them; do not invent an answer inside Task 18A.

---

## The verification regime

**A green suite is necessary and not sufficient.** Say it once and then behave as if it is true: every
batch of work in this campaign passed its full suite before review, and every review still found real
defects. Three rounds found gates that *could not fail the wrong implementation they named*. The suite
proves you did not break what exists. It does not prove you built a barrier.

### Half one — the mechanical regime

Per task, per the plan: failing test first, implementation, gate, one commit. Per phase, the phase
gate (Tasks 3, 9, 20, 25, 32, 40).

**Baselines, measured (Task 4b Step 3, run against `main` at `89c1f026`).** These are the numbers every
`pre + N` assertion in the plan is arithmetic on. **Re-measure rather than trusting them** — a stale
figure is worse than none, because a "pre + N" assertion against it reads a shortfall as a pass.

```bash
# The five packages the plan filters on, plus biorouter-sandbox (the sixth, added late).
for p in biorouter biorouter-server biorouter-mcp biorouter-cli biorouter-sandbox; do
  cargo test -p "$p" --lib -- --list 2>/dev/null | sed -n 's/: test$//p' | sort > "/tmp/56-filters/$p.txt"
  echo "$p: $(wc -l < /tmp/56-filters/$p.txt) tests"
done
```

| Filter | Measured | Why it matters |
|---|---|---|
| `-p biorouter-mcp --lib knowledge::` | **190** | The plan once said "~122". A "pre + N" against 122 reads a shortfall as a pass. |
| `-p biorouter-mcp --lib memory::` | **10** | ⚠ *not* 12. The trailing `::` excludes two tests. The plan uses **both** spellings; they are different filters. |
| `-p biorouter-mcp --lib secret_guard::` | **19** | ⚠ *not* 20, same reason. "Expect the SAME count as before" is 19. |
| `-p biorouter --lib agents::extension_manager` | **37** | `::tests` is 33; the filter also catches `extension_manager_extension::tests` (4) by substring. |
| `-p biorouter --lib agents::agent` | **21** | Three test modules, none discoverable from the filter (`::tests` 14, `::rewrite_basis_tests` 2, `::stall_seam_tests` 5). |
| `-p biorouter-server --lib routes::agent` | **8** | Untouched baseline. Assert **strictly more**, never "non-zero". |
| `-p biorouter-server --lib routes::session` | **20** | Untouched baseline. Same rule. |
| `-p biorouter --lib agents::chatrecall_extension` | **0** | Genuinely zero — no `#[cfg(test)]` at all. Here a non-zero count *is* the assertion. |
| `-p biorouter --lib session::chat_history_search` | **0** | Same. |
| `-p biorouter-server --lib routes::apps` | **90** | |
| `-p biorouter-mcp --lib agent_drafter::` | **244** | |

41 `(package, filter)` pairs resolve today with a measured pre-count; 18 are deferred to the task that
creates them. **A filter in neither set is the BR-71 defect** — a command that prints `0 passed`, exits
0, and has been reported as verification for forty tasks. Task 20 re-runs the audit with a shrinking
deferred set; Task 40 re-runs it with an **empty** one.

**Release-gate commands** (Task 40): the full workspace suite, `cargo fmt --check`,
`./scripts/clippy-lint.sh`, `just check-everything`, `npx tsc --noEmit`, `npm run lint:check`,
`npm run test:run`, `node scripts/check-contrast.mjs`, `npm run themes -- --check`, and the named
integration targets. **Two expected pre-existing failures**, to be verified on a clean checkout before
dismissing: `providers::test_anthropic_provider` (calls the live API, fails on billing) and the
`SessionListView.test.tsx` isolation flake.

⚠ **`check-contrast.mjs`'s expected total is wrong in the plan and must be re-derived** — see
[how to read the material](#how-to-read-the-material). Measure the pre-#56 baseline on your own branch
point first (it is **288** on `main` today, which is the number the plan expects *after* #56), then
assert baseline + Task 26's 36. The diagnostic reasoning in the plan still holds and is worth keeping:
a total higher than expected means `--background-medium` moved into `TEXT_GROUNDS` (and that run does
not print `OK` at all — three of its assertions fail AA at 3.75 / 4.45 / 4.28, and it exits 1); a total
short by exactly **10** or **20** means one of the two new blocks landed outside the per-scope loop and
ran once instead of six times. Both wrong numbers read to a worker as "the phase failed" when the phase
succeeded, which is why the plan has quoted three different totals and got two of them wrong.

⚠ `cargo test -p biorouter-mcp --test mcp_integration_test` is a cargo **hard error** — the file is
`crates/biorouter/tests/mcp_integration_test.rs`. Two invocations in an earlier draft had this wrong.

### Half two — the by-hand GUI scenarios

Nothing above renders a badge or clicks a button. These are run by hand, and they are the half that
catches the class of defect a jsdom test cannot see.

**Before launching anything, read
[`docs/desktop-ui/launching-the-dev-gui.md`](../desktop-ui/launching-the-dev-gui.md).** Five distinct
launcher failures produce symptoms that read as application bugs:

- `env -u ELECTRON_RUN_AS_NODE` — agent shells export it and Electron then exits instantly, no window,
  no error.
- Do **not** use `electron-forge start` — it reads stdin, so `< /dev/null` takes the app down.
- Pass `--config vite.renderer.config.mts` — a bare `npx vite` skips Tailwind and renders unstyled
  serif HTML that is fully functional.
- Set `BIOROUTER_NO_HMR=1` — any save under `ui/desktop/src` full-reloads the renderer and destroys
  the session under test.
- Verify with a **CDP screenshot**, never `screencapture` of the whole screen.

**Sandbox the config: `XDG_CONFIG_HOME=/tmp/privacy-check`.** Launching the dev GUI can wipe
`~/.config/biorouter`.

The scenarios, each with a screenshot as evidence:

1. A private chat's badge in **History, the chat header, the tab, and the sidebar** — four surfaces,
   all four checked.
2. Switching a private chat to a public model shows the Gate A repair card and **no success toast**.
   This is O3: `ui/desktop/src/components/ModelAndProviderContext.tsx:282` calls
   `updateAgentProvider` *without* `throwOnError` while `setConfigProvider` at `:294-300` has it
   (`:299`), so a Gate A refusal is discarded, execution continues, and a green success toast
   claims the switch worked while the session is still bound to the private model. **Gate A alone is
   worse than no Gate A.**
3. A private extension in the composer's selector is **visible-but-disabled with its reason** — not
   hidden, not silently failing.
4. The declassification dialog's phrase gate, Cancel focus, and the resulting *"Public — made public by
   you on …"* badge.
5. A full private → public declassification, end to end.
6. With the master toggle **off**: badges still render, restyled and suffixed *— enforcement off*,
   beside a persistent strip. A guardrail that vanishes when disabled cannot be noticed by the person
   who disabled it six months ago; a badge that still reads plain **Private** while nothing enforces it
   is a false statement.

Use the Versa models per standing policy (`versa_azure` / `gpt-5.5-2026-04-24`, and `versa_bedrock`
Opus 4.8); a local model only when local-model behaviour is itself under test. The operator's own
config has `cdwagent` and `ucsfomopagent` **disabled**, so seed a config in which something would
actually be refused — otherwise every scenario passes vacuously.

---

## Subagent guidance

**Where parallelism helps:**

- **Independent measurement.** "Sweep every caller of X and classify it" is the single highest-value
  subagent task in this campaign — it is what found `resolve_target_kb`, the third instance of a defect
  two detectors were blind to. Give it a symbol, a directory, and a demand for `file:line` evidence.
- **Adversarial review of a finished stage diff.** Independent, read-only, no shared context with the
  implementer. This is what has caught every real defect so far, and it should run at every phase gate,
  not only at Task 40 Step 5.
- **Phase 5 (`landing/`, Tasks 33–37).** O11: it is genuinely independent and may ship on any cadence.
  Enforcement runs off the compiled-in const, so the website blocks nothing.
- **Frontend work in Phase 4** (Tasks 26–28) against a backend that already exists.

**Where it actively hurts:**

- **Splitting one enforcement point across agents.** Every failure in the register above is a *seam*
  defect — a right value read in the wrong place, a check on the wrong side of a bind, a sampler left
  behind after threading. Seams are exactly what is lost when two agents each hold half a control.
  Stage 2 and Stage 3 are single-owner work.
- **Parallelising tasks that share a signature.** O13 exists because a previous draft left **nine
  consecutive commits** failing `cargo test`: one task changed three struct signatures and the only
  out-of-lib constructor was in no Files table, no `git add` and no run step. A task that changes a
  `pub` signature must run `cargo check --workspace --all-targets`, and every out-of-lib constructor of
  a changed type is a row in its Files table.
- **Running several stages' test suites at once.** Measured OOM hazard; see
  [where to work](#two-hazards-measured-in-this-campaign).
- **Trusting a subagent's count.** Ask for the command and its output, not the number. Three of the
  measured corrections in this campaign — `knowledge::` 190 vs 122, `memory::` 10 vs 12,
  `secret_guard::` 19 vs 20 — were counts a reasonable agent would have reported confidently.

---

## The reviewer's checklist

What the checking pass will verify. Pre-empt it.

1. **No enumeration.** Is any control phrased as a list of tools, servers, or argument names? Is the
   coverage gate phrased as *"every tool call passes through symbol X"*? Does a test-time in-process
   server with an invented path-taking tool get refused?
2. **One sample, threaded.** `grep -rn "capability_tier(" crates/` → 0. Is `McpMeta` built above the
   `async move`? Does `PrivatePathPolicy::for_call` re-read the toggle? Is there any path on which one
   call is evaluated under two different capabilities?
3. **The roots' doors.** Does every root have a resolver that enforces, or a documented reason it does
   not? Are `read_app`, `export_app`, `list_apps` and `computercontroller__cache` covered — the four
   that defeated round 3?
4. **Ordering.** Every read-deny emitted **after** the allow it subtracts from; every capability check
   **before** the early return it would otherwise sit behind. O14 names four such places and all four
   failures are silent: SBPL last-match-wins, bubblewrap later-option-wins, the `SandboxMode::Off` early
   return in `shell_sandbox_wrap`, and the `if jail_relaxed { return … }` in `resolve_path_jailed`
   (relaxed **is** Auto mode, the mode agents run in).
5. **Every gate can fail.** For each gate, name a plausible wrong implementation and show the gate
   rejecting it. Does the block exit non-zero? Does a `cargo test` line assert a count and not a pass?
   Is a deferral keyed on the `(package, filter)` pair? Is a provenance check two-directional?
6. **Deviations recorded.** Every place the plan's prescribed code did not compile, or its anchor did
   not resolve, is written into the task in the same commit. Silent fixes are a finding.
7. **The toggle's twenty rows.** Every enforcement point is in the matrix or the inventory, and session
   copy is asserted **identically in both columns** (DR-15: propagating a stamp is not classifying).
8. **The accepted risks are still the accepted risks.** AR-1 through AR-15 are rulings, not bug
   reports. A PR that closes one silently has changed a decision; a PR that *widens* one has introduced
   a defect. Either way it must say so.
9. **Commit hygiene.** Conventional commits, no `Co-Authored-By` trailers, one commit per task, nothing
   pushed.
10. **Docs.** Anything new under `docs/` carries the context header, sentence-case headings, a
    kebab-case filename and a `## Related documentation` close, and is indexed in
    [`docs/security/README.md`](README.md) and [`docs/organization.md`](../organization.md).

---

## Related documentation

- [Privacy tiers](privacy-tiers.md) — the design this brief fronts: the two lattices, the capability
  matrix, the five gates, and the cost.
- [Privacy tiers — implementation plan](privacy-tiers-execution-plan.md) — the fifty-one task units,
  the non-negotiable orderings, the decisions of record, the accepted risks and the open questions.
- [Multi-KB implementation plan](../knowledge-base/multi-kb-implementation-plan.md) — the "one axis,
  one pointer" visible-set model whose explicit-`kb_id` escape hatch the knowledge-base barrier
  qualifies.
- [Documentation style guide](../contributing/documentation-style.md) and
  [documentation organization](../organization.md) — both binding on anything this work adds to `docs/`.
- [Data privacy and patient data](data-privacy-and-phi.md) — the provider guidance this system enforces
  mechanically.
- [Secret storage](secret-storage.md) — the credential model, and the environment strip the review
  rounds confirmed sound.
- [BR-71 execution plan](../agent-loop/designs/br71-execution-plan.md) — the concurrent work Stage 2
  conflicts with.
- [Launching the dev GUI](../desktop-ui/launching-the-dev-gui.md) — required reading before any by-hand
  verification step.
