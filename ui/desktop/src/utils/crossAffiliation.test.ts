import { describe, it, expect } from 'vitest';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative, sep } from 'node:path';
import {
  CROSS_AFFILIATION_ACCEPT_MARKER,
  crossAffiliationOffer,
  mixingModeFromConfig,
} from './crossAffiliation';

/**
 * The daemon's refusal, assembled the way the daemon assembles it: a warning
 * composed by `privacy::affiliation`, then the accept frame.
 *
 * ⚠ The frame comes from the **mirrored constant**, never from a string typed
 * here. A hand-typed copy would keep passing after a Rust reword — which is the
 * one failure this whole marker exists to make loud — and the Rust side's
 * `only_the_refusal_a_grant_can_clear_offers_the_accept_marker` is what fails
 * when the two drift.
 */
const warning =
  'Cross-institutional data flow. The extension `ucsfomopagent` holds data belonging to ' +
  'UCSF, but this chat is bound to a model covered by Stanford’s agreements.';
const refusalTail =
  ' This call was not made. Only the user can accept a cross-institutional risk, and only ' +
  'after it has been stated to them, so do not retry.';
const grantable = (extension: string) =>
  `${warning}${refusalTail} ${CROSS_AFFILIATION_ACCEPT_MARKER}\`${extension}\`, on this chat's current model.`;
const notGrantable = `${warning}${refusalTail}`;

describe('crossAffiliationOffer', () => {
  it('reads the extension key out of the accept frame', () => {
    expect(crossAffiliationOffer(grantable('ucsfomopagent'))).toEqual({
      extension: 'ucsfomopagent',
    });
    // A different connector, so the parse is reading the refusal rather than
    // answering with a name it already knew.
    expect(crossAffiliationOffer(grantable('cdwagent'))).toEqual({ extension: 'cdwagent' });
  });

  it('offers nothing for the refusals no grant can clear', () => {
    // The two sites that compose this refusal WITHOUT the frame:
    // `assert_extension_reachable` and the agent's own enable path. Pressing an
    // accept control there would record a real acceptance and leave the retry
    // refused.
    expect(crossAffiliationOffer(notGrantable)).toBeNull();
    // Gate D's chat-recall refusal is about a history, not an extension.
    expect(
      crossAffiliationOffer(
        'This chat history was not read. Only the user can accept a cross-institutional risk.'
      )
    ).toBeNull();
    // Anything that is not a cross-affiliation refusal at all.
    expect(crossAffiliationOffer('The command could not be started')).toBeNull();
    expect(crossAffiliationOffer(null)).toBeNull();
    expect(crossAffiliationOffer(undefined)).toBeNull();
    expect(crossAffiliationOffer({ message: grantable('ucsfomopagent') })).toBeNull();
  });

  it('offers nothing when the frame carries no key to grant against', () => {
    // A truncated or reworded tail must not produce a grant keyed on the empty
    // string — which the daemon would `name_to_key` into a row nothing matches.
    expect(crossAffiliationOffer(`${warning} ${CROSS_AFFILIATION_ACCEPT_MARKER}`)).toBeNull();
    expect(crossAffiliationOffer(`${warning} ${CROSS_AFFILIATION_ACCEPT_MARKER}\`\`.`)).toBeNull();
    expect(
      crossAffiliationOffer(`${warning} ${CROSS_AFFILIATION_ACCEPT_MARKER}ucsfomopagent.`)
    ).toBeNull();
  });
});

describe('mixingModeFromConfig', () => {
  it('reads the three modes, and defaults to standard', () => {
    expect(mixingModeFromConfig('open')).toBe('open');
    expect(mixingModeFromConfig('standard')).toBe('standard');
    expect(mixingModeFromConfig('strict')).toBe('strict');
    // Whitespace and case, mirroring `privacyTiersEnabledFromConfig`'s rule:
    // without the same normalisation on both sides a hand-edited value renders
    // one policy while the daemon enforces another.
    expect(mixingModeFromConfig('  STRICT ')).toBe('strict');
  });

  it('resolves an absent or unreadable value to standard, never to open', () => {
    // `open` would silently take the control away; `strict` would silently take
    // the machine's only route past a warning away. `standard` is the documented
    // default and the only safe answer to "the setting is not there".
    expect(mixingModeFromConfig(undefined)).toBe('standard');
    expect(mixingModeFromConfig(null)).toBe('standard');
    expect(mixingModeFromConfig('')).toBe('standard');
    expect(mixingModeFromConfig('permissive')).toBe('standard');
    expect(mixingModeFromConfig(7)).toBe('standard');
    expect(mixingModeFromConfig({ mode: 'open' })).toBe('standard');
  });
});

/**
 * Task 57 gate (4), and the renderer's half of
 * `privacy::grant::tests::the_proof_of_user_is_constructed_in_exactly_one_place`.
 *
 * The Rust audit pins that only one HTTP handler can mint the proof-of-user. It
 * says nothing about the renderer, where the equivalent question is *which
 * module may ask*: a grant posted from a surface the model can drive — a tool
 * result renderer that acts on its own content, an app frame handler, an
 * elicitation reply — would be DR-19's "the agent may ask, only the user may
 * answer" undone on the client.
 *
 * So: exactly one module outside `src/api/` may name the generated call, and it
 * is the one whose only caller is an `onClick`.
 */
describe('the grant call has exactly one caller in the renderer', () => {
  const srcRoot = join(__dirname, '..');

  /**
   * The generated client, identified by PATH and not by directory name.
   *
   * ⚠ `src/api/` is exempt because it is where the call is *defined* — every
   * generated operation names itself there, so including it would make the audit
   * assert nothing. Matching the name `api` at any depth instead would exempt
   * `src/components/anything/api/` too, and a caller planted there was measured
   * to pass this audit untouched. One directory is exempt, and it is that one.
   */
  const generatedClient = join(srcRoot, 'api');

  /**
   * Production sources only.
   *
   * ⚠ Tests are excluded and it is a real exclusion, not a convenience: a
   * `vi.mock('../api', …)` names every function it replaces, so this file's own
   * sibling — the transcript reachability test — legitimately mentions the call
   * while shipping nothing. Nothing under a `.test.` name reaches a user, so the
   * property being audited ("which module may ask for a grant at run time") is
   * unaffected. It cost one red run to find out, which is the audit working.
   */
  const isProduction = (name: string) => /\.(ts|tsx)$/.test(name) && !/\.(test|spec)\./.test(name);

  const walk = (dir: string): string[] =>
    readdirSync(dir).flatMap((entry) => {
      const full = join(dir, entry);
      if (statSync(full).isDirectory()) {
        return full === generatedClient || entry === 'node_modules' ? [] : walk(full);
      }
      return isProduction(entry) ? [full] : [];
    });

  it('is named by the accept module alone', () => {
    const files = walk(srcRoot);
    // A broken walk reports the same empty set as a clean tree.
    expect(files.length).toBeGreaterThan(400);

    // Composed, so this test file does not match its own audit.
    const needle = ['agent', 'CrossAffiliationGrant'].join('');
    const callers = files
      .filter((file) => readFileSync(file, 'utf8').includes(needle))
      .map((file) => relative(srcRoot, file).split(sep).join('/'))
      .sort();

    expect(callers).toEqual(['utils/crossAffiliation.ts']);
  });
});
