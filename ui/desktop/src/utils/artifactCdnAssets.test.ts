import { describe, expect, it, vi } from 'vitest';
import {
  ARTIFACT_CDN_ASSETS,
  artifactCdnScriptPattern,
  inlineArtifactCdnAssets,
} from './artifactCdnAssets';

const MERMAID = 'https://cdn.jsdelivr.net/npm/mermaid@11/dist/mermaid.min.js';
const LEAFLET_CSS = 'https://cdn.jsdelivr.net/npm/leaflet@1.9.4/dist/leaflet.css';

describe('artifactCdnAssets', () => {
  it('covers Mermaid, which the artifact CSP would otherwise block outright', () => {
    expect(ARTIFACT_CDN_ASSETS).toContain(MERMAID);
  });

  it('replaces a CDN script tag with the library source, inlined', async () => {
    const html = `<head><script src="${MERMAID}" crossorigin="anonymous"></script></head>`;
    const out = await inlineArtifactCdnAssets(html, async () => 'globalThis.mermaid={};');

    expect(out).toBe('<head><script>globalThis.mermaid={};</script></head>');
    expect(out).not.toContain('cdn.jsdelivr.net');
  });

  it('does not expand $-sequences in a minified bundle', async () => {
    const html = `<head><script src="${MERMAID}"></script></head>`;
    const source = `var a="$&",b="$'",c="$1",d="$$";`;
    expect(await inlineArtifactCdnAssets(html, async () => source)).toContain(source);
  });

  it('inlines a stylesheet as a style element', async () => {
    const html = `<head><link rel="stylesheet" href="${LEAFLET_CSS}"/></head>`;
    expect(await inlineArtifactCdnAssets(html, async () => '.leaflet{}')).toBe(
      '<head><style>.leaflet{}</style></head>'
    );
  });

  it('leaves the document alone when a fetch fails, and reports it', async () => {
    const html = `<head><script src="${MERMAID}"></script></head>`;
    const onError = vi.fn();
    const out = await inlineArtifactCdnAssets(
      html,
      async () => {
        throw new Error('offline');
      },
      onError
    );

    expect(out).toBe(html);
    expect(onError).toHaveBeenCalledWith(MERMAID, expect.any(Error));
  });

  it('cannot rewrite an ESM import, which is why assets must be emitted as src tags', async () => {
    // Not a wish for future support: the replacement produces a *classic*
    // script, so module source spliced into it would be a syntax error. The Rust
    // side is what has to emit a `src=` tag — asserted in
    // `crates/biorouter-mcp/tests/autovis_cdn_desktop_contract.rs`.
    const esm = `<head><script type="module">import mermaid from '${MERMAID}/+esm';</script></head>`;
    expect(artifactCdnScriptPattern(MERMAID).test(esm)).toBe(false);
    expect(await inlineArtifactCdnAssets(esm, async () => 'globalThis.mermaid={};')).toBe(esm);
  });
});
