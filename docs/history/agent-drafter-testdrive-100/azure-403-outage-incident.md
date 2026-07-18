# UCSF Azure 403 outage incident

> **What this is.** The incident record for the UCSF Azure provider outage that halted the 100-app
> Agent Drafter test drive: what failed, what it cost the run, how it was resolved, and the fail-fast
> harness correction it forced.
> **Status:** Historical record — resolved. VPN connectivity was restored on 2026-07-12 and the run
> resumed at spec 011. No ongoing action. The outage start date was not recorded.
> **Audience:** maintainers of Agent Drafter and the Apps SDK.

The test drive contract pinned every authoring and runtime turn to a single UCSF Azure OpenAI
deployment, with no fallback model permitted. When UCSF stopped accepting the machine's egress IP,
that pin turned a network problem into a full stop for the campaign — and, because the CLI reported
success anyway, into silently corrupted test results. Both halves are recorded below.

This incident is also carried as finding 21 (the environment blocker) and finding 22 (the CLI exit
code) in the [audit findings register](audit-findings-register.md). That register is the source of
truth for the finding text; this file is the source of truth for the incident timeline and the
harness change.

## Incident status

- The required provider/model remained `versa_azure/gpt-5.5-2026-04-24` for every authoring and
  runtime turn throughout.
- VPN connectivity was restored on 2026-07-12. The UCSF endpoint then accepted both Agent Drafter
  authoring and app-runtime requests.
- No fallback model or provider was used at any point.
- The failure itself: three consecutive goal turns reproduced HTTP 403
  `The IP Address is invalid: 104.52.5.246`. Once the fail-fast driver correction below was in
  place, those turns recorded `rc 75` and no false app progress.
- Resolution proof: the resumed named spec 011 session ran for 324.8 seconds, authored the app, and
  passed static review with zero issues. Its runtime agent then completed all three declared worker
  consults using the same locked model.

## Blast radius

What the outage did and did not damage, per spec:

- Complete drafts and browser evidence for specs 001–010 remained intact.
- A valid Agent Drafter refinement completed for spec 004 before the outage began.
- Provider-blocked retries for spec 003 and specs 005–009 did not change their apps and are not
  credited as authoring rounds.
- Specs 011–013 were not authored during the outage. The early CLI attempts are preserved in
  `data/ledger.json` as `provider-blocked` / rc 75 and excluded from authoring-round counts. Spec 011
  was authored successfully after recovery.
- Runtime retests of specs 004 and 007 reproduced the same 403 before any agent reasoning ran.

## Harness correction

The outage exposed a worse problem than the outage: `biorouter run` exited zero on a provider 403, so
the driver credited two-to-six-second "rounds" as real authoring work and continued the batch. The
test driver was corrected to:

- scan combined CLI output for the UCSF 403 / IP marker;
- record the attempt as `kind=provider-blocked` in the ledger;
- exit with code `75`;
- exclude blocked attempts from per-app round budgets; and
- abort the batch immediately rather than continuing into fabricated results.

Live validations against specs 011 and 013 exited 75 after one rejected call and did not record false
build progress. Three regression tests cover the locked 100-spec corpus, the error detection, and the
blocked-round accounting.

> **Note.** `rc 75` is the harness's provider-authentication exit code and `kind=provider-blocked` is
> its ledger marker for an attempt that never reached the model. The remediation later moved the same
> distinction into the product: see
> [plan item 0.3, "A failed turn is a failed turn"](remediation-plan.md#03-a-failed-turn-is-a-failed-turn),
> which introduced `TurnAborted` and real CLI exit codes (75 auth, 70 provider, 76 tool loop,
> 77 worker).

## Recovery and resume point

The external remediation was reconnecting the required VPN route. The run resumed at spec 011 with
the fail-fast guard left enabled in case connectivity regressed, and continued sequentially through
spec 025 — where the campaign stopped for unrelated reasons. It never reached spec 100.

## Related documentation

- [Test drive README](README.md) — the index for this campaign and the reading order.
- [Audit findings register](audit-findings-register.md) — findings 21 and 22, the register entries
  for this incident.
- [Authored-app verdict index](authored-app-verdict-index.md) — which specs were authored before and
  after the outage.
- [Remediation plan, item 0.3](remediation-plan.md#03-a-failed-turn-is-a-failed-turn) — the product
  fix for the zero-exit-code half of this incident.
