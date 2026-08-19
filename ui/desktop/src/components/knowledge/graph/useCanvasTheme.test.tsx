import { createRef } from 'react';
import { renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { useCanvasTheme } from './useCanvasTheme';

/**
 * The canvas ink has to survive a LIVE theme toggle.
 *
 * The failure this pins is not "the observer is missing" — it was there. It is
 * that the value the observer hands you at the instant `<html>`'s class changes
 * can still be the old one, because an ancestor is transitioning `color` and
 * `getComputedStyle` during a transition returns the interpolated value. The
 * Knowledge view supplies exactly such an ancestor: `TabsContent`'s
 * `animate-in fade-in duration-[var(--motion-base)]` computes to
 * `transition-property: all` for 175 ms, and `all` includes the inherited
 * `color`. One cached read left every label drawn in the PREVIOUS mode's ink.
 *
 * jsdom runs no transitions, so the lag is modelled the only way it can be:
 * the element's colour is changed a few frames AFTER the class flips, which is
 * precisely what a transition looks like to `getComputedStyle`. A hook that
 * reads once on the mutation keeps the stale value here and fails; one that
 * reads until the cascade settles passes.
 */

function host(color: string): HTMLDivElement {
  const el = document.createElement('div');
  el.style.color = color;
  document.body.appendChild(el);
  return el;
}

const LIGHT_INK = 'rgb(5, 32, 73)';
const DARK_INK = 'rgb(242, 243, 244)';

beforeEach(() => {
  document.documentElement.className = 'light';
});

afterEach(() => {
  document.body.innerHTML = '';
  document.documentElement.className = '';
});

describe('useCanvasTheme', () => {
  it('resolves the ink on mount', async () => {
    const ref = createRef<HTMLElement>();
    (ref as { current: HTMLElement | null }).current = host(LIGHT_INK);

    const { result } = renderHook(() => useCanvasTheme(ref));
    await waitFor(() => expect(result.current.ink).toBe(LIGHT_INK));
  });

  it('re-resolves when the mode flips and the colour lands immediately', async () => {
    const el = host(LIGHT_INK);
    const ref = createRef<HTMLElement>();
    (ref as { current: HTMLElement | null }).current = el;

    const { result } = renderHook(() => useCanvasTheme(ref));
    await waitFor(() => expect(result.current.ink).toBe(LIGHT_INK));

    el.style.color = DARK_INK;
    document.documentElement.className = 'dark';
    await waitFor(() => expect(result.current.ink).toBe(DARK_INK));
  });

  // The real one. The class flips first and the colour follows it, because the
  // colour is being animated — so the read taken AT the mutation is the value
  // the app is transitioning away from.
  it('re-resolves when the colour arrives a few frames after the class does', async () => {
    const el = host(LIGHT_INK);
    const ref = createRef<HTMLElement>();
    (ref as { current: HTMLElement | null }).current = el;

    const { result } = renderHook(() => useCanvasTheme(ref));
    await waitFor(() => expect(result.current.ink).toBe(LIGHT_INK));

    document.documentElement.className = 'dark';
    // Three frames of "still the old colour", the shape of a 175ms transition.
    await new Promise<void>((resolve) => {
      let n = 0;
      const tick = () => {
        if (++n >= 3) {
          el.style.color = DARK_INK;
          resolve();
          return;
        }
        requestAnimationFrame(tick);
      };
      requestAnimationFrame(tick);
    });

    await waitFor(() => expect(result.current.ink).toBe(DARK_INK));
  });

  // Watching `data-theme` matters as much as watching the class: a family swap
  // moves the ink without touching light/dark at all.
  it('re-resolves on a theme-family swap', async () => {
    const el = host(LIGHT_INK);
    const ref = createRef<HTMLElement>();
    (ref as { current: HTMLElement | null }).current = el;

    const { result } = renderHook(() => useCanvasTheme(ref));
    await waitFor(() => expect(result.current.ink).toBe(LIGHT_INK));

    document.documentElement.setAttribute('data-theme', 'alma-mater');
    await new Promise((resolve) => requestAnimationFrame(resolve));
    el.style.color = 'rgb(42, 37, 32)';
    await waitFor(() => expect(result.current.ink).toBe('rgb(42, 37, 32)'));
    document.documentElement.removeAttribute('data-theme');
  });
});
