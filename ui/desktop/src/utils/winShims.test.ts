/** @vitest-environment node */

import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ensureBundledGit } from './winShims';

vi.mock('./logger', () => ({ default: { info: vi.fn(), error: vi.fn() } }));

const originalPath = process.env.PATH;

afterEach(() => {
  process.env.PATH = originalPath;
});

function fixture(version = '2.49.0') {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'biorouter-mingit-'));
  const srcBin = path.join(root, 'resources', 'bin');
  const srcGit = path.join(srcBin, 'git');
  const localAppData = path.join(root, 'local');
  fs.mkdirSync(path.join(srcGit, 'cmd'), { recursive: true });
  fs.writeFileSync(path.join(srcGit, 'cmd', 'git.exe'), `git-${version}`);
  fs.writeFileSync(path.join(srcGit, 'mingit-version.txt'), `${version}\n`);
  return { root, srcBin, srcGit, localAppData };
}

describe('ensureBundledGit', () => {
  it('replaces an older persistent MinGit tree', async () => {
    const f = fixture('2.49.0');
    const dstGit = path.join(f.localAppData, 'Biorouter', 'git');
    fs.mkdirSync(path.join(dstGit, 'cmd'), { recursive: true });
    fs.writeFileSync(path.join(dstGit, 'cmd', 'git.exe'), 'old-git');
    fs.writeFileSync(path.join(dstGit, 'mingit-version.txt'), '2.48.0\n');

    await ensureBundledGit(f.srcBin, f.localAppData);

    expect(fs.readFileSync(path.join(dstGit, 'cmd', 'git.exe'), 'utf8')).toBe('git-2.49.0');
    expect(fs.readFileSync(path.join(dstGit, 'mingit-version.txt'), 'utf8').trim()).toBe('2.49.0');
    fs.rmSync(f.root, { recursive: true, force: true });
  });

  it('repairs a partial tree even when its version marker matches', async () => {
    const f = fixture('2.49.0');
    const dstGit = path.join(f.localAppData, 'Biorouter', 'git');
    fs.mkdirSync(dstGit, { recursive: true });
    fs.writeFileSync(path.join(dstGit, 'mingit-version.txt'), '2.49.0\n');

    await ensureBundledGit(f.srcBin, f.localAppData);

    expect(fs.existsSync(path.join(dstGit, 'cmd', 'git.exe'))).toBe(true);
    fs.rmSync(f.root, { recursive: true, force: true });
  });

  it('keeps a complete current tree without replacing it', async () => {
    const f = fixture('2.49.0');
    const dstGit = path.join(f.localAppData, 'Biorouter', 'git');
    fs.cpSync(f.srcGit, dstGit, { recursive: true });
    fs.writeFileSync(path.join(dstGit, 'local-sentinel.txt'), 'keep');

    await ensureBundledGit(f.srcBin, f.localAppData);

    expect(fs.readFileSync(path.join(dstGit, 'local-sentinel.txt'), 'utf8')).toBe('keep');
    fs.rmSync(f.root, { recursive: true, force: true });
  });
});
