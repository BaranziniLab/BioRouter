// @vitest-environment node
//
// Main-process code, exercised against the real filesystem and a stubbed
// `sips`, so the node environment is the faithful one.
// `node:fs/promises` deliberately, not the mocked `node:fs`: the module under
// test reaches for the latter, so keeping the fixtures on the former means the
// helpers here cannot contaminate what the assertions measure.
import { open as openFile, rm as removePath, stat as statPath } from 'node:fs/promises';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

// `promisify` resolves `execFile` through this symbol, exactly as Node does, so
// the module under test still gets `{ stdout, stderr }` and not a bare string.
const PROMISIFY_CUSTOM = Symbol.for('nodejs.util.promisify.custom');

const sips = vi.fn<(args: string[]) => Promise<{ stdout: string; stderr: string }>>();

vi.mock('node:child_process', () => {
  const execFile = () => {
    throw new Error('the callback form must not be reached');
  };
  Object.defineProperty(execFile, PROMISIFY_CUSTOM, {
    value: (_binary: string, args: string[]) => sips(args),
  });
  return { execFile, default: { execFile } };
});

vi.mock('electron-log', () => ({ default: { info: vi.fn() } }));

/**
 * Real `fs`, with one exception: `access` is what probes for `/usr/bin/sips`,
 * and on a Linux runner that probe would fail and short-circuit every case
 * before it reached the code being tested.
 */
const createdTempDirs: string[] = [];
const bytesReadByPath = new Map<string, number>();

vi.mock('node:fs', async () => {
  const namespace = await vi.importActual<Record<string, unknown>>('node:fs');
  // Node builtins arrive with their surface on `default`, so spreading the
  // namespace alone would hand back a module missing every named export.
  const actual = (namespace.default ?? namespace) as Record<string, unknown>;
  const actualPromises =
    await vi.importActual<typeof import('node:fs/promises')>('node:fs/promises');
  const promises = {
    ...actualPromises,
    access: async () => undefined,
    mkdtemp: async (prefix: string) => {
      const directory = await actualPromises.mkdtemp(prefix);
      createdTempDirs.push(directory);
      return directory;
    },
    // Counts what the module actually pulls off disk, so "refused before
    // reading" is asserted rather than assumed.
    open: async (filePath: string, flags: string) => {
      const handle = await actualPromises.open(filePath, flags);
      const read = handle.read.bind(handle);
      bytesReadByPath.set(String(filePath), 0);
      Object.defineProperty(handle, 'read', {
        value: async (...args: Parameters<typeof read>) => {
          const result = await read(...args);
          bytesReadByPath.set(
            String(filePath),
            (bytesReadByPath.get(String(filePath)) ?? 0) + result.bytesRead
          );
          return result;
        },
      });
      return handle;
    },
  };
  return { ...actual, default: { ...actual, promises }, promises };
});

async function exists(target: string) {
  return statPath(target).then(
    () => true,
    () => false
  );
}

async function loadModule() {
  vi.resetModules();
  return import('./heicConvert');
}

function outPathOf(args: string[]): string {
  return args[args.indexOf('--out') + 1];
}

/** Answers the metadata probe, then writes a PNG of `outputBytes` for the conversion. */
function stubSips({
  width,
  height,
  outputBytes,
}: {
  width: number;
  height: number;
  outputBytes: number;
}) {
  sips.mockImplementation(async (args) => {
    if (args.includes('-g')) {
      return {
        stdout: `image.heic\n  pixelWidth: ${width}\n  pixelHeight: ${height}\n`,
        stderr: '',
      };
    }
    const handle = await openFile(outPathOf(args), 'w');
    // Sparse, so an over-limit output costs no disk and no wall clock while
    // still reporting its full size to `fstat`.
    await handle.truncate(outputBytes);
    await handle.close();
    return { stdout: '', stderr: '' };
  });
}

describe('heicToPng', () => {
  let platform: PropertyDescriptor | undefined;

  beforeEach(() => {
    sips.mockReset();
    createdTempDirs.length = 0;
    bytesReadByPath.clear();
    platform = Object.getOwnPropertyDescriptor(process, 'platform');
    Object.defineProperty(process, 'platform', { value: 'darwin', configurable: true });
  });

  afterEach(async () => {
    if (platform) Object.defineProperty(process, 'platform', platform);
    await Promise.all(
      createdTempDirs.map((directory) =>
        removePath(directory, { recursive: true, force: true }).catch(() => {})
      )
    );
  });

  it('returns the converted bytes for an ordinary photo', async () => {
    const { heicToPng } = await loadModule();
    stubSips({ width: 4032, height: 3024, outputBytes: 4096 });

    const png = await heicToPng('/work/photo.heic');

    expect(png).toHaveLength(4096);
    expect(createdTempDirs).toHaveLength(1);
    expect(await exists(createdTempDirs[0])).toBe(false);
  });

  // The blocker this file exists for. `sips` writes to a *file*, so
  // `execFile`'s maxBuffer never applied; an unbounded `readFile` here would
  // pull a multi-gigabyte PNG into the process that owns every window.
  it('refuses an oversized conversion without reading it', async () => {
    const { heicToPng, HEIC_PNG_MAX_BYTES } = await loadModule();
    stubSips({ width: 8_000, height: 4_000, outputBytes: HEIC_PNG_MAX_BYTES + 1 });

    expect(await heicToPng('/work/dense.heic')).toBeNull();

    expect([...bytesReadByPath.values()]).toEqual([0]);
    expect(await exists(createdTempDirs[0])).toBe(false);
  });

  it('refuses a gigapixel source before spawning the converter', async () => {
    const { heicToPng } = await loadModule();
    stubSips({ width: 30_000, height: 30_000, outputBytes: 1024 });

    expect(await heicToPng('/work/bomb.heic')).toBeNull();

    expect(sips).toHaveBeenCalledTimes(1);
    expect(sips.mock.calls[0][0]).toContain('-g');
    expect(createdTempDirs).toHaveLength(0);
  });

  it('bounds the raster a large-but-plausible source may produce', async () => {
    const { heicToPng } = await loadModule();
    stubSips({ width: 15_000, height: 12_000, outputBytes: 2048 });

    await heicToPng('/work/wide.heic');

    const convert = sips.mock.calls[1][0];
    const bound = Number(convert[convert.indexOf('-Z') + 1]);
    expect(bound).toBeGreaterThan(0);
    // Both caps, not just the dimension one: 8192 x 6553 would clear the
    // dimension limit and still be 53 megapixels.
    expect(bound).toBeLessThanOrEqual(8_192);
    expect(bound * Math.round(bound * (12_000 / 15_000))).toBeLessThanOrEqual(32_000_000);
  });

  // `sips -Z` resamples up as well as down, so an unconditional flag would
  // enlarge every ordinary image — a 64x32 source measurably came back
  // 8192x4096 — and push it past the caller's pixel cap.
  it('leaves the resample flag off for a source that already fits', async () => {
    const { heicToPng } = await loadModule();
    stubSips({ width: 640, height: 480, outputBytes: 512 });

    await heicToPng('/work/small.heic');

    expect(sips.mock.calls[1][0]).not.toContain('-Z');
  });

  it('cleans up when the converter fails', async () => {
    const { heicToPng } = await loadModule();
    sips.mockImplementation(async (args) => {
      if (args.includes('-g')) {
        return { stdout: 'image.heic\n  pixelWidth: 100\n  pixelHeight: 100\n', stderr: '' };
      }
      throw new Error('sips: no such file');
    });

    expect(await heicToPng('/work/broken.heic')).toBeNull();

    expect(createdTempDirs).toHaveLength(1);
    expect(await exists(createdTempDirs[0])).toBe(false);
  });

  it('gives up when the metadata probe returns nothing usable', async () => {
    const { heicToPng } = await loadModule();
    sips.mockResolvedValue({ stdout: 'image.heic\n', stderr: '' });

    expect(await heicToPng('/work/unreadable.heic')).toBeNull();
    expect(createdTempDirs).toHaveLength(0);
  });

  it('does not shell out at all off macOS', async () => {
    Object.defineProperty(process, 'platform', { value: 'linux', configurable: true });
    const { heicToPng } = await loadModule();

    expect(await heicToPng('/work/photo.heic')).toBeNull();
    expect(sips).not.toHaveBeenCalled();
  });
});
