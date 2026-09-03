import { afterEach, describe, expect, it, vi } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import {
  fileLinkChecksSupported,
  isOpenableFileLink,
  resetFileLinkStatusForTests,
  useFileLinkExistence,
  type FilePathCheckRequest,
  type FilePathCheckResult,
} from './fileLinkStatus';

/** Install a `window.electron` carrying only the existence bridge. */
function installCheckBridge(
  answer: (request: FilePathCheckRequest) => FilePathCheckResult = () => ({
    exists: true,
    isDirectory: false,
  })
) {
  const checkFilePaths = vi.fn(async (requests: FilePathCheckRequest[]) => requests.map(answer));
  Object.defineProperty(window, 'electron', {
    configurable: true,
    value: { checkFilePaths },
  });
  return checkFilePaths;
}

afterEach(() => {
  // @ts-expect-error — remove the per-test electron stub.
  delete window.electron;
  resetFileLinkStatusForTests();
  vi.restoreAllMocks();
});

describe('fileLinkStatus', () => {
  describe('without an Electron bridge', () => {
    it('reports the check unsupported and keeps every path openable', () => {
      expect(fileLinkChecksSupported()).toBe(false);

      const { result } = renderHook(() => useFileLinkExistence('/work/analysis.sql'));

      // The pre-existing contract on the browser surface (`biorouter serve`)
      // and in every suite that renders without a bridge: link everything.
      expect(result.current).toBe('unchecked');
      expect(isOpenableFileLink(result.current)).toBe(true);
    });

    // A bridge that is merely PRESENT is not a bridge that can answer. Each of
    // these reaches `checkBridge`'s `typeof === 'function'` guard by a different
    // route; probing only the absent key would leave that guard untested.
    it.each([
      ['no such key', { readArtifactFile: vi.fn() }],
      ['an explicitly null key', { checkFilePaths: null }],
      ['a key that is not callable', { checkFilePaths: 'not-a-function' }],
    ])('reports unsupported for a bridge with %s', (_label, electron) => {
      Object.defineProperty(window, 'electron', { configurable: true, value: electron });

      expect(fileLinkChecksSupported()).toBe(false);
    });
  });

  describe('with an Electron bridge', () => {
    it('starts unopenable and upgrades to a link only once existence is confirmed', async () => {
      installCheckBridge();

      const { result } = renderHook(() => useFileLinkExistence('/work/analysis.sql'));

      // The first frame must not be clickable — an orange link that greys out a
      // tick later is the reported bug on a shorter timescale.
      expect(result.current).toBe('checking');
      expect(isOpenableFileLink(result.current)).toBe(false);

      await waitFor(() => expect(result.current).toBe('present'));
      expect(isOpenableFileLink(result.current)).toBe(true);
    });

    it('reports a path the main process cannot find as missing, and never openable', async () => {
      installCheckBridge(() => ({ exists: false, isDirectory: false }));

      const { result } = renderHook(() => useFileLinkExistence('/work/imagined.py'));

      await waitFor(() => expect(result.current).toBe('missing'));
      expect(isOpenableFileLink(result.current)).toBe(false);
    });

    it('reports nothing for a link with no path, so an external URL stays clickable', () => {
      const checkFilePaths = installCheckBridge();

      const { result } = renderHook(() => useFileLinkExistence(null));

      expect(result.current).toBe('unchecked');
      expect(checkFilePaths).not.toHaveBeenCalled();
    });
  });

  describe('batching', () => {
    it('collapses repeated mentions of one path into a single request', async () => {
      const checkFilePaths = installCheckBridge();

      const { result } = renderHook(() => ({
        first: useFileLinkExistence('/work/analysis.sql'),
        second: useFileLinkExistence('/work/analysis.sql'),
        third: useFileLinkExistence('/work/analysis.sql'),
      }));

      await waitFor(() => expect(result.current.first).toBe('present'));
      expect(result.current.second).toBe('present');
      expect(result.current.third).toBe('present');
      expect(checkFilePaths).toHaveBeenCalledTimes(1);
      expect(checkFilePaths.mock.calls[0][0]).toEqual([{ path: '/work/analysis.sql' }]);
    });

    it('sends every distinct path of one message in ONE round trip', async () => {
      const checkFilePaths = installCheckBridge((request) => ({
        exists: request.path.endsWith('.sql'),
        isDirectory: false,
      }));

      const { result } = renderHook(() => ({
        sql: useFileLinkExistence('/work/analysis.sql'),
        py: useFileLinkExistence('/work/imagined.py'),
        md: useFileLinkExistence('/work/notes.md'),
      }));

      await waitFor(() => expect(result.current.sql).toBe('present'));
      expect(result.current.py).toBe('missing');
      expect(result.current.md).toBe('missing');
      expect(checkFilePaths).toHaveBeenCalledTimes(1);
      expect(checkFilePaths.mock.calls[0][0]).toHaveLength(3);
    });

    it('answers a freshly mounted link from cache rather than asking again', async () => {
      const checkFilePaths = installCheckBridge();

      const first = renderHook(() => useFileLinkExistence('/work/analysis.sql'));
      await waitFor(() => expect(first.result.current).toBe('present'));
      first.unmount();

      // A NEW mount runs the effect again, so only the recorded answer stops a
      // second round trip — the queue's own keying cannot, it is empty by now.
      const second = renderHook(() => useFileLinkExistence('/work/analysis.sql'));
      await waitFor(() => expect(second.result.current).toBe('present'));
      expect(checkFilePaths).toHaveBeenCalledTimes(1);
    });

    it('does not re-ask for a path whose check is still in flight', async () => {
      let release: ((results: FilePathCheckResult[]) => void) | undefined;
      const checkFilePaths = vi.fn(
        () =>
          new Promise<FilePathCheckResult[]>((resolve) => {
            release = resolve;
          })
      );
      Object.defineProperty(window, 'electron', { configurable: true, value: { checkFilePaths } });

      const first = renderHook(() => useFileLinkExistence('/work/analysis.sql'));
      await waitFor(() => expect(checkFilePaths).toHaveBeenCalledTimes(1));

      // The queue has been drained but no answer is recorded yet: this window
      // is covered by `inFlight` and by nothing else.
      const second = renderHook(() => useFileLinkExistence('/work/analysis.sql'));
      await Promise.resolve();
      expect(checkFilePaths).toHaveBeenCalledTimes(1);

      release?.([{ exists: true, isDirectory: false }]);
      await waitFor(() => expect(second.result.current).toBe('present'));
      first.unmount();
    });

    it('keys the cache by working directory, so one relative name is not two files', async () => {
      const checkFilePaths = installCheckBridge((request) => ({
        exists: request.workingDir === '/work/a',
        isDirectory: false,
      }));

      const { result } = renderHook(() => ({
        a: useFileLinkExistence('report.md', '/work/a'),
        b: useFileLinkExistence('report.md', '/work/b'),
      }));

      await waitFor(() => expect(result.current.a).toBe('present'));
      expect(result.current.b).toBe('missing');
      expect(checkFilePaths.mock.calls[0][0]).toEqual([
        { path: 'report.md', workingDir: '/work/a' },
        { path: 'report.md', workingDir: '/work/b' },
      ]);
    });
  });

  describe('when the check itself fails', () => {
    it('falls back to legacy behaviour instead of de-linking real files', async () => {
      const checkFilePaths = vi.fn(async () => {
        throw new Error('bridge exploded');
      });
      Object.defineProperty(window, 'electron', {
        configurable: true,
        value: { checkFilePaths },
      });

      const { result, rerender } = renderHook(() => useFileLinkExistence('/work/analysis.sql'));

      // A rejected call teaches us NOTHING — the same knowledge state as having
      // no bridge — so it takes the same verdict and stays clickable. Recording
      // `missing` here would let one transient hiccup silently strip every real
      // file in the transcript of its link, which is this module's own bug
      // pointed the other way.
      await waitFor(() => expect(result.current).toBe('unchecked'));
      expect(isOpenableFileLink(result.current)).toBe(true);

      // Still a RECORDED verdict, or every re-render would re-queue it.
      rerender();
      await waitFor(() => expect(checkFilePaths).toHaveBeenCalledTimes(1));
    });

    it('treats a short answer as unresolved instead of shifting every verdict', async () => {
      const checkFilePaths = vi.fn(async () => [{ exists: true, isDirectory: false }]);
      Object.defineProperty(window, 'electron', {
        configurable: true,
        value: { checkFilePaths },
      });

      const { result } = renderHook(() => ({
        first: useFileLinkExistence('/work/one.sql'),
        second: useFileLinkExistence('/work/two.sql'),
      }));

      // The answer covers index 0 only. Index 1 was not answered at all, which
      // is not the same as being answered "no" — the assertion that matters is
      // that the FIRST path's verdict did not slide onto the second.
      await waitFor(() => expect(result.current.first).toBe('present'));
      expect(result.current.second).toBe('unchecked');
    });
  });
});

describe('a path named before it exists', () => {
  /**
   * The commonest sequence there is, and the one the first version of this
   * cache got wrong.
   *
   * The assistant streams prose that NAMES a path, and only then calls the tool
   * that creates it. The link mounts during the prose, the bridge truthfully
   * answers "no", and — when a verdict was cached permanently — `requestCheck`
   * returned early for that key forever after. The file appeared a second later
   * and the link stayed grey and inert for the life of the renderer, on a file
   * the panel could open perfectly well. That is the module's own contract
   * ("a link is only a link when the panel could open it") failing open in the
   * unhelpful direction.
   *
   * `missing` is therefore provisional and `present` is not, and something has
   * to actually re-ask — expiring the entry alone would leave the stale answer
   * on screen until some unrelated link happened to mount.
   */
  it('becomes a link once the file the agent promised actually appears', async () => {
    vi.useFakeTimers();
    let exists = false;
    const checkFilePaths = vi.fn(async (reqs: { path: string }[]) =>
      reqs.map(() => ({ exists, isDirectory: false }))
    );
    Object.defineProperty(window, 'electron', { configurable: true, value: { checkFilePaths } });

    const { result } = renderHook(() => useFileLinkExistence('/work/promised.py'));

    // The prose mentioned it; the tool has not run yet.
    await vi.waitFor(() => expect(result.current).toBe('missing'));
    expect(isOpenableFileLink(result.current)).toBe(false);

    // …the tool runs.
    exists = true;
    await vi.advanceTimersByTimeAsync(2500);

    await vi.waitFor(() => expect(result.current).toBe('present'));
    expect(isOpenableFileLink(result.current)).toBe(true);
    expect(checkFilePaths.mock.calls.length).toBeGreaterThan(1);
    vi.useRealTimers();
  });

  it('does not re-ask about a file it has already found', async () => {
    vi.useFakeTimers();
    const checkFilePaths = vi.fn(async (reqs: { path: string }[]) =>
      reqs.map(() => ({ exists: true, isDirectory: false }))
    );
    Object.defineProperty(window, 'electron', { configurable: true, value: { checkFilePaths } });

    const { result } = renderHook(() => useFileLinkExistence('/work/settled.py'));
    await vi.waitFor(() => expect(result.current).toBe('present'));

    // `present` is final — a file does not stop existing mid-session, and
    // polling every extant link forever would be a cost with no payoff.
    await vi.advanceTimersByTimeAsync(10_000);
    expect(checkFilePaths).toHaveBeenCalledTimes(1);
    vi.useRealTimers();
  });
});
