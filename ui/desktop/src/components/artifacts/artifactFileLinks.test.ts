import { describe, expect, it } from 'vitest';
import { parseFileLink, resolveLocalFilePath } from './artifactFileLinks';

describe('file-link control-character boundaries', () => {
  it.each([...Array.from({ length: 32 }, (_, code) => code), 127])(
    'rejects literal and encoded control character %i',
    (code) => {
      const path = `/work/a${String.fromCharCode(code)}b.txt`;
      expect(parseFileLink(path)).toBeNull();
      expect(parseFileLink(encodeURI(path))).toBeNull();
      expect(resolveLocalFilePath(path, '/work')).toBeNull();
    }
  );

  it.each([' ', '~', '\u0080', 'é', '字', '🧬'])(
    'preserves non-control filename character %s',
    (character) => {
      const path = `/work/a${character}b.txt`;
      expect(parseFileLink(path)).toEqual({ path });
      expect(parseFileLink(encodeURI(path))).toEqual({ path });
      expect(resolveLocalFilePath(path, '/work')).toBe(path);
    }
  );
});
