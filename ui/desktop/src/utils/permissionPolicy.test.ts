import { describe, expect, it } from 'vitest';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import {
  isAllowedArtifactFrameNavigation,
  isAllowedRendererPermission,
  isAppOrigin,
  shouldOpenExternalNavigation,
} from './permissionPolicy';

describe('permissionPolicy', () => {
  it('matches only the configured development renderer origin', () => {
    const appUrl = new URL('http://localhost:5173/');
    expect(isAppOrigin('http://localhost:5173/pair', appUrl)).toBe(true);
    expect(isAppOrigin('http://127.0.0.1:5173/', appUrl)).toBe(false);
    expect(isAppOrigin('https://example.com/', appUrl)).toBe(false);
  });

  it('keeps packaged artifact files outside the renderer directory', () => {
    const rendererDir = path.resolve('test-fixtures', 'packaged-app', 'renderer');
    const appUrl = pathToFileURL(path.join(rendererDir, 'index.html'));
    expect(
      isAppOrigin(pathToFileURL(path.join(rendererDir, 'assets', 'app.js')).href, appUrl)
    ).toBe(true);
    expect(
      isAppOrigin(pathToFileURL(path.resolve('test-fixtures', 'artifact-1.html')).href, appUrl)
    ).toBe(false);
  });

  it('never hands Biorouter itself to the external browser', () => {
    const appUrl = new URL('http://localhost:5174/');
    expect(shouldOpenExternalNavigation('http://localhost:5174/', appUrl)).toBe(false);
    expect(shouldOpenExternalNavigation('http://localhost:5174/pair', appUrl)).toBe(false);
    expect(shouldOpenExternalNavigation('https://example.com/report', appUrl)).toBe(true);
    expect(shouldOpenExternalNavigation('https://user:secret@example.com/', appUrl)).toBe(false);
    expect(
      shouldOpenExternalNavigation(`https://example.com/${'x'.repeat(9 * 1024)}`, appUrl)
    ).toBe(false);
    expect(shouldOpenExternalNavigation('file:///tmp/report.pdf', appUrl)).toBe(false);
    expect(shouldOpenExternalNavigation('not a URL', appUrl)).toBe(false);
  });

  it('keeps artifact frames on srcdoc or the configured HTML proxy', () => {
    expect(isAllowedArtifactFrameNavigation('about:srcdoc')).toBe(true);
    expect(isAllowedArtifactFrameNavigation('about:blank')).toBe(true);
    expect(isAllowedArtifactFrameNavigation('data:text/html,escape')).toBe(false);
    expect(isAllowedArtifactFrameNavigation('blob:https://example.test/id')).toBe(false);
    const proxyBase = 'http://127.0.0.1:8765';
    expect(
      isAllowedArtifactFrameNavigation(
        'http://127.0.0.1:8765/mcp-ui-proxy?contentType=rawhtml&waitForRenderData=true',
        proxyBase
      )
    ).toBe(true);
    expect(isAllowedArtifactFrameNavigation('http://127.0.0.1:8765/mcp-ui-proxy', proxyBase)).toBe(
      false
    );
    expect(
      isAllowedArtifactFrameNavigation(
        'http://127.0.0.1:8765/mcp-ui-proxy?url=https://example.test',
        proxyBase
      )
    ).toBe(false);
    expect(
      isAllowedArtifactFrameNavigation(
        'http://127.0.0.1:8765/mcp-ui-proxy?contentType=rawhtml&secret=leak',
        proxyBase
      )
    ).toBe(false);
    expect(isAllowedArtifactFrameNavigation('http://127.0.0.1:9000/mcp-ui-proxy', proxyBase)).toBe(
      false
    );
    expect(
      isAllowedArtifactFrameNavigation('http://user:secret@127.0.0.1:8765/mcp-ui-proxy', proxyBase)
    ).toBe(false);
    expect(isAllowedArtifactFrameNavigation('http://127.0.0.1:8765/apps/escape/', proxyBase)).toBe(
      false
    );
    expect(isAllowedArtifactFrameNavigation('https://example.test/exfiltrate')).toBe(false);
    expect(isAllowedArtifactFrameNavigation('file:///etc/passwd')).toBe(false);
  });

  it('allows only audio capture requested by Biorouter itself', () => {
    const appUrl = new URL('http://localhost:5173/');
    expect(
      isAllowedRendererPermission('media', 'http://localhost:5173/pair', appUrl, ['audio'])
    ).toBe(true);
    expect(
      isAllowedRendererPermission('media', 'http://localhost:5173/pair', appUrl, ['video'])
    ).toBe(false);
    expect(
      isAllowedRendererPermission('media', 'http://localhost:5173/pair', appUrl, ['audio', 'video'])
    ).toBe(false);
    expect(isAllowedRendererPermission('geolocation', 'http://localhost:5173/', appUrl, [])).toBe(
      false
    );
    expect(
      isAllowedRendererPermission('media', 'file:///tmp/biorouter-artifacts/evil.html', appUrl, [
        'audio',
      ])
    ).toBe(false);
  });
});
