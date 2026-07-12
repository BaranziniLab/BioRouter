import { describe, expect, it } from 'vitest';
import {
  ARTIFACT_BROWSER_CSP,
  injectArtifactBrowserCsp,
  injectArtifactHostTheme,
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

  it('does not allow remote scripts, arbitrary network requests, forms, or plugins', () => {
    expect(ARTIFACT_BROWSER_CSP).not.toContain('script-src https:');
    expect(ARTIFACT_BROWSER_CSP).not.toContain('connect-src https:');
    expect(ARTIFACT_BROWSER_CSP).toContain("form-action 'none'");
    expect(ARTIFACT_BROWSER_CSP).toContain("object-src 'none'");
  });
});

describe('injectArtifactHostTheme', () => {
  it('handles generated documents whose head tag carries attributes', () => {
    expect(injectArtifactHostTheme('<html><head data-app="viz"></head></html>', 'dark')).toContain(
      '<head data-app="viz"><script>window.__BR_VIZ_HOST_THEME__="dark";</script>'
    );
  });
});
