# z.ai (GLM) Provider — Integration & Verification Checklist

z.ai (the international platform of **Zhipu AI**) is integrated as a
first-class, OpenAI-compatible provider (`zai`) serving the **GLM** model
family. This checklist enumerates every surface a user can select a
provider/model and how to verify z.ai appears and works. (Its sibling,
`xiaomi-mimo-integration-checklist.md`, covers the MiMo provider, which is
wired identically.)

## How it's wired

- **Native provider module:** `crates/biorouter/src/providers/zai.rs`
  (`ZaiProvider`), registered in `crates/biorouter/src/providers/factory.rs`.
  Provider id `zai`, display name **z.ai**, default model `glm-4.6`.
- **Auth / endpoint:** Bearer `ZAI_API_KEY`. Default host is the
  OpenAI-compatible base `https://api.z.ai/api/paas/v4`; override with
  `ZAI_HOST`. (z.ai also exposes an Anthropic-compatible surface at
  `https://api.z.ai/api/anthropic` used by Claude Code — not used here; we
  integrate the OpenAI surface, matching the other ~16 OpenAI-compatible
  providers.)
- **Models:** `glm-4.7`, `glm-4.6`, `glm-4.5`, `glm-4.5-air`, `glm-5.2`,
  `glm-5.1`, `glm-5`, `glm-5-turbo`. Context limits registered in
  `crates/biorouter/src/model.rs` (`MODEL_SPECIFIC_LIMITS`, `glm-*` patterns).
- Because every selection surface is **registry-driven**, the provider appears
  automatically once registered + configured. Only display polish
  (ordering/labels/onboarding text) needed explicit wiring.

## Surfaces — appears in every place a model can be chosen

- [ ] **Provider config dashboard (Settings → Providers)** — appears under
  *Commercial Models*. Backend-driven via `GET /config/providers`; ordering set
  in `ui/desktop/src/components/settings/providers/providerOrdering.ts` (`zai`).
- [ ] **Provider configuration modal** — `ZAI_API_KEY` (secret) + optional
  `ZAI_HOST` fields render from backend `config_keys`.
- [ ] **Onboarding** — listed under "Auto-detect from API key"; auto-detect
  wired in `crates/biorouter/src/providers/auto_detect.rs` and text in
  `ui/desktop/src/components/onboarding/CommercialSetupCard.tsx`.
- [ ] **Main model selector** (bottom menu / SwitchModelModal) — once
  configured, `glm-*` models appear in the picker (backend-driven).
- [ ] **Leader/Worker mode** (`LeadWorkerSettings.tsx`) — GLM models selectable
  for both lead and worker (backend-driven).
- [ ] **Knowledge base ingestion/digestion** (`IngestModelPicker.tsx`) — GLM
  models selectable for ingest (backend-driven).
- [ ] **CLI** (`biorouter configure`) — appears in the provider list under
  Commercial (`configure_provider_dialog()` reads the registry); usable via
  `biorouter run --provider zai --model glm-4.6`.
- [ ] **TUI** — stores the selected provider/model string; no separate list.
- [ ] **Daemon/server** — `GET /config/providers`,
  `GET /config/providers/zai/models`, and `/config/detect-provider` all surface
  it (registry-driven; no allowlist).

## Functional verification

- [ ] **Endpoint / auth reachable** — `GET {host}/models` with the key returns
  HTTP 200. *(Verified live: key authenticates; chat returns HTTP 429 code 1113
  "insufficient balance" — auth OK, account just needs credit. A bad key
  returns 401, confirming the 429 is a billing, not auth, state.)*
- [ ] **Registered in factory** — `cargo test -p biorouter --lib providers::zai`
  (`test_registered_in_factory`, `test_metadata_structure`).
- [ ] **Config keys correct** — `cargo test -p biorouter --lib test_openai_compatible_providers_config_keys`
  (first key `ZAI_API_KEY`, required + secret).
- [ ] **Live completion through the provider stack** (needs a funded key) —
  `ZAI_API_KEY=<key> cargo test -p biorouter --test providers test_zai_provider -- --nocapture`
  (exercises factory → `ZaiProvider::from_env` → live HTTP).
- [ ] **Context window** — `glm-4.6`/`glm-4.7` report ~200k tokens for token
  accounting (`MODEL_SPECIFIC_LIMITS`).

## Notes / gotchas

- z.ai keys have the form `<id>.<secret>` — pass the whole string (the dot is
  part of the key) as `ZAI_API_KEY`.
- The default endpoint is the OpenAI-compatible `/api/paas/v4` base. Don't point
  `ZAI_HOST` at the Anthropic surface (`/api/anthropic`) — the wire format
  differs and this provider speaks OpenAI chat/completions.
