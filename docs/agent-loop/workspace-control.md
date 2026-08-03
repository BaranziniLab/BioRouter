# Workspace control

> **What this is.** A task-oriented guide to running more than one BioRouter conversation at once: laying them out across tabs, panes and windows, delegating work to subagents you can watch, checking on what is running, and doing all of it from the terminal as well as the desktop app.
> **Status:** Current.
> **Audience:** end users — researchers organising a piece of work across several conversations.

A BioRouter conversation is a session: its own agent, its own working directory, its own extensions, skills and knowledge bases, its own history, and — in the desktop app — its own tab. Workspace control is the set of tools that lets the agent operate on that layer instead of only inside one chat. The practical effect is that "run the alignment while I write the methods" stops meaning *you* open a second chat, paste the context in, and remember to come back to it.

This page is the how-to. The per-tool reference — what each tool takes, the confirmation rules, how injected messages are labelled — is the [Workspace Control extension page](../extensions/built-in/workspace.md); the child-agent detail is [Subagents](subagents.md). Read this one when you have the app (or a terminal) open and want to lay work out.

## What it is for

Three jobs account for nearly all of it.

| Job | What you say | What you get |
|---|---|---|
| **Run several conversations side by side** | "Open a second chat for the QC pass, in a split, and start it on the flowcell report." | A new session in its own pane, already working, sharing your working directory unless you asked otherwise |
| **Delegate and keep working** | "Delegate the test suite to a subagent and tell me when it's done." | A child conversation in its own tab you can read, steer and stop, and a parent that waits on it properly instead of polling |
| **Fix another conversation's setup from here** | "Give the transcriptomics chat the single-cell skill." | That chat reconfigured, with a confirmation card first and a toast in its tab afterwards |

Everything below is the mechanics behind those three.

## Turning it on

Workspace control ships in two sizes. The small one is automatic: any session allowed to delegate gets the spawn tool (`subagent`) and nothing else. The full surface — reading other conversations, injecting prompts into them, changing their tool sets — is an explicit opt-in, because those tools reach into conversations the agent did not create.

In the desktop app, open **Extensions** in the left sidebar and turn on **Workspace Control**. From the terminal, run `biorouter configure`, choose `Toggle Extensions`, and enable `workspace`. The extension is registered `default_enabled: false`; nothing enables it for you.

Jobs 1 and 3 need that full surface. Job 2 does not — but "allowed to delegate" is a real gate, and it is the likeliest reason a request for a subagent quietly does nothing.

### When delegation is allowed

`Agent::subagents_enabled` (`crates/biorouter/src/agents/agent.rs:3711`) decides whether the spawn tool is offered at all, and it is consulted twice: once when the tool list is built, and again when a call arrives — so a model that simply remembers the name cannot spawn where delegation is off. It refuses with *Subagent delegation is not available in this session*.

All of the following must hold:

- **The permission mode is Completely Autonomous** (`auto`). In Manual Approval, Smart Approval and Chat Only — three of the [four modes](../security/permission-modes.md) — there is no spawn tool at all. Autonomous is the default, so most people never meet this; anyone who has turned the mode down will, and the symptom is an ordinary answer where a child conversation was expected.
- **The session is not itself a subagent.** A child cannot spawn grandchildren, which is the same rule that stops a child being granted workspace control.
- **At least one ordinary extension is loaded.** The auto-injected `workspace` entry is deliberately not counted — otherwise one turn's grant would justify the next one's, and an agent that dropped its last real extension would keep delegating forever off a grant it derived from itself.
- **The active model name does not begin with `gemini`.** This is a flat exclusion in the gate, with no rationale recorded in the source, so treat it as observed behaviour rather than a rule with a reason: on a Gemini model, delegation is off whatever your mode says.
- **The session is not a BioRouter app that delegates through `consult`.** Apps with worker profiles route delegation through their own mechanism and have the generic tool withdrawn so the two cannot both be offered.

## Running several conversations side by side

### The layout you are arranging

The desktop window holds **tab groups**. A group is a pane with its own tab strip; splitting gives you two panes side by side, each with its own tabs. A window can hold up to **six panes** (`MAX_GROUPS` in `ui/desktop/src/components/chatGroups/chatGroupsLayout.ts`), and you can open more windows.

What you can do by hand:

| Action | How |
|---|---|
| New chat tab | `Cmd`/`Ctrl`+`T` |
| New window | `Cmd`+`N` (macOS) / `Ctrl`+`N` |
| Split a pane | Drag a tab onto the **outer quarter** of a pane — left, right, top or bottom edge. Dropping in the middle half moves the tab into that pane instead of splitting |
| Close the tab | `Cmd`/`Ctrl`+`W` |
| Close the window | `Shift`+`Cmd`/`Ctrl`+`W` |

The six-pane ceiling is the edge of what was measured, not a hard limit of the renderer — panes are cheap, but a pane holding a long transcript is not, so it was left where the evidence stopped.

### Asking for a layout in words

The agent places conversations with the same vocabulary you use by hand:

- **`tab`** (the default) — a new tab in the current pane.
- **`split`** — a new pane beside the current one.
- **`window`** — a separate window.

Nothing else is accepted. A misspelling such as `"windows"` is refused outright rather than quietly treated as a tab, so an odd placement is an error message and never a silent surprise.

Two defaults are worth knowing because they are the opposite of what people expect:

- **Tabs open in the background.** Focus stays where you are typing. The agent has to ask for focus explicitly, and it rarely should.
- **A new conversation inherits your working directory.** If the agent puts one somewhere else, you are told — in a toast on the new tab, and in what the agent itself is told, so it can repeat it to you.

If the split is refused because the window is already at six panes, the agent is told the conversation was **NOT opened** and why. It should say so rather than claim a pane exists.

> **Note.** Opening a conversation never moves an existing one's working directory. Only a newly created session gets a directory, and it gets it at creation.

### Turning tabs off entirely

If you would rather nothing ever appeared on its own, turn on **Settings → App → Workspace → "Never open tabs automatically"**. Conversations and subagents still run; you get a notification naming them and open them from History when you want. The agent is explicitly told no tab opened, so it cannot report one. The setting is stored as `WORKSPACE_ANNOUNCE_ONLY` and is off by default.

## Delegating work you can watch

This section assumes delegation is available in the session — if asking for a subagent produces an ordinary answer instead, check [When delegation is allowed](#when-delegation-is-allowed) first.

Ask for a subagent in plain language ("delegate the QC pass to a subagent") and, whenever the desktop app is open, the child opens in its own background tab carrying a **`sub`** badge and a link back to the conversation that spawned it. Inside that tab you can read the child's transcript as it streams, type into it to steer it mid-run, and stop it from the header.

Three things about that tab that people get wrong:

- **Closing it does not kill the child.** Closing is a view operation everywhere in BioRouter. Stop is the kill switch, and a child whose tab you closed is still in History.
- **If you typed into it, the parent is told.** The parent's tool result carries a note that you intervened, so it weighs the child's self-report accordingly. Nothing is said when you did not.
- **`sub` badges are keyed to the session, not the tab.** A child that never got a tab is still badged when you open it later from History.

### Fanning out

At most **four** children *running at the same time* get a tab from one parent (`BIOROUTER_WORKSPACE_MAX_VISIBLE_CHILD_TABS`). Ask for ten in parallel and you get four tabs; the rest run in the background and appear in History nested under their parent. A spawn is never refused for hitting the cap — it is downgraded to a background run and the parent is told which children that happened to.

The cap counts *live* children, so a slot frees when a child finishes. Spawn four, wait, spawn four more, and you get tabs both times. It bounds the burst, not the running total.

### Long jobs

When background handles are enabled (`BIOROUTER_SUBAGENT_BACKGROUND`), a subagent can be started with `background: true`: the parent gets the child's session id straight back and keeps working. The right way to collect the result later is to **wait**, not poll — the agent parks on `workspace_watch` until one (or all) of up to 32 named conversations finishes, up to a 600-second bound. A timeout there is not an error; the conversations keep running and it can wait again.

## Checking on what is running

Ask "what am I running right now?" and the agent lists the workspace: id, name, whether a turn is in flight, which parent spawned it, the extensions and knowledge bases it has, and — when the app is attached — which window, pane and tab it sits in. The default scope is the *open* set (anything with a live agent, a turn in flight, or a tab); `all` and `running` are the other two. Results are paged, 50 rows at a time, up to 200 per page.

To look inside one conversation, four views are available, and the narrowest honest one is the right choice:

| View | What it shows | Reach for it when |
|---|---|---|
| `summary` | working directory, message count, first three and last three messages | "where has that chat got to?" |
| `transcript` (default) | the user-visible messages, with tool payloads collapsed to one-line stubs | you want the prose |
| `tool_calls` | request/response pairs, correlated by id, responses clipped | "what did it actually *do* to my repo?" |
| `spawn_context` | the instructions a subagent was started with | auditing a delegation |

Reads are clipped at 20,000 characters by default (200,000 maximum) and the clip names the controls that would narrow it. `last: N` tails the conversation. There is also a `from_msg_uid` slice that starts from a specific durable message id — but note that none of the four views print message ids, so in practice narrowing happens with `last`.

Hidden sessions are refused in every view. And because a read is an ordinary tool call, it is recorded in the *reading* conversation: there is always a trail of who read what.

## Fixing another conversation's setup

`workspace_set_tools` is the one tool that changes what a different conversation may use — add or remove extensions, scope skills to that conversation alone, switch its provider and model, or set its knowledge bases. A model switch takes effect on the target's **next** turn; a turn already running finishes on the provider it started with. Skills added this way never touch your machine-wide skill preferences.

Some of these changes raise a confirmation card **in every permission mode, including the fully automatic one** — handing a conversation a process-spawning extension, removing a security-relevant one, switching its provider, or adding a skill. The full list and the reasoning are in the [extension page's always-confirm rule](../extensions/built-in/workspace.md#the-always-confirm-rule). Whatever changes, the target's tab gets a toast saying what happened and who did it. Silent cross-session action is not a supported configuration.

Two refusals you may see and should not try to work around:

- An extension an operator wrote `enabled: false` for cannot be enabled here. The refusal names the operator's decision and tells the agent to ask you rather than route around it.
- A subagent session can never be granted workspace control, so a child cannot spawn grandchildren or steer its parent.

## The limits you will actually meet

Every one of these is a real, named bound rather than a guideline.

| Limit | Value | Change it with |
|---|---|---|
| Panes in one window | 6 | not configurable |
| Subagent tabs per parent, at once | 4 | `BIOROUTER_WORKSPACE_MAX_VISIBLE_CHILD_TABS` |
| Turns one conversation may inject into others at once | 4 | `BIOROUTER_WORKSPACE_MAX_INJECTED_TURNS` |
| Conversations one wait may watch | 32 | not configurable |
| Wait / injected-reply timeout | 120 s default, 600 s maximum | per call |
| Rows per listing page | 50 default, 200 maximum | per call |
| Characters per conversation read | 20,000 default, 200,000 maximum | per call |

The injected-turn cap is the one that surfaces as a puzzling message. If the agent reports that *this session already has 4 injected turns in flight*, it means this conversation is already driving four others and is being made to wait rather than saturate the daemon. A slot is released when the turn it started actually ends.

## Doing all of this from the terminal

The CLI is a first-class front end here, and it reaches the feature in **two different ways**. Conflating them is the main source of confusion.

### As tools, inside `biorouter session`

The CLI links the same core library the daemon does, so an interactive terminal chat advertises the same tool surface. `subagent` needs no extension enabled in `biorouter session` — only the same delegation gate as everywhere else, so `/mode auto` in the terminal chat is a prerequisite — and enabling the `workspace` extension adds the seven `workspace_*` tools there too. You ask for delegation in the terminal exactly as you would in the app.

What changes without a daemon is not the tool list but what the handlers can reach. These keep working headlessly: listing conversations, reading them, waiting on them (the wait consults the parent's own background-subagent registry, so it still knows a child is running), leaving a note in another conversation, and spawning subagents. These refuse **by name**, with "requires the BioRouter daemon" rather than an obscure failure: starting a new session, starting or steering a turn in another conversation, setting knowledge bases, and cancelling a turn or stopping an agent. With no app attached, closing a tab is a stated no-op and a newly created session reports plainly that no tab was opened.

### As subcommands you type

These are ordinary shell commands, and they split in two. The first three touch only the session store on disk and work with nothing else running. The last four — `send`, `watch`, `attach`, `cancel` — drive a live turn through a running `biorouterd` over HTTP and SSE, and fail without one.

| What you want | Command | Needs a daemon |
|---|---|---|
| See what exists, including subagent runs nested under their parents | `biorouter session list --subagents` | no |
| Read a conversation | `biorouter session export --id <id>` (or `--name <name>`; `--format markdown\|json\|yaml`) | no |
| Start or name a session | `biorouter session --name <name>` | no |
| Send a prompt into a session and stream its turn | `biorouter session send <id> "<text>"` (`--no-wait` to return as soon as the turn starts) | yes |
| Wait for a turn to end | `biorouter session watch <id>` (exits on finish or error; `--follow` keeps watching past it) | yes |
| Join a live session, follow it, and steer it | `biorouter session attach <id>` — `--name` to pick it by name, `--of <parent>` to attach to that parent's running subagent, `--read-only` to observe without participating | yes |
| Stop the turn a session is running | `biorouter session cancel <id>` | yes |

Give `session export` an identifier. With neither `--id` nor `--name` it drops into an interactive session picker instead of exporting the conversation you meant — which is a surprise in a script.

`session list --subagents` reads the store for the rows but has to ask the daemon who is still live, so it marks each run `● live`, `○ done`, or — when it could not ask — `· state unknown`. It deliberately does *not* blame a missing daemon for that third state: a stripped `BIOROUTER_SERVER__SECRET_KEY` (which is what an agent-spawned shell gets) produces it with a daemon running perfectly well. The actual reason is printed once on stderr, so `--format json` on stdout stays clean.

The four daemon-bound commands need one running:

```bash
BIOROUTER_SERVER__SECRET_KEY=<key> biorouterd agent
```

It listens on `127.0.0.1:3000` unless `BIOROUTER_PORT` says otherwise, and `send`, `watch`, `attach` and `cancel` authenticate with the same `BIOROUTER_SERVER__SECRET_KEY`. `biorouterd` invents a random key when that variable is unset, in which case no client can authenticate — so set it on both sides. A mismatch shows up as HTTP 401.

Use `session attach` rather than `session --resume` on a session that is running right now: resuming opens a second agent on the same conversation, and the two do not share the daemon's turn lock.

### What has no terminal equivalent

Exactly two capabilities, and both are declared rather than accidental. The mapping between the tools and the subcommands is not folklore — it is a compiled table in `crates/biorouter-cli/src/commands/workspace_parity.rs` whose rows are checked against the real command tree, against the advertised tool surface, and against the daemon's published routes. A new capability with no CLI row fails the build, and the number of declared exceptions is capped at two by a test.

- **Spawning.** It is a tool the model calls, and it already works inside `biorouter session`. There is nothing for a subcommand to add.
- **Reconfiguring another session's tools.** Out of scope for the terminal; `biorouter extension` and `biorouter skill` are machine-wide rather than session-scoped.

That gate is coarse by design, and it is worth knowing where it is coarse: `session cancel` covers only the *turn* scope of a three-scope tool, and `session export` dumps a whole transcript rather than offering the four views. The rows prove a subcommand **exists**, not that it is equivalent.

## When a tab does not appear

Most of the time the answer is one of the deliberate behaviours above — announce-only is on, the fan-out cap downgraded the child, or the window was already at six panes. Three honest caveats beyond those:

- **A subagent spawn does not wait for the window to answer.** When a conversation is opened directly, the agent parks on the renderer's reply and reports "NOT opened" with the reason if it was refused. A *subagent* announcement is fire-and-forget, deliberately: waiting would couple every spawn to the window, and one wedged window would stall a whole fan-out. So in the narrow case where the renderer refuses a subagent's tab, the agent may believe a tab opened when none did. The child still exists, still runs, and is still reachable from History and from a conversation read — check there before assuming the run failed.
- **A request for a separate window is reported as *requested*.** The window is asked for and the answer comes back before the window has actually been created, so a window that fails to appear is not distinguished from one that did.
- **A split can silently arrive as a plain tab.** The six-pane refusal above is announced; a second one is not. The renderer will not split a pane off a tab that is that pane's *only* tab — that would trade one pane for another and leave an empty one behind. So a `split` whose conversation ends up alone in its pane does not split: most often that is a conversation which already has a tab of its own in a pane by itself, which is simply focused instead. The planner asks the reducer first, so nothing pointless is dispatched, but the agent is told `opened` rather than `opened in split` and may not pass that difference on. If you asked for a pane and got a tab, drag the tab to a pane edge yourself.

## Related documentation

- [Workspace Control extension](../extensions/built-in/workspace.md) — the user-facing reference: the two tiers, what each tool asks you first, the always-confirm rule, and how injected messages are labelled forever.
- [Workspace Control tool reference](workspace-control-tools.md) — the developer-facing contract for the same eight tools: exact arguments and defaults, every refusal string, the caps and clamps, and the cases where a tool reports success it did not earn.
- [Subagents](subagents.md) — the glass-box tab in full, steering and stopping a child, and configuring one from a workflow file.
- [Tool routing](tool-routing.md) — which tool the agent should reach for, and what separates workspace control from Chat Recall, Memory and the knowledge base.
- [Permission modes](../security/permission-modes.md) — the four modes, what each one still puts to you for approval, and how to switch between them. It does not discuss delegation; that gate is [above](#when-delegation-is-allowed).
- [biorouter CLI command reference](../cli/command-reference.md) — the rest of the `biorouter session` surface.
- [Agent workspace control (BR-71 design)](designs/agent-workspace-control.md) — the design of record behind this feature, including its permissions and abuse-resistance analysis.
