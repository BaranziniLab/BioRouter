import { describe, expect, it } from 'vitest';
import {
  ARTIFACT_BROWSER_CSP,
  ARTIFACT_WRAPPER_CSP,
  injectArtifactBrowserCsp,
  injectArtifactHostTheme,
  wrapArtifactForBrowser,
} from './artifactSecurity';

describe('injectArtifactBrowserCsp', () => {
  it('places a restrictive policy inside an existing head', () => {
    const html = injectArtifactBrowserCsp('<html><head><title>Figure</title></head></html>');
    expect(html).toContain(`<head><meta http-equiv="Content-Security-Policy"`);
    expect(html).toContain(ARTIFACT_BROWSER_CSP);
  });

  it('prepends the policy when generated fragments omit a head', () => {
    expect(injectArtifactBrowserCsp('<main>Figure</main>')).toMatch(
      /^<head><meta http-equiv="Content-Security-Policy"/
    );
  });

  it('places the policy before executable content that precedes a late head', () => {
    const secured = injectArtifactBrowserCsp(
      '<script>window.ran=true</script><head><title>Late</title></head>'
    );

    expect(secured.indexOf('Content-Security-Policy')).toBeLessThan(secured.indexOf('<script>'));
  });

  it('does not allow remote scripts, arbitrary network requests, forms, or plugins', () => {
    expect(ARTIFACT_BROWSER_CSP).not.toContain('script-src https:');
    expect(ARTIFACT_BROWSER_CSP).not.toContain('connect-src https:');
    expect(ARTIFACT_BROWSER_CSP).toContain("connect-src 'none'");
    expect(ARTIFACT_BROWSER_CSP).not.toContain('img-src data: blob: https:');
    expect(ARTIFACT_BROWSER_CSP).toContain("frame-src 'none'");
    expect(ARTIFACT_BROWSER_CSP).toContain("navigate-to 'none'");
    expect(ARTIFACT_BROWSER_CSP).toContain("form-action 'none'");
    expect(ARTIFACT_BROWSER_CSP).toContain("object-src 'none'");
  });
});

describe('wrapArtifactForBrowser', () => {
  it('runs generated markup in a sandbox without top-navigation or same-origin access', () => {
    const wrapped = wrapArtifactForBrowser(
      '<script>top.location="https://example.test/?a=1&b=2"</script>'
    );

    expect(wrapped).toContain('&amp;');
    expect(wrapped).toContain('&quot;https://example.test/');
    expect(wrapped).toContain('name="biorouter-artifact-preview"');
    expect(wrapped).toContain('credentialless referrerpolicy="no-referrer"');
    expect(wrapped).toContain('sandbox="allow-scripts allow-downloads"');
    expect(wrapped).not.toContain('allow-top-navigation');
    expect(wrapped).not.toContain('allow-same-origin');
    expect(ARTIFACT_WRAPPER_CSP).toContain("frame-src 'self'");
    expect(ARTIFACT_WRAPPER_CSP).not.toContain("frame-src 'none'");
  });
});

describe('the wrapper policy against the policy it is inherited beside', () => {
  const directives = (csp: string) => csp.split('; ').map((d) => d.trim());

  // The failure this guards produced an empty card and no error message, because
  // a `srcdoc` document enforces its parent's policy as well as its own: a
  // wrapper grant that is *absent* is a guest grant that is *revoked*. Nothing
  // about the wrapper's own markup needs these; they exist so the intersection
  // leaves the figure exactly the policy it was written for.
  it('grants everything the figure inside it is granted', () => {
    const wrapper = new Set(directives(ARTIFACT_WRAPPER_CSP));
    const missing = directives(ARTIFACT_BROWSER_CSP).filter(
      (d) => !wrapper.has(d) && !d.startsWith('frame-src')
    );

    expect(missing).toEqual([]);
  });

  it('differs from it only in frame-src, which the wrapper needs and the figure does not', () => {
    const wrapperOnly = directives(ARTIFACT_WRAPPER_CSP).filter(
      (d) => !directives(ARTIFACT_BROWSER_CSP).includes(d)
    );

    expect(wrapperOnly).toEqual(["frame-src 'self'"]);
  });

  it('still default-denies, and names no remote origin, on either surface', () => {
    // Widening the wrapper to match the figure must not have widened it past
    // the figure: the grants are all `data:`/`blob:`/inline, never a scheme
    // that reaches a network.
    for (const csp of [ARTIFACT_BROWSER_CSP, ARTIFACT_WRAPPER_CSP]) {
      expect(csp).toContain("default-src 'none'");
      expect(csp).toContain("connect-src 'none'");
      expect(csp).not.toMatch(/https?:/);
      expect(csp).not.toContain('*');
    }
  });
});

describe('injectArtifactHostTheme', () => {
  it('handles generated documents whose head tag carries attributes', () => {
    expect(injectArtifactHostTheme('<html><head data-app="viz"></head></html>', 'dark')).toContain(
      '<head data-app="viz"><script>window.__BR_VIZ_HOST_THEME__="dark";</script>'
    );
  });
});
