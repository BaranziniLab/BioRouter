import { afterEach, describe, expect, it } from 'vitest';
import {
  PREVIEW_ACTIVITY_IDLE,
  PREVIEW_ACTIVITY_INSTALL,
  withPreviewActivityTracking,
} from './previewActivity';

const evaluate = (source: string) =>
  new Function('window', 'document', `return ${source}`)(window, document);
afterEach(() => {
  evaluate(`window[Symbol.for('biorouter.preview.activity.v1')]?.dispose()`);
  Reflect.deleteProperty(window, 'BioRouter');
  document.body.replaceChildren();
});

describe('value-free preview activity', () => {
  it('fails closed until tracking is installed, then allows untouched idle documents', () => {
    expect(evaluate(PREVIEW_ACTIVITY_IDLE)).toBe(false);
    evaluate(PREVIEW_ACTIVITY_INSTALL);
    expect(evaluate(PREVIEW_ACTIVITY_IDLE)).toBe(true);
  });
  it('latches dirty inputs even after blur, without copying their value', () => {
    evaluate(PREVIEW_ACTIVITY_INSTALL);
    const input = document.createElement('input');
    document.body.append(input);
    input.focus();
    input.value = 'synthetic unsaved draft';
    input.dispatchEvent(new Event('input', { bubbles: true }));
    input.blur();
    expect(evaluate(PREVIEW_ACTIVITY_IDLE)).toBe(false);
    expect(PREVIEW_ACTIVITY_INSTALL + PREVIEW_ACTIVITY_IDLE).not.toMatch(
      /\.value|textContent|innerHTML/
    );
  });
  it.each([
    'pendingCalls',
    'pendingKb',
    'callDebounce',
    'signalPending',
    'runDebounce',
    'agentInflight',
  ])('defers for actual SDK %s work without aria-busy', (field) => {
    evaluate(PREVIEW_ACTIVITY_INSTALL);
    const sdk: Record<string, unknown> = {
      pendingCalls: new Map(),
      pendingKb: new Map(),
      ws: { readyState: WebSocket.OPEN },
    };
    Reflect.set(window, 'BioRouter', sdk);
    expect(evaluate(PREVIEW_ACTIVITY_IDLE)).toBe(true);
    const pending = new Map([['synthetic-id', {}]]);
    sdk[field] = pending;
    expect(evaluate(PREVIEW_ACTIVITY_IDLE)).toBe(false);
    pending.clear();
    expect(evaluate(PREVIEW_ACTIVITY_IDLE)).toBe(true);
  });
  it('defers active runs, queued work, and unknown SDK versions', () => {
    evaluate(PREVIEW_ACTIVITY_INSTALL);
    const sdk = {
      pendingCalls: new Map(),
      pendingKb: new Map(),
      ws: { readyState: WebSocket.OPEN },
      activeRun: { settled: false },
      outbox: [] as unknown[],
    };
    Reflect.set(window, 'BioRouter', sdk);
    expect(evaluate(PREVIEW_ACTIVITY_IDLE)).toBe(false);
    sdk.activeRun.settled = true;
    expect(evaluate(PREVIEW_ACTIVITY_IDLE)).toBe(true);
    sdk.outbox.push({});
    expect(evaluate(PREVIEW_ACTIVITY_IDLE)).toBe(false);
    Reflect.set(window, 'BioRouter', {});
    expect(evaluate(PREVIEW_ACTIVITY_IDLE)).toBe(false);
  });
  it.each([undefined, WebSocket.CONNECTING, WebSocket.CLOSING, WebSocket.CLOSED])(
    'defers cold or reconnecting SDK work with empty maps (socket %s)',
    (readyState) => {
      evaluate(PREVIEW_ACTIVITY_INSTALL);
      const sdk = {
        pendingCalls: new Map(),
        pendingKb: new Map(),
        ws: readyState === undefined ? null : { readyState: Number(readyState) },
      };
      Reflect.set(window, 'BioRouter', sdk);
      expect(evaluate(PREVIEW_ACTIVITY_IDLE)).toBe(false);
      sdk.ws = { readyState: WebSocket.OPEN };
      expect(evaluate(PREVIEW_ACTIVITY_IDLE)).toBe(true);
    }
  );
  it.each([
    '<!doctype html><html><head></head><body>Text</body></html>',
    '<html><body>Text</body></html>',
    '<!doctype html><p>Text</p>',
    '<p>Text</p>',
  ])('adds only the fixed tracker to HTML without losing its source: %s', (source) => {
    const tracked = withPreviewActivityTracking(source);
    expect(tracked).toContain(PREVIEW_ACTIVITY_INSTALL);
    expect(tracked).toContain('Text');
    if (source.startsWith('<!doctype')) expect(tracked.startsWith('<!doctype')).toBe(true);
  });
});
