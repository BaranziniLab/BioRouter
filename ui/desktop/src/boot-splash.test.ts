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
function resolveFromPackage(relative: string): string {
  const candidates = [
    join(process.cwd(), relative),
    join(process.cwd(), 'ui', 'desktop', relative),
  ];
  const found = candidates.find((p) => existsSync(p));
  if (!found) throw new Error(`could not locate ${relative} from ${process.cwd()}`);
  return found;
}

function readIndexHtml(): string {
  return readFileSync(resolveFromPackage('index.html'), 'utf-8');
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

  /**
   * The splash paints before React, so it cannot import THEME_FAMILIES — it has
   * to restate every family in plain CSS, and the theme-init script has to
   * restate them again in plain JS. That is three lists that can drift.
   *
   * These tests are the lockstep enforcement: add a family to ThemeContext and
   * they fail until the splash and the init script both cover it. Without them
   * a new theme silently boots on Parchment cream — including, at its worst, a
   * cream flash on a dark ground.
   *
   * Deriving the colours from tokens instead is not an option: Tailwind v4's
   * `@theme inline` compiles --background-muted and friends away, so they do
   * not exist as runtime custom properties. See the comment in index.html.
   */
  describe('theme coverage', () => {
    /**
     * Reads the canonical family list in either shape it has taken: the runtime
     * `THEME_FAMILIES` array, or the older `ThemeFamily` type union it replaced.
     * Supporting both keeps this test from becoming a merge blocker depending on
     * which branch lands first.
     */
    function themeFamilies(): string[] {
      const src = readFileSync(resolveFromPackage('src/contexts/ThemeContext.tsx'), 'utf-8');
      const runtimeList = src.match(/export const THEME_FAMILIES\s*=\s*\[([^\]]+)\]/);
      if (runtimeList) return [...runtimeList[1].matchAll(/'([^']+)'/g)].map((m) => m[1]);
      const typeUnion = src.match(/export type ThemeFamily\s*=\s*([^;]+);/);
      if (typeUnion) return [...typeUnion[1].matchAll(/'([^']+)'/g)].map((m) => m[1]);
      throw new Error('could not determine the theme family list from ThemeContext.tsx');
    }

    it('finds more than one family, so the checks below are not vacuous', () => {
      expect(themeFamilies().length).toBeGreaterThan(1);
    });

    it('styles every theme family in both light and dark', () => {
      const html = readIndexHtml();
      const missing: string[] = [];

      for (const family of themeFamilies()) {
        // Parchment is the default and is styled by the bare #br-boot /
        // html.dark rules rather than a [data-theme] selector.
        if (family === 'parchment') {
          if (!/#br-boot\s*\{/.test(html)) missing.push('parchment (light)');
          if (!/html\.dark\s+#br-boot\s*\{/.test(html)) missing.push('parchment (dark)');
          continue;
        }
        const light = new RegExp(`html\\[data-theme='${family}'\\]\\s+#br-boot\\s*\\{`);
        const dark = new RegExp(`html\\.dark\\[data-theme='${family}'\\]\\s+#br-boot\\s*\\{`);
        if (!light.test(html)) missing.push(`${family} (light)`);
        if (!dark.test(html)) missing.push(`${family} (dark)`);
      }

      expect(missing, `boot splash has no rule for: ${missing.join(', ')}`).toEqual([]);
    });

    it("gives every family its own ground, never inheriting another family's", () => {
      const html = readIndexHtml();
      const grounds = [...html.matchAll(/#br-boot\s*\{[^}]*?--br-bg:\s*([^;]+);/g)].map((m) =>
        m[1].trim()
      );
      // One per family per mode; duplicates would mean a family is wearing
      // another family's background.
      expect(new Set(grounds).size).toBe(grounds.length);
    });

    it('keeps the pre-React theme script in lockstep with ThemeContext', () => {
      const html = readIndexHtml();
      const families = themeFamilies();
      const declared = html.match(/const FAMILIES\s*=\s*\[([^\]]+)\]/);

      if (declared) {
        // The script keeps an explicit allow-list: it must match exactly, or a
        // family is either unreachable or settable-but-unstyled.
        const inScript = [...declared[1].matchAll(/'([^']+)'/g)].map((m) => m[1]);
        expect(inScript).toEqual(families);
        return;
      }
      // Older shape: the script branches on family names inline. Each family
      // must at least be named there, or it can never be applied before paint.
      const missing = families.filter((f) => f !== 'parchment' && !html.includes(`'${f}'`));
      expect(missing, `theme script never mentions: ${missing.join(', ')}`).toEqual([]);
    });
  });

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
