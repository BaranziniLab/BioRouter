export const DEFAULT_IFRAME_HEIGHT = 200;

/**
 * Fetch the MCP App proxy URL from the Electron backend.
 * The proxy enforces CSP as a security boundary for sandboxed apps.
 * TODO(Douwe): make this work better with the generated API rather than poking around
 *
 * ⚠ The daemon secret must never appear in this URL. The proxy document embeds
 * untrusted extension HTML, and anything in `location.search` there is one
 * `parent.location.search` away from the guest — measured in Chromium: with the
 * guest frame carrying `allow-same-origin`, a guest script read the whole query
 * string, the full URL out of the parent's navigation timing entry, and the CSP
 * nonce out of the parent's DOM. So the page is gated by a short-lived **proxy token**
 * minted over the authenticated `POST /mcp-app-proxy/token`, which authorises
 * exactly one thing: being served that static sandbox document.
 */
export async function fetchMcpAppProxyUrl(
  csp?: {
    connectDomains?: string[] | null;
    resourceDomains?: string[] | null;
    frameDomains?: string[] | null;
    baseUriDomains?: string[] | null;
  } | null
): Promise<string | null> {
  try {
    const baseUrl = await window.electron.getBiorouterdHostPort();
    const secretKey = await window.electron.getSecretKey();
    if (!baseUrl || !secretKey) {
      console.error('Failed to get biorouterd host/port or secret key');
      return null;
    }

    // The secret travels in a header, on a request whose response the renderer
    // keeps — never in a URL a sandboxed document can read back.
    const response = await fetch(`${baseUrl}/mcp-app-proxy/token`, {
      method: 'POST',
      headers: { 'X-Secret-Key': secretKey },
    });
    if (!response.ok) {
      console.error(`Failed to mint MCP App proxy token: HTTP ${response.status}`);
      return null;
    }
    const { token } = (await response.json()) as { token?: string };
    if (!token) {
      console.error('MCP App proxy token response carried no token');
      return null;
    }

    const params = new URLSearchParams();
    params.set('t', token);

    if (csp?.connectDomains?.length) {
      params.set('connect_domains', csp.connectDomains.join(','));
    }
    if (csp?.resourceDomains?.length) {
      params.set('resource_domains', csp.resourceDomains.join(','));
    }
    if (csp?.frameDomains?.length) {
      params.set('frame_domains', csp.frameDomains.join(','));
    }
    if (csp?.baseUriDomains?.length) {
      params.set('base_uri_domains', csp.baseUriDomains.join(','));
    }

    return `${baseUrl}/mcp-app-proxy?${params.toString()}`;
  } catch (error) {
    console.error('Error fetching MCP App Proxy URL:', error);
    return null;
  }
}
