# Institutional Providers & Sectioned Provider Configuration

**Date:** 2026-05-07  
**Status:** Approved

## Overview

Add two UCSF-specific "institutional" providers — Versa API Azure and Versa API Bedrock — with pre-configured connection details so users only need to supply credentials. Restructure the Provider Configuration UI into three labeled sections (Institutional / Local / Commercial) matching the Extensions/Skills tab aesthetic.

---

## Rust: New Provider Files

### `crates/biorouter/src/providers/versa_azure.rs`

Duplicated from `azure.rs`. Key differences:

| Field | Value |
|-------|-------|
| Provider name | `versa_azure` |
| Display name | `"Versa API Azure"` |
| Hardcoded endpoint | `https://unified-api.ucsf.edu/general` |
| Hardcoded deployment | `gpt-5.2-2025-12-11` |
| Hardcoded API version | `2024-10-21` |

**Config keys:**

| Key | Required | Secret | Default |
|-----|----------|--------|---------|
| `VERSA_AZURE_API_KEY` | true | true | — |
| `AZURE_OPENAI_ENDPOINT` | false | false | `https://unified-api.ucsf.edu/general` |
| `AZURE_OPENAI_DEPLOYMENT_NAME` | false | false | `gpt-5.2-2025-12-11` |
| `AZURE_OPENAI_API_VERSION` | false | false | `2024-10-21` |

`from_env` reads `VERSA_AZURE_API_KEY` as the auth secret. Endpoint, deployment, and api_version fall back to the hardcoded constants if not overridden in config. The `post()` path and all other logic are identical to `azure.rs`.

### `crates/biorouter/src/providers/versa_bedrock.rs`

Duplicated from `bedrock.rs`. Key differences:

| Field | Value |
|-------|-------|
| Provider name | `versa_bedrock` |
| Display name | `"Versa API Bedrock"` |

**Config keys:**

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

All other `bedrock.rs` logic (converse API, retry config, credential validation) is unchanged.

### Registration

Both providers registered in `crates/biorouter/src/providers/factory.rs`:

```rust
registry.register::<VersaAzureProvider, _>(|m| Box::pin(VersaAzureProvider::from_env(m)), false);
registry.register::<VersaBedrockProvider, _>(|m| Box::pin(VersaBedrockProvider::from_env(m)), false);
```

Both exported from `crates/biorouter/src/providers/mod.rs`.

---

## Frontend: ProviderGrid Sectioning

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

`HIDDEN_PROVIDERS` set and `priorityOrder` sort are preserved within each section. The `"Add Custom Provider"` button stays at the bottom of the Commercial section.

---

## Frontend: Form Pre-Population

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

---

## Dependency Checker

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

Non-blocking: shown as a warning in the Dependency Setup Modal, not an error. App proceeds normally if absent.

---

## Playwright Validation

1. Start app: `just dev-ui-playwright` (debug binary + `ENABLE_PLAYWRIGHT=true`, CDP on port 9222)
2. Connect via `.mcp.json` Playwright MCP config (already present at repo root)
3. Verify:
   - Provider Configuration page shows three labeled sections
   - `versa_azure` / `versa_bedrock` appear under Institutional Models
   - Versa API Azure modal: only API Key above fold, optional fields pre-filled in collapsible
   - Versa API Bedrock modal: Access Key ID + Secret above fold, optional fields pre-filled
   - Enter UCSF credentials; both providers show "Configured" badge
   - Commercial Azure OpenAI and Amazon Bedrock remain in Commercial section unchanged
   - Ollama appears under Local Models

**Success criterion:** both UCSF institutional providers show as "Configured" in the live app UI.

---

## Files Changed

| File | Change |
|------|--------|
| `crates/biorouter/src/providers/versa_azure.rs` | New — duplicated from `azure.rs` |
| `crates/biorouter/src/providers/versa_bedrock.rs` | New — duplicated from `bedrock.rs` |
| `crates/biorouter/src/providers/mod.rs` | Export two new modules |
| `crates/biorouter/src/providers/factory.rs` | Register two new providers |
| `ui/desktop/src/components/settings/providers/ProviderGrid.tsx` | Add section categorization + render |
| `ui/desktop/src/components/settings/providers/modal/subcomponents/forms/DefaultProviderSetupForm.tsx` | Add `versa_azure` / `versa_bedrock` to `PROVIDER_KEY_DEFAULTS` |
| `ui/desktop/src/utils/dependencyChecker.ts` | Add non-blocking AWS CLI check |
