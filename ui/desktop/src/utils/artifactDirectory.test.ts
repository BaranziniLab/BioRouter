import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { readArtifactDirectoryTree } from './artifactDirectory';

const temporaryDirectories: string[] = [];

afterEach(async () => {
  await Promise.all(
    temporaryDirectories
      .splice(0)
      .map((directory) => fs.rm(directory, { recursive: true, force: true }))
  );
});

describe('plain artifact directory tree', () => {
  it('returns a nested root-scoped tree while omitting hidden entries', async () => {
    const root = await fs.mkdtemp(path.join(os.tmpdir(), 'biorouter-plain-tree-'));
    temporaryDirectories.push(root);
    await fs.mkdir(path.join(root, 'notes'));
    await fs.mkdir(path.join(root, '.git'));
    await fs.writeFile(path.join(root, 'README.md'), '# Demo');
    await fs.writeFile(path.join(root, 'notes', 'result.txt'), 'ready');
    await fs.writeFile(path.join(root, '.git', 'config'), 'hidden');
    const entries = await readArtifactDirectoryTree(root);

    expect(
      entries.map(({ name, relativePath, parentPath, isDirectory, size }) => ({
        name,
        relativePath,
        parentPath,
        isDirectory,
        size,
      }))
    ).toEqual([
      {
        name: 'notes',
        relativePath: 'notes',
        parentPath: '',
        isDirectory: true,
        size: undefined,
      },
      {
        name: 'result.txt',
        relativePath: 'notes/result.txt',
        parentPath: 'notes',
        isDirectory: false,
        size: 5,
      },
      {
        name: 'README.md',
        relativePath: 'README.md',
        parentPath: '',
        isDirectory: false,
        size: 6,
      },
    ]);
  });
});
