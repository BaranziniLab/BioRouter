# Institutional providers — Versa archive index

This folder holds the paper trail for the two UCSF-specific "institutional" large language
model (LLM) providers,
**Versa API Azure** and **Versa API Bedrock**, and for the change that split the desktop
Provider Configuration grid into labeled sections. Both were specified and built on
**2026-05-07**, and both **shipped**: `crates/biorouter/src/providers/versa_azure.rs` and
`crates/biorouter/src/providers/versa_bedrock.rs` exist in the tree today and are registered
in `crates/biorouter/src/providers/factory.rs`. This is a historical record kept for the
reasoning and the build sequence — not current guidance. The provider grid's *section
ordering* described here is superseded; the live truth is
`ui/desktop/src/components/settings/providers/ProviderGrid.tsx` together with
`providerOrdering.ts` in the same folder, which now render Local Models first.

Come here when you want to know **why** the Versa providers were built the way they were —
why they hardcode a UCSF gateway endpoint, and why their credential keys are
provider-namespaced (`VERSA_BEDROCK_*`) rather than reusing the global AWS keys. Do **not**
come here for constants: the endpoint, Azure deployment, API version, and Bedrock model
lists recorded in both documents are the 2026-05-07 values and several have since been
revised in the provider source. Read the two Rust files for anything live. For maintainer
guidance on provider integration generally, go to
[model provider integration references](../../providers/README.md); for the user-facing
inventory of which providers exist and what each needs, go to
[choosing a model provider](../../getting-started/choosing-a-model-provider.md).

## Documents in this folder

| Document | What it covers |
|---|---|
| [Versa institutional providers and sectioned provider configuration](versa-providers-design.md) | The approved design spec for the two providers with pre-configured connection details, and for splitting the desktop Provider Configuration grid into labeled sections — covering the Rust provider modules, grid sectioning, setup-form pre-population, and one optional dependency probe. |
| [Versa institutional providers implementation plan](versa-providers-plan.md) | The task-by-task plan that executed that spec across seven tasks, carrying the full original source of both provider files as first written. Its `- [ ]` checkboxes are the plan's original tracking state, not open work. |

Read the design first for *what* and *why*; the plan states *how*, step by step. Both carry
their own status headers and supersession notes — do not rewrite them to match current
reality, since they are dated records of their own moment.

> **Note on credentials.** `https://unified-api.ucsf.edu/general` is a UCSF-internal
> gateway, and the `VERSA_*` config keys hold UCSF-issued credentials. Both documents name
> the keys only, never their values. Credentials are issued by UCSF to UCSF users and are
> entered through the app's provider modal.

## Related documentation

- [Model provider integration references](../../providers/README.md) — the current maintainer-facing home for how a provider module is wired into the registry and factory; the successor to this folder for anything you intend to act on.
- [Choosing a model provider](../../getting-started/choosing-a-model-provider.md) — the user-facing reference for the credentials each provider needs and where the Versa providers sit among the rest.
- [Secret storage](../../security/secret-storage.md) — where provider credentials such as `VERSA_AZURE_API_KEY` are actually kept once entered.
- [Historical records index](../README.md) — the rest of the BioRouter archive, if you landed here looking for a different completed piece of work.
