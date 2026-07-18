import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { readFileSync, existsSync } from 'node:fs';
import { join } from 'node:path';

/**
 * These tests run the boot splash's REAL source, extracted out of
 * `index.html`, rather than a copy kept in the test file. The splash has to be
 * inline pre-React markup (see the comment in index.html), which means there is
 * no module to import — so we lift the markup and the script straight out of
 * the shipped page. If either block is renamed or removed, extraction throws
 * instead of quietly testing nothing.
 */

// `import.meta.url` is not a file: URL under Vitest's jsdom transform, so
// resolve from the working directory instead — vitest may be invoked from the
// package dir or from the repo root.
function resolveIndexHtml(): string {
  const candidates = [
    join(process.cwd(), 'index.html'),
    join(process.cwd(), 'ui', 'desktop', 'index.html'),
  ];
  const found = candidates.find((p) => existsSync(p));
  if (!found) throw new Error(`could not locate index.html from ${process.cwd()}`);
  return found;
}

function readIndexHtml(): string {
  return readFileSync(resolveIndexHtml(), 'utf-8');
}

function extractMarkup(html: string): string {
  const match = html.match(/<!-- br-boot:start -->([\s\S]*?)<!-- br-boot:end -->/);
  if (!match) throw new Error('boot splash markup delimiters not found in index.html');
  return match[1];
}

function extractScript(html: string): string {
  const match = html.match(/<script id="br-boot-splash-script">([\s\S]*?)<\/script>/);
  if (!match) throw new Error('boot splash script not found in index.html');
  return match[1];
}

const THRESHOLD_MS = 400;
const DURATION_MS = 420;
const STAGGER_MS = 70;

/** Only the fields the splash actually sets — avoids depending on DOM lib globals. */
type SplashKeyframe = { opacity?: number; transform?: string; filter?: string };
type SplashAnimOptions = {
  duration?: number;
  easing?: string;
  delay?: number;
  fill?: string;
};
type AnimateCall = { keyframes: SplashKeyframe[]; options: SplashAnimOptions };

/** Installs the splash exactly as the browser would, with timers already faked. */
function mountSplash(): void {
  const html = readIndexHtml();
  document.body.innerHTML = extractMarkup(html);
  new Function(extractScript(html))();
}

function splashEl(): HTMLElement | null {
  return document.getElementById('br-boot');
}

describe('boot splash', () => {
  let animateCalls: AnimateCall[];
  let cancelled: number;

  beforeEach(() => {
    vi.useFakeTimers();
    animateCalls = [];
    cancelled = 0;
    document.documentElement.className = '';
    document.documentElement.removeAttribute('data-theme');
    delete (window as { __brBootSplash?: unknown }).__brBootSplash;
  });

  afterEach(() => {
    vi.useRealTimers();
    document.body.innerHTML = '';
    delete (window as { __brBootSplash?: unknown }).__brBootSplash;
    delete (Element.prototype as { animate?: unknown }).animate;
  });

  /** jsdom has no Web Animations API; stand one up so the animated path runs. */
  function stubAnimate() {
    (Element.prototype as unknown as { animate: unknown }).animate = function (
      keyframes: SplashKeyframe[],
      options: SplashAnimOptions
    ) {
      animateCalls.push({ keyframes, options });
      return {
        cancel: () => {
          cancelled += 1;
        },
      };
    };
  }

  describe('markup', () => {
    it('ships four independently animatable mark parts', () => {
      mountSplash();
      expect(document.querySelectorAll('#br-boot .br-part')).toHaveLength(4);
    });

    it('announces the wait to screen readers, since nothing on screen is text', () => {
      mountSplash();
      const live = document.querySelector('#br-boot .br-sr');
      expect(live?.getAttribute('role')).toBe('status');
      expect(live?.textContent).toContain('Loading BioRouter');
    });

    it('starts hidden so a fast boot paints nothing at all', () => {
      mountSplash();
      expect(splashEl()?.hidden).toBe(true);
    });
  });

  describe('threshold gating', () => {
    it('stays hidden right up to the threshold', () => {
      mountSplash();
      vi.advanceTimersByTime(THRESHOLD_MS - 1);
      expect(splashEl()?.hidden).toBe(true);
      expect(window.__brBootSplash?.isShown()).toBe(false);
    });

    it('appears once the boot is slow enough to warrant it', () => {
      mountSplash();
      vi.advanceTimersByTime(THRESHOLD_MS);
      expect(splashEl()?.hidden).toBe(false);
      expect(window.__brBootSplash?.isShown()).toBe(true);
    });
  });

  describe('dismiss', () => {
    it('removes the splash outright when the boot beat the threshold', () => {
      mountSplash();
      vi.advanceTimersByTime(THRESHOLD_MS - 100);
      window.__brBootSplash?.dismiss();

      expect(splashEl()).toBeNull();
      // The pending show timer must not resurrect it after removal.
      vi.advanceTimersByTime(1000);
      expect(splashEl()).toBeNull();
      expect(window.__brBootSplash?.isShown()).toBe(false);
    });

    it('fades out, then removes, when the splash was already on screen', () => {
      mountSplash();
      vi.advanceTimersByTime(THRESHOLD_MS);
      expect(splashEl()).not.toBeNull();

      window.__brBootSplash?.dismiss();
      // Still present mid-fade so the handoff is not a hard cut.
      expect(splashEl()?.classList.contains('br-out')).toBe(true);

      vi.advanceTimersByTime(400);
      expect(splashEl()).toBeNull();
    });

    it('is idempotent — a second call cannot throw or double-remove', () => {
      mountSplash();
      vi.advanceTimersByTime(THRESHOLD_MS);
      window.__brBootSplash?.dismiss();
      vi.advanceTimersByTime(400);
      expect(() => window.__brBootSplash?.dismiss()).not.toThrow();
      expect(splashEl()).toBeNull();
    });

    it('cancels in-flight animations so they cannot outlive the element', () => {
      stubAnimate();
      mountSplash();
      vi.advanceTimersByTime(THRESHOLD_MS);
      expect(animateCalls.length).toBeGreaterThan(0);

      window.__brBootSplash?.dismiss();
      vi.advanceTimersByTime(400);
      expect(cancelled).toBe(animateCalls.length);
    });
  });

  describe('cascade timing', () => {
    it('staggers the four parts left to right on the agreed keyframes', () => {
      stubAnimate();
      mountSplash();
      vi.advanceTimersByTime(THRESHOLD_MS);

      // Four parts plus one sweep-bar fade.
      const partAnims = animateCalls.slice(0, 4);
      expect(partAnims).toHaveLength(4);

      partAnims.forEach((call, i) => {
        expect(call.options.delay).toBe(i * STAGGER_MS);
        expect(call.options.duration).toBe(DURATION_MS);
        expect(call.options.easing).toBe('cubic-bezier(0.22, 1, 0.36, 1)');
        expect(call.options.fill).toBe('forwards');
      });

      // Each part resolves out of blur, from transparent to opaque.
      const first = partAnims[0];
      expect(first.keyframes[0].opacity).toBe(0);
      expect(first.keyframes[first.keyframes.length - 1].opacity).toBe(1);
      expect(String(first.keyframes[0].filter)).toMatch(/^blur\(/);
      expect(first.keyframes[first.keyframes.length - 1].filter).toBe('blur(0px)');
    });

    it('converts blur into SVG user units, not raw device pixels', () => {
      stubAnimate();
      mountSplash();
      vi.advanceTimersByTime(THRESHOLD_MS);

      // 2px at a 165.672-unit viewBox rendered 84px wide ≈ 3.94 user units.
      // A literal blur(2px) here would mean the blur scaled with the mark.
      const startBlur = String(animateCalls[0].keyframes[0].filter);
      const value = parseFloat(startBlur.replace(/[^0-9.]/g, ''));
      expect(value).toBeCloseTo(2 * (165.672 / 84), 1);
      expect(value).not.toBeCloseTo(2, 1);
    });
  });

  describe('reduced motion', () => {
    it('renders the mark settled instead of cascading it', () => {
      stubAnimate();
      const original = window.matchMedia;
      window.matchMedia = ((query: string) =>
        ({
          matches: query.includes('prefers-reduced-motion'),
          media: query,
          addEventListener: vi.fn(),
          removeEventListener: vi.fn(),
          addListener: vi.fn(),
          removeListener: vi.fn(),
          onchange: null,
          dispatchEvent: vi.fn(),
        }) as unknown as MediaQueryList) as typeof window.matchMedia;

      try {
        mountSplash();
        vi.advanceTimersByTime(THRESHOLD_MS);

        expect(splashEl()?.hidden).toBe(false);
        expect(animateCalls).toHaveLength(0);
        document.querySelectorAll<HTMLElement>('#br-boot .br-part').forEach((part) => {
          expect(part.style.opacity).toBe('1');
        });
      } finally {
        window.matchMedia = original;
      }
    });
  });
});
