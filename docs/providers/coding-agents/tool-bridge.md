# The tool bridge

> **What this is.** How BioRouter's own extensions — SPOKE, UCSF OMOP, knowledge, Auto Visualiser,
> any marketplace plugin — reach a child coding agent while BioRouter still executes them behind
> its inspectors, permission mode, `.biorouterignore`, vault and privacy gates. Why MCP is the only
> channel that can do this, why the capability travels in the URL rather than a header, and why a
> call needing human approval is refused rather than parked.
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
filtered tool set, and the turn's privacy capability.

| Gate | Where it applies to a bridged call |
| --- | --- |
| Tool discovery / privacy Gate E | Inherited. The advertised set is the one `filter_tools` produced for the model — already tier-filtered and reach-filtered — and `tools/list` serves it verbatim, so there is no second policy here to drift from the first. |
| Tool inspectors (command policy, sensitive ops, everything in the inspector stack) | Run on every call, against the conversation snapshot and the session's permission mode. |
| Permission mode | The inspectors' permission decision is honoured: denied is refused, and "no decision was reached" is refused too — an absent decision must never read as approval. |
| Privacy Gate C | `dispatch_tool_call` is the one choke point every tool call passes through, and a bridged call goes through it with the turn's `CallCapability`. |
| `.biorouterignore`, vault, session working directory | Whatever BioRouter's dispatcher and inspectors already enforce, because BioRouter is the process executing the tool. |

⚠ **The inspector pass is why this is not a thin proxy onto `ExtensionManager`.**
`POST /agent/call_tool` *is* that thin proxy, and its own comment records the cost: it bypasses the
agent loop and therefore every `ToolInspector`. A child agent's tool calls are model-initiated and
must be inspected exactly like the parent model's.

The privacy capability is **sampled once**, when the grant is issued, and threaded from there. A
gate on this path asks the sampled capability rather than re-reading the master switch — a second
read is precisely the race `CallCapability` exists to close.

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
// Claude Agent: written to a 0600 temp file, passed as --mcp-config
{ "mcpServers": { "biorouter": { "type": "http", "url": "<bridge url>" } } }
```

```json
// Codex: thread/start's `config` override map, i.e. mcp_servers.<name>.url
{ "mcp_servers": { "biorouter": { "url": "<bridge url>" } } }
```

Claude Agent's configuration is a **file**, not an inline JSON string, even though the CLI accepts
both: the URL carries the turn's capability and `argv` is readable by any process running as the
same user. `NamedTempFile` creates it 0600, and the handle is deliberately bound for the whole run —
dropping it deletes the file, and a child that started a moment later would find no configuration.

Claude Agent's invocation also gains `--permission-mode bypassPermissions` when a bridge is
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
returns. A `stream()` implementation may therefore read the URL (it runs inside the scope, and is
where the child would be spawned); a poll of the returned stream may not, because by then the scope
is gone. The lease itself is not the constraint — `Agent::reply` binds it before the scope and it
lives to the end of that loop iteration, which outlasts stream consumption.

## Where the code is

| Concern | File |
| --- | --- |
| Grants, leases, the nonce, the task-local | [`crates/biorouter/src/providers/coding_agent/bridge.rs`](../../../crates/biorouter/src/providers/coding_agent/bridge.rs) |
| The HTTP/JSON-RPC endpoint | [`crates/biorouter-server/src/routes/tool_bridge.rs`](../../../crates/biorouter-server/src/routes/tool_bridge.rs) |
| Handing the URL to Claude Agent | [`crates/biorouter/src/providers/claude_code.rs`](../../../crates/biorouter/src/providers/claude_code.rs) |
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
  in prompt tokens, and why there is no streaming.
