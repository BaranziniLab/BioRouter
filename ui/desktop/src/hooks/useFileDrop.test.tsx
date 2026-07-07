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

  it('falls back to the file name when Electron cannot expose a path', async () => {
    vi.mocked(window.electron.getPathForFile).mockReturnValueOnce('');
    const { result } = renderHook(() => useFileDrop());
    const file = new File(['note'], 'note.txt', { type: 'text/plain' });

    await act(async () => {
      await result.current.handleDrop(dropEvent([file]));
    });

    expect(result.current.droppedFiles[0]).toMatchObject({
      path: 'note.txt',
      sourcePath: undefined,
      name: 'note.txt',
      isImage: false,
      canUploadAsImage: false,
    });
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
