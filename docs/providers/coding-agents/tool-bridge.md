# The tool bridge

> **What this is.** How BioRouter's own extensions — SPOKE, UCSF OMOP, knowledge, Auto Visualiser,
> any marketplace plugin — reach a child coding agent while BioRouter still executes them behind
> its inspectors, permission mode, `.biorouterignore`, vault and privacy gates. Why MCP is the only
> channel that can do this, why the capability travels in the URL rather than a header, how a
> bridged call becomes a visible tool card without being executed twice, and why a call needing
> human approval is refused rather than parked.
> **Status:** Current.
> **Audience:** developers working on the coding-agent providers, the extension layer, or the
> daemon's routes.

`claude` and `codex` are complete agents: they run their own loop and execute their own file and
shell tools. BioRouter switches those off, because a tool the child runs itself is invisible to
BioRouter's inspectors, permission modes, `.biorouterignore` and vault — see
[what the child agent may not do](child-agent-isolation.md). But a child with no tools can do
nothing, which is most of the point of using BioRouter at all. The bridge is how the tools come
back, with every gate intact.

## MCP is the mechanism, not one option among several

There is exactly one channel that returns a tool *result* into a live turn of either CLI: an MCP
server the child itself calls.

Neither CLI's permission protocol has an outcome meaning "the host already ran this, here is the
result". Claude Code's `can_use_tool` resolves only to allow — optionally with rewritten input — or
deny. Codex's approvals are approve or deny. So the obvious design, intercepting the child's own
tool call and answering it with BioRouter's result, is not expressible in either protocol. The
child has to *call BioRouter*, and MCP is the call.

```text
MCP tools/list  <-  the session's tool set, exactly as the model would see it
MCP tools/call   ->  the inspector stack, then ExtensionManager::dispatch_tool_call
```

## It is generic, and that is the point

Nothing in the bridge knows anything about any individual tool, and no tool needs per-tool work to
become available to a child. That falls out of both sides already speaking MCP: BioRouter's tools
*are* `rmcp::model::Tool`, and `ExtensionManager::dispatch_tool_call` already takes MCP's own
`CallToolRequestParams`. The bridge is a relay between two things that already fit.

The consequence worth stating plainly: **a new extension, a BAAM marketplace plugin or a future
built-in tool works the moment it loads.** One bridge, zero per-tool work.

Verified against a 60-tool surface — both CLIs accepted a 73-character prefixed tool name, a schema
using `$defs`/`$ref`/`oneOf`, an image result, and a `ui://` embedded resource, all passed through
unchanged.

## What still fires on a bridged call

A grant is a snapshot taken when the turn starts, not a handle back to the agent: the provider is
called from inside the agent's own stack and cannot hold a reference to it, and a grant that
outlived its turn would be a capability with no owner. The snapshot carries the session, the
permission mode, the extension manager, the inspection manager, the conversation, the already
filtered tool set, the turn's privacy capability, **the turn's own cancellation token**, the
session's hooks manager, and the app's secret vault when there is one. The last three are there
because a bridged call is a tool call: without the token nothing the child started is reachable by
Stop, without the hooks manager a `PreToolUse` rewrite cannot be collected, and without the vault a
`{{vault:NAME}}` reaches the tool as a literal string.

| Gate | Where it applies to a bridged call |
| --- | --- |
| Tool discovery / privacy Gate E | Inherited. The advertised set is the one `filter_tools` produced for the model — already tier-filtered and reach-filtered — and `tools/list` serves it verbatim, so there is no second policy here to drift from the first. |
| Tool inspectors (command policy, sensitive ops, everything in the inspector stack) | Run on every call, against the conversation snapshot and the session's permission mode. |
| Permission mode | The inspectors' permission decision is honoured: denied is refused, and "no decision was reached" is refused too — an absent decision must never read as approval. |
| Privacy Gate C | `dispatch_tool_call` is the one choke point every tool call passes through, and a bridged call goes through it with the turn's `CallCapability`. |
| `PreToolUse` hook rewrites | Applied and then **re-judged**. The hooks have already run inside the inspector pass, so their `updatedInput` is collected and applied, and every inspector except the hook one re-runs on the rewritten arguments — otherwise a hook would be a hole straight through the security and permission gates, which only ever saw what the child's model asked for. The rewrite is taken scoped to this call's own request id, because the staging buffer is per session and bridged calls run concurrently. |
| `text_editor` path jail | Pointed at **this** grant's mode before anything is dispatched. The jail is a process-global atomic whose only other setter is the agent's own inspection batch, which a coding-agent turn never reaches — so without this a bridged call ran under whatever the last session in the process left behind. It is correct at the instant it is written rather than for the duration of the call; see the residual noted below. |
| `.biorouterignore`, vault, session working directory | Whatever BioRouter's dispatcher and inspectors already enforce, because BioRouter is the process executing the tool. A `{{vault:NAME}}` in the arguments is resolved on the leaf dispatch path, after the call has been judged and immediately before it runs — the same position the agent's own path uses, so the inspectors and the user's hooks never see the decrypted secret. |
| Cancellation | The grant carries the turn's own token and hands it to `dispatch_tool_call`. Issue #72's process-tree kill, `AppState::cancel_turn` and the websocket `TurnGuard` all reach a running tool through that one token, so a token minted at the dispatch site would leave all three pulling on nothing — the user presses Stop, the child dies, and the `developer__shell` it launched keeps running detached. |

⚠ **The inspector pass is why this is not a thin proxy onto `ExtensionManager`.**
`POST /agent/call_tool` *is* that thin proxy, and its own comment records the cost: it bypasses the
agent loop and therefore every `ToolInspector`. A child agent's tool calls are model-initiated and
must be inspected exactly like the parent model's.

The privacy capability is **sampled once**, when the grant is issued, and threaded from there. A
gate on this path asks the sampled capability rather than re-reading the master switch — a second
read is precisely the race `CallCapability` exists to close.

⚠ **Two residuals, recorded rather than implied away.** The path jail is one process-global atomic
shared by every session, and between the write and the dispatch the call awaits through the
inspector pass — which executes the user's `PreToolUse` hooks as real shell commands. A concurrent
writer in that window (another bridged call in another session, or an ordinary agent turn) can flip
it, so an Approve-mode session's write can still land with the jail down. The agent's own path has
the identical window; what the bridge changes is the frequency, from once per batch of the model's
tool calls to once per bridged tool call. Closing it means making the jail per-call state instead of
a process global. Separately, a hook's `additionalContext` and `systemMessage` have **nowhere to go**
on this path — the model that made the call lives in another process, and the tool result is data
rather than a channel for out-of-band prose — so the bridge drops its own staged entries
deliberately, rather than leaving them for the session's next ordinary turn to inject into an
unrelated transcript.

## A call needing approval is refused, not parked

If the permission inspector routes a call to `needs_approval`, the bridge returns a refusal whose
text tells the child's model to ask the user in words:

```text
`<tool>` needs a person's approval, and this turn has no way to ask for one.
Tell the user what you wanted to run and why, and let them approve it.
```

Waiting is not available. The child is blocked on an HTTP response and there is no channel through
which a human could answer it, so parking the call would stall the turn until the timeout — a
half-hour of nothing. Refusing is also the fail-safe direction: the turn ends with the user knowing
what was wanted, rather than with something having happened.

Refusals travel as an MCP tool **result** with `isError`, not as a JSON-RPC error. The distinction
matters: a JSON-RPC error is a transport failure the child may retry or treat as a broken server,
whereas `isError` is a result the model reads and can act on. It is how the model learns to ask the
user instead of retrying.

## The mirror: how a bridged call becomes a visible card

For a long time a bridged call was completely invisible. It ran on a different task from the turn
that issued the grant, nothing on that path yielded an agent event or persisted a message, and the
GUI showed a spinner and then an answer — with no record that a tool had run at all.

The mirror closes that. As the child's stream reports each call, the provider mints an ordinary
**`ToolRequest` / `ToolResponse` message pair** carrying the call id, the tool name with the
`mcp__biorouter__` prefix stripped, the arguments and the result. Those are the same message types
every API provider produces, so the existing tool cards render them with no frontend change: a
skeleton the moment the tool's name is known, then a loading card, then a green or red card that
expands to the exact arguments and output. The pair persists like any other tool traffic, so
reopening the session shows the same cards, and the transcript flattener carries it into the next
turn's prompt as `[called tool: …]` / `[tool result: …]` — which is what stops the child re-running
lookups it has already done.

A failed call is recorded as a **successful transport carrying `isError: true`**, not as a
transport-level error. That is the shape the card reads to colour itself, and it keeps the failure
text readable in the card body instead of collapsing it to an error string
(`crates/biorouter/src/providers/coding_agent/mirror.rs:417-450`).

### Why the pair carries a marker

Each mirrored `ToolRequest` and `ToolResponse` carries a reserved key,
`biorouterProviderExecuted`, in the per-tool provider metadata the types already had
(`mirror.rs:63`). Its value is `bridged` for a call that ran on BioRouter's side of the bridge, and
`child` for one that ran inside the child's own sandbox.

The marker is not decoration. **An unmarked `ToolRequest` in a turn's response is dispatched by the
agent loop**, and `categorize_tool_requests` filters on message content only — it never reads
metadata. So a mirrored pair without the marker is either a `Tool '…' not found` error row (with the
bridge prefix intact) or a genuine **second execution** of a call the bridge already ran. A shell
command run twice is not a display glitch. The loop's one new branch asks
`mirror::contains_provider_executed` and, for a message carrying any mirrored content, persists and
yields it while dispatching nothing (`crates/biorouter/src/agents/agent.rs:7182-7202`).

Two design choices are worth knowing before touching this:

- **The predicate is "any", not "all".** A message that somehow mixed marked and unmarked content
  dispatches nothing at all. The worst case is then a card whose tool did not run — visible, and a
  decoder bug — rather than a command that ran twice, which is invisible and unrecoverable.
- **The marker is deliberately *not* a `MessageProvenance` variant.** That type is BR-71's
  security-purposed cross-session stamp, whose presence already means something specific to merge
  boundaries and the subagent surfaces, and whose unknown kinds degrade to `None`. Overloading a
  security signal with a display one would also lose the stamp silently on an older reader. The
  metadata home needed no schema change: `metadata` was already a free-form object on both types in
  the generated OpenAPI schema, so no client regeneration was required.

### `bridged` and `child` are different guarantees

| Marker | What ran, and under what | Where it comes from |
| --- | --- | --- |
| `bridged` | A BioRouter tool, executed by BioRouter's dispatcher behind every gate in the table above. | Claude Code's `tool_use`/`tool_result` frames; Codex `mcpToolCall` items whose server is `biorouter`. |
| `child` | Something the child ran itself, under **none** of BioRouter's gates: Codex's `exec`/`apply_patch` inside its read-only sandbox, and any MCP server the user configured in their own `~/.codex/config.toml`. | Codex `commandExecution` / `fileChange` items, and `mcpToolCall` items from any other server (`crates/biorouter/src/providers/codex.rs:789-826`). |

Showing a `child` call is a deliberate honesty choice: it happened whether or not BioRouter drew it,
and hiding it would be worse. It is not an endorsement, and the distinction is real rather than
cosmetic. ⚠ **The GUI does not yet draw a label separating the two** — the marker rides in the
persisted metadata, but a card reading `exec` looks like any other card today. Until that label
lands, read `exec` and `apply_patch` cards on a Codex turn as child-executed.

### What the mirror does not fix

A call routed to `needs_approval` is still refused rather than prompting, exactly as described
above. What changed is only the reporting: the refusal now shows as a red card naming the tool,
instead of vanishing into a turn that quietly did less than the user thought. Interactive approval
of a bridged call would require parking the child's HTTP request until a human answers, which is a
separate design and deliberately deferred — see
[streaming and tool-call parity](streaming-and-tool-call-parity.md).

## Transport: loopback HTTP, and the URL is the credential

The bridge is an HTTP endpoint on the daemon — `POST /tool_bridge/{nonce}` — rather than a spawned
stdio MCP server. That avoids inventing a second process and a socket, and it means the child talks
to the **live** `ExtensionManager` rather than to a copy. Both CLIs accept a remote MCP server on
loopback HTTP; that was verified before the design was chosen.

The capability cannot live in a header. Claude Code will send an `Authorization` header from its
config file, but **Codex sends none at all** — observed as `auth=None` on every request it made. A
header scheme would authenticate one client and not the other. So the capability lives in the URL
path, as an unguessable single-turn nonce that both CLIs transmit by construction:

| Property | Value, and why |
| --- | --- |
| Form | 32 hex characters from a v4 UUID — long enough that guessing is not a strategy. |
| Lifetime | One turn. The lease revokes the grant on drop, so a panicking or early-returning turn cannot leave a capability behind. |
| Scope | One session's already-filtered tool set, behind the full gate stack. Far narrower than the daemon's REST API. |
| Not the daemon secret | The child's environment is scrubbed of `BIOROUTER_SERVER__SECRET_KEY`, and the bridge route deliberately has no secret-key gate. |
| Handling | The route must not log the path, and an unknown nonce answers identically to any other unusable one, so it cannot be used as an oracle for which nonces exist. |

How the URL reaches each CLI:

```json
// Claude Code: written to a 0600 temp file, passed as --mcp-config
{ "mcpServers": { "biorouter": { "type": "http", "url": "<bridge url>" } } }
```

```json
// Codex: thread/start's `config` override map, i.e. mcp_servers.<name>.url
{ "mcp_servers": { "biorouter": { "url": "<bridge url>" } } }
```

Claude Code's configuration is a **file**, not an inline JSON string, even though the CLI accepts
both: the URL carries the turn's capability and `argv` is readable by any process running as the
same user. `NamedTempFile` creates it 0600, and the handle is deliberately bound for the whole run —
dropping it deletes the file, and a child that started a moment later would find no configuration.

Claude Code's invocation also gains `--permission-mode bypassPermissions` when a bridge is
present. That looks alarming and is correct here: with the built-ins off, the only tools that exist
are BioRouter's, and each one is inspected and permission-checked on BioRouter's side of the bridge
before it runs. Leaving the child to prompt instead would stall the turn, since a `-p` session has
nobody to ask.

## The protocol surface

| JSON-RPC method | Behaviour |
| --- | --- |
| `initialize` | Answers protocol version `2024-11-05`, a `tools` capability, and `serverInfo` naming `biorouter` and its version. |
| `tools/list` | The grant's tool set, verbatim: name, description, input schema. |
| `tools/call` | Validated (`name` required, `arguments` must be an object or absent), then run through the gate stack above. |
| `server/discover`, `ping` | An empty result. Claude Code probes `server/discover` before `initialize`; an empty result is a clean "nothing extra to discover" rather than an error it would log. |
| Anything else | JSON-RPC `-32601`. |
| A notification (no `id`) | `202 Accepted` with no body. Answering a notification with a JSON-RPC envelope is a protocol error some clients reject. |
| An unknown or expired nonce | JSON-RPC `-32001`, "this tool bridge is no longer active; its turn has finished" — the same answer for a well-formed miss and for a malformed nonce. |

## Lifecycle, and running with no bridge at all

The daemon publishes its base URL once it has bound a port; grants live in a process-global map
keyed by nonce, and only `claude_code` and `codex` are ever issued one. `live_grants()` exists for
an operator looking for a leak, not for tests — the map is process-global, so a count would measure
other work in the same process.

If there is **no base URL** — a CLI process with no HTTP server — the providers run the child
**tool-less** rather than failing. That is the correct degradation: there would be nothing for the
child to connect to, and a turn that can answer from the conversation should still answer.

⚠ **The bridge URL rides a task-local, and it must be read at construction time.** The `Provider`
trait has no session in scope — `complete_with_model` receives a system prompt, messages and tools
and nothing else — so `Agent::reply` scopes the URL around the call it makes into the provider.

The scope wraps the awaited call that *builds* the response, not the consumption of what that call
returns. A `stream()` implementation therefore reads the URL and spawns the child inside the scope,
**before returning the stream**; a poll of the returned stream may not read it, because by then the
scope is gone. Both providers do exactly that, and each says so in its own `stream()` header
(`crates/biorouter/src/providers/coding_agent/bridge.rs:625`,
`crates/biorouter/src/providers/claude_code.rs:770-777`). The lease itself is not the constraint —
`Agent::reply` binds it before the scope and it lives to the end of that loop iteration, which
outlasts stream consumption.

## Where the code is

| Concern | File |
| --- | --- |
| Grants, leases, the nonce, the task-local | [`crates/biorouter/src/providers/coding_agent/bridge.rs`](../../../crates/biorouter/src/providers/coding_agent/bridge.rs) |
| The mirror marker, and the request/response pair builders | [`crates/biorouter/src/providers/coding_agent/mirror.rs`](../../../crates/biorouter/src/providers/coding_agent/mirror.rs) |
| The loop branch that persists a mirrored pair without dispatching it | [`crates/biorouter/src/agents/agent.rs`](../../../crates/biorouter/src/agents/agent.rs) |
| The HTTP/JSON-RPC endpoint | [`crates/biorouter-server/src/routes/tool_bridge.rs`](../../../crates/biorouter-server/src/routes/tool_bridge.rs) |
| Handing the URL to Claude Code | [`crates/biorouter/src/providers/claude_code.rs`](../../../crates/biorouter/src/providers/claude_code.rs) |
| Handing the URL to Codex | [`crates/biorouter/src/providers/codex.rs`](../../../crates/biorouter/src/providers/codex.rs) |

On the Codex side, `dynamicTools` would remove the HTTP hop entirely and is the eventual
replacement — but the installed Codex declares the `DynamicToolSpec` types without any request that
accepts them, so it is not reachable yet.

## Related documentation

- [What the child agent may not do](child-agent-isolation.md) — the other half of the argument: the
  child's own tools are off, which is why the bridge exists.
- [How the coding-agent providers work](how-it-works.md) — the mechanism the bridge sits inside.
- [Permission modes](../../security/permission-modes.md) — the modes whose decisions the bridge
  honours.
- [Privacy tiers](../../security/privacy-tiers.md) — Gate C, Gate E and the `CallCapability` the
  grant samples once.
- [Extensions](../../extensions/README.md) — the tools that become available to a child for free.
- [Performance, limits and known gaps](performance-and-limits.md) — what a large tool surface costs
  in prompt tokens, and what the streaming path does and does not cover.
- [Streaming and tool-call parity](streaming-and-tool-call-parity.md) — the design record behind the
  mirror, including the two alternatives that were rejected.
