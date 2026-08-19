/**
 * The update download streams to disk. The hand-rolled read/write loop it
 * replaced had two bugs that only surface on a failing disk, and both were bad
 * enough to justify pinning the behaviour:
 *
 *  - with no prior backpressure there was NO 'error' listener on the write
 *    stream at all, so a failed write became an unhandled 'error' — which
 *    reaches the main process's `uncaughtException` handler and replaces every
 *    open window with the fatal error screen;
 *  - after any backpressure the listener from that round was never removed, so a
 *    later write error settled an already-settled promise and the download hung
 *    forever with the progress bar frozen.
 *
 * Neither is reachable through the UI on a healthy machine, so only a test that
 * forces a write failure covers them.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import * as fs from 'fs/promises';
import { readFileSync } from 'fs';
import * as path from 'path';

let tmpHome: string;

vi.mock('electron', () => ({ app: { getVersion: () => '1.0.0' } }));
vi.mock('./logger', () => ({
  default: { info: vi.fn(), warn: vi.fn(), error: vi.fn() },
}));
// `os` has to be MOCKED, not spied: an ES module namespace is not configurable,
// so `vi.spyOn(os, 'homedir')` throws "Cannot redefine property". The test itself
// therefore cannot import from `os` either — hence the env-var temp base below.
vi.mock('os', async (importOriginal) => {
  const actual = await importOriginal<typeof import('os')>();
  return { ...actual, default: { ...actual, homedir: () => tmpHome }, homedir: () => tmpHome };
});

const TMP_BASE = process.env.TMPDIR || process.env.TEMP || '/tmp';

import { GitHubUpdater } from './githubUpdater';

function bodyOf(bytes: Buffer, contentLength?: number): Response {
  const stream = new ReadableStream<Uint8Array>({
    start(controller) {
      // Several chunks, so backpressure has a chance to engage.
      for (let i = 0; i < bytes.length; i += 8) controller.enqueue(bytes.subarray(i, i + 8));
      controller.close();
    },
  });
  return {
    ok: true,
    status: 200,
    statusText: 'OK',
    headers: new Headers({ 'content-length': String(contentLength ?? bytes.length) }),
    body: stream,
  } as unknown as Response;
}

beforeEach(async () => {
  tmpHome = await fs.mkdtemp(path.join(TMP_BASE, 'br-dl-'));
});

afterEach(async () => {
  vi.restoreAllMocks();
  await fs.rm(tmpHome, { recursive: true, force: true });
});

const url = 'https://example.invalid/Biorouter-9.9.9.dmg';

describe('GitHubUpdater.downloadUpdate', () => {
  it('writes the asset and leaves no .part behind', async () => {
    const payload = Buffer.from('x'.repeat(200));
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => bodyOf(payload))
    );

    const res = await new GitHubUpdater().downloadUpdate(url, '9.9.9');
    expect(res.success).toBe(true);

    const written = await fs.readFile(res.downloadPath!);
    expect(written.length).toBe(payload.length);
    await expect(fs.access(`${res.downloadPath}.part`)).rejects.toThrow();
  });

  it('reports progress without exceeding 100', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => bodyOf(Buffer.from('y'.repeat(160))))
    );
    const seen: number[] = [];

    await new GitHubUpdater().downloadUpdate(url, '9.9.9', (p) => seen.push(p));

    expect(seen.length).toBeGreaterThan(0);
    expect(Math.max(...seen)).toBeLessThanOrEqual(100);
    // Monotonic — the UI treats a backward jump as a restart.
    expect([...seen].sort((a, b) => a - b)).toEqual(seen);
  });

  it('rejects a truncated transfer instead of leaving a broken installer', async () => {
    // Content-Length promises more than the body delivers.
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => bodyOf(Buffer.from('z'.repeat(50)), 5000))
    );

    const res = await new GitHubUpdater().downloadUpdate(url, '9.9.9');
    expect(res.success).toBe(false);
    expect(res.error).toMatch(/truncated/i);

    const downloads = path.join(tmpHome, 'Downloads');
    expect(await fs.readdir(downloads)).toEqual([]);
  });

  // NOTE ON WHAT THIS PROVES. It forces the failure at OPEN (EISDIR), which the
  // old hand-rolled loop also handled — its `fs.open` rejected before any stream
  // existed. The bug needed a write to fail MID-STREAM, which a unit test cannot
  // force without a real full disk. So this test pins the contract, and the
  // structural assertion at the bottom of this file pins the mechanism.
  it('turns a write failure into a returned error, not an uncaught exception', async () => {
    const payload = Buffer.from('w'.repeat(400));
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => bodyOf(payload))
    );

    // Occupy the .part path with a DIRECTORY, so opening it for writing fails
    // deterministically (EISDIR) — a stand-in for a full or read-only disk.
    const downloads = path.join(tmpHome, 'Downloads');
    await fs.mkdir(downloads, { recursive: true });
    await fs.mkdir(path.join(downloads, 'Biorouter-9.9.9.dmg.part'), { recursive: true });

    const uncaught: unknown[] = [];
    const onUncaught = (e: unknown) => uncaught.push(e);
    process.on('uncaughtException', onUncaught);
    try {
      const res = await new GitHubUpdater().downloadUpdate(url, '9.9.9');
      expect(res.success).toBe(false);
      expect(res.error).toBeTruthy();
      await new Promise((r) => setTimeout(r, 50));
      expect(uncaught).toEqual([]);
    } finally {
      process.off('uncaughtException', onUncaught);
    }
  });

  it('does not treat a body larger than content-length as truncated', async () => {
    // A proxy that re-encodes, or a mis-reported header. Failing this would
    // delete a download that actually succeeded.
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => bodyOf(Buffer.from('q'.repeat(300)), 100))
    );

    const res = await new GitHubUpdater().downloadUpdate(url, '9.9.9');
    expect(res.success).toBe(true);
  });
});

describe('download stream mechanics', () => {
  // The two bugs were in HOW the stream was driven, and a mid-stream write
  // failure cannot be forced from a unit test — so the shape is asserted
  // directly. `once('error', reject)` inside the write loop is the exact pattern
  // that (a) left no listener at all before the first backpressure event and
  // (b) leaked a settled one after every subsequent event.
  // Comments stripped: this file explains the old pattern in prose, and a naive
  // grep matches that explanation instead of the code.
  const source = readFileSync(path.join(__dirname, 'githubUpdater.ts'), 'utf8')
    .replace(/\/\*[\s\S]*?\*\//g, '')
    .replace(/^\s*\/\/.*$/gm, '');

  it('drives the download with pipeline rather than a hand-rolled write loop', () => {
    expect(source).toContain("from 'stream/promises'");
    expect(source).toMatch(/await pipeline\(/);
  });

  it('attaches no transient error listener to a write stream', () => {
    expect(source).not.toMatch(/once\(\s*'error'/);
    expect(source).not.toMatch(/writeStream\.write\(/);
  });

  it('removes the partial file on any failure, not only on truncation', () => {
    // The catch around the pipeline, plus the truncation branch's own removal.
    expect(source.match(/fs\.rm\(partPath/g)?.length ?? 0).toBeGreaterThanOrEqual(2);
  });
});
