import { describe, expect, it } from 'vitest';
import { marketplacePhase, primaryInstallAction, resultingPrivacyTier } from './brxtInstallFlow';
import type { BrxtEnvVar, BrxtManifest } from '../types/brxt';

const envVar = (over: Partial<BrxtEnvVar> & { key: string }): BrxtEnvVar => ({
  required: false,
  auto_propagate: false,
  description: '',
  secret: false,
  ...over,
});

const manifest = (env_vars: BrxtEnvVar[]): BrxtManifest => ({
  name: 'x',
  display_name: 'X',
  description: '',
  version: '1.0.0',
  entry_point: 'main.py',
  repository: '',
  env_vars,
});

describe('primaryInstallAction (issue #116)', () => {
  it('installs, and says so, when the manifest declares no variables', () => {
    expect(primaryInstallAction(manifest([]))).toEqual({
      kind: 'install',
      label: 'Install extension',
    });
  });

  it('names the configuration step when a variable is required', () => {
    expect(primaryInstallAction(manifest([envVar({ key: 'TOKEN', required: true })]))).toEqual({
      kind: 'configure',
      label: 'Next: configure',
    });
  });

  it('says "optional" when nothing is required but settings exist', () => {
    expect(primaryInstallAction(manifest([envVar({ key: 'HEADLESS' })]))).toEqual({
      kind: 'optional',
      label: 'Next: optional settings',
    });
  });

  it('a single required variable among optional ones still gates the flow', () => {
    expect(
      primaryInstallAction(
        manifest([envVar({ key: 'HEADLESS' }), envVar({ key: 'TOKEN', required: true })])
      ).kind
    ).toBe('configure');
  });

  it('is total: no manifest yet is not a crash', () => {
    expect(primaryInstallAction(null).kind).toBe('install');
  });
});

describe('marketplacePhase (issue #116)', () => {
  const base = {
    downloading: false,
    downloadError: null,
    isValidating: false,
    validationError: null,
    hasManifest: false,
  };

  it('reports the download while it runs', () => {
    expect(marketplacePhase({ ...base, downloading: true })).toBe('downloading');
  });

  it('reports validation while the bundle is read', () => {
    expect(marketplacePhase({ ...base, isValidating: true })).toBe('validating');
  });

  it('is ready only once a manifest exists and nothing is in flight', () => {
    expect(marketplacePhase({ ...base, hasManifest: true })).toBe('ready');
    expect(marketplacePhase({ ...base, hasManifest: true, isValidating: true })).toBe('validating');
  });

  /**
   * A failure wins over a running phase: Retry / Back to marketplace is the
   * only useful thing on screen at that point, and a spinner beside it reads
   * as "still working".
   */
  it('an error from either half wins over progress', () => {
    expect(marketplacePhase({ ...base, downloading: true, downloadError: 'no route' })).toBe(
      'error'
    );
    expect(marketplacePhase({ ...base, isValidating: true, validationError: 'bad zip' })).toBe(
      'error'
    );
  });

  /**
   * The one-tick gap between "download resolved" and "validation started" has
   * no manifest and nothing in flight. Calling it `downloading` would run the
   * progress backwards under the user.
   */
  it('never runs backwards across the download/validate seam', () => {
    expect(marketplacePhase(base)).toBe('validating');
  });
});

describe('resultingPrivacyTier (issue #116)', () => {
  it('is public only when both sources say public', () => {
    expect(resultingPrivacyTier('public', 'public')).toBe('public');
    expect(resultingPrivacyTier(null, undefined)).toBe('public');
  });

  /**
   * §10.2's union rule: `effectivePrivacy` (the marketplace row) is a superset
   * of `classifyExtension` (the installed config), so a confirmation screen
   * that took only the second could lower a badge the row already raised.
   */
  it('private from either source wins, and neither can lower the other', () => {
    expect(resultingPrivacyTier('private', 'public')).toBe('private');
    expect(resultingPrivacyTier('public', 'private')).toBe('private');
    expect(resultingPrivacyTier(null, 'private')).toBe('private');
    expect(resultingPrivacyTier('private', undefined)).toBe('private');
  });
});
