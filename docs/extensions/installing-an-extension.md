# Installing an extension, and where its credentials go

> **What this is.** How a `.brxt` extension gets installed from each of the four surfaces that can do it, and the design of the credential path — why a secret never enters the conversation, and what that costs.
> **Status:** Current.
> **Audience:** contributors, and operators deciding how to configure extensions safely.

Four surfaces install extensions: **Browse Extensions** in the desktop app, the **Add Extension** file drop beside it, `biorouter extension install` at a terminal, and an agent asked to do it in chat. They are one transaction with four front doors — [`crates/biorouter/src/extension_install/`](../../crates/biorouter/src/extension_install/) — and before that existed they were three implementations that disagreed on the one thing that matters.

---

## What an install actually does

```
download → validate → extract → uv sync → credentials → register → attach
```

Only the fifth step can stop and wait for a person, and every step after a failure has to undo the ones before it. That is why these are one type ([`ExtensionInstallTransaction`](../../crates/biorouter/src/extension_install/transaction.rs)) rather than a sequence a caller composes: without the undo, a machine is left with a half-registered extension, an orphaned `~/.config/biorouter/extensions/<name>/` tree, and a credential in the keychain for something that is not installed.

The states an install can end in are all reportable, to a person and to a model:

| State | Means |
|---|---|
| `attached` | Registered, and live in the chat that asked for it. |
| `installed` | Registered, but not attached — nothing asked, or the hot-load failed and the extension is still correct for the next chat. |
| `needsCredentials` | Required values are missing and this run may not ask. **Nothing was registered.** The key *names* are reported so an operator can configure them and re-run. |
| `cancelled` | A person declined. Nothing was registered. |
| `failed` | Something broke. Everything this run created was undone. |

Rollback is scoped to the run's own work. A tree that already existed is not deleted — a failed upgrade that removes a working extension is worse than a failed upgrade — and a credential the machine already held is not revoked, because other extensions may share it.

> **A cancelled credential step keeps the expensive half.** The extracted tree and its built Python environment survive as a resume record ([`ResumableInstalls`](../../crates/biorouter/src/extension_install/transaction.rs)), holding the extension's name and which keys are still needed — and no values. Retrying does not re-download or rebuild.

---

## The credential path

**A credential goes from the trusted surface to the OS credential store, and nothing else ever sees it.** Not the model, not the provider, not a session row, not a log line, not a process argument, not a diagnostics bundle.

```
 install ──park(Secrets{keys, destination})──▶ card in the chat   (KEY NAMES ONLY)
                                                   │
        user types into Biorouter's own dialog ────┘
                                                   │
        POST /action-required/secrets  (X-User-Action)
                                                   ▼
                                          store the values
                                            ├─ declared secret → OS credential store
                                            ├─ ordinary setting → config.yaml
                                            └─ release the install with the NAMES
 install ◀──── configuredKeys: ["SPOKEAGENT_PASSCODE"] ─────┘
```

### Why the obvious implementation is wrong

BioRouter already had a mechanism for asking the user something mid-turn: MCP elicitation. Reusing it and giving the form `type="password"` inputs looks like a two-line change, and it fails on its first turn — invisibly.

`createElicitationResponseMessage` builds an `elicitationResponse` whose `user_data` is marked `agentVisible: true`, and `Agent::reply` forwards that whole object to the waiting request. Masking the characters would hide the secret from **the person typing it** and from nobody else: it would still be serialised into the transcript, persisted to the session row, replayed into the next prompt, and flattened into a child agent's transcript.

So the values do not travel on the conversation transport at all. [`SecretRequestCard`](../../ui/desktop/src/components/SecretRequestCard.tsx) takes no `append` and no `onSubmit` — it cannot put anything into the conversation even if it tried — and there is deliberately **no** `SecretResponse` sibling to `ElicitationResponse` for one to be serialised into.

### The guarantee is enforced, not documented

- `UserActionOutcome::SecretsConfigured` has no field a value can sit in.
- `PendingUserActions::resolve` **refuses** a value-bearing answer to a credential card and leaves the caller parked, so the property holds for surfaces that module has never heard of.
- The card carries `{key, label, description, required}` and no `secret` flag — it is the *ask*, and widening it would be one more place a value could be attached. The writer tells a credential from a setting using the manifest's declaration, registered beside the park.
- A key the card never asked for is **dropped**, not written. A form post does not get to choose what lands in the keychain.
- `SubmitSecretsRequest` hand-writes `Debug` to print key names, because a derive would put a passcode into any tracing line or test failure that formatted the body.

### Proof of user

`POST /action-required/secrets` requires DR-16's `X-User-Action` header. The model reaches the same daemon over the same HTTP with the same secret key; without the proof it could satisfy its own credential card with a value it invented and drive the install past the one step that exists to involve a person.

Two consequences worth knowing:

- **The refusal is written for a model to read.** It forecloses a retry and never suggests asking for the value in chat, because that is the exact failure this feature exists to end.
- **A daemon started without a user-action key cannot accept credentials over HTTP.** That includes `biorouter serve` (browser access), which spawns its daemon with `Stdio::null()` on purpose — see [SD-1](../deployment/serve-decisions.md). Configure those at a terminal with `biorouter extension install` or `biorouter extension configure`, which prompt with echo off.

---

## Per-surface behaviour

### Desktop — Browse Extensions

Add downloads the bundle inside the installer, so progress, failure, **Retry** and **Back to marketplace** are all one surface. Local-file controls never appear on this route (issue #116). If the manifest declares values, the configuration step opens; if it declares none, the button says **Install extension** and installs.

### Desktop — Add Extension (local `.brxt`)

Keeps the drag/drop and file picker, including after a bundle is loaded, so a mis-picked file can be replaced. A path preloaded over IPC (double-clicking a `.brxt` in Finder) is still this route — a preloaded path means "a file is chosen", never "this came from the marketplace".

### CLI

```bash
biorouter extension install ./spokeagent-0.4.1.brxt
```

At a terminal, missing values are prompted for with **echo off** for anything the manifest declares secret. Unattended, the install stops with a non-zero exit naming the keys it needs and registers nothing.

> ⚠ **This used to warn and register anyway.** A human reading the warning scroll past might catch it; an agent reads the exit code, reports success, and leaves an extension that starts and cannot authenticate. There is no install-it-broken path any more.

For unattended runs:

```bash
printf '%s\n' 'SPOKEAGENT_PASSCODE=…' | biorouter extension install ./bundle.brxt --secret-stdin
```

`--secret KEY=VALUE` still works and now says out loud that the value is in your shell history and visible to `ps`. Do not put a credential in a command-line argument.

To re-enter credentials later without reinstalling:

```bash
biorouter extension configure spokeagent
```

Already-configured keys are listed **by name** and never read back to pre-fill a field — reading one back would put it on screen, which is the one thing an echo-off prompt exists to prevent.

### From a chat

The agent calls `install_extension` with the BAAM registry id and download URL. It never shells out, never sees a credential, and is told in its own tool description that a value in a chat message cannot configure anything and would expose it.

Privacy Gate F1 lands on the **attach**, not on the install: installing is the user's explicit request and writes to disk, while attaching loads the server into this chat, which is what a public model may not do to a private extension. A refused attach still leaves the extension correctly installed for a session that may use it.

---

## Tests

```bash
cargo test -p biorouter --lib -- extension_install
cargo test -p biorouter --test extension_install_secrets
cargo test -p biorouter-server --lib -- routes::action_required
cargo test -p biorouter-cli --lib -- commands::extension   # needs an isolated HOME
cd ui/desktop && npx vitest run src/components/SecretRequestCard.test.tsx
```

`extension_install_secrets` is the one that matters, and every test in it asserts an **absence**. That is the point: an implementation that renders the dialog perfectly, stores the value correctly and *also* writes it into `config.yaml` would pass every functional test this feature could have. The surfaces it checks are the plaintext config entry, the published card, the parked call's outcome and its `Debug` rendering, a diagnostics bundle, the session store (the card is ephemeral, so it is never written), and the widest outbound provider payload there is — the coding-agent transcript flattener.

The desktop suite adds a structural assertion for the same reason: `types/message.ts` must define no `createSecretResponseMessage`, because that one line beside `createElicitationResponseMessage` would look like the obvious next step and would put the credential straight back on the transport.

## Related documentation

- [Extensions, skills, and MCP agents](extensions-and-skills-guide.md) — the end-user guide to all three extension kinds.
- [Extension Manager](built-in/extension-manager.md) — the built-in that hosts `install_extension`.
- [Secret storage](../security/secret-storage.md) — where a credential actually lives once stored.
- [Privacy tiers](../security/privacy-tiers.md) — Gate F1, and why it lands on the attach.
- [Browser access decisions](../deployment/serve-decisions.md) — SD-1, and why a browser daemon cannot accept credentials over HTTP.
