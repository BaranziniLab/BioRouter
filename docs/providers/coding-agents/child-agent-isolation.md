# What the child agent may not do

> **What this is.** The isolation applied to the spawned `claude` or `codex` process: the exact
> flags and thread parameters, the measurement behind each one, and why they are security controls
> rather than hygiene. Includes the two flags whose absence was demonstrated to be exploitable, and
> the one flag that must never be passed.
> **Status:** Current.
> **Audience:** developers working on the coding-agent providers, and reviewers auditing what a
> child process can reach.

Both vendor CLIs are full agents with their own file and shell tools, their own settings files and
their own MCP servers. BioRouter switches all of that off. The reason is not tidiness: a tool the
child runs itself is invisible to BioRouter's inspectors, permission modes, `.biorouterignore` and
vault, so an unisolated child is a second agent operating on the user's machine under none of
BioRouter's controls. What the child gets instead is BioRouter's own tools, over
[the tool bridge](tool-bridge.md), executed by BioRouter's dispatcher where every existing gate
still fires.

## Claude Code: the arguments, and which ones are load-bearing

Every invocation is `claude -p` with the following. The **argument builder** varies on exactly one
axis — the output format — and a unit test pins that
(`the_output_format_is_the_only_axis_that_varies`,
`crates/biorouter/src/providers/claude_code.rs:1250-1267`). The streaming path then appends two more
flags after the builder has run; see below.

| Argument | Purpose | Security-relevant |
| --- | --- | --- |
| `-p` | Headless (non-interactive) mode. | — |
| `--setting-sources ""` | Load **no** settings sources. | **Yes** — see below |
| `--strict-mcp-config` | Use only the MCP servers BioRouter passes. | **Yes** — see below |
| `--tools ""` | Disable the child's own built-in tools. | **Yes** — see below |
| `--mcp-config <file>` | BioRouter's tools, when the turn has a bridge. | — |
| `--permission-mode bypassPermissions` | Only alongside `--mcp-config`; see [the tool bridge](tool-bridge.md#transport-loopback-http-and-the-url-is-the-credential). | — |
| `--system-prompt <prompt>` | Replace Claude Code's default prompt with BioRouter's. | — |
| `--no-session-persistence` | Do not write the CLI's own session files. | — |
| `--output-format json` \| `stream-json` | One result object per turn on the blocking path; framed events on the streaming path. | — |
| `--include-partial-messages` | Streaming path only. Turns the `stream_event` frames on. | — |
| `--verbose` | Streaming path only. Required by the CLI alongside `stream-json` under `--print`. | — |
| `--model <id>` | The model chosen in BioRouter. | — |

### `--setting-sources ""` — without it, a `-p` run executes the working directory's hooks

Without this flag a `-p` session **executes the hooks in the working directory's
`.claude/settings.json`**, because `-p` shows no workspace-trust dialog.

⚠ **Which directory that is, exactly** — this page claimed for a while that BioRouter sets the
child's current directory to the session's working directory. It does not, and it never has: no
spawn site on this path calls `Command::current_dir`, so the child inherits BioRouter's own process
working directory. Under the desktop app that is the shared daemon's spawn cwd, `os.homedir()`
(`ui/desktop/src/main.ts`, `dir: os.homedir()`), because since BR-54 one daemon serves every window
and `biorouterdSingleton.ts` says outright that its spawn cwd is "only a fallback the GUI never
relies on". Under the CLI it is the user's shell cwd, which really can be any repository they happen
to be sitting in.

The flag is load-bearing either way, and the CLI case is why: a `-p` run started from a hostile
checkout executes that checkout's hooks. What changes is only the reach — on the desktop path the
file within range is the user's own `~/.claude/settings.json`, not an arbitrary repository's.

Tested against a hostile fixture: without the flag, the fixture's `SessionStart` hook ran.
`--strict-mcp-config` alone did **not** stop it — the two flags cover different surfaces and
neither substitutes for the other.

### `--strict-mcp-config` — without it, the child connects the user's own MCP servers

Measured: a bare run showed the developer's **personal clinical-database MCP server** as
`connected` inside a child that BioRouter believed it was fully isolating. `--tools ""` does not
cover this; it suppresses built-ins only. With the flag, the bridge is the *only* MCP server the
child sees, so its tool surface is exactly the session's and nothing of the user's own.

### `--tools ""` — the child's own Read, Edit and Bash stay off

This is the flag that makes every other BioRouter control meaningful. A file read the child performs
itself does not pass `.biorouterignore`; a shell command it runs itself does not pass the command
policy or the sensitive-operations inspector. Switching the built-ins off and routing tools back
through BioRouter is what keeps one enforcement path rather than two.

### `--bare` must never be passed

`--bare` is documented as never reading OAuth credentials or the system keychain — precisely the
credential this provider exists to use. Passing it would silently defeat the whole feature, and
worse, could shift the run onto a metered API key.

⚠ **This is a live maintenance hazard, not a hypothetical.** `--bare` is documented as becoming the
**default for `-p`** in a future release. Two assertions stand against that: a unit test fails if
`--bare` ever appears in the argument list, and `ClaudeCodeProvider::assert_subscription_auth` stops
any turn whose reported `apiKeySource` is not `none`, so the day the default flips, BioRouter fails
loudly instead of quietly billing an API account. If you are here because turns started failing with
an authentication error naming `apiKeyHelper` after a `claude` upgrade, that assertion is what
fired.

The Codex provider has its own half of the same assertion, and the two are not interchangeable —
they read different credentials on different protocols, so naming only the Claude Code one describes
half the running system. `CodexProvider::assert_subscription` asks the live app server
`account/read` before `thread/start` and refuses anything whose account `type` is not `chatgpt`
(`apiKey`, `amazonBedrock`, or a type this build has never heard of). Both stop the turn with a
`ProviderError::Authentication`, which is not retried and carries its own exit code; and both fail
**open** when the child says nothing at all, including when an app server too old to know
`account/read` simply ignores it and the ten-second timeout expires. See
[How the coding-agent providers work](how-it-works.md) for why silence is not treated as evidence.

### The system prompt is replaced, not appended

`--system-prompt` replaces Claude Code's default prompt with BioRouter's, rather than
`--append-system-prompt` adding to it. Besides being correct — the child is answering as BioRouter,
not as a coding assistant — it is a 16x saving: the default prompt measured **25,022 tokens per
call** and BioRouter's measured **1,527**.

### Sessions are BioRouter's to persist

`--no-session-persistence` keeps the CLI from writing its own transcript. A second, divergent
transcript on disk would be governed by none of BioRouter's controls — not compaction, not message
editing, not `.biorouterignore`-driven redaction.

### The streaming path adds two flags and removes none

The streaming invocation is the same argument list with `--output-format stream-json` and two
additions, appended in `ClaudeCodeProvider::stream`
(`crates/biorouter/src/providers/claude_code.rs:815-823`):

- **`--include-partial-messages`** is what makes the path live at all. Without it, `stream-json`
  still emits only whole messages, and the turn would arrive in one piece exactly as the blocking
  path's does. With it, the CLI wraps raw Anthropic Messages-API events in a `stream_event`
  envelope, which is what BioRouter decodes.
- **`--verbose`** is required by the CLI alongside `stream-json` under `--print`; it is a
  format precondition, not a logging preference.

Neither is security-relevant on its own, and that is the point worth stating: **every isolation flag
above is present unchanged on the streaming path**. `--setting-sources ""`, `--strict-mcp-config`,
`--tools ""` and the absence of `--bare` are properties of the shared builder that both paths call,
so a streamed turn is isolated exactly as a blocking one is.

## Codex: the thread parameters

Codex is configured at `thread/start` rather than by flags. Four values are decisions rather than
defaults, and each is pinned by a test.

| Parameter | Value | Why |
| --- | --- | --- |
| `sandbox` | `"read-only"` | The child cannot change anything on the machine. |
| `approvalPolicy` | `"never"` | It must not try to negotiate its way out; approvals are answered by BioRouter's own policy, not by prompting. |
| `ephemeral` | `true` | No Codex session files. BioRouter owns the transcript, for the same reason as `--no-session-persistence` above. |
| `baseInstructions` | BioRouter's system prompt | Replaces Codex's own preamble, which measured ~15k input tokens on a trivial prompt. |
| `config.mcp_servers.biorouter.url` | The bridge URL, when the turn has one | The streamable-HTTP MCP form, which needs no second process. |
| `CODEX_HOME` | Ephemeral directory containing only the existing `auth.json` as a link, or a temporary OS-level copy on Windows when source and temp are on different volumes | Prevents the child's config merge from loading personal MCP servers. The Windows fallback never places credential bytes in a BioRouter-owned buffer and disappears with the isolated home. |
| app-server flags | `--strict-config` plus explicit feature disables | Fails closed on an unsupported isolation setting and removes shell, browser, plugin, image, nested-agent, and other local model-controlled capabilities. |
| `cwd` | The process working directory | BioRouter's own, not the session's. The `Provider` trait has no session in scope (`providers/base.rs`, `complete_with_model` takes a system prompt, messages and tools), which is the same reason the bridge URL has to travel as a task-local. |

### Every child-local approval request is refused

`codex app-server` routes requests back to the host as server-originated messages that block the
turn. BioRouter accepts only the MCP elicitation used by its own gated bridge and refuses every
child-local command, file, patch, or permission escalation in one small decision function:

| Server request | Answer |
| --- | --- |
| `mcpServer/elicitation/request` | **Accept.** This is how an MCP tool call BioRouter is itself serving gets its go-ahead, and those run in BioRouter's dispatcher behind BioRouter's gates. |
| `item/commandExecution/requestApproval` | Denied |
| `item/fileChange/requestApproval` | Denied |
| `item/permissions/requestApproval` | Denied |
| `applyPatchApproval`, `execCommandApproval` | Denied |
| Anything unrecognised | **Denied.** An unanswered request stalls the turn forever, so refusing beats guessing. |

The child is configured with a read-only sandbox and no tools of its own, so a command or
file-change approval request means it is reaching for authority it was not given. The honest answer
is no.

## What the child still has

Isolation is not a sandbox, and this section is the honest statement of the boundary.

- **The child process runs as the user**, in BioRouter's own process working directory — **not**
  the session's; see the `cwd` row above and the note under `--setting-sources ""` — with an
  augmented `PATH` and the user's `HOME`. Claude's built-ins are off. Codex combines a read-only
  sandbox with process feature disables; read-only alone is not a
  confidentiality boundary because it permits host reads.
- **It does not load the user's own MCP servers.** Claude Code enforces that with
  `--strict-mcp-config`. Codex has no equivalent flag and merges thread overrides with its config,
  so BioRouter starts it under an ephemeral `CODEX_HOME` with no `config.toml`; the only retained
  file is a filesystem link to the existing `auth.json`. The bridge declared on `thread/start` is
  therefore the child's complete MCP server set. An event from any other MCP server is still shown
  as a `child` tool card so an upstream isolation regression is visible rather than misattributed.
- **It has network access**, because it must reach its vendor to do inference at all.
- **It has only an audited workspace-control subset, read-only Knowledge tools, and the transactional
  `platform__ingest_source` macro in the turn's bridge grant**, gated as described in
  [the tool bridge](tool-bridge.md#what-still-fires-on-a-bridged-call). Raw Knowledge mutations,
  generic extensions and custom extensions are withheld because they can read or write arbitrary
  host files, including the subscription credential the child process itself needs. A delegated
  child's persisted extension profile is narrowed to the same Knowledge surface even if it
  overrides its provider. An explicit request for an extension outside that surface is refused
  before the child is created instead of recording a capability the child cannot receive.
  `tools/call` also checks exact membership in the grant, so calling an unadvertised bare or
  prefixed name cannot bypass the filter.
- **It does not have BioRouter's own credentials.** `BIOROUTER_SERVER__SECRET_KEY` and the rest of
  the daemon's secrets are stripped from the child's environment, so it cannot act as the daemon
  against its REST API. The inference-diverting credentials are stripped too — see
  [keeping the run on the subscription](how-it-works.md#keeping-the-run-on-the-subscription).
- **It is reaped.** Each turn has a 30-minute ceiling, and the Codex app server is always shut down
  after a turn whether the turn succeeded or not: a leaked `codex app-server` is a live process
  holding the user's credential.

This is the same posture BioRouter's own shell tooling takes, and it rests on the same premise
recorded in [where the privacy campaign stands](../../security/privacy-tiers-campaign-state.md):
these are safety boundaries, not security boundaries — they reliably prevent mistakes, and they do
not pretend to withstand a determined path.

## Related documentation

- [The tool bridge](tool-bridge.md) — what the child gets *instead* of its own tools, and the gates
  every bridged call passes.
- [How the coding-agent providers work](how-it-works.md) — the invocation these flags belong to, and
  the credential scrub that accompanies them.
- [Compliance: vendor terms, BAA and PHI](compliance.md) — why isolation is necessary but not
  sufficient for clinical data.
- [Permission modes](../../security/permission-modes.md) — the modes that govern a bridged tool
  call.
- [Claude Code](../../research/coding-agent-landscape/claude-code.md) and
  [OpenAI Codex CLI](../../research/coding-agent-landscape/codex-cli.md) — external studies of the
  two agents being isolated here, including their own permission and sandbox models.
