import { describe, it, expect } from 'vitest';
import { parseWorkflowDeeplink } from './workflowDeeplink';

/** Mirrors the Rust encoder: URL-safe base64, no padding. */
function encodeUrlSafe(json: string): string {
  return Buffer.from(json, 'utf8')
    .toString('base64')
    .replace(/\+/g, '-')
    .replace(/\//g, '_')
    .replace(/=+$/, '');
}

const WORKFLOW_JSON = JSON.stringify({
  version: '1.0.0',
  title: 'Deeplink Smoke Test',
  description: 'A workflow shared via a biorouter:// deeplink',
  instructions: 'You are a helpful assistant.',
});

describe('parseWorkflowDeeplink', () => {
  it('round-trips a URL-safe base64 config emitted by the Rust encoder', () => {
    const config = encodeUrlSafe(WORKFLOW_JSON);
    const result = parseWorkflowDeeplink(`biorouter://workflow?config=${config}`);

    expect(result?.config).toBe(config);
    expect(result?.parameters).toBeUndefined();

    // The recovered payload must decode back to the original workflow.
    const padded = config.replace(/-/g, '+').replace(/_/g, '/');
    const decoded = Buffer.from(padded, 'base64').toString('utf8');
    expect(JSON.parse(decoded).title).toBe('Deeplink Smoke Test');
  });

  it('preserves "+" in legacy standard-base64 configs that URLSearchParams would eat', () => {
    // URLSearchParams.get() turns '+' into a space, which corrupts the payload.
    const legacyConfig = 'aGVsbG8+d29ybGQ+Zm9v';
    const result = parseWorkflowDeeplink(`biorouter://workflow?config=${legacyConfig}`);
    expect(result?.config).toBe(legacyConfig);
    expect(result?.config).not.toContain(' ');
  });

  it('collects workflow parameters but excludes config and scheduledJob', () => {
    const config = encodeUrlSafe(WORKFLOW_JSON);
    const result = parseWorkflowDeeplink(
      `biorouter://workflow?config=${config}&scheduledJob=job-1&gene=BRCA1&cohort=cohort%20A`
    );

    expect(result?.config).toBe(config);
    expect(result?.parameters).toEqual({ gene: 'BRCA1', cohort: 'cohort A' });
  });

  it('returns undefined when there is no config parameter', () => {
    expect(parseWorkflowDeeplink('biorouter://workflow')).toBeUndefined();
    expect(parseWorkflowDeeplink('biorouter://workflow?scheduledJob=job-1')).toBeUndefined();
  });

  it('returns undefined instead of throwing on a malformed URL', () => {
    expect(parseWorkflowDeeplink('not a url')).toBeUndefined();
    expect(parseWorkflowDeeplink('')).toBeUndefined();
  });

  it('does not throw on an invalid percent-escape in the config', () => {
    const result = parseWorkflowDeeplink('biorouter://workflow?config=abc%2');
    expect(result?.config).toBe('abc%2');
  });

  it('does not mistake a param whose name ends in "config" for the config value', () => {
    const config = encodeUrlSafe(WORKFLOW_JSON);
    const result = parseWorkflowDeeplink(`biorouter://workflow?myconfig=decoy&config=${config}`);
    expect(result?.config).toBe(config);
    expect(result?.parameters).toEqual({ myconfig: 'decoy' });
  });
});
