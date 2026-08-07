# Privacy and workspace test checklist

> **What this is.** The manual test pass for the privacy tiers, the institutional-affiliation axis,
> and workspace control — written to be run against a **release build**, not a dev build.
> **Status:** Current. Extend it when a behaviour changes; do not delete rows that still describe
> shipped behaviour.
> **Audience:** whoever is verifying a release that touches any of these features.

Every row states what to do, what should happen, **and what a failure looks like** — because several
of these fail by doing nothing visible, which reads as a pass.

⚠ **Run this against an installed, signed build.** A dev build differs in ways that matter here: it
uses a different config root, the desktop daemon's port and secret are minted per launch, and
`BIOROUTER_NO_HMR` changes reload behaviour. A pass on the dev build is not a pass.

## 0. Before you start

- [ ] Note the build under test — version, platform, arch, and whether it is notarized.
- [ ] Have **two** providers configured: one **private** (Versa API Azure, Versa API Bedrock, Llama
      Server, or Ollama on loopback) and one **public** (Anthropic, OpenAI, …). Several rows need
      both, and there is no way to fake a tier from the UI.
- [ ] Confirm privacy tiers are **on** (Settings → Privacy). Most of this checklist is vacuous with
      the master switch off — which is itself row 6.4.

---

## 1. Subagent spawn — desktop

| # | Do this | Expect | Failure looks like |
|---|---|---|---|
| 1.1 | Ask for one subagent | A new tab opens, carrying a **`sub` badge** | No tab; or a tab with no badge |
| 1.2 | Ask for **ten** in parallel | **Four** tabs; the rest run in the background and appear in History nested under their parent. The parent is told which children were downgraded | Ten tabs; or four tabs and silence about the other six |
| 1.3 | Let two of the four finish, then ask for four more | Tabs appear again — the cap counts **live** children, not total | No new tabs after the first four ever |
| 1.4 | Close a child's tab while it is running | The child **keeps running**; it is still in History | The child dies |
| 1.5 | Steer a child (type into its tab) | The parent is **told** the child was steered | The parent finishes on a stale picture |
| 1.6 | Settings → Workspace → enable announce-only, then spawn | Announced, **no tab opened** | A tab opens anyway |
| 1.7 | Spawn beyond 8 concurrent | Numbers nine and ten **queue**. With background handles **off** (default) a queued child has no session and no tab; with them **on**, the session exists and the tab is announced before it starts | A queued child that is invisible when it should be visible, or vice versa |
| 1.8 | Spawn past 64 in flight | **Refused**, with `Subagent limit reached: N already in flight (max 64)` | Silently queued forever |

## 2. Subagent spawn — CLI

⚠ **The CLI is deliberately different, and "nothing happened" is often correct.** Children are
invisible headless by default; there is no tab to announce.

| # | Do this | Expect | Failure looks like |
|---|---|---|---|
| 2.1 | `biorouter session`, ask for a subagent | The `subagent` tool call renders, **blocks**, and returns the child's answer as the tool result. No live view of the child's turns | A hang with no tool call rendered; or a claim of backgrounding |
| 2.2 | `biorouter session list` | Subagent rows **absent** (historical behaviour) | Children listed by default |
| 2.3 | `biorouter session list --subagents` | Children **appear** | Same list as 2.2 — the flag would be filtering the rendering, not widening the query |
| 2.4 | ⚠ Spawn a child that produces **no messages**, then `session list --subagents` | The child **appears** | It is missing — the listing query inner-joins messages. **This was a known defect; verify it is fixed** |
| 2.5 | `biorouter session export --session-id <child-id>` | The child's transcript exports | Not found |
| 2.6 | Ask for `background: true` without setting the env var | The argument is **not advertised** and is ignored; the call blocks | A claim that it detached |

## 2b. Subagent runs in History

⚠ Several of these need **two panes** to reproduce; a single-window pass will miss them.

| # | Do this | Expect | Failure looks like |
|---|---|---|---|
| 2b.1 | Open History with **Show subagent runs** off | Children **absent** | Children listed, cluttering the parent list |
| 2b.2 | Turn it on | Children appear **nested under their parent**, and the toggle **refetches** with `include_subagents` | A flat list; or the same rows re-rendered without a refetch |
| 2b.3 | Find a child that ran **past midnight** relative to its parent | Still nested under its parent, not orphaned into its own date bucket | It appears alone in a later day group |
| 2b.4 | Delete a parent, or filter so it is absent, then show its child | The child is **badged as a subagent run** | It renders as an ordinary chat — the misleading case |
| 2b.5 | Two panes: toggle **on** in one, **off** in the other | The off pane shows **no** children | Children leak in from the shared cache |
| 2b.6 | Same two panes, let one push children into the shared cache | The off pane still does not adopt them | It adopts them on the next render |
| 2b.7 | Cross-check the CLI: `biorouter session list` then `--subagents` | Matches the GUI's toggle-off / toggle-on behaviour | The two surfaces disagree about what a default listing contains |

## 3. Subagent inheritance — the rules

| # | Do this | Expect |
|---|---|---|
| 3.1 | From a **private** chat, spawn with no `settings` | Child runs the **same private model** |
| 3.2 | From a **public** chat, spawn naming a **private** provider | **Refused** — no approval path |
| 3.3 | From a **private** chat, spawn naming a **public** provider | **Refused** — no approval path |
| 3.4 | From a **UCSF** chat, spawn naming **Llama Server** (`Local`) | **Refused** — `Local` is the top of the affiliation lattice, so this is an elevation |
| 3.5 | From a **UCSF** chat, spawn moving between `versa_azure` and `versa_bedrock` | **Permitted** — same affiliation |
| 3.6 | From inside a subagent, ask for another subagent | **Refused** — no grandchildren |
| 3.7 | Set permission mode to anything other than Completely Autonomous, then spawn | **No spawn tool offered at all** |

⚠ For 3.2–3.4 the refusal must **name the remedy** (start a chat on that model yourself), not just say
no. A refusal that leaves the user stuck is the failure mode that gets features turned off.

## 4. Privacy tiers — the barrier

| # | Do this | Expect | Failure looks like |
|---|---|---|---|
| 4.1 | In a **public** chat, try to use the OMOP or CDW connector | The tools are **not in the list at all** | They appear, then error |
| 4.2 | In a public chat, ask to enable a private extension | **Refused**, and the refusal says the route is to switch the chat's model | An approval prompt — this is deliberately **not** user-authorizable |
| 4.3 | Bind a private model to a chat, then try to bind a public one | Refused — classification is a one-way ratchet | It succeeds |
| 4.4 | In a public chat, ask it to read another (private) conversation | **Refused**, and the refusal cannot distinguish "private" from "does not exist" | The transcript appears; or the wording reveals the chat exists |
| 4.5 | Same, but via **every** door: `workspace_read_conversation`, `workspace_list`, `workspace_send_prompt`, `workspace_open`, `workspace_set_tools`, `workspace_watch`, `workspace_close`, `manage_schedule` | **All refused** | Any one succeeding. ⚠ This row exists because the hole was found at one door and turned out to be four |
| 4.6 | `GET /sessions/{id}/events` and `GET /sessions/{id}/export` against a private id, from a public context | Refused | Either streams or exports |
| 4.7 | Ask a public chat to list extensions | Private connectors' **names and descriptions** absent | They are listed and described |

## 5. Affiliation

| # | Do this | Expect |
|---|---|---|
| 5.1 | With a UCSF model bound, use a UCSF-tagged connector | Works, no warning |
| 5.2 | With a UCSF model bound, use an **untagged** private connector | Works — untagged means unconstrained |
| 5.3 | With a **local** model bound, use any private connector | Works — local reaches everything, because nothing leaves the machine |
| 5.4 | Construct a cross-institution mismatch | **Warned**, naming **both** institutions, with an **accept control you can actually press** |
| 5.5 | Accept it, then make the same call again | No second prompt — the grant is per (session, extension, model affiliation) |
| 5.6 | Rebind to a different institution's model, then repeat | **Prompted again** — the triple changed |
| 5.7 | Set the mixing mode to `open` / `standard` / `strict` and repeat 5.4 | Silent / in-app confirm / **system password** |

⚠ **5.4 is the row most likely to fail by doing nothing.** A mismatch that logs but shows no warning
looks identical to a mismatch that was permitted. If you see no warning, check whether the call
succeeded — that distinguishes the two.

⚠ In a stock install **no mismatch is constructible** — every tagged connector and every affiliated
provider is `ucsf` or `Local`. Rows 5.4–5.7 need a second institution's connector, or a test build.

## 6. Declassification, export, and the switch

| # | Do this | Expect |
|---|---|---|
| 6.1 | Declassify a chat that only ran a private model (`turn:*`) | Single confirmation |
| 6.2 | Declassify a chat that touched a private connector (`mcp:*`), or one branched from one | **Typed phrase** *and* the **operating-system password prompt** |
| 6.3 | Export a **private** chat | Gated, audited, and the copy says the exported file is **not protected**. The chat stays **private** afterwards |
| 6.4 | Turn the master switch off | Enforcement stops; **the disclosure still shows**. Turn it back on — no chat has been silently reclassified |
| 6.5 | Try to turn it off by editing `config.yaml` | No effect, including after a restart |

## 7. Regressions to confirm fixed

Each was a real defect found in this campaign. A silent pass here is the point.

- [ ] `open_tab` no longer reports success when it did not open a tab
- [ ] `close_tab` no longer returns success with `wait_result: false`
- [ ] The knowledge **backfill runs and its first-run notice renders**
- [ ] The **CLI can reach the desktop app's daemon** (or says clearly why not)
- [ ] `biorouter session watch` is spelled with a command that exists
- [ ] A **backfilled** chat's declassification message does not claim it "reached a private data source"
- [ ] Project-local memory written in a private chat is **not** inlined into a later public chat
- [ ] A legacy import, or a rebuilt `sessions.db`, does not bring chats back **public**

## 8. Visual verification — look at it, do not only assert on it

⚠ **Most of section 4–6 can pass functionally and still be unusable.** A refusal that fires correctly
but renders as a bare error, a badge that is present in the DOM but invisible against its background,
an accept control that exists but sits below the fold — each is a pass in a unit test and a failure in
front of a user. This section is looked at, not asserted.

### Driving a release build

A release app has no debugging port by default. Launch it with one:

```bash
open -a BioRouter --args --remote-debugging-port=9223
```

⚠ **Screenshot over CDP, never `screencapture` of the whole screen.** The app sits behind the editor,
raising it is unreliable, and a full-screen grab captures whatever else is on the display. Use the
agent-browser connection against port 9223.

⚠ **This is a release build, so the dev-GUI launch problems do not apply** — no
`ELECTRON_RUN_AS_NODE`, no vite config, no HMR. If it will not start, that is a real packaging
failure, not a launcher quirk. Check the bundled binaries' architecture with `file` before
diagnosing anything else.

### What to look at

| # | Surface | Confirm visually |
|---|---|---|
| 8.1 | A child tab | The **`sub` badge** is legible and distinguishable from the privacy badge beside it |
| 8.2 | Chat list, tab strip, composer, model picker, Settings → Providers, Settings → Extensions | The **privacy badge** appears in each, and reads the same in all of them |
| 8.3 | The pre-first-turn **disclosure** | Present, readable, and not dismissible in a way that hides it permanently |
| 8.4 | A **cross-affiliation mismatch** | The warning names **both institutions**, and the **accept control is visible without scrolling** inside the failed tool call |
| 8.5 | A **refused** tool call (public chat, private extension) | The refusal reads as an explanation with a remedy, not as a stack trace or a bare "error" |
| 8.6 | History with subagent runs shown | The nesting is visually obvious — a reader can tell parent from child at a glance |
| 8.7 | The **declassification** dialog at both grades | The typed-phrase field and the OS password prompt both appear, in the right order |
| 8.8 | Settings → Extensions | A **daemon-enforced** private badge is visually distinct from a **catalogue-only** one |
| 8.9 | Settings → Privacy | The master switch, the mixing mode, and their current state are all legible |

### Across themes

⚠ **Repeat 8.1, 8.2, 8.4 and 8.8 in all three theme families — Parchment, Alma Mater, Roche Limit —
in both light and dark.** Badges and warnings carry colour, and colour is exactly what a theme
changes. A badge that is legible in Parchment light can vanish in Roche Limit dark.

At three viewport widths: **1440×900, 1120×800, 800×600**. The accept control at 8.4 is the one most
likely to fall below the fold on the narrow one.

## 9. What cannot be tested here — say so, do not imply coverage

- **Windows Hello** and **polkit** system-authentication prompts. macOS only on this hardware.
- The **Windows zip** and **Linux deb/rpm** beyond "it builds" — no machine to run them.
- Any **second institution's** connector (rows 5.4–5.7) without a test build.

⚠ Record these as untested rather than passing. A checklist that quietly omits what it could not run
is worse than one with gaps marked.

## Related documentation

- [Where the privacy campaign stands](../security/privacy-tiers-campaign-state.md) — what the barrier
  does and does not guarantee, and the accepted residuals.
- [Privacy tiers](../security/privacy-tiers.md) — the design.
- [Workspace control](../agent-loop/workspace-control.md) — the user guide.
- [Auto-update test checklist](auto-update-test-checklist.md) — the sibling pass for updates.
