import { isIP } from 'node:net';
import { assertPublicEmbeddedUrl } from './embeddedBrowserPolicy';
import { normalizeExternalHttpUrl } from './externalUrl';

type PublicUrlValidator = (candidate: string) => Promise<URL>;

function unbracketedHostname(hostname: string): string {
  return hostname.replace(/^\[|\]$/g, '');
}

/**
 * Validate a target before asking the system browser to open it.
 *
 * The system browser performs its own DNS lookup, so the app cannot pin that
 * browser to the public address checked here. Literal IPs are refused and a
 * caller must pair this check with native confirmation naming the exact host.
 */
export async function validateExternalBrowserTarget(
  candidate: unknown,
  validatePublic: PublicUrlValidator = assertPublicEmbeddedUrl
): Promise<URL> {
  const parsed = new URL(normalizeExternalHttpUrl(candidate));
  if (isIP(unbracketedHostname(parsed.hostname)) !== 0) {
    throw new Error('Literal IP addresses cannot be opened in the system browser.');
  }

  const validated = await validatePublic(parsed.href);
  if (validated.href !== parsed.href || validated.hostname !== parsed.hostname) {
    throw new Error('External browser validation changed the requested target.');
  }
  return parsed;
}
