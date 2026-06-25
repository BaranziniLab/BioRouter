import { describe, expect, it } from 'vitest';
import AdmZip from 'adm-zip';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { safeExtractZip, safeZipEntryTarget } from './safeZip';

function tempDir(): string {
  return fs.mkdtempSync(path.join(os.tmpdir(), 'biorouter-safe-zip-'));
}

describe('safeExtractZip', () => {
  it('extracts regular nested files', () => {
    const dir = tempDir();
    const zip = new AdmZip();
    zip.addFile('src/main.py', Buffer.from('print("ok")'));

    safeExtractZip(zip, dir);

    expect(fs.readFileSync(path.join(dir, 'src', 'main.py'), 'utf8')).toBe('print("ok")');
  });

  it('rejects parent-directory traversal', () => {
    const dir = tempDir();
    expect(() => safeZipEntryTarget(dir, '../outside.txt')).toThrow(/Unsafe zip entry path/);
    expect(fs.existsSync(path.join(dir, '..', 'outside.txt'))).toBe(false);
  });

  it('rejects absolute paths', () => {
    const dir = tempDir();
    expect(() => safeZipEntryTarget(dir, '/tmp/outside.txt')).toThrow(/Unsafe zip entry path/);
  });

  it('rejects windows absolute paths on every platform', () => {
    const dir = tempDir();
    expect(() => safeZipEntryTarget(dir, 'C:\\Users\\Public\\outside.txt')).toThrow(
      /Unsafe zip entry path/
    );
  });
});
