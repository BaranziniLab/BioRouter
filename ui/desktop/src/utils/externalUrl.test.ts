import { describe, expect, it } from 'vitest';
import { normalizeExternalHttpUrl } from './externalUrl';

describe('normalizeExternalHttpUrl', () => {
  it('preserves query delimiters and shell metacharacters as URL data', () => {
    const input = 'https://example.test/search?a=1&b=two|three^four';
    expect(normalizeExternalHttpUrl(input)).toBe(new URL(input).href);
  });

  it('accepts only credential-free HTTP(S) URLs', () => {
    expect(normalizeExternalHttpUrl('https://example.test/path')).toBe('https://example.test/path');
    expect(() => normalizeExternalHttpUrl('file:///C:/Windows/System32/calc.exe')).toThrow(
      'unsafe URL protocol'
    );
    expect(() => normalizeExternalHttpUrl('https://user:secret@example.test/')).toThrow(
      'unsafe URL protocol'
    );
  });

  it('rejects non-strings and oversized URLs', () => {
    expect(() => normalizeExternalHttpUrl(null)).toThrow('invalid or oversized URL');
    expect(() => normalizeExternalHttpUrl(`https://example.test/${'x'.repeat(8192)}`)).toThrow(
      'invalid or oversized URL'
    );
  });
});
