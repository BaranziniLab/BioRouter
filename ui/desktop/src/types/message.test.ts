import { describe, it, expect, vi, beforeEach } from 'vitest';
import { createUserMessage } from './message';

describe('createUserMessage', () => {
  beforeEach(() => {
    (globalThis as unknown as { window: unknown }).window = {
      electron: {
        readTempImageAsBase64: vi.fn(async (p: string) => ({
          data: `B64-${p}`,
          mimeType: p.endsWith('.jpg') ? 'image/jpeg' : 'image/png',
        })),
      },
    };
  });

  it('text only produces a single text content block', async () => {
    const msg = await createUserMessage('hello world');
    expect(msg.content).toEqual([{ type: 'text', text: 'hello world' }]);
    expect(msg.role).toBe('user');
  });

  it('text + 1 image produces text + image blocks', async () => {
    const msg = await createUserMessage('describe this', [
      { path: '/tmp/biorouter-images/foo.png', kind: 'image' },
    ]);
    expect(msg.content).toHaveLength(2);
    expect(msg.content[0]).toEqual({ type: 'text', text: 'describe this' });
    expect(msg.content[1]).toMatchObject({
      type: 'image',
      data: 'B64-/tmp/biorouter-images/foo.png',
      mimeType: 'image/png',
    });
  });

  it('text + 3 images preserves order', async () => {
    const msg = await createUserMessage('compare', [
      { path: '/tmp/biorouter-images/a.png', kind: 'image' },
      { path: '/tmp/biorouter-images/b.jpg', kind: 'image' },
      { path: '/tmp/biorouter-images/c.png', kind: 'image' },
    ]);
    expect(msg.content).toHaveLength(4);
    expect(msg.content[0]).toEqual({ type: 'text', text: 'compare' });
    expect(msg.content[1]).toMatchObject({ type: 'image', mimeType: 'image/png' });
    expect(msg.content[2]).toMatchObject({ type: 'image', mimeType: 'image/jpeg' });
    expect(msg.content[3]).toMatchObject({ type: 'image', mimeType: 'image/png' });
  });

  it('empty text + 1 image omits the text block', async () => {
    const msg = await createUserMessage('', [
      { path: '/tmp/biorouter-images/foo.png', kind: 'image' },
    ]);
    expect(msg.content).toEqual([
      {
        type: 'image',
        data: 'B64-/tmp/biorouter-images/foo.png',
        mimeType: 'image/png',
      },
    ]);
  });

  it('throws if an image read fails', async () => {
    const mockFn = (
      window as unknown as { electron: { readTempImageAsBase64: ReturnType<typeof vi.fn> } }
    ).electron.readTempImageAsBase64;
    mockFn.mockRejectedValueOnce(new Error('boom'));
    await expect(
      createUserMessage('hi', [{ path: '/tmp/biorouter-images/broken.png', kind: 'image' }])
    ).rejects.toThrow(/boom/);
  });
});
