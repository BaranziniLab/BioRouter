# Xiaomi MiMo Provider — Integration & Verification Checklist

Xiaomi's **MiMo** LLM family is integrated as a first-class, OpenAI-compatible
provider (`xiaomi_mimo`). This checklist enumerates every surface a user can
select a provider/model and how to verify MiMo appears and works.

## How it's wired

- **Native provider module:** `crates/biorouter/src/providers/xiaomi_mimo.rs`
  (`XiaomiMimoProvider`), registered in `crates/biorouter/src/providers/factory.rs`.
  Provider id `xiaomi_mimo`, display name **Xiaomi MiMo**, default model
  `mimo-v2.5`.
- **Auth / endpoint:** Bearer `XIAOMI_MIMO_API_KEY`. Default host is the
  live-verified Singapore Token-Plan endpoint
  `https://token-plan-sgp.xiaomimimo.com/v1`; override with `XIAOMI_MIMO_HOST`
  for another region/tier:
  - Pay-as-you-go (`sk-` keys): `https://api.xiaomimimo.com/v1`
  - Token Plan (`tp-` keys): `https://token-plan-{cn,sgp,ams}.xiaomimimo.com/v1`
- **Models:** `mimo-v2.5`, `mimo-v2.5-pro` (~1M ctx), `mimo-v2-pro`,
  `mimo-v2-omni` (~256k ctx). Context limits also registered in
  `crates/biorouter/src/model.rs` (`MODEL_SPECIFIC_LIMITS`).
- Because every selection surface is **registry-driven**, the provider appears
  automatically once registered + configured. Only display polish
  (ordering/labels/onboarding text) needed explicit wiring.

## Surfaces — appears in every place a model can be chosen

- [ ] **Provider config dashboard (Settings → Providers)** — appears under
  *Commercial Models*. Backend-driven via `GET /config/providers`; ordering set
  in `ui/desktop/src/components/settings/providers/providerOrdering.ts`
  (`xiaomi_mimo`).
- [ ] **Provider configuration modal** — `XIAOMI_MIMO_API_KEY` (secret) +
  optional `XIAOMI_MIMO_HOST` fields render from backend `config_keys`; labels
  in `ui/desktop/src/utils/configUtils.ts`.
- [ ] **Onboarding** — listed under "Auto-detect from API key" and "View all
  commercial providers"; auto-detect wired in
  `crates/biorouter/src/providers/auto_detect.rs` and text in
  `ui/desktop/src/components/onboarding/CommercialSetupCard.tsx`.
- [ ] **Main model selector** (bottom menu / SwitchModelModal) — once
  configured, `mimo-*` models appear in the picker (backend-driven).
- [ ] **Leader/Worker mode** (`LeadWorkerSettings.tsx`) — MiMo models selectable
  for both lead and worker (backend-driven).
- [ ] **Knowledge base ingestion/digestion** (`IngestModelPicker.tsx`) — MiMo
  models selectable for ingest (backend-driven).
- [ ] **CLI** (`biorouter configure`) — appears in the provider list under
  Commercial; usable via `biorouter run --provider xiaomi_mimo --model mimo-v2.5`.
- [ ] **TUI** — stores the selected provider/model string; no separate list.
- [ ] **Daemon/server** — `GET /config/providers`,
  `GET /config/providers/xiaomi_mimo/models`, and `/config/detect-provider`
  all surface it (registry-driven; no allowlist).

## Functional verification

- [ ] **Live endpoint reachable** — `POST {host}/v1/chat/completions` with the
  key returns HTTP 200 + a `mimo-v2.5` completion. *(Verified: returned
  `BIOROUTER-OK`, `reasoning_tokens: 0` with thinking disabled.)*
- [ ] **Registered in factory** — `cargo test -p biorouter --lib providers::xiaomi_mimo`
  (`test_registered_in_factory`, `test_metadata_structure`).
- [ ] **Config keys correct** — `cargo test -p biorouter --lib test_openai_compatible_providers_config_keys`
  (first key `XIAOMI_MIMO_API_KEY`, required + secret). *(Verified passing.)*
- [ ] **Live completion through the provider stack** —
  `XIAOMI_MIMO_API_KEY=<key> cargo test -p biorouter --test providers test_xiaomi_mimo_provider -- --nocapture`
  (exercises factory → `XiaomiMimoProvider::from_env` → live HTTP).
- [ ] **Context window** — `mimo-v2.5` reports ~1M tokens for token accounting
  (`MODEL_SPECIFIC_LIMITS`).

## Notes / gotchas

- MiMo enables **thinking** by default (adds reasoning tokens/latency). The
  OpenAI surface disables it via `chat_template_kwargs.enable_thinking=false`.
- A `tp-` key is bound to a single region — set `XIAOMI_MIMO_HOST` to the
  matching regional endpoint if not on Singapore.
- Token accounting: the API reports cached prompt tokens separately; counts are
  not directly comparable across regions/tiers.
