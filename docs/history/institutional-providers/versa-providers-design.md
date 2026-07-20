# Versa institutional providers and sectioned provider configuration

> **What this is.** The design spec for two UCSF-specific "institutional" providers — Versa API Azure and Versa API Bedrock — with pre-configured connection details, and for splitting the desktop Provider Configuration grid into labeled sections.
> **Status:** Historical record — approved 2026-05-07 and shipped. Both providers exist in the tree today as `crates/biorouter/src/providers/versa_azure.rs` and `crates/biorouter/src/providers/versa_bedrock.rs`, and are registered in `crates/biorouter/src/providers/factory.rs`. The sectioned provider grid also shipped, but its section order has since changed (see the note below).
> **Audience:** developers working on LLM providers or the desktop settings UI.

BioRouter ships a long flat list of large language model (LLM) providers. UCSF users reach commercial Azure OpenAI and Amazon Bedrock through a UCSF-operated gateway ("Versa"), which means a fixed endpoint, a fixed deployment, and credentials issued by the university rather than by Microsoft or AWS. This spec adds two dedicated providers that hardcode those connection details so a UCSF user only supplies a key, and it groups the provider grid so institution-managed options are visually distinct from commercial and local ones.

> **Note.** The section ordering specified here — Institutional, then Local, then Commercial — is no longer what ships. `ui/desktop/src/components/settings/providers/ProviderGrid.tsx` now renders **Local Models first**, and the Llama Server (`llamacpp`) card that leads the grid post-dates this spec entirely. The live truth for section order and grouping is `ProviderGrid.tsx` together with `ui/desktop/src/components/settings/providers/providerOrdering.ts`. Everything in this document about the two Versa providers themselves is still structurally accurate; the ordering half is superseded.

> **Warning.** The endpoint, model deployment, and API-version constants recorded below are the values chosen on 2026-05-07. Several have since been revised in the provider source files. Treat `crates/biorouter/src/providers/versa_azure.rs` and `crates/biorouter/src/providers/versa_bedrock.rs` as the live truth for any constant, not this document.

> **Warning.** `https://unified-api.ucsf.edu/general` is a UCSF-internal gateway and the `VERSA_*` config keys hold UCSF-issued credentials. This document names the keys, not their values. Credentials are issued by UCSF to UCSF users; they are entered through the app's provider modal and stored in the OS credential store as described in [secret storage](../../security/secret-storage.md). Do not commit credential values to this repository or to any document in it.

## How to read this document

The spec covers four separate layers, in the order a change would land in them:

1. **Rust providers** — two new provider modules and their registration.
2. **Provider grid sectioning** — how the settings list is grouped and rendered.
3. **Form pre-population** — which fields arrive pre-filled in the setup modal.
4. **Dependency checker** — one new optional tool probe.

A fifth section records the Playwright verification pass used to check the result in the running desktop app, and a final table lists every file the change touches.

## Rust providers

### Versa API Azure

**File:** `crates/biorouter/src/providers/versa_azure.rs`, duplicated from `azure.rs`. Key differences:

| Field | Value |
|-------|-------|
| Provider name | `versa_azure` |
| Display name | `"Versa API Azure"` |
| Hardcoded endpoint | `https://unified-api.ucsf.edu/general` |
| Hardcoded deployment | `gpt-5.2-2025-12-11` |
| Hardcoded API version | `2024-10-21` |

Config keys:

| Key | Required | Secret | Default |
|-----|----------|--------|---------|
| `VERSA_AZURE_API_KEY` | true | true | — |
| `AZURE_OPENAI_ENDPOINT` | false | false | `https://unified-api.ucsf.edu/general` |
| `AZURE_OPENAI_DEPLOYMENT_NAME` | false | false | `gpt-5.2-2025-12-11` |
| `AZURE_OPENAI_API_VERSION` | false | false | `2024-10-21` |

`from_env` reads `VERSA_AZURE_API_KEY` as the auth secret. Endpoint, deployment, and api_version fall back to the hardcoded constants if not overridden in config. The `post()` path and all other logic are identical to `azure.rs`.

### Versa API Bedrock

**File:** `crates/biorouter/src/providers/versa_bedrock.rs`, duplicated from `bedrock.rs`. Key differences:

| Field | Value |
|-------|-------|
| Provider name | `versa_bedrock` |
| Display name | `"Versa API Bedrock"` |

Config keys:

| Key | Required | Secret | Default |
|-----|----------|--------|---------|
| `VERSA_BEDROCK_ACCESS_KEY_ID` | true | true | — |
| `VERSA_BEDROCK_SECRET_ACCESS_KEY` | true | true | — |
| `AWS_PROFILE` | false | false | `"default"` |
| `AWS_REGION` | false | false | `"us-west-2"` |

Provider-namespaced key names (`VERSA_BEDROCK_*`) avoid colliding with the commercial `aws_bedrock` provider's global `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY`, allowing both providers to be configured simultaneously with different credentials.

`from_env` manually maps the namespaced secrets to the standard AWS env vars before loading the SDK:

```rust
if let Ok(v) = config.get_secret("VERSA_BEDROCK_ACCESS_KEY_ID") {
    std::env::set_var("AWS_ACCESS_KEY_ID", v);
}
if let Ok(v) = config.get_secret("VERSA_BEDROCK_SECRET_ACCESS_KEY") {
    std::env::set_var("AWS_SECRET_ACCESS_KEY", v);
}
```

> **Warning.** `std::env::set_var` mutates the environment of the **whole process**, not just this provider. Every other provider, extension, and subprocess spawned afterwards inherits these AWS credentials. This is how `bedrock.rs` already worked and the design deliberately follows it, but it means configuring Versa Bedrock changes global process state and can affect the commercial `aws_bedrock` provider in the same run. Review this alongside any change to credential handling.

All other `bedrock.rs` logic (converse API, retry config, credential validation) is unchanged.

### Registering both providers

Both providers are registered in `crates/biorouter/src/providers/factory.rs`:

```rust
registry.register::<VersaAzureProvider, _>(|m| Box::pin(VersaAzureProvider::from_env(m)), false);
registry.register::<VersaBedrockProvider, _>(|m| Box::pin(VersaBedrockProvider::from_env(m)), false);
```

Both are exported from `crates/biorouter/src/providers/mod.rs`.

## Sectioning the provider grid

**File:** `ui/desktop/src/components/settings/providers/ProviderGrid.tsx`

Replace the flat sorted list with three labeled sections. Add a static category map:

```ts
const INSTITUTIONAL_PROVIDERS = new Set(['versa_azure', 'versa_bedrock']);
const LOCAL_PROVIDERS = new Set(['ollama']);
// all other visible providers → Commercial
```

Render pattern (mirrors `ExtensionList.tsx` exactly):

```tsx
<div className="space-y-8">
  <div>
    <h2 className="text-xs font-medium text-text-muted uppercase tracking-wider mb-3 flex items-center gap-2">
      <span className="w-1.5 h-1.5 bg-indigo-500 rounded-full flex-shrink-0" />
      Institutional Models
    </h2>
    <div className="divide-y divide-border-subtle">
      {institutionalCards}
    </div>
  </div>
  {/* Local: green dot */}
  {/* Commercial: amber dot — includes "Add Custom Provider" at bottom */}
</div>
```

Dot colors: Institutional = indigo-500, Local = green-500 (existing pattern), Commercial = amber-500.

The `HIDDEN_PROVIDERS` set and `priorityOrder` sort are preserved within each section. The `"Add Custom Provider"` button stays at the bottom of the Commercial section.

## Pre-populating the setup form

**File:** `ui/desktop/src/components/settings/providers/modal/subcomponents/forms/DefaultProviderSetupForm.tsx`

Add entries to the existing `PROVIDER_KEY_DEFAULTS` map:

```ts
versa_azure: {
  AZURE_OPENAI_ENDPOINT:        'https://unified-api.ucsf.edu/general',
  AZURE_OPENAI_DEPLOYMENT_NAME: 'gpt-5.2-2025-12-11',
  AZURE_OPENAI_API_VERSION:     '2024-10-21',
},
versa_bedrock: {
  AWS_PROFILE: 'default',
  AWS_REGION:  'us-west-2',
},
```

The existing required/optional split already handles the rest:

- **Versa API Azure modal:** API Key field above fold; endpoint/deployment/version in collapsible, pre-filled.
- **Versa API Bedrock modal:** `AWS_ACCESS_KEY_ID` + `AWS_SECRET_ACCESS_KEY` above fold; profile/region in collapsible, pre-filled.

No other changes to the modal or form components.

## Adding the AWS CLI dependency check

**File:** `ui/desktop/src/utils/dependencyChecker.ts`

Add one non-blocking AWS CLI entry:

```ts
{
  name: 'aws',
  displayName: 'AWS CLI',
  required: false,
  checkCommand: 'aws --version',
  installInstructions: {
    darwin:  'brew install awscli',
    linux:   'pip install awscli  OR  sudo apt install awscli',
    windows: 'winget install Amazon.AWSCLI',
  },
  learnMoreUrl: 'http://biorouter.ucsf.edu/docs',
  reason: 'Required for Bedrock SSO profile auth. Not needed for access-key auth.',
}
```

Non-blocking: shown as a warning in the Dependency Setup Modal, not an error. The app proceeds normally if the AWS CLI is absent.

## Validating in the running app with Playwright

1. Start the app: `just dev-ui-playwright` (debug binary plus `ENABLE_PLAYWRIGHT=true`, Chrome DevTools Protocol (CDP) on port 9222).
2. Connect via the `.mcp.json` Playwright Model Context Protocol (MCP) config, already present at the repo root.
3. Verify each of the following:
   - The Provider Configuration page shows three labeled sections.
   - `versa_azure` and `versa_bedrock` appear under Institutional Models.
   - Versa API Azure modal: only API Key above the fold, optional fields pre-filled in the collapsible.
   - Versa API Bedrock modal: Access Key ID and Secret above the fold, optional fields pre-filled.
   - After entering UCSF credentials, both providers show a "Configured" badge.
   - Commercial Azure OpenAI and Amazon Bedrock remain in the Commercial section, unchanged.
   - Ollama appears under Local Models.

**Success criterion:** both UCSF institutional providers show as "Configured" in the live app UI.

## Files changed

| File | Change |
|------|--------|
| `crates/biorouter/src/providers/versa_azure.rs` | New — duplicated from `azure.rs` |
| `crates/biorouter/src/providers/versa_bedrock.rs` | New — duplicated from `bedrock.rs` |
| `crates/biorouter/src/providers/mod.rs` | Export two new modules |
| `crates/biorouter/src/providers/factory.rs` | Register two new providers |
| `ui/desktop/src/components/settings/providers/ProviderGrid.tsx` | Add section categorization + render |
| `ui/desktop/src/components/settings/providers/modal/subcomponents/forms/DefaultProviderSetupForm.tsx` | Add `versa_azure` / `versa_bedrock` to `PROVIDER_KEY_DEFAULTS` |
| `ui/desktop/src/utils/dependencyChecker.ts` | Add non-blocking AWS CLI check |

## Related documentation

- [Versa providers implementation plan](versa-providers-plan.md) — the task-by-task plan that executed this spec, with the full source of both provider files as first written.
- [Choosing a model provider](../../getting-started/choosing-a-model-provider.md) — the user-facing provider reference, including where the two Versa providers sit among the rest.
- [Secret storage](../../security/secret-storage.md) — where provider credentials such as `VERSA_AZURE_API_KEY` are actually kept.
- [Environment variables](../../configuration/environment-variables.md) — the wider set of config and env keys the providers read.
- [Debugging the dev GUI with agent-browser](../../desktop-ui/agent-browser-debugging.md) — the current way to drive the desktop app for UI verification like the Playwright pass above.
