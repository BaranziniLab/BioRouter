import { describe, expect, it } from 'vitest';
import {
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
    const appUrl = new URL(
      'file:///Applications/Biorouter.app/Contents/Resources/renderer/index.html'
    );
    expect(
      isAppOrigin(
        'file:///Applications/Biorouter.app/Contents/Resources/renderer/assets/app.js',
        appUrl
      )
    ).toBe(true);
    expect(isAppOrigin('file:///tmp/biorouter-artifacts/artifact-1.html', appUrl)).toBe(false);
  });

  it('never hands Biorouter itself to the external browser', () => {
    const appUrl = new URL('http://localhost:5174/');
    expect(shouldOpenExternalNavigation('http://localhost:5174/', appUrl)).toBe(false);
    expect(shouldOpenExternalNavigation('http://localhost:5174/pair', appUrl)).toBe(false);
    expect(shouldOpenExternalNavigation('https://example.com/report', appUrl)).toBe(true);
    expect(shouldOpenExternalNavigation('file:///tmp/report.pdf', appUrl)).toBe(false);
    expect(shouldOpenExternalNavigation('not a URL', appUrl)).toBe(false);
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
