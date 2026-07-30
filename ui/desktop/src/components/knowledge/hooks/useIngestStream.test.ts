import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useIngestStream } from './useIngestStream';

vi.mock('./knowledgeRequest', () => ({
  buildKnowledgeUrl: async (path: string) => `http://backend.test${path}`,
  getSecretKey: async () => 'secret-123',
}));

/** A `fetch` that replies 200 with the given SSE text, delivered in one chunk. */
function sseResponse(body: string): Response {
  const encoder = new TextEncoder();
  const stream = new ReadableStream<Uint8Array>({
    start(controller) {
      controller.enqueue(encoder.encode(body));
      controller.close();
    },
  });
  // jsdom's Response does not accept a stream body, so hand the hook the two
  // fields it actually reads.
  return { ok: true, status: 200, body: stream } as unknown as Response;
}

describe('useIngestStream terminal frames', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('reports a backend error frame as an error, with its message', async () => {
    vi.stubGlobal(
      'fetch',
      vi
        .fn()
        .mockResolvedValue(
          sseResponse(
            'data: {"kind":"step","index":0,"assistant_text":""}\n\n' +
              'event: error\ndata: {"message":"ingest wrote no knowledge pages for source hrv-note"}\n\n'
          )
        )
    );

    const { result } = renderHook(() => useIngestStream());

    let run!: Awaited<ReturnType<typeof result.current.start>>;
    await act(async () => {
      run = await result.current.start('/knowledge/bases/kb/ingest', {});
    });

    expect(run.status).toBe('error');
    expect(run.error).toContain('no knowledge pages');
    expect(result.current.status).toBe('error');
    expect(result.current.error).toContain('no knowledge pages');
  });

  it('reports a done frame as done', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(sseResponse('event: done\ndata: {"source_id":"hrv","steps":3}\n\n'))
    );

    const { result } = renderHook(() => useIngestStream());

    let run!: Awaited<ReturnType<typeof result.current.start>>;
    await act(async () => {
      run = await result.current.start('/knowledge/bases/kb/ingest', {});
    });

    expect(run.status).toBe('done');
    expect(result.current.status).toBe('done');
  });

  // Issue #71. The backend closes the stream with either `event: done` or
  // `event: error`; a body that just stops means the digest died on the way —
  // the daemon crashed, the socket dropped, a proxy cut it. Defaulting that to
  // "done" told the user a digest had completed when nothing had come back to
  // say so, and `IngestPanel` then marked the source ingested and cleared it off
  // the staged list.
  it('treats a stream that ends without a terminal frame as a failure', async () => {
    vi.stubGlobal(
      'fetch',
      vi
        .fn()
        .mockResolvedValue(
          sseResponse(
            'data: {"kind":"step","index":0,"assistant_text":"reading the source"}\n\n' +
              'data: {"kind":"tool_call","name":"kb_read_page","args":{}}\n\n'
          )
        )
    );

    const { result } = renderHook(() => useIngestStream());

    let run!: Awaited<ReturnType<typeof result.current.start>>;
    await act(async () => {
      run = await result.current.start('/knowledge/bases/kb/ingest', {});
    });

    expect(run.status).toBe('error');
    expect(result.current.status).toBe('error');
    expect(result.current.error ?? '').not.toBe('');
  });

  it('treats an entirely empty stream as a failure too', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(sseResponse('')));

    const { result } = renderHook(() => useIngestStream());

    let run!: Awaited<ReturnType<typeof result.current.start>>;
    await act(async () => {
      run = await result.current.start('/knowledge/bases/kb/ingest', {});
    });

    expect(run.status).toBe('error');
    expect(result.current.status).toBe('error');
  });
});
