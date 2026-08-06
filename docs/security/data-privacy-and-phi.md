# Data privacy and protected health information

> **What this is.** UCSF-specific guidance on which model providers are acceptable for
> protected health information (PHI), clinical records, and other sensitive research data,
> plus the de-identification and data-minimisation practices expected before any such data
> enters a BioRouter session.
> **Status:** Current. **Last reviewed 2026-08-06 — against the shipping provider inventory
> only.** That review corrected the local-model guidance, which previously named only Ollama and
> predated the bundled Llama Server provider. It did **not** re-confirm the institutional
> guidance on this page — which providers UCSF approves for which data classification — with
> UCSF compliance, and data use agreements change. Treat the provider names as current and the
> approvals as needing confirmation; verify with UCSF compliance before relying on this page.
> **Audience:** end users at UCSF handling patient, clinical, or otherwise regulated data.

BioRouter routes your inputs and conversation context to a large language model (LLM) provider
for processing. The privacy properties of a session therefore depend entirely on **which
provider you selected**, not on BioRouter itself. This page states which provider classes are
acceptable for which data classifications, and what to do before sending data at all.

> **Note — two local providers, one data-handling property.** BioRouter ships two local
> options: **Llama Server** (`llamacpp`, a bundled llama.cpp sidecar, ranked first and the
> first card a new user sees) and **Ollama**. Both run the model on your own device, so
> neither transmits your data to an external service — that property is what the guidance
> below rests on, and it holds for both. Which of them your institution *authorises* for
> regulated data is a separate question this page cannot answer; confirm with UCSF IT. See
> [choosing a model provider](../getting-started/choosing-a-model-provider.md).

## How provider choice determines data handling

Different providers have fundamentally different data handling policies:

- **Commercial cloud APIs** (Anthropic, OpenAI, Google, etc.) — data is processed on the
  provider's cloud infrastructure. Review the provider's privacy policy and data processing
  terms before use.
- **Institution-managed cloud services** (UCSF Azure OpenAI, UCSF Amazon Bedrock) — data is
  processed within infrastructure governed by UCSF's institutional agreements. These may offer
  stronger privacy protections than personal API accounts.
- **Local models** (bundled Llama Server, Ollama) — data is processed entirely on your own
  device. Nothing is transmitted to any external service.

## Patient data and sensitive research data

> **Warning.** If you need to work with patient data, PHI, clinical records, genomic data
> linked to individuals, or any data subject to the Health Insurance Portability and
> Accountability Act (HIPAA), institutional data governance policies,
> or other regulatory requirements:
>
> - **Use only institution-managed services or fully local models.**
> - Do NOT use personal commercial API accounts (for example, your personal Anthropic API key
>   or personal OpenAI account) with patient or sensitive data.
> - The safest option for data that must remain completely private is a **local model —
>   bundled Llama Server or Ollama** — because the data never leaves your device.

### Providers recommended for sensitive data

| Provider | Data stays within | Recommended for |
|---|---|---|
| **Llama Server or Ollama (local)** | Your device only — no external transmission | Highest sensitivity data, air-gapped requirements |
| **UCSF Azure OpenAI** | UCSF's institutional Azure tenant | Institution-approved use cases — verify with your institution |
| **UCSF Amazon Bedrock** | UCSF's institutional AWS environment | Institution-approved use cases — verify with your institution |

### Providers not recommended for patient data

These providers use personal or commercial API accounts and are generally **not appropriate**
for patient data without explicit institutional authorization:

- Anthropic (direct API)
- OpenAI (direct API)
- Google Gemini (direct API)
- OpenRouter
- Venice AI
- X.AI (Grok)
- Any other third-party commercial API

## Verifying before you begin

**Always verify with your institution before working with sensitive data.**

Even institution-managed services (UCSF Azure OpenAI, UCSF Amazon Bedrock) may have specific
terms of use, approved use cases, and restrictions that change over time. Before using
BioRouter with any sensitive data:

1. Confirm that your intended use case is covered by the institutional data use agreement for
   that provider.
2. Check with UCSF IT or your Institutional Review Board (IRB) or compliance office if you are
   unsure.
3. Ensure that the data classification level of your data is compatible with the service tier
   you are using.

UCSF policies around data handling, HIPAA compliance, and acceptable use of cloud services
evolve. The BioRouter development team cannot advise on the current status of institutional
agreements. Always check directly with UCSF compliance and IT.

## Handling data inside a session

**De-identify before using BioRouter.** Remove names, dates of birth, medical record numbers,
addresses, and other direct identifiers before inputting clinical data into any BioRouter
session, unless you have explicit authorization and a compliant data pathway to do so with
identifiers present.

**Minimize data exposure.** Provide only the data necessary for the task. Avoid pasting entire
datasets into the chat when a representative sample or summary would suffice.

**Use local models when possible.** For exploratory work, algorithm development, or testing
with real data, a capable local model — through the bundled Llama Server or Ollama — is the
safest option; see the provider table above.

**Review session logs.** BioRouter logs sessions locally. Session history stored in
`~/.config/biorouter/` on your device may contain data you entered. Protect access to your
device accordingly.

**Do not share sessions containing sensitive data.** BioRouter supports sharing sessions and
workflows. Do not share sessions that contain patient data or other sensitive information.

## Summary by data type

| Data type | Recommended approach |
|---|---|
| De-identified research data | Institution-managed providers, or a local model |
| Patient data / PHI | A local model only, or institution-managed with explicit compliance approval |
| Public / non-sensitive data | Any provider |
| Proprietary unpublished research data | A local model or institution-managed — verify confidentiality requirements |

**When in doubt: use a local model (Llama Server or Ollama) or check with your institution.**

## Who to contact

For questions about data governance, HIPAA compliance, and approved data use pathways, contact
**UCSF IT Security or your departmental compliance officer** — not the BioRouter team, which
cannot speak to the status of institutional agreements.

UCSF BioRouter is developed by Wanjun Gu (`wanjun.gu@ucsf.edu`) at the
[Baranzini Lab](https://baranzinilab.ucsf.edu/) at UCSF, with support from UCSF IT and
Information Commons.

## Related documentation

- [Choosing a model provider](../getting-started/choosing-a-model-provider.md) — the full
  provider inventory, including the institutional `versa_azure` / `versa_bedrock` providers and
  the bundled Llama Server.
- [Permission modes](permission-modes.md) — limiting what the agent may do with the data once
  it is in a session.
- [Managed enterprise policy](managed-policy.md) — how an administrator enforces tool
  restrictions that a user cannot disable.
- [Sessions](../getting-started/managing-sessions.md) — what session history retains on disk, which matters if a
  session ever contained regulated data.
- [Secret storage](secret-storage.md) — where the provider API keys behind these choices are
  held.
