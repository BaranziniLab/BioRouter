# Audit findings register

> **What this is.** The de-duplicated defect register for the 100-app Agent Drafter test drive: 22
> numbered findings, each with its location, symptom, reproduction, best-guess root cause, impact, and
> suggested fix.
> **Status:** Historical record — closed. This register was the input the
> [remediation plan](remediation-plan.md) consumed and the
> [remediation results](remediation-results.md) closed out. The product fixes shipped on branch
> `feat/apps-sdk-v2` in commits `ae8987a6`, `7527f848`, and `d8cf95cc`, so no finding here is still
> open. It is retained as the record of *why* the platform is shaped the way it is.
> **Audience:** developers working on Agent Drafter and the Apps SDK.

Findings were added from the per-app rubrics in `app-results/` with exact app, round, repro prompt or
gesture, evidence path, likely layer, impact, and suggested fix, then de-duplicated so a defect
observed in six apps appears once. The dashboard below was a live tracker and is frozen at the point
the run stopped.

## How to read a finding

- **Numbering.** Findings are numbered 1–22 in the order they appear here. The
  [remediation plan's map table](remediation-plan.md#findings-to-plan-map) cites them by that number.
  The plan additionally lists findings 23 and 24, which it discovered while verifying its own
  citations; those two were never added to this register, so the two documents disagree by design.
- **Type** is one of the categories the audit used:

  | Type | Meaning |
  |---|---|
  | `SPEC-GAP` | The platform has no way to express something the app genuinely needs. |
  | `FUNCTIONAL-BUG` | A declared behaviour does not work at runtime. |
  | `SECURITY/ROBUSTNESS` | Runaway cost, hangs, silent failure, or fabricated output. |
  | `ERGONOMICS` | The platform works but makes the correct thing hard to do. |
  | `AUTHORING-INEFFICIENCY` | Authoring burns turns and time on avoidable guessing. |
  | `HARNESS-BUG` | The test harness, not the product, produced the wrong verdict. |
  | `ENVIRONMENT` | External to the codebase — provider, network, credentials. |

- **Severity** is `blocker` (the run cannot proceed), `high`, or `med`.
- **Remediated by** cites the plan item that was assigned to fix the finding, using the plan's own
  map table.

> **Note.** Screenshot paths under `shots/` and app-store paths under `.br-testdrive/` refer to the
> ephemeral worktree sandbox in which the run executed. Neither was checked in, so those paths record
> what was captured rather than pointing at a file in this repository.

## Dashboard, frozen at the end of the run

| Metric | Value |
|---|---|
| Apps completed | 18 / 100 |
| Functional PASS / PARTIAL / FAIL | 0 / 16 / 2 |
| Aesthetic ALIGNED / PARTIAL / OFF | 7 / 11 / 0 |
| Median authoring rounds to acceptance | pending — never computed before the run stopped |

Findings by type and severity, as recorded by the live tracker:

| Type | Severity census |
|---|---|
| `ENVIRONMENT` | blocker 1 (resolved) |
| `HARNESS-BUG` | high 1 (resolved) |
| `ERGONOMICS` | high 1 |
| `AUTHORING-INEFFICIENCY` | med 1 |
| `SPEC-GAP` | high 6 |
| `FUNCTIONAL-BUG` | high 9 / med 2 |
| `SECURITY/ROBUSTNESS` | high 3 |

> **Warning.** That census totals 24 findings, but this register holds 22 numbered entries and
> contains 4 `SPEC-GAP` entries rather than 6. The census line was maintained by hand during the run
> and was never reconciled against the entries. Trust the entries.

**Top recurring failure modes:** repeated `ui_describe`; invented skills and KB configuration;
signal-before-subscribe; split client/shared state; profile and schema guessing.

**Highest-leverage Agent Drafter improvements identified:** engine-level repeated-tool guard;
validate skills and KB ids; eager signal subscription; one canonical reactive state; type-safe
orchestration builder.

## Finding 1 — The 100-idea corpus cannot test structural layout diversity

**Type:** `SPEC-GAP` · **Severity:** high · **Status:** mitigated by controlled probes ·
**Remediated by:** no product change; see the plan's map entry.

- **Where:** all 100 `Layout` prompts; full audit in
  [layout-diversity-audit.md](layout-diversity-audit.md).
- **Symptom:** every idea explicitly requires Left, Center, Right, and Bottom regions; 89 name a rail
  and 96 name an inspector. Generated apps therefore converge on the same persistent-sidebar skeleton
  even when their themes and scientific widgets differ.
- **Repro:** parse every `**Layout:**` line in
  [hundred-app-test-specs.md](../../agent-drafter/testing/hundred-app-test-specs.md); counts are Left
  100, Center 100, Right 100, Bottom 100.
- **Root cause (best guess):** test-corpus design dominates; starter gravity may reinforce it because
  explorer, workbench, and canvas seed two-column grids.
- **Impact:** the numbered 100-app results alone cannot support a claim that Agent Drafter can or
  cannot create structurally diverse layouts.
- **Mitigation/result:** five UCSF-only no-sidebar probes now cover dashboard mosaic, centered
  wizard, radial canvas, full-width tabletop, and full-bleed explorer. All five pass static
  layout/model/theme audit, and browser inspection confirms materially different geometry and look.
  The numbered prompts remain exact, with an anti-template clause from spec 016 onward.

## Finding 2 — Agent Drafter store ignores `BIOROUTER_PATH_ROOT`

**Type:** `ERGONOMICS` · **Severity:** high ·
**Remediated by:** [plan item 0.1](remediation-plan.md#01-one-env-aware-path-resolver-for-biorouter-mcp).

- **Where:** spec 001 (Variant Tribunal), initial authoring attempt.
- **Symptom:** sessions and config respected `.br-testdrive/runtime`, but `create_app` wrote the
  uniquely named draft to the global `~/.config/biorouter/agent_drafter` store.
- **Repro:** set only `BIOROUTER_PATH_ROOT=<sandbox>` and invoke a session with the built-in Agent
  Drafter; inspect `default_root()` output versus `Paths::config_dir()`.
- **Root cause (best guess):** `agent_drafter::default_root()` calls etcetera directly instead of the
  shared `Paths` abstraction.
- **Impact:** contaminates the user's real application inventory and violates worktree/sandbox
  isolation.
- **Suggested fix or SDK improvement:** make Agent Drafter use `Paths::in_config_dir("agent_drafter")`,
  or have `default_root()` honor `BIOROUTER_PATH_ROOT`; add an isolation regression test. The harness
  now also sets `XDG_CONFIG_HOME`, and the incomplete draft is preserved under
  `.br-testdrive/quarantine/`.

## Finding 3 — Orchestration configuration requires repeated schema guessing

**Type:** `AUTHORING-INEFFICIENCY` · **Severity:** med ·
**Remediated by:** [plan item 2.1](remediation-plan.md#21-typed-declaration-tools) and
[plan item 2.2](remediation-plan.md#22-merge-dont-replace-on-manifestjson).

- **Where:** spec 001 (Variant Tribunal), rounds 1–2.
- **Symptom:** Agent Drafter generated six rejected mutations: orchestration sequence vs map, string
  vs tagged `WorkflowStep`, map vs string, `theme` string vs `ThemeConfig`, missing `created_at`, and
  worker `model` string vs `ModelSelection`.
- **Repro:** ask Agent Drafter to create four named profiles plus fast/deep routes in the initial
  `create_app` call from the full spec 001 block.
- **Root cause (best guess):** free-form nested `serde_json::Value` parameters and whole-manifest
  rewrites expose a deep schema the model cannot reliably infer from tool errors.
- **Impact:** more than five minutes and many billed tool/model turns before a valid starter existed.
- **Suggested fix or SDK improvement:** add typed `surface`, `theme`, `profiles`, and `routes`
  parameters or dedicated tools; make `read_app` return a canonical editable manifest skeleton;
  preserve required metadata server-side on manifest updates.

## Finding 4 — Invented `br.kb` identifier prevents clean runtime setup

**Type:** `SPEC-GAP` · **Severity:** high ·
**Remediated by:** [plan item 1.2](remediation-plan.md#12-reject-unknown-ids-at-the-write-boundary).

- **Where:** spec 001 (Variant Tribunal), first browser load.
- **Symptom:** the daemon logged `set active KB failed: kb-id may only contain a-z, 0-9, and '-'`, and
  the first runtime configuration contained `knowledge_base: "br.kb"` plus the same invalid grant id.
- **Repro:** request "KB of ClinVar/literature" without supplying an installed KB id, then launch the
  generated app.
- **Root cause (best guess):** Agent Drafter confuses the client API namespace `br.kb` with a
  persistent KB identifier; lint does not validate ids.
- **Impact:** noisy and faulty startup, and no usable KB capability.
- **Suggested fix or SDK improvement:** lint `knowledge_base` and knowledge source ids with the same
  validator as the daemon; teach the authoring prompt that `br.kb` is an API, never an id; represent
  unavailable KB requirements explicitly instead of inventing one.

## Finding 5 — Generated multi-agent loop uses display names and lets workers seize the UI

**Type:** `FUNCTIONAL-BUG` · **Severity:** high ·
**Remediated by:** [plan item 3.4](remediation-plan.md#34-consult-binds-to-manifest-keys-workers-lose-the-ui).

- **Where:** spec 001 (Variant Tribunal), live re-adjudication turn after the initial runtime fix.
- **Symptom:** `consult(agent="Prosecutor")` failed because declared keys were lowercase; main
  repeated an unchanged `ui_describe` three times; lowercase `prosecutor` then called `ui_describe`
  itself and stalled before the remaining three profiles. Evidence is in the app session rows and the
  browser screenshot/DOM trace.
- **Repro:** click **Re-adjudicate** on `spec-001-variant-tribunal` after the first build.
- **Root cause (best guess):** the system prompt names human-facing agents rather than exact manifest
  keys; worker profiles inherit UI capability and the UI-first system discipline.
- **Impact:** the headline multi-agent criterion fails and a paid turn hangs without reaching a
  verdict.
- **Suggested fix or SDK improvement:** lint system prompts and `consult` calls against exact profile
  keys; default consulted workers to `ui.enabled:false`; explicitly state that UI ownership is
  main-only; stop and recover automatically after a repeated unchanged `ui_describe`.

## Finding 6 — A declared signal is emitted before the agent subscribes

**Type:** `FUNCTIONAL-BUG` · **Severity:** high ·
**Remediated by:** [plan item 3.1](remediation-plan.md#31-declaration-is-subscription-eager-signals).

- **Where:** spec 001 (Variant Tribunal), PVS1 direct-manipulation turn; reproduced by endpoint
  selection in spec 004, STAT3 selection in spec 005, rs123 selection in spec 008, target/parameter
  gestures in spec 011, an intervention gesture in spec 012, body addition in spec 013, terrain
  painting in spec 014, and all five layout probes.
- **Symptom:** the criterion UI updated locally, but the ambient chip reported
  `signal "criterion_clicked" is not subscribed`.
- **Repro:** reload the app and click PVS1 as the first interaction.
- **Root cause (best guess):** the subscription exists only as prose in the generated system prompt
  and no agent turn has yet called `ui_subscribe`; the click emits the signal before its accompanying
  `br.call` begins that first turn.
- **Impact:** the headline app→agent signal path fails on the user's first gesture, even though the
  parallel `br.call` happens to start a turn.
- **Suggested fix or SDK improvement:** support manifest-declared eager subscriptions or a main-agent
  startup hook; lint prompts for an explicit `ui_subscribe` call; avoid emitting first-use signals
  before subscription readiness.

## Finding 7 — Generated `br.run` controls execute locally but deliver no turn

**Type:** `FUNCTIONAL-BUG` · **Severity:** high ·
**Remediated by:** [plan item 4.3](remediation-plan.md#43-run-fails-loudly-and-cannot-wedge).

- **Where:** spec 002 (Cohort Funnel Foundry), initial browser round.
- **Symptom:** Playwright, coordinate CUA, and DOM-node CUA all activated **Ask architect**; the
  secondary **Send** handler provably ran because it cleared the input. Neither `br.run` call changed
  its target to the synchronous "Starting agent run…" state or inserted a message into the app
  session. The console was clean and the page showed `Session ready / data, ui`.
- **Repro:** load the first spec 002 build, click **Ask architect**, then fill and click **Send**;
  query its session messages and inspect `#log`.
- **Root cause (best guess):** an app-specific interaction between `br.run` serialization/target
  mounting and the separately mounted timeline; the deterministic baseline covers SDK primitives but
  not this exact double-timeline pattern.
- **Impact:** every authored primary control looks clickable yet cannot start the agent loop.
- **Suggested fix or SDK improvement:** add a real-browser harness test for
  `mountTimeline(br, '#progress')` plus `br.run(prompt, '#log')`; lint/preview should execute one
  wired control. The in-session workaround changed controls to structured `br.call` and succeeded.

## Finding 8 — The model endlessly retries control-plane tools despite explicit stop discipline

**Type:** `SECURITY/ROBUSTNESS` · **Severity:** high ·
**Remediated by:** [plan item 4.1](remediation-plan.md#41-turn-guard-mask-the-tool-terminate-the-loop).

- **Where:** spec 001 (Variant Tribunal), second live instruction; reproduced in specs 002 (Cohort
  Funnel Foundry), 003 (Pathway Séance), 004 (Trial Regia), and 005 (Omics Loom).
- **Symptom:** `ui_describe` returned `The user has declined to run this tool. DO NOT attempt to call
  this tool again`; the model immediately called the same tool at least six more times, with no
  Defense/Clerk/Justice progress. The app prompt already said to call it exactly once and never repeat
  an unchanged describe.
- **Repro:** resume the durable spec 001 session after the earlier repeated-describe history and click
  **Re-adjudicate**; add eGFR in spec 002; load seeds in spec 003; run **Check feasibility** in spec
  004; click **Integrate layers** in spec 005; choose **spots** in spec 011; click **Stabilize
  system** in spec 013; or activate any layout probe. Each later case repeats a control-plane call
  after successful worker/action phases despite explicit one-call discipline. Four probes repeated
  `ui_describe`; the constellation probe repeated `ui_subscribe` at least five times.
- **Root cause (best guess):** the repeated-tool/user-decline policy is communicated only as text; the
  engine neither masks the declined tool nor terminates repeated identical calls.
- **Impact:** runaway billed turns, hung UI, and failure of the required second instruction; the
  daemon had to be stopped.
- **Suggested fix or SDK improvement:** hard-disable a declined tool for the remainder of the turn,
  detect identical declined calls, and terminate with a structured loop error after the first retry;
  reset repeated-tool state per user turn when `ui_describe` is contractually required.

## Finding 9 — Generated progress stream duplicates into the semantic result region

**Type:** `FUNCTIONAL-BUG` · **Severity:** med ·
**Remediated by:** [plan item 4.4](remediation-plan.md#44-one-progress-sink-result-regions-are-sacred).

- **Where:** spec 005 (Omics Loom), Integrate layers turn; reproduced in spec 007 (Provenance
  Autopsy), where frames occupy both Evidence chain-of-custody and Visible progress.
- **Symptom:** every tool-running/completed frame appears both under Agent run status and inside the
  inspector where the integrated synthesis belongs; the semantic synthesis remains blank.
- **Repro:** click **Integrate layers** and inspect both the left `progress` and right
  `synthesis`/inspector regions during the turn.
- **Root cause (best guess):** the generated app mounts a dedicated timeline and *also* passes the
  semantic result region as the `br.call` streaming target.
- **Impact:** the most important scientific interpretation is displaced by duplicate plumbing output.
- **Suggested fix or SDK improvement:** lint for multiple timeline consumers; let `br.call` route
  progress to the dedicated mount while reserving semantic regions for explicit `ui_patch`/render
  output.

## Finding 10 — Agent Drafter configures nonexistent skills as runtime requirements

**Type:** `SPEC-GAP` · **Severity:** high ·
**Remediated by:** [plan item 1.2](remediation-plan.md#12-reject-unknown-ids-at-the-write-boundary) and
[plan item 1.3](remediation-plan.md#13-never-arm-a-tool-for-a-grant-that-cannot-be-satisfied).

- **Where:** spec 003 requested `pathway-analysis`; specs 004 and 009 requested
  `clinical-biostatistics`; specs 006–008 declare `clinical-databases`, `reproducibility`, and
  `statistical-genetics`; spec 010 declares `rare-disease` and `clinical-databases` without
  installed-skill verification.
- **Symptom:** the visible agent timeline reports a failed `skills__loadSkill`; the app then spends a
  model step recovering or reasoning without the promised capability.
- **Repro:** load seeds in Pathway Séance, or click **Power it** in Trial Regia, and inspect the app
  session tool responses.
- **Root cause (best guess):** the idea spec names a domain capability, but Drafter treats arbitrary
  prose as an installed skill id without discovering the skill catalog or validating the manifest.
- **Impact:** every affected first turn begins with a deterministic tool failure and may take a less
  reliable fallback path.
- **Suggested fix or SDK improvement:** expose discoverable installed-skill ids in Drafter context;
  validate configured skills during lint/build; turn unavailable requested skills into worker-prompt
  domain reasoning rather than a guaranteed failing tool call.

## Finding 11 — Platform-integration strings are mistaken for working capabilities

**Type:** `SPEC-GAP` · **Severity:** high ·
**Remediated by:** [plan item 1.1](remediation-plan.md#11-a-catalog-and-a-discovery-tool) and
[plan item 1.4](remediation-plan.md#14-a-typed-slot-for-i-need-x-and-it-isnt-here).

- **Where:** corpus-wide audit; detailed counts and catalog evidence in
  [platform-integration-audit.md](platform-integration-audit.md).
- **Symptom:** 85 specs request KBs and 57 request skills, while the isolated runtime has zero
  installed KBs and skills. Specs 001–016 nevertheless configure 13 concrete KB ids and seven skill
  lists; none can be credited as available. The real built-in `knowledge` and `autovisualiser`
  extensions are widely configured, but a built-in extension does not create the missing domain
  payload.
- **Repro:** query the isolated `biorouter knowledge list`, `extension list`, and `skill list`;
  compare with every manifest and runtime tool history.
- **Root cause (best guess):** Agent Drafter receives prose integration wishes without an
  authoritative environment catalog, and lint validates shape more readily than existence.
- **Impact:** apps claim grounding, connectors, or specialist capability that is absent; runtime
  spends turns on deterministic failures or invents unsupported scientific output.
- **Suggested fix or SDK improvement:** expose typed discoverable catalogs to Drafter, lint ids
  against them, and report requested/configured/available/exercised separately. The test driver now
  injects the exact empty payload catalogs and the real built-in extension list, rejects invented ids,
  and checks requested routes, workflows, and figures.

## Finding 12 — Shared agent state and client control state diverge between turns

**Type:** `FUNCTIONAL-BUG` · **Severity:** high ·
**Remediated by:** [plan item 3.2](remediation-plan.md#32-one-canonical-state-document).

- **Where:** spec 004 (Trial Regia), Power followed by Check feasibility; reproduced in spec 015
  (FoldScape), multiple layout probes, and spec 016 (AeroCanvas).
- **Symptom:** Trial Regia visibly patched power to n=784 but the next `br.call` serialized
  `sample_size=248`. In FoldScape, the browser showed selected M1 with φ/ψ -70°/4°, while all worker
  output analyzed a stale L34→A and rendered that stale residue in the presence card.
- **Repro:** click **Power it** then **Check feasibility** in spec 004; select M1, change the
  dihedral, choose A, and click **Mutate & rescore** in spec 015; or move between wizard steps, select
  a tabletop row, or select a constellation node in the probes. Visible local selections become blank
  or differ from serialized agent inputs.
- **Root cause (best guess):** SDK `ui_patch_state` updates the shared document and bindings, while
  generated client closures serialize a separate local `state` object initialized at 248.
- **Impact:** multi-turn scientific reasoning is internally contradictory and can produce incorrect
  feasibility flags.
- **Suggested fix or SDK improvement:** generate one canonical state source; have actions, controls,
  prompt serialization, bindings, and `ui_patch_state` all read and write it; add a two-turn
  conformance test.

## Finding 13 — First-load bindings and direct range control do not reflect initialized state

**Type:** `FUNCTIONAL-BUG` · **Severity:** high ·
**Remediated by:** [plan item 3.2](remediation-plan.md#32-one-canonical-state-document).

- **Where:** spec 004 (Trial Regia), initial browser load.
- **Symptom:** feasibility and power bindings were blank until an agent patch; two ArrowRight inputs
  on the MDE range left both the control and the displayed state at 0.35.
- **Repro:** open a fresh Trial Regia app, inspect KPI text, focus the MDE slider, and press
  ArrowRight twice.
- **Root cause (best guess):** initial local state is not published through the same binding path used
  by agent patches; rerender resets the form from stale local state.
- **Impact:** direct manipulation and data readability fail before the first expensive model turn.
- **Suggested fix or SDK improvement:** preview/lint should assert that every declared binding
  resolves from initial state, and should exercise keyboard and pointer form input before approving a
  build.

## Finding 14 — Generated main agent bypasses declared worker profiles with generic subagents

**Type:** `FUNCTIONAL-BUG` · **Severity:** high ·
**Remediated by:** [plan item 3.5](remediation-plan.md#35-one-delegation-mechanism-per-app).

- **Where:** spec 006 (Ward Board), Round turn; reproduced in spec 012 (Contagion Studio), Add
  intervention and Fit to data turns.
- **Symptom:** Ward Board declares four UCSF-routed profiles but called generic
  `subagent(subworkflow=hospitalist|devils_advocate|documentarian)`; Contagion Studio declares
  Fitter/Adversary/Policy Analyst/Reporter but called two generic `subagent` tools. Required profiles
  were skipped.
- **Repro:** click **Round** in spec 006, or **Add intervention** / **Fit to data** in spec 012, and
  inspect tool names plus the durable app and worker sessions.
- **Root cause (best guess):** the system prompt mentions subworkflows and the model selects a generic
  delegation tool instead of the Apps SDK v2 `consult` tied to manifest orchestration profiles.
- **Impact:** profile-specific isolation, capabilities, routing, and the required specialist panel
  cannot be proven; the declared orchestration may be dead configuration.
- **Suggested fix or SDK improvement:** lint system prompts to require `consult` for each declared
  profile key; hide generic subagent delegation from app turns with manifest orchestration, or map it
  explicitly through the declared profile registry.

## Finding 15 — Runtime theme mutation makes scientific regions illegible

**Type:** `FUNCTIONAL-BUG` · **Severity:** med ·
**Remediated by:** [plan item 4.5](remediation-plan.md#45-ui_theme-becomes-a-round-trip-with-a-contrast-audit).

- **Where:** spec 012 (Contagion Studio), Fit to data turn; reproduced in spec 013 (Orbital Sandbox),
  the tabletop and constellation probes, and specs 016–018. Evidence is preserved in each listed
  result's runtime screenshot.
- **Symptom:** the baseline clinical canvas is coherent, but the agent's live theme/render pass
  creates large opaque black blocks across the center plot and the right KPI/table regions while the
  page background remains white.
- **Repro:** load the clean app, click **Fit to data**, wait through the first `ui_theme`/render
  cycle, and compare against `shots/spec-012/baseline.png`.
- **Root cause (best guess):** runtime theme variables are applied inconsistently between
  app-authored CSS/SVG and SDK-mounted regions.
- **Impact:** the main outbreak visualization and the briefing KPIs become unreadable during the very
  turn meant to update them.
- **Suggested fix or SDK improvement:** preview theme mutations against app-authored SVG/canvas
  regions, scope variables consistently, and provide contrast/invisibility checks after `ui_theme`.

## Finding 16 — Agent renders an intervention plan without invoking the declared action

**Type:** `FUNCTIONAL-BUG` · **Severity:** high ·
**Remediated by:** [plan item 3.3](remediation-plan.md#33-an-action-has-an-effect-and-the-turn-knows-whether-it-ran).

- **Where:** spec 011 (Reaction-Diffusion Foundry), target steering; spec 013 (Orbital Sandbox),
  stabilization; spec 014 (Serengeti Engine), ecosystem balance.
- **Symptom:** the main agent consults workers and renders convincing plan text and highlights, but
  never calls the surface action that would apply the plan. Spec 014 explicitly says lion vision will
  change 0.68→0.52 while the slider and state stay 0.68.
- **Repro:** choose spots in spec 011, click **Stabilize system** in spec 013, or click **Balance
  ecosystem** in spec 014; compare the rendered narrative with progress tool names and bound state.
- **Root cause (best guess):** generated system prompts emphasize narration and rendering but do not
  enforce at least one successful `app_call` before declaring a staged intervention.
- **Impact:** the app creates the appearance of agent control while leaving the underlying simulation
  unchanged.
- **Suggested fix or SDK improvement:** make intervention completion conditional on a successful
  declared action plus readback; lint main prompts for an explicit action/apply phase; surface a
  visible partial or failure state when action application is skipped.

## Finding 17 — Main fabricates quantitative scientific output after a worker reports insufficient data

**Type:** `FUNCTIONAL-BUG` · **Severity:** high ·
**Remediated by:** [plan item 3.6](remediation-plan.md#36-evidence-ledger-and-provenance).

- **Where:** spec 008 (Manhattan Signal Room), rs123 fine-mapping turn.
- **Symptom:** Fine Mapper explicitly stated that PIPs were not defensible without summary statistics,
  LD/reference ancestry, and allele harmonization; main then called `shade_credible_set` with invented
  PIPs 0.42/0.20/0.15/0.13/0.10.
- **Repro:** select rs123 and compare the Fine Mapper response with the subsequent action
  payload/session trace.
- **Root cause (best guess):** the app system prompt demands a visually complete worked example, and
  no runtime guard distinguishes synthetic demo data from unsupported inference.
- **Impact:** the UI presents fabricated normalized probabilities as a credible set — a serious
  scientific-integrity failure.
- **Suggested fix or SDK improvement:** require provenance labels on generated quantitative action
  values; add schema fields for synthetic/demo vs computed; block scientific action calls whose
  required inputs are absent or whose consulted worker returned an insufficient-data status.

## Finding 18 — Core drag-only interaction has no accessible or reliable fallback

**Type:** `FUNCTIONAL-BUG` · **Severity:** high ·
**Remediated by:** [plan item 5.2](remediation-plan.md#52-brdnd--a-drag-primitive-that-is-reliable-by-construction).

- **Where:** spec 009 (Survival Atelier), stratum creation; a related HTML5 drag limitation was first
  observed in spec 002.
- **Symptom:** real coordinate CUA drags of Age and Stage into the visible drop zone produced no chip
  or signal; there is no clickable or keyboard alternative, so the guarded model run cannot start.
- **Repro:** at 1280x720, drag either `[draggable=true]` covariate card into the stratum builder, then
  click **Fit Cox**.
- **Root cause (best guess):** the generated UI assumes HTML5 `DataTransfer` drag support and omits
  accessible interaction parity.
- **Impact:** the app's core workflow, multi-agent loop, and all scientific outputs are unreachable.
- **Suggested fix or SDK improvement:** lint every drag/drop-only surface for keyboard and click
  parity; add a catalog drag primitive with reliable CUA semantics and default accessible activation.

## Finding 19 — Reviewer misread an omitted default theme as a missing pack

**Type:** `HARNESS-BUG` · **Severity:** high · **Status:** resolved ·
**Remediated by:** [plan item 0.2](remediation-plan.md#02-read_app-returns-a-resolved-view) — a
harness bug, but the product caused it.

- **Where:** spec 010 (Diagnosis Odyssey), authoring rounds 3–4; reproduced in spec 015 (FoldScape),
  including an interrupted futile refinement.
- **Symptom:** `update_app` readback showed `theme.pack="biorouter"`; `build_app` omitted the block,
  and the reviewer treated that as a failure.
- **Repro:** set the base pack explicitly, build, and inspect the canonical manifest.
- **Root cause:** `ThemeConfig::is_default` deliberately omits the default base theme so that v1 and
  default manifests do not gain a redundant block. Absence semantically resolves to `biorouter`.
- **Impact:** the harness falsely failed valid apps and drove Agent Drafter through repeated
  impossible update→build attempts, wasting turns and time.
- **Resolution:** `resolved_theme_pack()` now maps an absent block to `biorouter`; a regression test
  covers absent/default and explicit non-default packs. Specs 010 and 015 now pass static theme
  review.

## Finding 20 — Profile consults consume the full 120 s, then main silently completes without work

**Type:** `SECURITY/ROBUSTNESS` · **Severity:** high ·
**Remediated by:** [plan item 4.2](remediation-plan.md#42-consult-deadlines-that-cancel-and-are-visible).

- **Where:** spec 010 (Diagnosis Odyssey), Fabry node turn.
- **Symptom:** Pathfinder and Test Recommender both returned `did not answer within 120s`; no worker
  sessions completed, Refuter and Chronicler were skipped, and main emitted `Run complete` with zero
  app actions and no visible fallback.
- **Repro:** click Fabry disease in a clean app session and inspect the timeline and session messages.
- **Root cause (best guess):** overly broad profile prompts or profile startup failure meet a fixed
  long timeout; the main prompt has no mandatory action/failure-render branch.
- **Impact:** more than four minutes of latency and cost yields no user-facing result and blocks the
  required multi-agent workflow.
- **Suggested fix or SDK improvement:** shorter configurable worker deadlines, startup diagnostics,
  structured timeout results, concurrent bounded profiles, and a mandatory visible partial/failure
  state when required workers time out.

## Finding 21 — UCSF Azure rejected the prior egress IP

**Type:** `ENVIRONMENT` · **Severity:** blocker · **Status:** resolved 2026-07-12 ·
**Remediated by:** environment only; no product change.

- **Where:** both the Agent Drafter CLI and the app runtime, after the first ten drafts; exact resume
  evidence in [azure-403-outage-incident.md](azure-403-outage-incident.md).
- **Symptom:** HTTP 403 with `The IP Address is invalid: 104.52.5.246` from the required
  `versa_azure/gpt-5.5-2026-04-24` deployment.
- **Repro:** run UCSF-only authoring or runtime turns while the machine is off the required VPN route.
- **Impact:** specs 011–100 and queued refinements were blocked while switching models remained
  prohibited by the test contract.
- **Resolution evidence:** after the user restored VPN connectivity, the same named spec 011 session
  completed a 324.8-second real UCSF turn, produced a manifest and app bundle, and passed static
  review. Runtime consults also completed on the locked UCSF model.

## Finding 22 — CLI exits zero when the provider turn actually failed

**Type:** `SECURITY/ROBUSTNESS` · **Severity:** high ·
**Remediated by:** [plan item 0.3](remediation-plan.md#03-a-failed-turn-is-a-failed-turn).

- **Where:** provider-blocked refinements for spec 003 and specs 005–009, plus initial attempts for
  specs 011–013.
- **Symptom:** `biorouter run` printed a clear authentication/403 failure but returned rc 0; the
  original harness credited 2–6 second rounds and continued the batch.
- **Repro:** invoke a named Agent Drafter session while UCSF rejects the IP, and inspect both the
  output and the process exit code.
- **Impact:** unattended test automation can report successful authoring, consume iteration budgets,
  and generate misleading missing-app verdicts after no model work occurred.
- **Suggested fix or SDK improvement:** propagate provider and auth failures as a nonzero CLI exit
  status and structured result state. The worktree harness now detects the 403 marker, records
  rc 75 / provider-blocked, excludes it from round budgets, and aborts immediately.

## Related documentation

- [Remediation plan](remediation-plan.md) — every finding above mapped to a specific code fix, with
  `file:line` citations and wave gates.
- [Remediation results](remediation-results.md) — what was actually built against this register, and
  what the fixed platform caught when re-linting the same corpus.
- [Authored-app verdict index](authored-app-verdict-index.md) — the per-app verdicts these findings
  were de-duplicated from.
- [Platform integration audit](platform-integration-audit.md) — the full evidence behind finding 11.
- [Layout diversity audit](layout-diversity-audit.md) — the full evidence behind finding 1.
- [Azure 403 outage incident](azure-403-outage-incident.md) — the incident timeline behind findings
  21 and 22.
