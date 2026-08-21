/** @vitest-environment node */

import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { createRequire } from 'node:module';
import AdmZip from 'adm-zip';
import { afterEach, describe, expect, it, vi } from 'vitest';

const require = createRequire(import.meta.url);
const { extract, handleMinGitFailure, missingMinGitAllowed } =
  require('../../scripts/download-mingit.js') as {
    extract: (zipPath: string, destDir: string) => void;
    handleMinGitFailure: (error: Error, env?: Record<string, string | undefined>) => void;
    missingMinGitAllowed: (env?: Record<string, string | undefined>) => boolean;
  };

const roots: string[] = [];

afterEach(() => {
  for (const root of roots.splice(0)) fs.rmSync(root, { recursive: true, force: true });
  vi.restoreAllMocks();
});

describe('download-mingit', () => {
  it('extracts without interpreting apostrophes in the destination path', () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'biorouter-mingit-download-'));
    roots.push(root);
    const zipPath = path.join(root, 'mingit.zip');
    const destination = path.join(root, "O'Neil", 'git');
    const zip = new AdmZip();
    zip.addFile('cmd/git.exe', Buffer.from('git'));
    zip.writeZip(zipPath);

    extract(zipPath, destination);

    expect(fs.readFileSync(path.join(destination, 'cmd', 'git.exe'), 'utf8')).toBe('git');
  });

  it('requires an explicit truthy opt-out before allowing a missing fallback', () => {
    const error = new Error('download failed');
    vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    expect(missingMinGitAllowed({})).toBe(false);
    expect(missingMinGitAllowed({ BIOROUTER_ALLOW_MISSING_MINGIT: 'false' })).toBe(false);
    expect(missingMinGitAllowed({ BIOROUTER_ALLOW_MISSING_MINGIT: '1' })).toBe(true);
    expect(missingMinGitAllowed({ BIOROUTER_ALLOW_MISSING_MINGIT: ' YES ' })).toBe(true);
    expect(() => handleMinGitFailure(error, {})).toThrow(error);
    expect(() =>
      handleMinGitFailure(error, { BIOROUTER_ALLOW_MISSING_MINGIT: 'true' })
    ).not.toThrow();
  });
});
