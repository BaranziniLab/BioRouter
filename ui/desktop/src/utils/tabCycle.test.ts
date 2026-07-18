import { describe, it, expect } from 'vitest';
import {
  isTabCycleEvent,
  tabCycleOffset,
  nextTabIndex,
  isWithinArtifactPanel,
  ARTIFACT_PANEL_ATTR,
} from './tabCycle';

const evt = (over: Partial<Parameters<typeof isTabCycleEvent>[0]> = {}) => ({
  key: 'Tab',
  ctrlKey: true,
  metaKey: false,
  altKey: false,
  ...over,
});

describe('isTabCycleEvent — what counts as the gesture', () => {
  it('accepts Ctrl+Tab', () => {
    expect(isTabCycleEvent(evt())).toBe(true);
  });

  it('accepts Ctrl+Shift+Tab (shift picks direction, not whether it fires)', () => {
    expect(isTabCycleEvent({ ...evt(), shiftKey: true } as never)).toBe(true);
  });

  it('ignores plain Tab, so Tab still moves focus normally', () => {
    // The whole reason the shortcut can live without a text-input guard.
    expect(isTabCycleEvent(evt({ ctrlKey: false }))).toBe(false);
  });

  it('ignores Cmd+Tab — the OS owns it and we must never appear to handle it', () => {
    expect(isTabCycleEvent(evt({ ctrlKey: false, metaKey: true }))).toBe(false);
    // Even Ctrl+Cmd+Tab is not ours.
    expect(isTabCycleEvent(evt({ metaKey: true }))).toBe(false);
  });

  it('ignores Ctrl+Alt+Tab', () => {
    expect(isTabCycleEvent(evt({ altKey: true }))).toBe(false);
  });

  it('ignores other Ctrl keys', () => {
    expect(isTabCycleEvent(evt({ key: 'w' }))).toBe(false);
  });
});

describe('tabCycleOffset — Shift reverses', () => {
  it('forward without Shift', () => {
    expect(tabCycleOffset({ shiftKey: false })).toBe(1);
  });
  it('backward with Shift', () => {
    expect(tabCycleOffset({ shiftKey: true })).toBe(-1);
  });
});

describe('nextTabIndex — which tab is next', () => {
  it('steps left to right', () => {
    expect(nextTabIndex(3, 0, 1)).toBe(1);
    expect(nextTabIndex(3, 1, 1)).toBe(2);
  });

  it('wraps forward off the last tab to the first', () => {
    expect(nextTabIndex(3, 2, 1)).toBe(0);
  });

  it('wraps backward off the first tab to the LAST, not to -1', () => {
    // JS % keeps the dividend's sign: (0 - 1) % 3 === -1. The `+ length` in the
    // implementation is the whole point of this case.
    expect(nextTabIndex(3, 0, -1)).toBe(2);
  });

  it('steps backward', () => {
    expect(nextTabIndex(3, 2, -1)).toBe(1);
  });

  it('no-ops with one tab — cycling to yourself is not a move', () => {
    expect(nextTabIndex(1, 0, 1)).toBeNull();
    expect(nextTabIndex(1, 0, -1)).toBeNull();
  });

  it('no-ops with zero tabs', () => {
    expect(nextTabIndex(0, -1, 1)).toBeNull();
  });

  it('no-ops when the active index is unknown or out of range', () => {
    expect(nextTabIndex(3, -1, 1)).toBeNull();
    expect(nextTabIndex(3, 3, 1)).toBeNull();
  });

  it('a full forward cycle returns to the start and visits every tab once', () => {
    const seen: number[] = [];
    let i = 0;
    for (let n = 0; n < 4; n++) {
      i = nextTabIndex(4, i, 1)!;
      seen.push(i);
    }
    expect(seen).toEqual([1, 2, 3, 0]);
  });

  it('forward then backward is the identity, at every position', () => {
    for (let i = 0; i < 4; i++) {
      expect(nextTabIndex(4, nextTabIndex(4, i, 1)!, -1)).toBe(i);
    }
  });
});

describe('isWithinArtifactPanel — the sole arbiter between the two strips', () => {
  it('true for an element inside the panel', () => {
    const panel = document.createElement('aside');
    panel.setAttribute(ARTIFACT_PANEL_ATTR, '');
    const inner = document.createElement('button');
    panel.appendChild(inner);
    document.body.appendChild(panel);

    expect(isWithinArtifactPanel(inner)).toBe(true);
    // The panel root itself counts — closest() includes self.
    expect(isWithinArtifactPanel(panel)).toBe(true);
    panel.remove();
  });

  it('false for an element outside it — the chat strip answers there', () => {
    const outside = document.createElement('textarea');
    document.body.appendChild(outside);
    expect(isWithinArtifactPanel(outside)).toBe(false);
    outside.remove();
  });

  it('false for a non-Element target (window/document keydowns)', () => {
    // The default when nothing is focused: keydown targets document.body or
    // window. Chat must answer, which is the browser default.
    expect(isWithinArtifactPanel(null)).toBe(false);
    expect(isWithinArtifactPanel(window)).toBe(false);
  });
});
