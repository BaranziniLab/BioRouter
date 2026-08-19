# Coding-agent providers

This folder documents the two providers that run inference on the user's **own vendor
subscription** by driving a coding-agent CLI the user already installed and signed in to:
`claude_code`, shown as **Claude Code**, and `codex`, shown as **Codex**. They are unlike
every other provider in the tree — there is no base URL and no API key, BioRouter never sees
or handles a credential, and the child process is a complete agent of its own whose tools
have to be switched off and replaced. That combination raises questions no other provider
reference has to answer: what the child may do, how BioRouter's own tools reach it, and
whether a consumer subscription may be used this way at all in a clinical-research setting.

Come here when you are **using, verifying or changing either provider**. The pages are
ordered from "make it work" to "why it is allowed", and the compliance page is not optional
reading for anyone deploying this at UCSF: a consumer Pro/Max or ChatGPT Plus plan carries
no Business Associate Agreement and no zero-data-retention agreement, so protected health
information must never reach these providers, and the page explains both the rule and the
gate that enforces it.

## Documents

| Document | What it covers |
| --- | --- |
| [How the coding-agent providers work](how-it-works.md) | The mechanism: what each provider spawns, where each vendor's credential lives, how the binary is found without spawning anything, how the conversation becomes one prompt, and how usage is accounted for a run that billed no tokens. |
| [Installing and signing in](installing-and-signing-in.md) | The user-facing setup: install each CLI, sign in by running the vendor's own command yourself, the four states the settings card can show, and the `CLAUDE_CODE_COMMAND` / `CODEX_COMMAND` escape hatch when the binary lives somewhere BioRouter does not search. |
| [The tool bridge](tool-bridge.md) | How BioRouter's extensions — SPOKE, UCSF OMOP, knowledge, Auto Visualiser, any marketplace plugin — reach the child over MCP while BioRouter still executes them behind its inspectors, permission mode, `.biorouterignore`, vault and privacy gates. Why MCP is the only channel that can do this, and why the capability travels in the URL. |
| [What the child agent may not do](child-agent-isolation.md) | The isolation flags, each of which is security-relevant rather than hygiene: the hostile-fixture result behind `--setting-sources ""`, the measured MCP leak behind `--strict-mcp-config`, why `--tools ""` is not a substitute for either, and why `--bare` must never be passed. |
| [Compliance: vendor terms, BAA and PHI](compliance.md) | The most important page. What Anthropic's terms permit and forbid, the current state of subscription usage limits for `claude -p`, the unresolved OpenAI position, and why both providers are `ProviderTier::Public` so the privacy bind gate keeps them away from clinical sessions. |
| [Performance, limits and known gaps](performance-and-limits.md) | Measured latency and prompt overhead, the cost of a large tool surface, why conversation history is flattened rather than replayed, why there is no streaming yet, and the failure modes worth recognising. |

Read [how it works](how-it-works.md) first if you are changing code, and
[installing and signing in](installing-and-signing-in.md) first if you are trying to get a
card out of "not signed in".

## Boundary with neighbouring folders

- The [providers index](../README.md) holds the per-provider integration references for
  ordinary API-key providers. This folder is a subdirectory of it because these two share a
  mechanism, not just a vendor.
- [Data privacy and PHI](../../security/data-privacy-and-phi.md) and
  [privacy tiers](../../security/privacy-tiers.md) own the institutional data-handling rules
  and the enforcement design. The [compliance page](compliance.md) here states how these two
  providers sit inside them; it does not restate the rules.
- [The coding-agent landscape](../../research/coding-agent-landscape/README.md) studies these
  vendors' agents as *external systems*. Nothing there describes BioRouter's own behaviour.

## Related documentation

- [Model provider integration references](../README.md) — the parent folder, and the
  references for the API-key providers.
- [Choosing a model provider](../../getting-started/choosing-a-model-provider.md) — the
  user-facing survey of every provider, of which these two are the subscription-billed pair.
- [Data privacy and protected health information](../../security/data-privacy-and-phi.md) —
  which providers are acceptable for which data classification at UCSF.
- [Privacy tiers](../../security/privacy-tiers.md) — the capability/classification design
  whose bind gate keeps these providers out of private sessions.
