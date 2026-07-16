import { describe, expect, it } from 'vitest';
import { buildGitArtifactEntries, parseGitArtifactStatus } from './artifactGit';

describe('Git artifact tree', () => {
  it('classifies working, staged, and untracked porcelain records', () => {
    const statuses = parseGitArtifactStatus(
      ' M src/modified.ts\0M  src/staged.ts\0?? src/new.ts\0MM src/partial.ts\0'
    );
    expect(Object.fromEntries(statuses)).toEqual({
      'src/modified.ts': 'modified',
      'src/staged.ts': 'staged',
      'src/new.ts': 'untracked',
      'src/partial.ts': 'modified',
    });
  });

  it('builds a nested tree with committed and pushed file states', () => {
    const entries = buildGitArtifactEntries({
      rootPath: '/repo',
      trackedPaths: ['README.md', 'src/committed.ts', 'src/pushed.ts', 'src/staged.ts'],
      statuses: new Map([['src/staged.ts', 'staged']]),
      committedPaths: new Set(['src/committed.ts']),
    });
    expect(entries.find((entry) => entry.relativePath === 'README.md')?.status).toBe('pushed');
    expect(entries.find((entry) => entry.relativePath === 'src/committed.ts')?.status).toBe(
      'committed'
    );
    expect(entries.find((entry) => entry.relativePath === 'src/staged.ts')?.status).toBe('staged');
    expect(entries.find((entry) => entry.relativePath === 'src')?.isDirectory).toBe(true);
    expect(entries.find((entry) => entry.relativePath === 'src')?.status).toBe('staged');
  });
});
