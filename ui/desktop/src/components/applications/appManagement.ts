import { client } from '../../api/client.gen';

export function configuredBaseUrl(): string {
  const cfg = client.getConfig();
  return ((cfg.baseUrl as string) || '').replace(/\/+$/, '');
}

export async function resolveBaseUrl(): Promise<string> {
  const configured = configuredBaseUrl();
  if (configured) return configured;
  return ((await window.electron.getBiorouterdHostPort()) || '').replace(/\/+$/, '');
}

export function appUrl(id: string, baseUrl = configuredBaseUrl()): string {
  return `${baseUrl}/apps/${encodeURIComponent(id)}/`;
}

export async function secretHeader(): Promise<Record<string, string>> {
  try {
    const key = await window.electron.getSecretKey();
    if (key) return { 'X-Secret-Key': key };
  } catch {
    // Fall back to the generated client config below.
  }

  const cfg = client.getConfig() as {
    headers?: Record<string, string> | { get: (name: string) => string | null };
  };
  const headers = cfg.headers;
  if (headers && typeof headers.get === 'function') {
    const key = headers.get('X-Secret-Key');
    return key ? { 'X-Secret-Key': key } : {};
  }
  const headerRecord = headers as Record<string, string> | undefined;
  const key = headerRecord?.['X-Secret-Key'] ?? headerRecord?.['x-secret-key'];
  return key ? { 'X-Secret-Key': key } : {};
}

export async function requireOk(res: Response): Promise<void> {
  if (res.ok) return;
  const detail = await res.text().catch(() => '');
  throw new Error(`HTTP ${res.status}${detail ? `: ${detail}` : ''}`);
}

export async function deleteAgentDrafterApp(id: string): Promise<void> {
  const baseUrl = await resolveBaseUrl();
  if (!baseUrl) {
    throw new Error('Backend URL unavailable. Is biorouterd running?');
  }
  const res = await fetch(`${baseUrl}/apps/${encodeURIComponent(id)}`, {
    method: 'DELETE',
    headers: await secretHeader(),
  });
  await requireOk(res);
}
