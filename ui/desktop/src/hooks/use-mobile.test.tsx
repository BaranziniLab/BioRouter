import { act, renderHook } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { useIsMobile } from './use-mobile';

const originalInnerWidth = window.innerWidth;
const originalMatchMedia = window.matchMedia;

function setInnerWidth(width: number) {
  Object.defineProperty(window, 'innerWidth', { configurable: true, value: width });
}

afterEach(() => {
  setInnerWidth(originalInnerWidth);
  Object.defineProperty(window, 'matchMedia', {
    configurable: true,
    value: originalMatchMedia,
  });
  vi.restoreAllMocks();
});

describe('useIsMobile', () => {
  it('reports a narrow persisted window on the first render', () => {
    setInnerWidth(720);
    Object.defineProperty(window, 'matchMedia', {
      configurable: true,
      value: vi.fn(() => ({
        matches: true,
        addEventListener: vi.fn(),
        removeEventListener: vi.fn(),
      })) as unknown as typeof window.matchMedia,
    });

    const { result } = renderHook(() => useIsMobile());

    expect(result.current).toBe(true);
  });

  it('updates when the viewport crosses the mobile breakpoint', () => {
    setInnerWidth(1200);
    let handleChange: (() => void) | undefined;
    Object.defineProperty(window, 'matchMedia', {
      configurable: true,
      value: vi.fn(() => ({
        matches: false,
        addEventListener: vi.fn((_type: string, listener: unknown) => {
          handleChange = listener as () => void;
        }),
        removeEventListener: vi.fn(),
      })) as unknown as typeof window.matchMedia,
    });

    const { result } = renderHook(() => useIsMobile());
    expect(result.current).toBe(false);

    setInnerWidth(800);
    act(() => handleChange?.());

    expect(result.current).toBe(true);
  });
});
