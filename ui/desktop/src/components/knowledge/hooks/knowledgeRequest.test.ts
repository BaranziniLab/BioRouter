import { beforeEach, describe, expect, it, vi } from 'vitest';
import { client } from '../../../api/client.gen';
import { buildKnowledgeUrl, getBackendBaseUrl, knowledgeFetch } from './knowledgeRequest';

describe('knowledgeRequest', () => {
  beforeEach(() => {
    client.setConfig({ baseUrl: 'http://client-config.test' });
    vi.restoreAllMocks();
    Object.assign(window, {
      electron: {
        getBiorouterdHostPort: vi.fn().mockResolvedValue('http://electron-host.test/'),
        getSecretKey: vi.fn().mockResolvedValue('secret-123'),
      },
    });
  });

  it('prefers the Electron backend host for knowledge requests', async () => {
    await expect(getBackendBaseUrl()).resolves.toBe('http://electron-host.test');
    await expect(buildKnowledgeUrl('/knowledge/bases/demo/ingest')).resolves.toBe(
      'http://electron-host.test/knowledge/bases/demo/ingest',
    );
  });

  it('falls back to the SDK client base url when the Electron bridge is unavailable', async () => {
    Object.assign(window, { electron: undefined });
    await expect(getBackendBaseUrl()).resolves.toBe('http://client-config.test');
  });

  it('adds the secret header without forcing a JSON content type', async () => {
    const fetchMock = vi.fn().mockResolvedValue(new Response(null, { status: 200 }));
    vi.stubGlobal('fetch', fetchMock);

    await knowledgeFetch('/knowledge/bases/demo/ingest', {
      method: 'POST',
      body: new FormData(),
    });

    expect(fetchMock).toHaveBeenCalledOnce();
    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe('http://electron-host.test/knowledge/bases/demo/ingest');

    const headers = new Headers(init.headers);
    expect(headers.get('X-Secret-Key')).toBe('secret-123');
    expect(headers.has('Content-Type')).toBe(false);
  });
});
