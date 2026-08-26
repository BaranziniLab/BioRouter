import { describe, expect, it } from 'vitest';
import { artifactSourceRevision } from './artifactSourceRevision';

describe('artifactSourceRevision', () => {
  it('does not alias same-size rewrites whose integer mtime was preserved', () => {
    const before = artifactSourceRevision(4, 1_700_000_000_000, Buffer.from('left'));
    const after = artifactSourceRevision(4, 1_700_000_000_000, Buffer.from('rite'));

    expect(after).not.toBe(before);
  });

  it('is stable for the same rendered bytes and metadata', () => {
    const bytes = Buffer.from('same preview');
    expect(artifactSourceRevision(bytes.length, 12.75, bytes)).toBe(
      artifactSourceRevision(bytes.length, 12.75, bytes)
    );
  });
});
