# Folding browser mode into the command-line interface

> **What this is.** The record of the campaign that replaced the standalone
> `biorouter-headless` binary with `biorouter serve` — what was investigated, what was
> decided, what shipped, and what was found along the way that had nothing to do with the
> goal.
> **Status:** Historical record — completed 2026-08-23.
> **Audience:** anyone auditing why browser mode is built the way it is, or picking up the
> follow-up items at the end.

Biorouter could be reached from a browser before this campaign, but only on a provisioned
Linux server, through a separate binary shipped as its own release artifact. The goal was to
make browser access an ordinary capability of the product: a command anyone can run on any
platform, on the install they already have.

The living documentation this produced is in [`deployment/`](../../deployment/README.md) —
[the decisions](../../deployment/serve-decisions.md), [the
architecture](../../deployment/serve-architecture.md) and [the user
guide](../../deployment/browser-access.md). This page records the work, not the result.

## What shipped

| Area | Change |
|---|---|
| Daemon | Serves the interface itself, gated by a token-for-cookie exchange on the document only |
| Daemon | The sixteen interface endpoints moved in, behind the existing authentication |
| Daemon | Both WebSocket origin gates learned the daemon's own serving origin |
| Command | `biorouter serve`, alias `headless`; `biorouter web` deprecated |
| Packaging | The interface bundle ships in every artifact, including the CLI-only Linux packages |
| Release | The standalone binary, its tarball and seven scripts retired; assets 11 → 10 |
| Testing | A CI job and a release smoke test, where there had been no coverage at all |

Net change to the tree was **negative**: the proxy, the child supervisor, the readiness
poll, the header allowlists and the path-prefix rewriting machinery were all deleted rather
than moved.

## How it was investigated

The campaign opened with a twenty-two agent study of the existing crate, including six
adversarial verifiers briefed to refute the load-bearing claims and a completeness critic.
That study is worth knowing about for one reason: **four of its own claims were refuted
during it**, including one that an entire work unit rested on (a package-collision that did
not exist). The habit that produced that outcome — asking someone to disprove the finding
rather than confirm it — is the part worth copying.

A second fan-out re-measured everything against the tree immediately before implementation,
because the study had been written one release earlier.

## What was found that was not the goal

Three of these were pre-existing defects the work merely walked past.

- **The retired binary was a trusted-LAN appliance.** It bound `0.0.0.0` by default with a
  single `TraceLayer` in front of it — no authentication of any kind — and `fs_read` had no
  path validation at all. The daemon was forced into plaintext-secret mode, so that endpoint
  would hand out `secrets.yaml` to anyone who asked. Retiring the binary removes this;
  the ported endpoints refuse traversal, symlink escape and credential stores by name.
- **Its verifier could pass vacuously.** `verify-headless-artifact.sh` gated its architecture
  check on `command -v file` with no `else` branch, so on a machine without `file(1)` a macOS
  binary staged into the Linux tarball would have satisfied every assertion in it.
- **The Linux GUI `.deb` ships Windows binaries.** Thirty-one files under
  `resources/bin/llamacpp/`, including `llama-server.exe`, because the Linux packaging path
  calls `npm run make` directly and so never runs `prepare-platform-binaries.js` — meaning it
  gets neither the binary validation nor the Linux `llama-server` fetch. Previously suspected;
  confirmed here by extracting a built package. **Not fixed by this campaign** — it is
  independent, and it is tracked separately.
- **Two renderer bugs that would each have failed silently.** The interface endpoints became
  authenticated, but the renderer sent no key and returns `null` on failure, so the whole
  surface would have fallen into its local-storage fallback with nothing logged. And the
  runtime configuration carried `apiBaseUrl` as an empty string, which is falsy, so the
  renderer fell through to a hardcoded `127.0.0.1:3000`.

## Decisions, and the one that mattered most

Seven decisions are recorded in full at
[serve-decisions.md](../../deployment/serve-decisions.md). The one that shaped everything
else was **SD-1**: a browser session cannot change its model or provider.

That began as the campaign's blocking question — `POST /config/set_provider` returns `409`
without proof a human made the request, and a browser has no way to supply one. The three
options on the table were all ways to manufacture that proof. The ruling instead **withdrew
the capability**: the operator chooses the provider at the terminal before anyone opens a
tab, and the privacy tier that choice implies then holds for every session in that daemon.

This is a better answer than any of the three, because the refusal stops being a limitation
and becomes the mechanism. It also closes `privacy-tiers-execution-plan.md` **Open Question
23**, which had been explicitly left unruled on the grounds that a headless deployment "has
no GUI, so there is no process that can mint a key on the user's behalf."

## Follow-ups this campaign did not do

- The Linux GUI package shipping Windows binaries, described above.
- `GET /headless/fs/artifact` is called by the renderer and has never existed, on any
  implementation. The artifact panel silently falls back in browser mode.
- `getPathForFile` returns a bare filename in browser mode, so a dropped file may resolve to
  a different file that happens to exist on the server. A wrong answer with no error.

## Related documentation

- [Browser access](../../deployment/browser-access.md) — the user-facing guide.
- [Decisions behind `biorouter serve`](../../deployment/serve-decisions.md).
- [Architecture of the serving path](../../deployment/serve-architecture.md).
- [Privacy tiers](../../security/privacy-tiers.md) — the classification SD-1 protects.
