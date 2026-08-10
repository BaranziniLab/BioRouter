import type { ProviderDetails } from '../../../api';

const PRIORITY_ORDER: Record<string, number> = {
  versa_azure: 0,
  versa_bedrock: 1,
  llamacpp: 0,
  ollama: 1,
  azure_openai: 0,
  aws_bedrock: 1,
  anthropic: 2,
  openai: 3,
  google: 4,
  zai: 5,
  xiaomi_mimo: 6,
};

export type ProviderGroupKey = 'institutional' | 'local' | 'commercial';

export interface OrderedProviderGroup {
  key: ProviderGroupKey;
  /**
   * §14.5: the group heading names the privacy taxonomy and the hosting
   * taxonomy in the same words, in the same place. A UCSF user whose Azure
   * OpenAI account is provisioned and paid for by UCSF IT reads
   * "Institutional"; under this design that account is **Public**, and the old
   * three headings never said so.
   *
   * ⚠ Consumed by TWO surfaces and hardcoded by neither. `ProviderGrid` used to
   * print its own literals and ignore this field, so relabelling here changed
   * nothing a user could see; `IngestModelPicker` has always read it. Both read
   * it now. Do not inline either one back.
   */
  label: string;
  /**
   * The one line of card copy §14.5 asks for — *why* this group has the tier it
   * has. Rendered under the heading, once per section, rather than per card:
   * the reason is a property of the group's classification rule, not of any one
   * vendor.
   */
  note: string;
  accentClassName: string;
  providers: ProviderDetails[];
}

function compareProviders(a: ProviderDetails, b: ProviderDetails): number {
  const pa = PRIORITY_ORDER[a.name] ?? 999;
  const pb = PRIORITY_ORDER[b.name] ?? 999;
  if (pa !== pb) {
    return pa - pb;
  }
  return a.name.localeCompare(b.name);
}

/**
 * Grouping is the backend's answer, never a list kept here. `runs_locally` is
 * the display-only fact that splits the private tier into the two sections this
 * grid has always had. A renderer-side copy of either field is a second source
 * of truth that drifts silently the moment a provider is added, renamed, or
 * re-pointed.
 *
 * ⚠ `metadata.tier` is the *type-level* claim — the tier computed from the
 * endpoint a provider ships with — NOT the tier of the instance actually bound
 * to a session. `GET /config/providers` serves `ProviderMetadata` verbatim, and
 * for a built-in that struct is static, so an `ollama` re-pointed off this
 * machine by `OLLAMA_HOST` still arrives here as `private` while its instance
 * `Provider::tier()` resolves `public`. The two can only ever disagree in that
 * direction, which is why this module may read it.
 *
 * ⚠ The headings below now say the word "Private", which the old ones
 * ("Local Models" / "Institutional Models") only implied — so the residual
 * inaccuracy is louder than it was, and it is written down here rather than
 * discovered later. Exactly one configuration reaches it: a genuine built-in
 * whose endpoint was re-pointed off this machine by an environment variable
 * (`OLLAMA_HOST`) still ships `tier: 'private'` in its type-level metadata and
 * therefore still sits under "Private · Local". A provider *declared* public by
 * the daemon is grouped Commercial, which `providerOrdering.test.ts` pins.
 * Closing the residual case needs the instance tier plumbed to the UI; §14.5
 * asks for the relabel regardless, because the heading a user misreads today
 * ("my institution's Azure is institutional") is wrong far more often.
 *
 * It is still **not** a licence to hang a `PrivacyBadge` on this field. A badge
 * asserts the tier of a *bound* instance; hung here it would read Private in
 * exactly the demotion case the tier exists to catch. The composer chip's badge
 * therefore reads the session's own ratcheted classification, never this. See
 * the `tier` field's doc comment in `crates/biorouter/src/providers/base.rs`.
 */
function classifyProvider(provider: ProviderDetails): ProviderGroupKey {
  if (provider.metadata.tier !== 'private') {
    return 'commercial';
  }
  return provider.metadata.runs_locally ? 'local' : 'institutional';
}

/**
 * Every provider the daemon serves is shown. There used to be a hide-list here
 * for `claude-code`, `codex` and `cursor-agent` — soft-disabled shims that drove
 * another vendor's installed CLI as a subprocess. Those modules were removed, so
 * the daemon no longer serves them and there is nothing left to hide. A new
 * entry in this grid is a decision made where the provider is registered, not a
 * name recognised here.
 */
export function getOrderedProviderGroups(providers: ProviderDetails[]): OrderedProviderGroup[] {
  const grouped: Record<ProviderGroupKey, ProviderDetails[]> = {
    institutional: [],
    local: [],
    commercial: [],
  };

  for (const provider of providers) {
    grouped[classifyProvider(provider)].push(provider);
  }

  grouped.institutional.sort(compareProviders);
  grouped.local.sort(compareProviders);
  grouped.commercial.sort(compareProviders);

  return [
    {
      key: 'local',
      label: 'Private · Local',
      note: 'Private because inference runs on this machine. Nothing leaves it.',
      accentClassName: 'bg-background-success',
      providers: grouped.local,
    },
    {
      key: 'institutional',
      label: 'Private · Institutional',
      note: 'Private because Biorouter recognises this institutional gateway endpoint.',
      accentClassName: 'bg-background-info',
      providers: grouped.institutional,
    },
    {
      key: 'commercial',
      // ⚠ §14.5's note, and the wording matters. The obvious copy — "a direct
      // cloud account, even if your institution pays for it" — is NOT accurate
      // as shipped: `azure.rs` defaults `AZURE_OPENAI_ENDPOINT` to
      // `https://unified-api.ucsf.edu/general`, the same UCSF gateway
      // `versa_azure` uses. A name-keyed tier calls `azure_openai` Public even
      // when it in fact resolves to that gateway — conservative and fail-safe,
      // but the copy must not claim something the configuration contradicts.
      label: 'Public · Commercial',
      note: "Public. Biorouter can't verify where this account's endpoint points, even one your institution pays for.",
      accentClassName: 'bg-background-warning',
      providers: grouped.commercial,
    },
  ];
}
