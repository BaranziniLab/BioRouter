# UCSF Azure provider blocker (resolved)

## Status

- Required provider/model remains `versa_azure/gpt-5.5-2026-04-24` for every authoring and runtime turn.
- VPN connectivity was restored on 2026-07-12. The UCSF endpoint now accepts both Agent Drafter authoring and app-runtime requests.
- No fallback model or provider has been used or will be used for this test.
- Historical failure: three consecutive goal turns reproduced HTTP 403 `The IP Address is invalid: 104.52.5.246`; the corrected fail-fast driver recorded rc 75 and no false app progress.
- Resolution proof: the resumed named Spec 011 session ran for 324.8 seconds, authored the app, and passed static review with zero issues. Its runtime agent then completed all three declared worker consults using the same locked model.

## Impact

- Complete drafts and browser evidence remain intact for Specs 001–010.
- A valid Agent Drafter refinement completed for Spec 004 before the outage.
- Provider-blocked retries for Specs 003 and 005–009 did not change their apps and are not credited as authoring rounds.
- Specs 011–013 were not authored during the outage. The early CLI attempts remain preserved as `provider-blocked` / rc 75 in the ledger and excluded from authoring-round counts. Spec 011 was authored successfully after recovery.
- Runtime retests of Specs 004 and 007 reproduced the same 403 before any agent reasoning.

## Harness correction

The test driver now scans combined CLI output for the UCSF 403/IP marker, records `kind=provider-blocked`, uses exit code 75, excludes blocked attempts from round budgets, and aborts the batch immediately. Live validations against Specs 011 and 013 exited 75 after one rejected call and did not record false build progress. Three regression tests cover the locked 100-spec corpus, error detection, and blocked-round accounting.

## Recovery / resume point

The external remediation was reconnecting the required VPN route. The run resumed at Spec 011 and is continuing sequentially with Specs 012–100, using only the locked UCSF model. The fail-fast guard remains enabled in case connectivity regresses.
