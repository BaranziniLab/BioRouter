# Context injection, system prompts & environment awareness

Scope: how each agent builds its system prompt, what project-context file
convention it reads, when context is injected (startup vs per-turn vs
mid-turn), and how the agent understands its surroundings (working directory,
time, repo structure).

BioRouter claims are grounded in `internal/context-injection.md` and
`internal/state-awareness.md`. Pi, OpenHands, and Codex CLI claims are grounded
in their `external/*.md` reports. **No report files exist for Goose upstream,
Cline, OpenCode, Aider, or Gemini CLI**; their cells below are drawn from
widely-documented public behavior of those tools and are marked where
uncertain — they are secondary to the primary BioRouter-vs-{Pi,OpenHands,Codex}
comparison.

## Comparison table

| Aspect | BioRouter | Goose upstream | Cline | OpenCode | Pi | Aider | OpenHands | Codex CLI | Gemini CLI | Claude Code |
|---|---|---|---|---|---|---|---|---|---|---|
| System prompt source | Embedded `system.md`, MiniJinja-rendered | Embedded template (Goose lineage) | Large hardcoded prompt | Templated prompt | ~1000-token minimal prompt | Compact prompt | Jinja `.j2` preset registry | Checked-in Markdown per model | Templated prompt | Hardcoded prompt |
| Per-model prompt variants | None except toolshim JSON rewrite | None | None | None | None | None | Presets (DEFAULT/PLANNING), not per-model | **Yes — per-model files** (`gpt-5.2-codex_prompt.md`, etc.) | None | None |
| Project context file | `.biorouterhints` + `AGENTS.md` (configurable, `CLAUDE.md` works) | `.goosehints` | `.clinerules` | `AGENTS.md` | `AGENTS.md`/`CLAUDE.md` | `CONVENTIONS.md` | `AGENTS.md` (+`CLAUDE.md`/`GEMINI.md` variants) | `AGENTS.md` (+`.override.md`) | `AGENTS.md`/`CLAUDE.md` | `CLAUDE.md` |
| Hierarchy / precedence | Global + git-root→cwd walk, deeper appended | Global + project | Project + global | Dir walk | Global→parents→cwd, most-specific last | Repo-level | Repo skill always-loaded | Global override→root→cwd, deeper overrides (32 KiB cap) | Global→hierarchical `GEMINI.md` | Hierarchical enterprise→project→user |
| Untrusted-file framing | **No** — hint body inserted verbatim as system-level instruction | No | No | No | No (explicitly "expected local risk") | No | Skill/repo context trusted | Trusted | No | **Yes — project files treated as lower-trust** |
| When base prompt built | **Rebuilt every user turn** (re-reads hints from disk) | Per turn | Per turn (+ env each msg) | Per turn | Startup + per-turn extension rewrite | Startup/per-turn | Conversation start | Once per run/session | Startup | Session start |
| Ambient/env awareness | cwd + time via per-turn MOIM `<info-msg>` | cwd/time | **Rich `environment_details`** each msg (file list, open tabs, cwd) | cwd + LSP | cwd in prompt options | cwd, git | cwd, workspace | world-state + `current_time.rs` context fragments | cwd, env | git status + cwd + dir snapshot at start |
| Repo map / structure | **None** — one "Working directory" line only | None | File list injected | LSP symbols | None | **Ranked tree-sitter repo-map** | None (agent explores) | None (agent explores) | Dir tree snapshot | Dir listing at start |
| Mid-conversation dynamic injection | MOIM re-injected **every action**; explicit-resource-context; skill inlining | No | env_details each msg | LSP diagnostics | `before_agent_start`/`context` extension hooks rewrite prompt/msgs | repo-map refresh | Path-triggered rules on file ops; condensation | `context-fragments`, `@`-mention, memories layer | context refresh | Reminders, `@`-mention |
| Transclusion / imports | **`@import`** with boundary+depth(3)+cycle+gitignore guards | No | No | No | No | No | No | `@`-mention file expansion | `@`-mention imports | `@`-mention imports |
| Total context budget | **None** — 128 KB per-file *parse* cap only, no aggregate | None | Window-managed | Window-managed | Window-managed | Repo-map token budget | Condenser (`max_size`, `keep_first`) | `project_doc_max_bytes` 32 KiB + compaction | Window-managed | Window-managed |
| MCP/extension instructions → prompt | Each server's `instructions` rendered verbatim under `## <name>` (Unicode-sanitized) | Same (Goose lineage) | MCP instructions | MCP | MCP omitted by design (skills instead) | n/a | MCP tools + skills | MCP instructions | MCP | MCP + skills |
| Cross-session durable memory into prompt | Opt-in KB/"Soul", not auto-injected | No | Memory-bank (user pattern) | No | File-based (`TODO.md`) | No | Persisted event store | **Self-maintained ranked `~/.codex/memories/`** | No | Memory files |

## Where BioRouter is ahead

- **Per-turn prompt rebuild with live hint re-read.** BioRouter re-renders the
  whole system prompt on every user turn and re-reads `.biorouterhints`/`AGENTS.md`
  from disk each time (`reply_parts.rs:149-156`, per `internal/context-injection.md`).
  Codex, OpenHands, and Gemini build the instruction chain once per run/session,
  so an edit to a context file mid-session is not seen until restart. BioRouter
  picks it up on the next turn.
- **MOIM: durable state re-injected into fresh context every action.** The
  `<info-msg>` block re-presents time, cwd, and each platform extension's
  `get_moim()` (notably the live todo list) on every provider call
  (`moim.rs:12`, `todo_extension.rs:197`). Because todos live in
  `extension_data` and are re-injected, task state survives compaction and is
  always in-window — a pattern the internal review rightly calls "genuinely
  good" and that most agents only approximate with fragile system-prompt
  reminders. Its MOIM insertion invariant (after trailing tool_results, never
  between a tool_call and its result, `moim.rs:31-41`) is a correctness win Pi
  and Codex only replicate for compaction cut-points.
- **`@import` transclusion with a real security model.** Hints support `@path`
  imports resolved within an import boundary (git root/cwd), depth-capped at 3,
  cycle-guarded, gitignore-suppressed, with adversarial tests
  (`import_files.rs`). Among peers only `@`-mention *file expansion*
  (Codex/Gemini/Claude Code) is comparable, and none of the reviewed reports
  document boundary/depth/cycle guards as thorough as BioRouter's.
- **Prompt-injection hardening on untrusted strings.** Every extension
  instruction, override, and extra is Unicode-tag-sanitized (strips U+E00xx
  blocks) and a contract test pins each behavioral clause against silent
  removal (`prompt_manager.rs:118-124,368-415`). This is a concrete defense the
  external reports do not mention for any competitor.
- **Cache-stable assembly.** Hour-granularity system-prompt date, minute-
  granularity MOIM, name-sorted extensions/tools are all deliberately tuned for
  multi-session prompt caching — an optimization Codex/OpenHands discuss for
  compaction but not for the base prompt.

## Where BioRouter is behind

- **No repo map / structure awareness — Aider does it best.** BioRouter tells
  the model exactly one line about the project ("Working directory: …",
  `extension_manager.rs:1490`) and has *no* code that generates a file listing
  or symbol map (`internal/state-awareness.md`, absence finding). Aider's
  **ranked repo-map** is the mechanism to copy: it parses every source file with
  tree-sitter to extract definitions and references, builds a graph, runs a
  PageRank-style ranking weighted by which symbols matter to the current chat,
  and emits a token-budgeted tree of the most relevant files/signatures into the
  prompt — so the model sees project structure without spending tool calls
  rediscovering it. A lighter first step: Cline/Claude Code inject a cwd file
  listing / directory snapshot at session start. BioRouter forces the model to
  glob/`ls` its way to structure every session.
- **No per-model prompt variants — Codex does it best.** BioRouter ships one
  fixed `system.md` for 43+ providers of wildly varying capability; the only
  provider-conditional transform is the toolshim JSON rewrite
  (`reply_parts.rs:160-166`). Codex CLI checks in a **separate Markdown prompt
  per model** (`gpt-5.2-codex_prompt.md`, `gpt-5.1-codex-max_prompt.md`,
  `prompt_with_apply_patch_instructions.md`) selected at load time, letting it
  tune persona, tool-use rules, and verification rigor to each model. Reimplement
  as a prompt-variant table keyed on provider/model with a default fallback.
- **No total context budget — OpenHands/Codex do it best.** BioRouter
  concatenates hints, extension instructions, inlined skill bodies, and MOIM
  with only a 128 KB per-file *parse* cap and no aggregate accounting
  (`import_files.rs:53`; gaps #1). OpenHands' `LLMSummarizingCondenser`
  (`max_size=80, keep_first=4`, pinned head + compressible middle +
  minimum-progress guard) and Codex's `project_doc_max_bytes` (32 KiB
  accumulation cap) + dual-strategy compaction both bound injected context
  deterministically. A large `AGENTS.md`, a chatty MCP server, or several
  `@import`s can silently blow BioRouter's window today.
- **Project context files are trusted as system-level instruction — Claude Code
  does it best.** BioRouter inserts hint bodies and their `@import` expansions
  verbatim (Unicode-tag-stripped only) with no "treat as untrusted data" framing
  (gaps #5), so a malicious repo `AGENTS.md` becomes system-prompt-level
  instruction. Claude Code treats project files as lower-trust context. The
  mechanism to copy: wrap repo-file content in explicit data-not-instruction
  framing and/or gate loading behind a per-directory trust decision — Pi's
  **Project Trust** (`~/.pi/agent/trust.json`, ask/always/never per directory)
  is the cleanest documented split of "should I load this repo's files?" from
  "is this content trusted instruction?".
- **Self-maintained cross-session memory — Codex does it best.** BioRouter's
  durable memory (KB/"Soul") is opt-in per query and never auto-injected
  (`internal/state-awareness.md` #6). Codex's `memories` crate distills finished
  sessions into ranked, cited `~/.codex/memories/` entries
  (`usage_count`/`last_usage`) injected as developer instructions — durable,
  self-maintained lab know-how vs. static hint files.
- **Frozen system-prompt clock.** `{{current_date_time}}` is set once at agent
  construction and never refreshed (UTC hour granularity), while MOIM uses Local
  minute granularity — two clocks the model sees at once (gaps #2). Codex
  refreshes time per turn via `current_time.rs` context fragments.

## Best-in-class and worst-in-class per aspect

- **System-prompt design & discipline:** Best = **Codex CLI** (per-model files,
  explicit completion audit, path/permission awareness injected). Runner-up =
  BioRouter (compact, contract-tested clauses). Worst = **Pi** *by choice*
  (~1000 tokens, no planning/verification norms) — excellent minimalism but the
  least behavioral guidance.
- **Project context file convention:** Best = **Codex CLI / Claude Code** —
  clear precedence, override files, size caps, and (Claude Code) lower-trust
  framing. BioRouter is strong on flexibility (configurable filenames + import)
  but weakest on **trust** — worst-in-class for treating repo files as trusted
  instruction, tied with Pi (which at least declares the risk openly).
- **Environment / surroundings awareness:** Best = **Cline** (rich
  `environment_details` — file list, open editor tabs, cwd — injected every
  message) closely followed by **Aider** (repo-map). Worst = **BioRouter** and
  **Pi**: one cwd line, no structure. This is BioRouter's single largest gap.
- **When context is injected (freshness):** Best = **BioRouter** — full per-turn
  rebuild + per-action MOIM re-injection means live hints and live state every
  turn. Worst = the once-per-session builders (**Codex, OpenHands, Gemini**) for
  freshness, though they trade it for cache stability and lower cost.
- **Dynamic mid-conversation injection mechanism:** Best = **Pi** (typed
  `before_agent_start`/`context` hooks that can rewrite the prompt and message
  array non-destructively) and **OpenHands** (path-triggered rules on file ops).
  BioRouter's MOIM is powerful but hardcoded to extensions with no user hook and
  is re-injected without dedup (gaps #3), so it accumulates repeated blocks.
- **Import/transclusion safety:** Best = **BioRouter** (`@import` with
  boundary/depth/cycle/gitignore guards). Others offer at most `@`-mention file
  expansion without documented guards. Worst = tools with none (Cline, Aider,
  Goose).
- **Context budgeting:** Best = **OpenHands** (structured condenser with pinned
  head + minimum-progress + hard-reset). Worst = **BioRouter** (no aggregate
  budget at all).

## Implications

1. **Add a repo map / workspace summary.** The highest-value, lowest-risk change
   per both internal reviews: inject a cached, gitignore-aware directory/symbol
   summary into MOIM or the system prompt. Start with a cwd file listing (Cline/
   Claude Code level); graduate to an Aider-style ranked tree-sitter repo-map.
   This directly closes BioRouter's worst-in-class aspect.
2. **Introduce a total context budget.** Add aggregate token accounting and
   ranking/truncation across hints + extension instructions + inlined skills +
   MOIM, mirroring OpenHands' pinned-head + minimum-progress condenser and
   Codex's `project_doc_max_bytes` cap. Today any of those inputs can silently
   overflow the window.
3. **Treat project context files as lower-trust.** Wrap hint/`@import` bodies in
   explicit data-not-instruction framing and consider Pi-style per-directory
   trust gating before loading repo resources. Keep the existing Unicode-tag
   sanitization; extend the trust boundary to content semantics, not just
   character classes.
4. **Per-model prompt variants.** Replace the single `system.md` with a
   provider/model-keyed variant table (Codex pattern) so ask-vs-act,
   verification, and tool-use norms can be tuned to model strength — important
   given 43+ providers. Retain the contract test per variant.
5. **Refresh the clock and dedup MOIM.** Recompute `current_date_time` per turn
   (or drop it in favor of MOIM's timestamp) to remove the two-clock
   contradiction, and remove the prior MOIM block before inserting the new one
   so ambient context stops accumulating.
6. **Move core disciplines out of extensions into `system.md`.** Planning/todo
   and batching guidance currently live only in the todo/code-execution
   extensions' `get_moim`, so sessions without them lose the guidance entirely.
   Base them in the system prompt.
7. **Auto-inject durable memory.** Consider a Codex-style self-maintained,
   ranked cross-session memory (or auto-consulted "Soul" facts) rather than the
   current opt-in-per-query KB, so lab-specific know-how accumulates without the
   user asking.
