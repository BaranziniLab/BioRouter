import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { NavigateFunction } from 'react-router-dom';
import { createNavigationHandler, navigateWithViewTransition } from './navigationUtils';

describe('navigation helpers', () => {
  const navigate = vi.fn() as unknown as NavigateFunction;

  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    Reflect.deleteProperty(document, 'startViewTransition');
  });

  it('updates the route synchronously without starting a document snapshot transition', () => {
    const startViewTransition = vi.fn();
    Object.defineProperty(document, 'startViewTransition', {
      configurable: true,
      value: startViewTransition,
    });

    navigateWithViewTransition(navigate, '/settings', { section: 'models' });

    expect(startViewTransition).not.toHaveBeenCalled();
    expect(navigate).toHaveBeenCalledWith('/settings', {
      replace: undefined,
      state: { section: 'models' },
    });
  });

  it('preserves every rapid navigation intent in order', () => {
    navigateWithViewTransition(navigate, '/settings');
    navigateWithViewTransition(navigate, '/workflows');
    navigateWithViewTransition(navigate, '/knowledge');

    expect(navigate).toHaveBeenNthCalledWith(1, '/settings', {
      replace: undefined,
      state: undefined,
    });
    expect(navigate).toHaveBeenNthCalledWith(2, '/workflows', {
      replace: undefined,
      state: undefined,
    });
    expect(navigate).toHaveBeenNthCalledWith(3, '/knowledge', {
      replace: undefined,
      state: undefined,
    });
  });

  it('keeps the shared view mapping and replace option intact', () => {
    const setView = createNavigationHandler(navigate);

    setView('chat', { disableAnimation: true });
    navigateWithViewTransition(navigate, '/sessions', undefined, { replace: true });

    expect(navigate).toHaveBeenNthCalledWith(1, '/', {
      replace: undefined,
      state: { disableAnimation: true },
    });
    expect(navigate).toHaveBeenNthCalledWith(2, '/sessions', {
      replace: true,
      state: undefined,
    });
  });
});
