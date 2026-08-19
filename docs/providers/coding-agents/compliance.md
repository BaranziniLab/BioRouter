# Compliance: vendor terms, BAA and PHI

> **What this is.** The factual compliance position for the two subscription-billed providers: what
> Anthropic's terms permit and forbid, how BioRouter stays inside the permitted lane, what the
> current state of subscription usage limits is, why the OpenAI position is unresolved, and the hard
> rule that follows for UCSF — **no protected health information may reach either provider** — plus
> the gate that enforces it rather than merely advising it.
> **Status:** Current. Quotations and links below were re-verified against their sources on
> 2026-08-19. Vendor terms change without notice; re-verify before relying on this page, and treat
> nothing here as legal advice.
> **Audience:** UCSF researchers deciding whether to use these providers, maintainers changing them,
> and anyone reviewing BioRouter for institutional use.

BioRouter is a clinical-research tool at UCSF, and these two providers run inference on a
**consumer subscription** the user holds personally. Two separate questions follow, and conflating
them is the mistake this page exists to prevent:

1. **Is it permitted** for BioRouter to use a user's own vendor subscription this way?
2. **What data may travel over it**, given that a consumer subscription carries no Business
   Associate Agreement and no zero-data-retention agreement?

The answer to the first is yes, within a narrow lane that BioRouter's design stays inside by
construction. The answer to the second is: **not PHI, ever** — and that one is enforced by the
privacy bind gate, not left to good intentions.

## What Anthropic's terms say

From Anthropic's [Claude Code legal and compliance page](https://code.claude.com/docs/en/legal-and-compliance),
under "Authentication and credential use":

> Anthropic does not permit third-party developers to offer Claude.ai login or to route requests
> through Free, Pro, or Max plan credentials on behalf of their users.

The same section states that OAuth authentication is intended exclusively for purchasers of the
subscription plans and is designed to support ordinary use of Claude Code and other native Anthropic
applications, and it directs developers *building products that interact with Claude's
capabilities* — including those using the Agent SDK — to API-key authentication through the Claude
Console or a supported cloud provider. The usage-policy section adds that advertised Pro and Max
usage limits assume ordinary, individual usage of Claude Code and the Agent SDK.

Read together, the prohibited conduct is a third party **offering the login** or **routing requests
on behalf of its users**. The permitted conduct is ordinary individual use of one's own plan.

## The permitted lane, and how BioRouter stays in it

| The prohibition | What BioRouter does |
| --- | --- |
| Offering Claude.ai login | BioRouter never implements a vendor login flow. It shows the vendor's own command — `claude auth login`, `codex login` — for the **user** to run in their own terminal. |
| Routing requests through plan credentials on behalf of users | There is no routing. Nothing passes through BioRouter's servers, because BioRouter has no servers in this path: the user's own CLI, on the user's own machine, talks to the vendor directly. |
| Handling the credential | BioRouter never sees, stores, brokers, proxies or transmits it. The CLI resolves its own credential from the OS keychain or the vendor's own file; BioRouter reads at most one non-secret field (`auth_mode`) to report whether the user is signed in. |
| Impersonating the vendor's first-party client | BioRouter identifies itself honestly — `clientInfo.name = "biorouter"`. No entrypoint spoofing, no header rewriting, no proxy. Some harnesses do exactly this, and it is what the terms target. |
| Diverting the run somewhere else | The environment the child is started in is scrubbed of every base-URL override and alternate-backend switch, so a run cannot be silently redirected through a rewriting proxy. |

Two design choices that look like implementation details are actually part of this posture, and
should not be "simplified" away:

- **BioRouter deliberately does not perform the sign-in**, and the signed-out error message says so
  in as many words. A unit test pins that sentence.
- **The Agent SDK is not used.** Its own overview directs third-party developers to API-key
  authentication under Anthropic's Commercial Terms. `claude -p` is the documented way to drive the
  same agent loop from another language, and it is what Anthropic's own help centre names when
  describing subscription usage.

> **Note — this is the compliance boundary, so it is also the design boundary.** Every rule in
> [how the coding-agent providers work](how-it-works.md) follows from staying on the permitted side:
> no login flow, no harness spoofing, no credential handling, and a scrubbed child environment.

## Subscription usage limits: the current state

Agent SDK, `claude -p` and third-party app usage currently draw from the **normal subscription usage
limits**. A change had been announced that would have moved them onto a separate Agent SDK credit
from 15 June 2026; that change was **paused and never took effect**. From Anthropic's support
article [Agent SDK usage changes](https://support.claude.com/en/articles/15036540):

> For now, nothing has changed: Claude Agent SDK, `claude -p`, and third-party app usage still draw
> from your subscription's usage limits.

The same notice states that the previously announced monthly credit is not available. Practically:
a long BioRouter turn on the Claude Code provider consumes the user's ordinary plan allowance, and
heavy use will hit the plan's limits like any other use. Do not plan capacity around the paused
credit, and re-check the article before assuming this paragraph is still current.

## The part that matters most at UCSF: no PHI

A consumer Claude Pro/Max subscription and a ChatGPT Plus subscription carry **no BAA and no
zero-data-retention agreement**. Anthropic's BAA extends only to API traffic with ZDR enabled, per
organization. From the same [legal and compliance page](https://code.claude.com/docs/en/legal-and-compliance):

> The BAA will be applicable to that customer's API traffic flowing through Claude Code. ZDR is
> enabled on a per-organization basis, so each organization must have ZDR enabled separately to be
> covered under the BAA.

A subscription-authenticated `claude -p` run is not API traffic under an organization's BAA. It is
ordinary consumer usage of a personal plan. **Therefore protected health information, identifiable
clinical records, and any other data your institutional agreements restrict must never reach these
providers.** For that work, use the institutionally hosted providers or a local model — see
[data privacy and protected health information](../../security/data-privacy-and-phi.md), which is
the authoritative page for which provider class is acceptable for which data classification, and
confirm with UCSF compliance rather than with this page.

## How that rule is enforced, not merely advised

Both providers declare `ProviderTier::Public` and leave `runs_locally` false. That is not a label;
it is what the privacy bind gate reads.

- A session that has touched an institutional clinical extension or a private knowledge base is
  **classified Private**, permanently — the classification is a ratchet.
- Binding a provider requires the provider's capability tier to be at least the session's
  classification.
- So the bind gate **refuses to attach either coding-agent provider to a Private session.** The
  refusal happens at the bind, before a turn can send anything.

> **Warning — the gate is enforcement, not a guarantee.** Privacy tiers have a master switch, and
> turning them off removes the *enforcement*, not the exposure: with the feature disabled nothing
> stops a user binding Claude Code to a chat that has touched clinical data. The gate is also a
> **safety** boundary rather than a security one — it prevents mistakes, and it does not pretend to
> withstand a determined path, because the filesystem is open to any chat with a shell. The rule
> above ("no PHI") is therefore a rule you follow, with a gate that catches the ordinary way of
> breaking it.

The design, the eight gates and the two lattices are in [privacy tiers](../../security/privacy-tiers.md);
what a user sees is in
[data privacy and PHI](../../security/data-privacy-and-phi.md). Neither provider declares an
institutional affiliation, so they are also outside the affiliation checks described in
[institutional affiliation](../../security/institutional-affiliation.md) — they are not any
institution's, which is the point.

> **Warning — declaring these providers Private "because the subprocess is local" would be a
> serious bug.** The tempting reasoning is that the CLI runs on the user's machine, so the traffic
> never leaves it. `runs_locally` does not mean that: it is defined as whether **inference** happens
> on the user's machine, and here it does not. Inference happens at Anthropic or at OpenAI. `Public`
> is, per the privacy-tier design, "everything hosted by an AI company or a large cloud."
>
> Marking them Private would forge that badge and delete the protection: the bind gate would then
> happily attach a consumer subscription with no BAA to a session holding clinical data, and every
> downstream check would believe the session was still private. Both values are the trait defaults,
> and both are restated explicitly in each provider's metadata so the decision is visible rather
> than inherited by omission.

Isolation does not change this either. [What the child agent may not do](child-agent-isolation.md)
constrains what the child can touch **on the machine**; it does nothing about what the model sees.
A prompt containing PHI is PHI in transit regardless of how locked down the subprocess is.

## The OpenAI position is unresolved

State this honestly, because the honest answer is what makes the posture defensible: **no
first-party OpenAI clause governing third-party use of a ChatGPT plan could be obtained.** OpenAI's
policy pages return HTTP 403 to automated retrieval — re-confirmed for
`openai.com/policies/row-terms-of-use/` on 2026-08-19 — so the terms could not be read or quoted
here.

The only on-record OpenAI statement found is in the Codex repository's
[discussion #8338](https://github.com/openai/codex/discussions/8338), where an OpenAI engineer
confirms that forking Codex CLI is permitted under its Apache-2.0 licence and **explicitly declines
the Terms of Service question** — describing themselves as "an engineer, not a lawyer" and pointing
readers to OpenAI's terms of use and to their own legal counsel. Several subsequent integration
questions in the same thread received no authoritative clarification.

So: the licence question is settled, and the subscription-terms question is not. BioRouter therefore
applies the **same hands-off posture** to Codex as to Claude Code — no login flow, no credential
handling, no proxying, honest client identification — and treats the position as unresolved rather
than as permitted-by-silence. If a first-party OpenAI clause becomes readable, update this section
rather than inferring one.

## If you need a supported, covered path instead

| You want | Use |
| --- | --- |
| Anthropic or OpenAI models under a commercial agreement, with usage accounting that reconciles | The ordinary `anthropic` / `openai` providers with an API key. |
| Clinical or otherwise regulated data | The institutionally hosted providers, or a local model — per [data privacy and PHI](../../security/data-privacy-and-phi.md). |
| No data leaving the machine at all | The bundled **Llama Server** (`llamacpp`) or Ollama; see [the llama-server folder](../llama-server/README.md). |

## Related documentation

- [Data privacy and protected health information](../../security/data-privacy-and-phi.md) — the
  authoritative UCSF guidance on provider classes and data classifications.
- [Privacy tiers](../../security/privacy-tiers.md) — the bind gate that refuses these providers to a
  private session, and what shipped versus what did not.
- [Institutional affiliation](../../security/institutional-affiliation.md) — the third axis, and why
  an unaffiliated provider is treated as it is.
- [How the coding-agent providers work](how-it-works.md) — the credential handling this page rests
  on, stated as mechanism.
- [What the child agent may not do](child-agent-isolation.md) — the machine-side isolation, which is
  a different question from data exposure.
- [Installing and signing in](installing-and-signing-in.md) — the user-facing consequence: you run
  the sign-in command, not BioRouter.
- [Choosing a model provider](../../getting-started/choosing-a-model-provider.md) — the alternatives
  in the table above.
