# Ingesting documents from a chat

> **What this is.** The design of `platform__ingest_source`, the chat-level operation that
> folds documents, folders and links into a knowledge base through the same transactional
> macro the desktop uses — and the record of why a chat had no such operation until
> [issue #108](https://github.com/BaranziniLab/biorouter/issues/108).
> **Status:** Current.
> **Audience:** anyone changing knowledge ingestion, the platform tool surface, or how a
> provider is chosen for a Biorouter-run tool loop.

## The gap this closes

The transactional ingest pipeline has existed since Plan 2. `macros::ingest` materializes
the raw source, opens a git transaction branch, runs a bounded knowledge sub-agent, checks
that the `knowledge/` tree actually changed, commits or aborts, rebuilds the graph cache,
and re-scans the committed tree. The desktop reaches it through
`POST /knowledge/bases/{id}/ingest`.

A **chat could not reach it at all.** The knowledge MCP extension exposes only low-level
primitives — `kb_add_raw_source`, `kb_read_page`, `kb_write_page`, `kb_begin_txn` — and its
instructions told the model, in as many words, to read a raw source and write pages with
`kb_write_page`. So a model asked to "create a knowledge base and ingest these three PDFs"
did exactly that: it extracted text by hand, staged raw sources, deleted and re-added them
while retrying extraction, assembled pages inside large `execute_code` scripts, and wrote
them one at a time.

Every guarantee of the real pipeline was absent from that path — no transaction, no abort,
no verification that curated pages exist. The observed run ended with **raw files on disk
and no knowledge pages**, and nothing in the system said so.

## What the tool owns, and what it does not

`platform__ingest_source` is a thin, honest shell around the macro. It lives in
`biorouter` (beside `platform__ingest_conversation`) rather than in the knowledge MCP
server, for one structural reason: the macro needs a `Box<dyn Completer>` built from a
real provider, and `biorouter-mcp` cannot depend on `biorouter`, where `Provider` lives.
The same reason put `conversation_ingest` there.

| Layer | File | Owns |
|---|---|---|
| Tool spec | [`agents/platform_tools.rs`](../../crates/biorouter/src/agents/platform_tools.rs) | The name, description and schema the model sees |
| Handler | [`agents/knowledge_source_tool.rs`](../../crates/biorouter/src/agents/knowledge_source_tool.rs) | Reading arguments, resolving the target base, choosing a provider |
| Pipeline | [`knowledge/source_ingest.rs`](../../crates/biorouter/src/knowledge/source_ingest.rs) | Path expansion, the per-source batch, the report |
| The work itself | `biorouter-mcp/src/knowledge/macros/ingest.rs` | Transaction, sub-agent, commit/abort, graph rebuild, verification |

Three consequences worth stating, because each is a thing a future change could quietly
undo:

- **There is no new write choke point.** The privacy ratchet (issue #56) is the macro's:
  it raises the base's tier and affiliation once, under the KB lock, before the sub-agent
  runs. `source_ingest` calls that macro and writes nothing itself, so the four documented
  choke points remain four. A path that wrote pages beside the macro would be a fifth, and
  the lint-ratchet regression is what that mistake looks like in practice.
- **One transaction per source, and that is the unit of atomicity.** A batch is N macro
  calls. A source that fails aborts its own transaction and leaves the others alone —
  which is what makes per-source status meaningful and a retry safe. One transaction over
  the whole batch would let one bad PDF discard four good digests.
- **The report cannot read as success when only raw sources exist.** The macro raises an
  error when the `knowledge/` tree did not change, so such a source is counted a failure,
  its own line says so, and a run where nothing succeeded says *"Ingested nothing"* in its
  first sentence. This is the single behaviour the issue is about, and
  `a_run_that_curated_nothing_never_reads_as_ingested` pins it.

## Provider selection: reported, never substituted

The ingest runs on the model bound to the chat, unless the caller explicitly names another
in `model` — in which case it is an alternate provider and **Gate H**
(`privacy::assert_alt_provider_allowed`) applies, through the one call site in
`knowledge_tool.rs` that both knowledge paths now share.

Either way the provider is asked, **before any source is touched**, whether it can drive a
tool loop that Biorouter itself runs:

```rust
fn supports_tool_calls(&self) -> bool { true }   // Provider trait default
```

`claude_code` and `codex` override it to `false`. They accept a `tools` argument and
forward nothing: Biorouter's tools reach their child agent over the MCP tool bridge, which
only the agent *turn loop* establishes. A knowledge sub-agent runs outside that turn, so
the child would be handed no tools and the run would end having written nothing — a
failure indistinguishable, at the far end, from a model that had nothing more to do. That
is why it is caught by asking rather than diagnosed afterwards from a silent run.

`LeadWorkerProvider` folds the two halves with `&&`, exactly as it folds `tier` and
`affiliation`, because a turn lands on either one.

When the answer is no, the tool **refuses by name and ingests nothing**. It does not fall
back to an API provider that would work: that would move the user's inference onto a
different account and a different bill without the user choosing it. The refusal states
both remedies — switch the chat's model, or name one in `model` — and leaves the choice
where it belongs.

Every summary names the model the work ran on, so the provider choice is visible in the
answer rather than inferred from context.

## Auto mode asks for nothing extra

An explicit "ingest these PDFs" already *is* the user's approval. `platform__ingest_source`
carries no mutating verb in its name, so `security::sensitive_ops` never treats its `path`
arguments as candidate write targets and Auto mode raises no second prompt. That is a
property, not an accident, and `the_source_ingest_tool_is_not_a_filesystem_mutation` pins
it — a rename to something like `write_sources` would silently start escalating every
ingest.

The sibling defect, where an unrelated `split("/")` beside a `kb_write_page` call *did*
raise a filesystem-root prompt, is [issue #106](https://github.com/BaranziniLab/biorouter/issues/106).

## Reading what the caller named

`parse_sources` accepts a batch (`sources`) and the single-source shorthands (`path` /
`url` / `text`), and both together. A bare string in `sources` is a URL when it carries an
`http://` or `https://` scheme and a local path otherwise.

**Scheme-first, never "does it exist on disk."** A disk test would make the meaning of an
argument depend on the machine's state at that instant: the same string would be a path on
one run and a URL on the next, and the failure would surface as a confusing fetch rather
than a missing file. Relative paths resolve against the session's working directory and
`~` expands, so what the user typed in chat means what it means in their shell.

Local paths then go through `source_paths::expand_ingest_path` — the desktop dropzone's own
expander — so folders, archives, size caps and unreadable-binary detection behave
identically on both surfaces instead of being re-derived here.

## Tests

```bash
cargo test -p biorouter --lib knowledge::source_ingest
cargo test -p biorouter --lib agents::knowledge_source_tool
cargo test -p biorouter --lib agents::platform_tools
cargo test -p biorouter --lib providers::lead_worker
cargo test -p biorouter --lib security::sensitive_ops
```

Three of the `source_ingest` tests drive the **real macro** against a temporary knowledge
base and assert on files on disk: a batch of local documents produces curated pages and
commits; a source that curates nothing aborts and is reported as a failure with no page
left behind; and one failing source does not discard the others.

## Still open

The Claude Code / Codex end-to-end path waits on
[issue #109](https://github.com/BaranziniLab/biorouter/issues/109), which adds a
provider-driven tool-turn primitive. When it lands, those two providers' `supports_tool_calls`
overrides come off and this tool needs no change — that seam is the whole reason the
capability is declared on the provider rather than checked by name here.

## Related documentation

- [Knowledge base](README.md) — the subsystem index.
- [Knowledge ingestion format roadmap](ingestion-format-roadmap.md) — the conversion layer every source travels through before the sub-agent sees it.
- [Coding-agent providers](../providers/coding-agents/README.md) — why `claude_code` and `codex` reach Biorouter's tools only over the MCP bridge.
- [Privacy tiers](../security/privacy-tiers.md) — the ratchet and the gates the macro applies, including Gate H.
