# Agent Drafter Apps SDK v2 — cumulative findings

## Dashboard

- Apps completed: 18 / 100
- Functional PASS / PARTIAL / FAIL: 0 / 16 / 2
- Aesthetic ALIGNED / PARTIAL / OFF: 7 / 11 / 0
- Median authoring rounds to acceptance: pending
- Findings by type/severity: ENVIRONMENT blocker 1 (resolved); HARNESS-BUG high 1 (resolved); ERGONOMICS high 1; AUTHORING-INEFFICIENCY med 1; SPEC-GAP high 6; FUNCTIONAL-BUG high 9 / med 2; SECURITY/ROBUSTNESS high 3
- Top recurring failure modes: repeated `ui_describe`; invented skills/KB configuration; signal-before-subscribe; split client/shared state; profile/schema guessing
- Highest-leverage Agent Drafter improvements: engine-level repeated-tool guard; validate skills and KB ids; eager signal subscription; one canonical reactive state; type-safe orchestration builder

This file is updated continuously from the per-app rubrics. Findings are added
with exact app, round, repro prompt/gesture, evidence path, likely layer, impact,
and suggested fix.

### [SPEC-GAP][SEV: high][MITIGATED BY CONTROLLED PROBES] The 100-idea corpus cannot test structural layout diversity
- **Where:** all 100 `Layout` prompts; full audit in [`LAYOUT-DIVERSITY.md`](LAYOUT-DIVERSITY.md).
- **Symptom:** every idea explicitly requires Left, Center, Right, and Bottom regions; 89 name a rail and 96 name an inspector. Generated apps therefore converge on the same persistent-sidebar skeleton even when their themes and scientific widgets differ.
- **Repro:** parse every `**Layout:**` line in `docs/agentic-app-test-ideas-100.md`; counts are Left 100, Center 100, Right 100, Bottom 100.
- **Root cause (best guess):** test-corpus design dominates; starter gravity may reinforce it because explorer/workbench/canvas seed two-column grids.
- **Impact:** the numbered 100-app results alone cannot support a claim that Agent Drafter can or cannot create structurally diverse layouts.
- **Mitigation/result:** five UCSF-only no-sidebar probes now cover dashboard mosaic, centered wizard, radial canvas, full-width tabletop, and full-bleed explorer. All five pass static layout/model/theme audit and browser inspection confirms materially different geometry and look. The numbered prompts remain exact, with an anti-template clause from Spec 016 onward.

### [ERGONOMICS][SEV: high] Agent Drafter store ignores `BIOROUTER_PATH_ROOT`
- **Where:** spec 001 (Variant Tribunal), initial authoring attempt.
- **Symptom:** sessions/config respected `.br-testdrive/runtime`, but `create_app` wrote the uniquely named draft to the global `~/.config/biorouter/agent_drafter` store.
- **Repro:** set only `BIOROUTER_PATH_ROOT=<sandbox>` and invoke a session with the builtin Agent Drafter; inspect `default_root()` output versus `Paths::config_dir()`.
- **Root cause (best guess):** `agent_drafter::default_root()` calls etcetera directly instead of the shared `Paths` abstraction.
- **Impact:** contaminates the user's real application inventory and violates worktree/sandbox isolation.
- **Suggested fix or SDK improvement:** make Agent Drafter use `Paths::in_config_dir("agent_drafter")`, or have `default_root()` honor `BIOROUTER_PATH_ROOT`; add an isolation regression test. The harness now also sets `XDG_CONFIG_HOME`, and the incomplete draft is preserved under `.br-testdrive/quarantine/`.

### [AUTHORING-INEFFICIENCY][SEV: med] Orchestration configuration requires repeated schema guessing
- **Where:** spec 001 (Variant Tribunal), rounds 1–2.
- **Symptom:** Agent Drafter generated six rejected mutations: orchestration sequence vs map, string vs tagged `WorkflowStep`, map vs string, `theme` string vs `ThemeConfig`, missing `created_at`, and worker `model` string vs `ModelSelection`.
- **Repro:** ask Agent Drafter to create four named profiles plus fast/deep routes in the initial `create_app` call from the full Spec 001 block.
- **Root cause (best guess):** free-form nested `serde_json::Value` parameters and whole-manifest rewrites expose a deep schema the model cannot reliably infer from tool errors.
- **Impact:** more than five minutes and many billed tool/model turns before a valid starter existed.
- **Suggested fix or SDK improvement:** add typed `surface`, `theme`, `profiles`, and `routes` parameters or dedicated tools; make `read_app` return a canonical editable manifest skeleton; preserve required metadata server-side on manifest updates.

### [SPEC-GAP][SEV: high] Invented `br.kb` identifier prevents clean runtime setup
- **Where:** spec 001 (Variant Tribunal), first browser load.
- **Symptom:** daemon logged `set active KB failed: kb-id may only contain a-z, 0-9, and '-'`, and the first runtime configuration contained `knowledge_base: "br.kb"` plus the same invalid grant id.
- **Repro:** request “KB of ClinVar/literature” without supplying an installed KB id, then launch the generated app.
- **Root cause (best guess):** Agent Drafter confuses the client API namespace `br.kb` with a persistent KB identifier; lint does not validate IDs.
- **Impact:** noisy/faulty startup and no usable KB capability.
- **Suggested fix or SDK improvement:** lint `knowledge_base` and knowledge source ids with the same validator as the daemon; teach the authoring prompt that `br.kb` is an API, never an id; represent unavailable KB requirements explicitly instead of inventing one.

### [FUNCTIONAL-BUG][SEV: high] Generated multi-agent loop uses display names and lets workers seize UI
- **Where:** spec 001 (Variant Tribunal), live readjudication turn after initial runtime fix.
- **Symptom:** `consult(agent="Prosecutor")` failed because declared keys were lowercase; main repeated unchanged `ui_describe` three times; lowercase `prosecutor` then called `ui_describe` itself and stalled before the remaining three profiles. Evidence is in the app session rows and the browser screenshot/DOM trace.
- **Repro:** click **Re-adjudicate** on `spec-001-variant-tribunal` after the first build.
- **Root cause (best guess):** system prompt names human-facing agents rather than exact manifest keys; worker profiles inherit UI capability and the UI-first system discipline.
- **Impact:** headline multi-agent criterion fails and a paid turn hangs without reaching a verdict.
- **Suggested fix or SDK improvement:** lint system prompts/consult calls against exact profile keys; default consulted workers to `ui.enabled:false`; explicitly state UI ownership is main-only; stop/recover automatically after repeated unchanged `ui_describe`.

### [FUNCTIONAL-BUG][SEV: high] Declared signal is emitted before the agent subscribes
- **Where:** spec 001 (Variant Tribunal), PVS1 direct-manipulation turn; reproduced by endpoint selection in spec 004, STAT3 selection in spec 005, rs123 selection in spec 008, target/parameter gestures in spec 011, an intervention gesture in spec 012, body addition in spec 013, terrain painting in spec 014, and all five layout probes.
- **Symptom:** criterion UI updated locally, but the ambient chip reported `signal "criterion_clicked" is not subscribed`.
- **Repro:** reload the app and click PVS1 as the first interaction.
- **Root cause (best guess):** subscription exists only as prose in the generated system prompt and no agent turn has yet called `ui_subscribe`; the click emits the signal before its accompanying `br.call` begins that first turn.
- **Impact:** the headline app→agent signal path fails on the user's first gesture, even though the parallel `br.call` happens to start a turn.
- **Suggested fix or SDK improvement:** support manifest-declared eager subscriptions or a main-agent startup hook; lint prompts for an explicit `ui_subscribe` call; avoid emitting first-use signals before subscription readiness.

### [FUNCTIONAL-BUG][SEV: high] Generated `br.run` controls execute locally but deliver no turn
- **Where:** spec 002 (Cohort Funnel Foundry), initial browser round.
- **Symptom:** Playwright, coordinate CUA, and DOM-node CUA all activated **Ask architect**; the secondary **Send** handler provably ran because it cleared the input. Neither `br.run` call changed its target to the synchronous “Starting agent run…” state or inserted a message into the app session. Console was clean and the page showed `Session ready / data, ui`.
- **Repro:** load the first Spec 002 build, click **Ask architect**, then fill and click **Send**; query its session messages and inspect `#log`.
- **Root cause (best guess):** app-specific interaction between `br.run` serialization/target mounting and the separately mounted timeline; deterministic baseline covers SDK primitives but not this exact double-timeline pattern.
- **Impact:** every authored primary control looks clickable yet cannot start the agent loop.
- **Suggested fix or SDK improvement:** add a real-browser harness test for `mountTimeline(br, '#progress')` plus `br.run(prompt, '#log')`; lint/preview should execute one wired control. The in-session workaround changed controls to structured `br.call` and succeeded.

### [SECURITY/ROBUSTNESS][SEV: high] Model endlessly retries control-plane tools despite explicit stop discipline
- **Where:** spec 001 (Variant Tribunal), second live instruction; reproduced in specs 002 (Cohort Funnel Foundry), 003 (Pathway Séance), 004 (Trial Regia), and 005 (Omics Loom).
- **Symptom:** `ui_describe` returned `The user has declined to run this tool. DO NOT attempt to call this tool again`; the model immediately called the same tool at least six more times, with no Defense/Clerk/Justice progress. The app prompt already said to call it exactly once and never repeat an unchanged describe.
- **Repro:** resume the durable Spec 001 session after the earlier repeated-describe history and click **Re-adjudicate**; add eGFR in Spec 002; load seeds in Spec 003; run **Check feasibility** in Spec 004; click **Integrate layers** in Spec 005; choose **spots** in Spec 011; click **Stabilize system** in Spec 013; or activate any layout probe. Each later case repeats a control-plane call after successful worker/action phases despite explicit one-call discipline. Four probes repeated `ui_describe`; the constellation probe repeated `ui_subscribe` at least five times.

### [FUNCTIONAL-BUG][SEV: med] Generated progress stream duplicates into the semantic result region
- **Where:** spec 005 (Omics Loom), Integrate layers turn; reproduced in spec 007 (Provenance Autopsy) where frames occupy both Evidence chain-of-custody and Visible progress.
- **Symptom:** every tool-running/completed frame appears both under Agent run status and inside the inspector where the integrated synthesis belongs; the semantic synthesis remains blank.
- **Repro:** click **Integrate layers** and inspect both left `progress` and right `synthesis`/inspector regions during the turn.
- **Root cause (best guess):** the generated app mounts a dedicated timeline and also passes the semantic result region as the `br.call` streaming target.
- **Impact:** the most important scientific interpretation is displaced by duplicate plumbing output.
- **Suggested fix or SDK improvement:** lint for multiple timeline consumers; let `br.call` route progress to the dedicated mount while reserving semantic regions for explicit `ui_patch`/render output.
- **Root cause (best guess):** repeated-tool/user-decline policy is communicated only as text; the engine neither masks the declined tool nor terminates repeated identical calls.
- **Impact:** runaway billed turns, hung UI, and failure of the required second instruction; daemon had to be stopped.
- **Suggested fix or SDK improvement:** hard-disable a declined tool for the remainder of the turn, detect identical declined calls, and terminate with a structured loop error after the first retry; reset repeated-tool state per user turn when `ui_describe` is contractually required.

### [SPEC-GAP][SEV: high] Agent Drafter configures nonexistent skills as runtime requirements
- **Where:** spec 003 requested `pathway-analysis`; specs 004/009 requested `clinical-biostatistics`; specs 006–008 declare `clinical-databases`, `reproducibility`, and `statistical-genetics`; spec 010 declares `rare-disease` and `clinical-databases` without installed-skill verification.
- **Symptom:** the visible agent timeline reports failed `skills__loadSkill`; the app then spends a model step recovering or reasoning without the promised capability.
- **Repro:** load seeds in Pathway Séance or click **Power it** in Trial Regia and inspect the app session tool responses.
- **Root cause (best guess):** the idea spec names a domain capability, but Drafter treats arbitrary prose as an installed skill id without discovering the skill catalog or validating the manifest.
- **Impact:** every affected first turn begins with a deterministic tool failure and may take a less reliable fallback path.
- **Suggested fix or SDK improvement:** expose discoverable installed-skill ids in Drafter context; validate configured skills during lint/build; turn unavailable requested skills into worker-prompt domain reasoning rather than a guaranteed failing tool call.

### [SPEC-GAP][SEV: high] Platform-integration strings are mistaken for working capabilities
- **Where:** corpus-wide audit; detailed counts and catalog evidence in [`PLATFORM-INTEGRATIONS.md`](PLATFORM-INTEGRATIONS.md).
- **Symptom:** 85 specs request KBs and 57 request skills, while the isolated runtime has zero installed KBs/skills. Specs 001–016 nevertheless configure 13 concrete KB ids and seven skill lists; none can be credited as available. The real built-in `knowledge`/`autovisualiser` extensions are widely configured, but a built-in extension does not create the missing domain payload.
- **Repro:** query isolated `biorouter knowledge list`, `extension list`, and `skill list`; compare with every manifest and runtime tool history.
- **Root cause (best guess):** Agent Drafter receives prose integration wishes without an authoritative environment catalog, and lint validates shape more readily than existence.
- **Impact:** apps claim grounding, connectors, or specialist capability that is absent; runtime spends turns on deterministic failures or invents unsupported scientific output.
- **Suggested fix or SDK improvement:** expose typed discoverable catalogs to Drafter, lint ids against them, and report requested/configured/available/exercised separately. The test driver now injects the exact empty payload catalogs and real built-in extension list, rejects invented ids, and checks requested routes/workflows/figures.

### [FUNCTIONAL-BUG][SEV: high] Shared agent state and client control state diverge between turns
- **Where:** spec 004 (Trial Regia), Power followed by Check feasibility; reproduced in spec 015 (FoldScape), multiple layout probes, and spec 016 (AeroCanvas).
- **Symptom:** Trial Regia visibly patched power to n=784 but the next `br.call` serialized `sample_size=248`. In FoldScape, the browser showed selected M1 with φ/ψ -70°/4°, while all worker output analyzed stale L34→A and rendered that stale residue in the presence card.
- **Repro:** click **Power it** then **Check feasibility** in spec 004; select M1/change dihedral/choose A/click **Mutate & rescore** in spec 015; or move between wizard steps/select a tabletop row/select a constellation node in the probes. Visible local selections become blank or differ from serialized agent inputs.
- **Root cause (best guess):** SDK `ui_patch_state` updates the shared document/bindings, while generated client closures serialize a separate local `state` object initialized at 248.
- **Impact:** multi-turn scientific reasoning is internally contradictory and can produce incorrect feasibility flags.
- **Suggested fix or SDK improvement:** generate one canonical state source; have actions, controls, prompt serialization, bindings, and `ui_patch_state` all read/write it; add a two-turn conformance test.

### [FUNCTIONAL-BUG][SEV: high] First-load bindings and direct range control do not reflect initialized state
- **Where:** spec 004 (Trial Regia), initial browser load.
- **Symptom:** feasibility/power bindings were blank until an agent patch; two ArrowRight inputs on the MDE range left both the control and displayed state at 0.35.
- **Repro:** open a fresh Trial Regia app, inspect KPI text, focus the MDE slider, and press ArrowRight twice.
- **Root cause (best guess):** initial local state is not published through the same binding path used by agent patches; rerender resets the form from stale local state.
- **Impact:** direct-manipulation and data readability fail before the first expensive model turn.
- **Suggested fix or SDK improvement:** preview/lint should assert every declared binding resolves from initial state and exercise keyboard/pointer form input before approving a build.

### [FUNCTIONAL-BUG][SEV: high] Generated main agent bypasses declared worker profiles with generic subagents
- **Where:** spec 006 (Ward Board), Round turn; reproduced in spec 012 (Contagion Studio), Add intervention and Fit to data turns.
- **Symptom:** Ward Board declares four UCSF-routed profiles but called generic `subagent(subworkflow=hospitalist|devils_advocate|documentarian)`; Contagion Studio declares Fitter/Adversary/Policy Analyst/Reporter but called two generic `subagent` tools. Required profiles were skipped.
- **Repro:** click **Round** in spec 006, or **Add intervention** / **Fit to data** in spec 012, and inspect tool names plus durable app/worker sessions.
- **Root cause (best guess):** system prompt mentions subworkflows and the model selects a generic delegation tool instead of Apps SDK v2 `consult` tied to manifest orchestration profiles.
- **Impact:** profile-specific isolation, capabilities, routing, and the required specialist panel cannot be proven; the declared orchestration may be dead configuration.
- **Suggested fix or SDK improvement:** lint system prompts to require `consult` for each declared profile key; hide generic subagent delegation from app turns with manifest orchestration, or map it explicitly through the declared profile registry.

### [FUNCTIONAL-BUG][SEV: med] Runtime theme mutation makes scientific regions illegible
- **Where:** spec 012 (Contagion Studio), Fit to data turn; reproduced in spec 013 (Orbital Sandbox), the tabletop/constellation probes, and specs 016–018. Evidence is preserved in each listed result's runtime screenshot.
- **Symptom:** the baseline clinical canvas is coherent, but the agent's live theme/render pass creates large opaque black blocks across the center plot and right KPI/table regions while the page background remains white.
- **Repro:** load the clean app, click **Fit to data**, wait through the first `ui_theme`/render cycle, and compare against `shots/spec-012/baseline.png`.
- **Root cause (best guess):** runtime theme variables are applied inconsistently between app-authored CSS/SVG and SDK-mounted regions.
- **Impact:** the main outbreak visualization and briefing KPIs become unreadable during the very turn meant to update them.
- **Suggested fix or SDK improvement:** preview theme mutations against app-authored SVG/canvas regions, scope variables consistently, and provide contrast/invisibility checks after `ui_theme`.

### [FUNCTIONAL-BUG][SEV: high] Agent renders an intervention plan without invoking the declared action
- **Where:** spec 011 (Reaction-Diffusion Foundry), target steering; spec 013 (Orbital Sandbox), stabilization; spec 014 (Serengeti Engine), ecosystem balance.
- **Symptom:** the main agent consults workers and renders convincing plan text/highlights, but never calls the surface action that would apply the plan. Spec 014 explicitly says lion vision will change 0.68→0.52 while the slider/state stays 0.68.
- **Repro:** choose spots in spec 011, click **Stabilize system** in spec 013, or click **Balance ecosystem** in spec 014; compare rendered narrative with progress tool names and bound state.
- **Root cause (best guess):** generated system prompts emphasize narration/rendering but do not enforce at least one successful `app_call` before declaring a staged intervention.
- **Impact:** the app creates the appearance of agent control while leaving the underlying simulation unchanged.
- **Suggested fix or SDK improvement:** make intervention completion conditional on a successful declared action plus readback; lint main prompts for an explicit action/apply phase; surface a visible partial/failure state when action application is skipped.

### [FUNCTIONAL-BUG][SEV: high] Main fabricates quantitative scientific output after worker reports insufficient data
- **Where:** spec 008 (Manhattan Signal Room), rs123 fine-mapping turn.
- **Symptom:** Fine Mapper explicitly stated PIPs were not defensible without summary statistics, LD/reference ancestry, and allele harmonization; main then called `shade_credible_set` with invented PIPs 0.42/0.20/0.15/0.13/0.10.
- **Repro:** select rs123 and compare the Fine Mapper response with the subsequent action payload/session trace.
- **Root cause (best guess):** app system prompt demands a visually complete worked example, and no runtime guard distinguishes synthetic demo data from unsupported inference.
- **Impact:** the UI presents fabricated normalized probabilities as a credible set, a serious scientific-integrity failure.
- **Suggested fix or SDK improvement:** require provenance labels on generated quantitative action values; add schema fields for synthetic/demo vs computed; block scientific action calls whose required inputs are absent or whose consulted worker returned an insufficient-data status.

### [FUNCTIONAL-BUG][SEV: high] Core drag-only interaction has no accessible/reliable fallback
- **Where:** spec 009 (Survival Atelier), stratum creation; related HTML5 drag limitation first observed in spec 002.
- **Symptom:** real coordinate CUA drags of Age and Stage into the visible drop zone produced no chip/signal; there is no clickable or keyboard alternative, so the guarded model run cannot start.
- **Repro:** at 1280x720 drag either `[draggable=true]` covariate card into the stratum builder, then click **Fit Cox**.
- **Root cause (best guess):** generated UI assumes HTML5 `DataTransfer` drag support and omits accessible interaction parity.
- **Impact:** the app's core workflow, multi-agent loop, and all scientific outputs are unreachable.
- **Suggested fix or SDK improvement:** lint every drag/drop-only surface for keyboard/click parity; add a catalog drag primitive with reliable CUA semantics and default accessible activation.

### [HARNESS-BUG][SEV: high][RESOLVED] Reviewer misread omitted default theme as a missing pack
- **Where:** spec 010 (Diagnosis Odyssey), authoring rounds 3–4; reproduced in spec 015 (FoldScape), including an interrupted futile refinement.
- **Symptom:** `update_app` readback showed `theme.pack="biorouter"`; `build_app` omitted the block, and the reviewer treated that as a failure.
- **Repro:** set the base pack explicitly, build, and inspect the canonical manifest.
- **Root cause:** `ThemeConfig::is_default` deliberately omits the default base theme so v1/default manifests do not gain a redundant block. Absence semantically resolves to `biorouter`.
- **Impact:** the harness falsely failed valid apps and drove Agent Drafter through repeated impossible update→build attempts, wasting turns and time.
- **Resolution:** `resolved_theme_pack()` now maps an absent block to `biorouter`; a regression test covers absent/default and explicit non-default packs. Specs 010 and 015 now pass static theme review.

### [SECURITY/ROBUSTNESS][SEV: high] Profile consults consume full 120s then main silently completes without work
- **Where:** spec 010 (Diagnosis Odyssey), Fabry node turn.
- **Symptom:** Pathfinder and Test Recommender both returned `did not answer within 120s`; no worker sessions completed, Refuter/Chronicler were skipped, and main emitted `Run complete` with zero app actions or visible fallback.
- **Repro:** click Fabry disease in a clean app session and inspect the timeline/session messages.
- **Root cause (best guess):** overly broad profile prompts or profile startup failure meet a fixed long timeout; main prompt has no mandatory action/failure-render branch.
- **Impact:** more than four minutes of latency/cost yields no user-facing result and blocks the required multi-agent workflow.
- **Suggested fix or SDK improvement:** shorter configurable worker deadlines, startup diagnostics, structured timeout results, concurrent bounded profiles, and a mandatory visible partial/failure state when required workers time out.

### [ENVIRONMENT][SEV: blocker][RESOLVED 2026-07-12] UCSF Azure rejected the prior egress IP
- **Where:** both Agent Drafter CLI and app runtime after the first ten drafts; exact resume evidence in [`PROVIDER-BLOCKER.md`](PROVIDER-BLOCKER.md).
- **Symptom:** HTTP 403 with `The IP Address is invalid: 104.52.5.246` from the required `versa_azure/gpt-5.5-2026-04-24` deployment.
- **Repro:** UCSF-only authoring or runtime turns while the machine was off the required VPN route.
- **Impact:** Specs 011–100 and queued refinements were blocked while switching models remained prohibited by the test contract.
- **Resolution evidence:** after the user restored VPN connectivity, the same named Spec 011 session completed a 324.8-second real UCSF turn, produced a manifest/app bundle, and passed static review. Runtime consults also completed on the locked UCSF model.

### [SECURITY/ROBUSTNESS][SEV: high] CLI exits zero when the provider turn actually failed
- **Where:** provider-blocked refinements for Specs 003 and 005–009 plus initial attempts for Specs 011–013.
- **Symptom:** `biorouter run` printed a clear authentication/403 failure but returned rc 0; the original harness credited 2–6 second rounds and continued the batch.
- **Repro:** invoke a named Agent Drafter session while UCSF rejects the IP and inspect both output and process exit code.
- **Impact:** unattended test automation can report successful authoring, consume iteration budgets, and generate misleading missing-app verdicts after no model work occurred.
- **Suggested fix or SDK improvement:** propagate provider/auth failures as nonzero CLI exit status and structured result state. The worktree harness now detects the 403 marker, records rc 75/provider-blocked, excludes it from round budgets, and aborts immediately.
