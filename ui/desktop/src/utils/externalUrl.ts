const MAX_EXTERNAL_URL_LENGTH = 8 * 1024;

/** Validate and normalize an external web URL before handing it to Electron. */
export function normalizeExternalHttpUrl(rawUrl: unknown): string {
  if (typeof rawUrl !== 'string' || rawUrl.length > MAX_EXTERNAL_URL_LENGTH) {
    throw new Error('Blocked: invalid or oversized URL');
  }

  const parsed = new URL(rawUrl);
  if (
    (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') ||
    parsed.username !== '' ||
    parsed.password !== ''
  ) {
    throw new Error(`Blocked: unsafe URL protocol '${parsed.protocol}'`);
  }
  return parsed.href;
}
