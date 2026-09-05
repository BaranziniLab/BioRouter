# The bug reporter

> **What this is.** The design of `platform__report_bug` — the tool that lets the agent gather a session's own evidence, work out what went wrong, and file a Biorouter issue after the user approves the exact text. Covers where it lives, why it is two calls, what it refuses, and how it reaches GitHub.
> **Status:** Current.
> **Audience:** contributors

The user-facing guide is [Diagnostics and bug reports](../troubleshooting/diagnostics-and-bug-reports.md).
This page is the reasoning behind it.

## Why a tool at all

Before this, reporting a bug took three clicks to reach a modal whose "File Bug on GitHub"
button opened a template with the system information filled in and **nothing else** — no
description of what went wrong, no failure list, and a diagnostics zip the user had to
generate separately and drag on by hand. The information a maintainer actually needs was
in the session, and the one participant that had read the session was the agent.

So the shape is: the agent reads its own transcript, distils it, and asks the user to
approve a finished report.

## Where it lives, and why it is not an extension

It is the fifth `platform__*` tool — declared in
[`agents/platform_tools.rs`](../../crates/biorouter/src/agents/platform_tools.rs),
dispatched by `Agent::dispatch_tool_call`, implemented in
[`agents/bug_report/`](../../crates/biorouter/src/agents/bug_report/).

A platform tool rather than a `PlatformExtension` because it needs the session row and the
session manager, which `PlatformExtensionContext` does not carry — the same reason
`platform__ingest_source` is dispatched by the agent loop. Widening that context to carry
a provider is a documented security regression, so it is not the answer here either.

Adding a fifth name to `PLATFORM_TOOL_NAMES` is what makes it survive the Code Execution
filter and stay out of the JS module catalogue; that list is the single source those gates
read, and two tests pin the membership and the dispatch branch.

## Two calls, not one

| Call | Reads | Writes | Approval |
|---|---|---|---|
| `action: "analyze"` | The session's failed tool calls, graded | Nothing | None |
| `action: "file"` | The report the model wrote | A GitHub issue | Proof-backed, showing the body |

The action is **inferred toward `analyze`** whenever the model does not say — including for
an unrecognised value, and for a call carrying a title but no description. A model that
omits an enum must not thereby publish something.

## Finding what went wrong

There is no failures table, no error index and no persisted failure record in Biorouter.
`tool_monitor`'s `ToolOutcome` is in-memory and per-turn, so by the time anyone says
"report a bug" it is gone. The durable account of a failed call is the tool response in
`messages.content_json`, and that is what `evidence.rs` reads.

Two things about that data decide the implementation:

- **A failure has two spellings**, and the commoner one does not look like a failure.
  `{"status":"error"}` is a transport-level failure; `{"status":"success","value":{…,"isError":true}}`
  is a tool that ran and reported a domain failure — a build that broke, a query that
  errored. Both are handled by going through `tool_errors::classify`, which already knows
  the difference, rather than by testing `status` directly. A scan for the first alone
  reports a clean session while the user looks at a red error.
- **Some failures are Biorouter refusing on purpose.** The only failed call in the one real
  session read while designing this was privacy Gate C refusing
  `workspace_read_conversation` on a public model — not retryable, `ToolFailure` kind, a
  long error string, indistinguishable from a hard bug on every coarse signal. Filing it
  would file "the privacy boundary worked" as a defect. Such failures are detected from
  constants `privacy::refusal` exports (so a reword there is a compile error, not a silent
  mis-grade), **labelled rather than hidden** — a refusal genuinely can be the wrong thing
  refused — and excluded from the evidence's own "is this conclusive?" test.

## The push-back

`is_conclusive()` is not "a failure exists". One retryable 429 is the ordinary weather of a
long session; bad arguments are the model's own mistake, which it can already see. When
nothing conclusive happened **and** the user has not said what to report, `analyze` returns
an instruction to ask the user a specific question and not to file until they answer.

That is a plain result, not an error. An error invites a retry, and a retry cannot produce
information the model does not have.

## The redaction harness

Two shapes, deliberately different:

- **`scrub`** rewrites what it recognises — home paths (to `~`), other accounts' usernames,
  vendor tokens, JWTs, bearer tokens, credential assignments (keeping the *key*, dropping
  the value), URL passwords, e-mail addresses.
- **`validate_issue`** re-runs the scrub and **refuses** anything still recognisable, plus
  a missing template section, an unusable title, or an over-long body.

One pass that both rewrote and approved would report success for every pattern it forgot.
Nothing here is a guarantee — a secret that looks like prose survives any pattern set — and
the design assumes the person reading the approval card is the last check.

The section list `validate_issue` requires is asserted against the repository's own
`.github/ISSUE_TEMPLATE/bug_report.md`, so a template rename fails a test rather than
silently producing reports that no longer match it.

## What the user approves

The card carries the rendered body **verbatim**, names the destination repository (loudly
if it has been redirected away from the project's own), says whether pressing the button
publishes immediately or opens a page the user still has to submit, and sets
`requires_user_proof`. Those facts are the consent: a card reading "file a bug report?"
would be asking about a category, not about the paragraph that is going to be
world-readable.

Ordering follows `install_extension` — preflight, await approval, re-check that nothing
changed between the card and the click, then act.

## What it refuses

- **A session classified `Private` is not filed from.** A GitHub issue is world-readable
  and permanent; the report is written from the transcript by a model reading it, and
  nothing here can certify the distillation carries none of it. It refuses rather than
  warns: a user who insists can still file it themselves, with their own eyes on the text.
  What must not happen is the agent publishing private material because a card was clicked.
  The finished report comes back with the refusal, so the work is not thrown away.
- **It honours the DR-15 master switch**, like every other gate. A switch some gates ignore
  is not a switch, and with tiers off, a `Private` marking was written by machinery the user
  has disabled. The classification is on the card either way.
- **It is not advertised where no person can approve.** A `biorouter serve` daemon holds no
  proof-of-user key, so the approval refuses forever there. Both halves are withheld,
  including the read-only analysis: a tool that lets the model write a whole report and
  *then* discover it has nowhere to put it is worse than one that is absent.

## Coding-agent children

It **is** available to a coding-agent child (Claude Code, Codex). That took a dispatch arm
of its own: `ChatBridgeDispatch` routes tools by name and hands everything it does not
recognise to the extension manager, which has never heard of a `platform__*` tool, so a
bridged platform tool without an arm answers `Tool not found` — after the child has already
written a whole report. Three things make it work, and each is a place the next platform
tool will need too:

- `dispatch_report_bug`, beside its two ingest siblings in `ChatBridgeDispatch`.
- A `bug_report` flag on `CodingAgentBridgePlan`, **not** a capability target. Every other
  bridged tool is reached through a bundled capability or an installed extension, and
  `enforce_tool_access` re-checks that grant at dispatch — but filing a bug belongs to no
  extension and is not something the user switches off in Settings. Its one gate is
  `user_proof_available()`, the same as in the main roster. Riding on the Knowledge target,
  which is how the two ingest tools travel, would mean a chat without a knowledge base
  cannot report a bug.
- An explicit arm in `enforce_tool_access` before the grant lookup, because a tool with no
  grant would otherwise fall through to "not in this turn's coding-agent bridge".

⚠ Adding a name to the bridge's roster is a security-surface change: that allowlist is
described in the code as "the reviewed builtin router rosters" and deserves a human look
rather than a silent edit. Two things bound the risk here. The child reaches the tool over
the same relay every other bridged tool uses, so it still runs behind Biorouter's
inspectors, permission mode and privacy gates; and the approval still has to reach a
person — on a coding-agent turn the child is blocked on `POST /tool_bridge/{nonce}`, which
is parked on the card, so `next_provider_wake` is what surfaces it (the #107 mechanism).
Both coding-agent providers are also `ProviderTier::Public`, so Gate A has already refused
to bind one to a private chat and the private-session refusal cannot fire on that path.

## Reaching GitHub

Nothing in this tree had ever authenticated to the GitHub API. Every existing call is
read-only and unauthenticated; the single `gh` shell-out lives in a CLI workflow whose auth
helper launches an **interactive** `gh auth login`, unusable from a tool call. So there are
two filers:

1. **The user's own `gh`**, when `gh auth status` succeeds *non-interactively* — stdin is
   `null` and prompting is disabled, because the failure mode being guarded against is not
   "gh is missing" but "gh would open a login and hang the turn". This genuinely creates the
   issue, under the user's own account, with no credential passing through Biorouter.
2. **A prefilled compose URL**, opened in the browser; the user's click is the submit.

Not a third option: a token Biorouter stores. It would need the credential store, a scope
the user has to reason about and a revocation story, to replace a `gh` most of this
project's users already have.

⚠ There is a **size cliff** between the two. GitHub answers 414 on a compose URL well before
any browser's own limit, and the body is percent-encoded on the way in — markdown roughly
triples. So the same report can be fileable through `gh` and far too large for a URL, and
`compose_url` returns `None` rather than producing a link that 414s. The cap is applied to
the *encoded* URL, not the raw body.

`file_with_gh` refuses outright under `cfg!(test)`. A test that approved the card would
otherwise create a real, public, permanent issue from `cargo test`, on whatever machine
happened to have `gh` signed in.

## The bundle is not attached

The tool posts a short distilled report; it never uploads a diagnostics zip. The zip
contains `session.json`, which is the whole transcript and is **not** redacted — see the
warning in the [user guide](../troubleshooting/diagnostics-and-bug-reports.md). Attaching it
stays a deliberate act by the user, and the receipt says how.

## Tests

```sh
cargo test -p biorouter --lib bug_report
cargo test -p biorouter --test bug_report_evidence   # against a real exported session
cargo test -p biorouter --lib -- platform_tool       # the roster and dispatch invariants
```

The integration binary runs the extractor against an actual `session.json` produced by
`generate_diagnostics` on a running desktop app — 30 messages, four extensions, a bridged
coding-agent provider — with the user's own content replaced and both failure spellings
injected in the exact form `tool_result_serde` writes. It is there because the unit tests
build their conversations in Rust and can therefore only exercise shapes the author already
believed in; the real export is what turned up the third failure nobody had planned for.

## Related documentation

- [Diagnostics and bug reports](../troubleshooting/diagnostics-and-bug-reports.md) — the user-facing guide to this tool, the diagnostics bundle, and filing an issue by hand.
- [Tool routing](tool-routing.md) — how a tool call reaches its handler, and where the platform tools sit in that path.
- [Privacy tiers](../security/privacy-tiers.md) — the classification lattice this tool's refusal reads, and the master switch it honours.
- [Common problems and fixes](../troubleshooting/common-problems-and-fixes.md) — the symptom-by-symptom reference to check before filing anything.
