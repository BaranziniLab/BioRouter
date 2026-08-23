import { act, renderHook, waitFor } from '@testing-library/react';
import type React from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useFileDrop } from './useFileDrop';

class MockFileReader {
  onload: ((event: { target: { result: string } }) => void) | null = null;
  onerror: (() => void) | null = null;
  onabort: (() => void) | null = null;

  readAsDataURL() {
    queueMicrotask(() => {
      this.onload?.({ target: { result: 'data:image/png;base64,AAAA' } });
    });
  }

  abort() {
    this.onabort?.();
  }
}

function dropEvent(files: File[], data: Record<string, string> = {}) {
  return {
    preventDefault: vi.fn(),
    stopPropagation: vi.fn(),
    dataTransfer: {
      files,
      getData: (type: string) => data[type] ?? '',
    },
  } as unknown as React.DragEvent<HTMLDivElement>;
}

describe('useFileDrop', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    Object.defineProperty(window, 'electron', {
      configurable: true,
      value: {
        getPathForFile: vi.fn((file: File) => `/tmp/${file.name}`),
        saveDataUrlToTemp: vi.fn(async (_dataUrl: string, id: string) => ({
          id,
          filePath: `/tmp/biorouter-pasted-images/${id}.png`,
        })),
      },
    });
    Object.defineProperty(globalThis, 'FileReader', {
      configurable: true,
      value: MockFileReader,
    });
  });

  it('tracks drag hover state', () => {
    const { result } = renderHook(() => useFileDrop());
    const event = dropEvent([]);

    act(() => result.current.handleDragEnter(event));
    expect(result.current.isDraggingOver).toBe(true);

    act(() => result.current.handleDragLeave(event));
    expect(result.current.isDraggingOver).toBe(false);
  });

  it('stages uploadable images while preserving the original source path', async () => {
    const { result } = renderHook(() => useFileDrop());
    const file = new File(['image'], 'scan.png', { type: 'image/png' });

    await act(async () => {
      const event = dropEvent([file]);
      await result.current.handleDrop(event);
      expect(event.stopPropagation).toHaveBeenCalled();
    });

    expect(result.current.droppedFiles[0]).toMatchObject({
      path: '/tmp/scan.png',
      sourcePath: '/tmp/scan.png',
      canUploadAsImage: true,
    });

    await waitFor(() => {
      expect(result.current.droppedFiles[0]).toMatchObject({
        sourcePath: '/tmp/scan.png',
        stagedPath: expect.stringContaining('/tmp/biorouter-pasted-images/'),
        isLoading: false,
      });
    });
  });

  it('keeps unsupported image types as path references', async () => {
    const { result } = renderHook(() => useFileDrop());
    const file = new File(['image'], 'photo.heic', { type: 'image/heic' });

    await act(async () => {
      await result.current.handleDrop(dropEvent([file]));
    });

    expect(result.current.droppedFiles[0]).toMatchObject({
      path: '/tmp/photo.heic',
      sourcePath: '/tmp/photo.heic',
      isImage: true,
      canUploadAsImage: false,
      isLoading: false,
    });
    expect(window.electron.saveDataUrlToTemp).not.toHaveBeenCalled();
  });

  /// Previously named "falls back to the file name when Electron cannot expose
  /// a path", and it asserted exactly that fallback. The fallback was the bug.
  ///
  /// A bare name is not a safe default, it is a *plausible* one: whatever
  /// eventually opens it resolves it against its own working directory, so the
  /// agent can be handed a different file of the same name and no error is
  /// raised anywhere. Electron returns an empty path for a File that is not
  /// backed by a real file on disk, and guessing a path for those is the same
  /// wrong answer -- merely likelier to be right by accident, since at least
  /// the machine matches.
  it('refuses to invent a path when none can be exposed', async () => {
    vi.mocked(window.electron.getPathForFile).mockReturnValueOnce('');
    const { result } = renderHook(() => useFileDrop());
    const file = new File(['note'], 'note.txt', { type: 'text/plain' });

    await act(async () => {
      await result.current.handleDrop(dropEvent([file]));
    });

    expect(result.current.droppedFiles[0]).toMatchObject({
      path: '',
      sourcePath: undefined,
      name: 'note.txt',
      isImage: false,
      canUploadAsImage: false,
    });
    expect(result.current.droppedFiles[0].error).toBeTruthy();
  });

  it('adds file URI-list paths when drops do not expose File objects', async () => {
    const { result } = renderHook(() => useFileDrop());

    await act(async () => {
      await result.current.handleDrop(
        dropEvent([], { 'text/uri-list': 'file:///Users/wgu/Desktop/Folder%20A\n' })
      );
    });

    expect(result.current.droppedFiles[0]).toMatchObject({
      path: '/Users/wgu/Desktop/Folder A',
      sourcePath: '/Users/wgu/Desktop/Folder A',
      name: 'Folder A',
      isImage: false,
      canUploadAsImage: false,
    });
  });
});

/// Browser mode: the surface cannot supply a filesystem path, and the failure
/// this guards against is silent rather than loud.
///
/// Every assertion here fails against the previous implementation, which read
/// `sourcePath || file.name`. That fallback produced a *plausible* path — the
/// bare file name — which the daemon then resolved against its OWN working
/// directory on a different machine. Dropping `results.csv` could hand the
/// agent an unrelated `results.csv`, and nothing reported anything.
describe('useFileDrop when the surface has no filesystem paths', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    Object.defineProperty(window, 'electron', {
      configurable: true,
      value: {
        // What the browser shim returns: the "no path" sentinel.
        getPathForFile: vi.fn(() => ''),
        saveDataUrlToTemp: vi.fn(async (dataUrl: string, id: string) => ({
          id,
          filePath: dataUrl,
        })),
      },
    });
    Object.defineProperty(globalThis, 'FileReader', {
      configurable: true,
      value: MockFileReader,
    });
  });

  it('never invents a path from the file name', async () => {
    const { result } = renderHook(() => useFileDrop());
    const file = new File(['a,b\n1,2'], 'results.csv', { type: 'text/csv' });

    await act(async () => {
      await result.current.handleDrop(dropEvent([file]));
    });

    const dropped = result.current.droppedFiles[0];
    expect(dropped).toBeDefined();
    // The old code set this to 'results.csv'. That is the bug: it names a real
    // file on the server often enough to be dangerous.
    expect(dropped.path).toBe('');
    expect(dropped.sourcePath).toBeUndefined();
  });

  it('says so, rather than failing silently', async () => {
    const { result } = renderHook(() => useFileDrop());
    const file = new File(['x'], 'notes.txt', { type: 'text/plain' });

    await act(async () => {
      await result.current.handleDrop(dropEvent([file]));
    });

    const dropped = result.current.droppedFiles[0];
    expect(dropped.error).toBeTruthy();
    expect(dropped.error).toMatch(/another machine/i);
    expect(dropped.isLoading).toBe(false);
  });

  /// Images must keep working: they are read as data URLs and never need a
  /// path, so a fix that refused everything without a path would break the one
  /// drop that browser mode can actually honour.
  it('still accepts an image, which needs no path at all', async () => {
    const { result } = renderHook(() => useFileDrop());
    const image = new File(['png'], 'figure.png', { type: 'image/png' });

    await act(async () => {
      await result.current.handleDrop(dropEvent([image]));
    });

    const dropped = result.current.droppedFiles[0];
    expect(dropped.error).toBeUndefined();
    expect(dropped.canUploadAsImage).toBe(true);
    await waitFor(() => {
      expect(result.current.droppedFiles[0].stagedPath).toBeTruthy();
    });
  });
});
