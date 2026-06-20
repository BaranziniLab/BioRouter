# Reliable Slack posting — options & setup burden (investigation, not implemented)

UI automation of Slack is inherently brittle (Slack exposes almost no AppleScript API,
and the Electron UI shifts). The robust path is Slack's HTTP API. There are three ways to
wire that into BioRouter, ranked here by **how much the user has to set up** (lowest
first), since a complicated setup is not wanted. **Nothing below is implemented yet** —
this is for you to choose.

What BioRouter already provides (relevant facts):
- The Computer Controller's `automation_script` tool runs shell, so it can already
  `curl -X POST` to a URL. (Its `web_scrape` tool is GET-only, so it can't post.)
- External MCP servers can be added via **Settings → Extensions → Add custom extension**
  (type: Standard IO / Streamable HTTP), with secrets stored in the OS Keychain
  (`env_keys`). The repo already anticipates a `slack-mcp` stdio extension keyed on
  `SLACK_TOKEN` (`secret_discovery.rs`).

---

## Option A — Incoming Webhook (simplest; works with zero new code)

**What it is:** a Slack "Incoming Webhook" is one URL tied to one channel. POST
`{"text":"..."}` to it and the message appears in that channel. Send-only, one channel.

**BioRouter support today:** already works — the agent can post via `automation_script`:
`curl -s -X POST -H 'Content-type: application/json' -d '{"text":"<msg>"}' <WEBHOOK_URL>`.
No code change required. (Optional nicety later: store the URL so the agent doesn't need
it pasted each time — see "Optional polish" below.)

**One-time user setup (~2–3 min, no scopes, no bot):**
1. https://api.slack.com/apps → **Create New App** → **From scratch** → pick the
   Broccolito Lab workspace.
2. **Incoming Webhooks** → toggle **On**.
3. **Add New Webhook to Workspace** → choose the channel (e.g. `#spoke-tech`) → **Allow**.
4. Copy the URL (`https://hooks.slack.com/services/T…/B…/…`).
5. Hand it to BioRouter (paste in the request, or store it once — see below).

**Good for:** "announce the report in the lab channel." **Limits:** one channel per URL,
send-only (no reading/searching/threads), can't pick a channel dynamically (you'd add one
webhook per channel).

---

## Option B — Slack MCP server with a bot token (most capable; more setup)

**What it is:** add an external Slack MCP server (a small program that wraps Slack's Web
API). The agent then gets real tools: list channels, post to **any** channel, read
history, search, reply in threads, etc.

**BioRouter support:** add it in **Settings → Extensions → Add custom extension**
(type **Standard IO**), set the command to a Slack MCP server, and store the token as an
env secret. No BioRouter code change — it's a configuration step. Requires `npx` (Node) or
`uvx` (uv) on the machine to launch the server.

**One-time user setup (~10–15 min):**
1. Create a Slack app (From scratch) in the workspace (as above).
2. **OAuth & Permissions → Bot Token Scopes**: add `chat:write` (post), plus
   `channels:read` + `channels:history` (and `groups:read`/`groups:history` for private
   channels, `search:read` to search).
3. **Install to Workspace** → copy the **Bot User OAuth Token** (`xoxb-…`).
4. **Invite the bot** to each channel it should use (`/invite @YourBot`).
5. In BioRouter: add the Slack MCP extension (command + paste the `xoxb-…` token as a
   secret env var; some servers also want the workspace/team ID).

**Good for:** flexible, robust posting/reading across channels with no UI flakiness.
**Cost:** scopes + install + inviting the bot + needing npx/uvx, and trusting a
third-party MCP server.

---

## Option C — User token / Slack CLI

More involved (user-token OAuth or the Slack CLI). Not recommended given the "simple
setup" preference; listed only for completeness.

---

## Recommendation

- For the stated need ("post an announcement / message to the Broccolito Lab channel"):
  **Option A (Incoming Webhook)** — ~2–3 minutes, no scopes, no bot to invite, and it
  works with the tooling that already ships (no implementation, nothing to maintain).
- If you later need to post to **multiple channels** or **read/search** Slack from the
  agent: **Option B**.

## Optional polish (only if you later want it — small, would need implementing)

To make Option A first-class instead of the agent shelling out `curl`, we could add a tiny
`slack_post` tool (or a documented config key like `SLACK_WEBHOOK_URL`) so the agent posts
with a clean tool call and the URL is stored once in the Keychain. This is a small,
self-contained change — but per your instruction it is **not implemented**; flag it if you
want it.
