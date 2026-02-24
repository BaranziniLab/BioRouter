# UCSF BioRouter — Data Privacy and Patient Data Guidelines

This document outlines the data privacy considerations for using UCSF BioRouter, with specific guidance on handling patient data, clinical information, and other sensitive research data.

---

## Overview

BioRouter routes your inputs and conversation context to an LLM provider for processing. The data privacy properties of any given session depend entirely on **which provider you are using**. Different providers have fundamentally different data handling policies:

- **Commercial cloud APIs** (Anthropic, OpenAI, Google, etc.) — data is processed on the provider's cloud infrastructure. Review the provider's privacy policy and data processing terms before use.
- **Institution-managed cloud services** (UCSF Azure OpenAI, UCSF Amazon Bedrock) — data is processed within infrastructure governed by UCSF's institutional agreements. These may offer stronger privacy protections than personal API accounts.
- **Local models** (Ollama) — data is processed entirely on your own device. Nothing is transmitted to any external service.

---

## Patient Data and Sensitive Research Data

**IMPORTANT NOTICE:**

If you need to work with patient data, protected health information (PHI), clinical records, genomic data linked to individuals, or any data subject to HIPAA, institutional data governance policies, or other regulatory requirements:

- **Use only institution-managed services or fully local models.**
- Do NOT use personal commercial API accounts (e.g., your personal Anthropic API key, personal OpenAI account) with patient or sensitive data.
- The safest option for data that must remain completely private is a **local model via Ollama** — data never leaves your device.

### Recommended Providers for Sensitive Data

| Provider | Data Stays Within | Recommended For |
|---|---|---|
| **Ollama (local)** | Your device only — no external transmission | Highest sensitivity data, air-gapped requirements |
| **UCSF Azure OpenAI** | UCSF's institutional Azure tenant | Institution-approved use cases — verify with your institution |
| **UCSF Amazon Bedrock** | UCSF's institutional AWS environment | Institution-approved use cases — verify with your institution |

### Providers NOT Recommended for Patient Data

The following providers use personal/commercial API accounts and are generally **not appropriate** for patient data without explicit institutional authorization:

- Anthropic (direct API)
- OpenAI (direct API)
- Google Gemini (direct API)
- OpenRouter
- Venice AI
- X.AI (Grok)
- Any other third-party commercial API

---

## Verification Requirement

**Always verify with your institution before working with sensitive data.**

Even institution-managed services (UCSF Azure OpenAI, UCSF Amazon Bedrock) may have specific terms of use, approved use cases, and restrictions that change over time. Before using BioRouter with any sensitive data:

1. Confirm that your intended use case is covered by the institutional data use agreement for that provider.
2. Check with UCSF IT or your IRB/compliance office if you are unsure.
3. Ensure that the data classification level of your data is compatible with the service tier you are using.

UCSF policies around data handling, HIPAA compliance, and acceptable use of cloud services evolve. The BioRouter development team cannot advise on the current status of institutional agreements. Always check directly with UCSF compliance and IT.

---

## Best Practices for Data Handling

**De-identify before using BioRouter:**
- Remove names, dates of birth, medical record numbers, addresses, and other direct identifiers before inputting clinical data into any BioRouter session, unless you have explicit authorization and a compliant data pathway to do so with identifiers present.

**Minimize data exposure:**
- Provide only the data necessary for the task. Avoid pasting entire datasets into the chat when a representative sample or summary would suffice.

**Use local models when possible:**
- For exploratory work, algorithm development, or testing with real data, Ollama with a capable local model is the safest option.

**Review session logs:**
- BioRouter logs sessions locally. Be aware that session history stored in `~/.config/biorouter/` on your device may contain data you entered. Protect access to your device accordingly.

**Do not share sessions containing sensitive data:**
- BioRouter supports sharing sessions and recipes. Do not share sessions that contain patient data or other sensitive information.

---

## Summary

| Data Type | Recommended Approach |
|---|---|
| De-identified research data | Institution-managed providers or local Ollama |
| Patient data / PHI | Local Ollama only, or institution-managed with explicit compliance approval |
| Public / non-sensitive data | Any provider |
| Proprietary unpublished research data | Local Ollama or institution-managed — verify confidentiality requirements |

**When in doubt: use Ollama (local) or check with your institution.**

---

## Contact

UCSF BioRouter is developed by Wanjun Gu (wanjun.gu@ucsf.edu) at the Baranzini Lab (https://baranzinilab.ucsf.edu/) at UCSF, with support from UCSF IT and Information Commons.

For questions about data governance, HIPAA compliance, and approved data use pathways, contact UCSF IT Security or your departmental compliance officer.
