import { describe, it, expect, beforeEach, vi } from 'vitest';
import {
  registerCloseActiveTab,
  closeActiveTab,
  resetCloseActiveTabRegistry,
} from './closeActiveTabRegistry';

describe('closeActiveTabRegistry — the Cmd+W hand-off', () => {
  beforeEach(resetCloseActiveTabRegistry);

  it('returns false with no handler, so the root closes the WINDOW instead', () => {
    // The tabless routes (Settings, Hub): Cmd+W must still behave like macOS.
    expect(closeActiveTab()).toBe(false);
  });

  it('delegates to the registered handler and reports what it claimed', () => {
    const handler = vi.fn().mockReturnValue(true);
    registerCloseActiveTab(handler);

    expect(closeActiveTab()).toBe(true);
    expect(handler).toHaveBeenCalledTimes(1);
  });

  it('a handler that has nothing to close does NOT swallow the keystroke', () => {
    registerCloseActiveTab(() => false);
    expect(closeActiveTab()).toBe(false);
  });

  it('disposing unregisters', () => {
    const dispose = registerCloseActiveTab(() => true);
    dispose();
    expect(closeActiveTab()).toBe(false);
  });

  it('a StrictMode double-mount cannot leave the registry empty', () => {
    // Mount A, mount B, THEN dispose A — React's order. A's disposer must not
    // clear B's handler, or Cmd+W would start closing the window with tabs open.
    const disposeA = registerCloseActiveTab(() => false);
    registerCloseActiveTab(() => true);
    disposeA();

    expect(closeActiveTab()).toBe(true);
  });
});
