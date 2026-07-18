# Spec 008 — Manhattan Signal Room
- **App id:** spec-008-manhattan-signal-room
- **Authoring rounds:** 1 successful + 1 provider-blocked   **Reached acceptance:** no
- **Channel:** CLI (named resumable BioRouter session)
- **Provider/model:** `versa_azure/gpt-5.5-2026-04-24` (UCSF Azure OpenAI)

## Functional verdict: PARTIAL
| Check | Result | Notes |
|---|---|---|
| Not a chatbot (5.2) | ✅ | Wide interactive Manhattan/locus workbench with peak rail, inspector, tissues, and transport |
| Layout matches (5.3) | ⚠️ | Regions exist, but transport is at y≈1111 and document is 1173px tall at 720p |
| Declared surface (5.4) | ✅ | Static manifest/source cross-check |
| Client reactivity (5.5) | ✅ | rs123 selection immediately updated both plots and the inspector; actions later rendered 5 SNPs and λ=1.00 |
| Agent-driven loop (5.6) | ⚠️ | Action batches ran, but repeated describe prevented locus brief, remaining workers, and second instruction |
| Multi-agent ran (5.7) | ⚠️ | Prospector and Fine Mapper ran on UCSF; Colocalizer and Interpreter were not reached |
| Signals round-trip (5.8) | ⚠️ | Gesture started a turn but ambient status still reported `peak_clicked` unsubscribed |

## Aesthetic verdict: PARTIAL
- The dark violet Manhattan plot, ranked peak rail, locus panel, inspector cards, and floating KPI strongly match `midnight`.
- The primary transport is far below the acceptance viewport and the page overflows horizontally by 16px.

## Screenshots
- [`../shots/spec-008-initial.png`](../shots/spec-008-initial.png)

## Friction encountered
- The generated `statistical-genetics` skill failed to load.
- Fine Mapper responsibly refused defensible PIPs from insufficient inputs, but main then invented a normalized five-SNP PIP vector and rendered it.
- Prospector/Fine Mapper sessions were verified on the UCSF model; Colocalizer/Interpreter were never reached.
- Main made four extra `ui_describe` calls between action phases; locus story stayed blank.
- The queued refinement hit the UCSF IP-allowlist 403 in 4.2s and made no app change; local retest still showed below-fold transport, first-signal loss, and split/stale selection state.
