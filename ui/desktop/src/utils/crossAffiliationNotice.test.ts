import { describe, expect, it, vi, beforeEach } from 'vitest';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative, sep } from 'node:path';

const toastWarning = vi.fn();
vi.mock('../toasts', () => ({
  toastWarning: (...args: unknown[]) => toastWarning(...args),
}));

import {
  CROSS_AFFILIATION_NOTICE_SEPARATOR,
  crossAffiliationNotices,
  showCrossAffiliationNotice,
} from './crossAffiliationNotice';

/**
 * A sentence shaped like the daemon's — composed once in `privacy::affiliation`
 * and shipped verbatim, so what matters here is that it survives the trip
 * unchanged rather than what it says.
 */
const warning = (connector: string, owner: string, model: string) =>
  `\`${connector}\` holds data owned by ${owner}, and the model bound to this chat is covered by ${model}'s agreements instead.`;

beforeEach(() => {
  toastWarning.mockClear();
});

describe('crossAffiliationNotices', () => {
  it('reads an empty body as nothing to say', () => {
    // ⚠ The case that fires on nearly every model switch in the product. A
    // presenter that spoke here would train the user to dismiss the one that
    // matters.
    expect(crossAffiliationNotices('')).toEqual([]);
    expect(crossAffiliationNotices('   \n\n  ')).toEqual([]);
  });

  it('reads a non-string body as nothing to say', () => {
    // A changed response shape, or a caller handing over the whole response
    // object, must read as silence rather than stringify into the toast.
    expect(crossAffiliationNotices(undefined)).toEqual([]);
    expect(crossAffiliationNotices(null)).toEqual([]);
    expect(crossAffiliationNotices({ data: 'x' })).toEqual([]);
    expect(crossAffiliationNotices(['x'])).toEqual([]);
  });

  it('carries one warning through verbatim', () => {
    const one = warning('ucsfomopagent', 'UCSF (ucsf)', 'stanford');
    expect(crossAffiliationNotices(one)).toEqual([one]);
  });

  it('splits a multi-warning body on the separator the daemon joins with', () => {
    // A bind can mismatch several connectors at once, each naming a different
    // pair of institutions. Folding them into one message loses which sentence
    // is about which connector.
    const a = warning('ucsfomopagent', 'UCSF (ucsf)', 'stanford');
    const b = warning('cdwagent', 'UCSF (ucsf)', 'stanford');
    expect(crossAffiliationNotices([a, b].join(CROSS_AFFILIATION_NOTICE_SEPARATOR))).toEqual([
      a,
      b,
    ]);
  });
});

describe('showCrossAffiliationNotice', () => {
  it('shows the daemon sentence unchanged, and does not auto-close it', () => {
    const one = warning('ucsfomopagent', 'UCSF (ucsf)', 'stanford');
    expect(showCrossAffiliationNotice(one)).toBe(1);
    expect(toastWarning).toHaveBeenCalledTimes(1);
    expect(toastWarning).toHaveBeenCalledWith({
      // ⚠ Verbatim, and with NO title beside it. The whole point of shipping the
      // statement from the daemon is that the three surfaces cannot describe one
      // boundary differently; a paraphrase, a truncation, or a renderer-invented
      // heading over the daemon's own opening clause undoes that inside the
      // renderer. `toHaveBeenCalledWith` matches the argument exactly, so an
      // added `title` fails here.
      msg: one,
      // ⚠ A privacy statement that expires while the user is still reading the
      // model picker is one they were not shown.
      toastOptions: { autoClose: false },
    });
  });

  it('shows nothing at all for an empty body', () => {
    expect(showCrossAffiliationNotice('')).toBe(0);
    expect(showCrossAffiliationNotice(undefined)).toBe(0);
    expect(toastWarning).not.toHaveBeenCalled();
  });

  it('shows one toast per warning', () => {
    const a = warning('ucsfomopagent', 'UCSF (ucsf)', 'stanford');
    const b = warning('cdwagent', 'UCSF (ucsf)', 'stanford');
    expect(showCrossAffiliationNotice([a, b].join(CROSS_AFFILIATION_NOTICE_SEPARATOR))).toBe(2);
    expect(toastWarning).toHaveBeenCalledTimes(2);
    expect(toastWarning.mock.calls.map((call) => (call[0] as { msg: string }).msg)).toEqual([a, b]);
  });
});

/**
 * The audit that makes the rest of this file worth anything.
 *
 * ⚠ **The defect being repaired was a correct function with no user-facing
 * caller.** `Agent::cross_affiliation_warnings` was right, tested, and read only
 * by a `tracing::warn!` loop — so DR-26's ruling was, on two of its three
 * surfaces, unimplemented while every test in the tree stayed green. A presenter
 * that nothing calls reproduces that exactly, and the unit tests above would not
 * notice: they drive it directly.
 *
 * So the property pinned is possession, not shape: both warn-and-proceed
 * surfaces name the presenter. Deleting either call — the realistic regression,
 * since both are one line appended to a `try` block that already looks finished
 * — turns this red.
 *
 * ⚠ **It is a floor and not a census.** A third surface adding a call passes
 * untouched, deliberately: more places showing the daemon's own sentence is the
 * direction this fix is going, and an exact-set assertion would make every new
 * one a red build with nothing wrong.
 */
describe('the two warn-and-proceed surfaces show the notice they are sent', () => {
  const srcRoot = join(__dirname, '..');
  const isProduction = (name: string) => /\.(ts|tsx)$/.test(name) && !/\.(test|spec)\./.test(name);

  const walk = (dir: string): string[] =>
    readdirSync(dir).flatMap((entry) => {
      const full = join(dir, entry);
      if (statSync(full).isDirectory()) {
        return entry === 'node_modules' ? [] : walk(full);
      }
      return isProduction(entry) ? [full] : [];
    });

  /**
   * ⚠ **Import lines do not count, and this exclusion is the whole test.**
   * Written without it, the audit was measured to PASS against a
   * `ModelAndProviderContext.tsx` whose call had been deleted — the surviving
   * `import { showCrossAffiliationNotice }` satisfied a plain substring scan.
   * That is the same class of defect this whole fix repairs (a symbol that
   * exists, is imported, and is never invoked), so an audit blind to it is
   * worth nothing.
   */
  const invokesOutsideAnImport = (source: string, needle: string) =>
    source
      .split('\n')
      .filter((line) => !/^\s*import\b/.test(line) && !/^\s*}?\s*from\s+'/.test(line))
      .some((line) => line.includes(needle));

  it('is invoked by the bind surface and by the enable surface', () => {
    const files = walk(srcRoot);
    // A broken walk reports the same empty set as a clean tree.
    expect(files.length).toBeGreaterThan(400);

    // Composed, so this file does not satisfy its own audit.
    const needle = ['show', 'CrossAffiliationNotice'].join('');
    const callers = files
      .filter((file) => invokesOutsideAnImport(readFileSync(file, 'utf8'), needle))
      .map((file) => relative(srcRoot, file).split(sep).join('/'))
      .sort();

    expect(callers).toContain('utils/crossAffiliationNotice.ts');
    // The BIND surface: the model picker's `changeModel`.
    expect(callers).toContain('components/ModelAndProviderContext.tsx');
    // The USER's enable surface: the one renderer funnel to
    // `POST /agent/add_extension`.
    expect(callers).toContain('components/settings/extensions/agent-api.ts');
  });
});
