import { client } from '../../../api/client.gen';

type ElectronBridge = {
  getBiorouterdHostPort?: () => Promise<string | null>;
  getSecretKey?: () => Promise<string>;
};

function getElectronBridge(): ElectronBridge | undefined {
  return (window as unknown as { electron?: ElectronBridge }).electron;
}

export async function getSecretKey(): Promise<string> {
  const electron = getElectronBridge();
  if (electron?.getSecretKey) {
    try {
      return await electron.getSecretKey();
    } catch {
      return '';
    }
  }
  return '';
}

export async function getBackendBaseUrl(): Promise<string> {
  const electron = getElectronBridge();
  if (electron?.getBiorouterdHostPort) {
    try {
      const baseUrl = await electron.getBiorouterdHostPort();
      if (baseUrl) {
        return baseUrl.replace(/\/$/, '');
      }
    } catch {
      // Fall back to the SDK client config below.
    }
  }

  const cfg = client.getConfig();
  return ((cfg.baseUrl as string | undefined) ?? '').replace(/\/$/, '');
}

export async function buildKnowledgeUrl(path: string): Promise<string> {
  return `${await getBackendBaseUrl()}${path}`;
}

export async function knowledgeFetch(path: string, init: RequestInit = {}): Promise<Response> {
  const headers = new Headers(init.headers ?? {});
  const secret = await getSecretKey();
  if (secret) {
    headers.set('X-Secret-Key', secret);
  }

  return fetch(await buildKnowledgeUrl(path), {
    ...init,
    headers,
  });
}
