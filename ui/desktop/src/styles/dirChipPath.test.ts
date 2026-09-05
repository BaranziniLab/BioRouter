import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

/**
 * The composer's working-directory chip: how wide it is, which end of the path
 * gets clipped, and whether the path it shows is the path that exists.
 *
 * ⚠ **Asserted at the SOURCE, and it has to be.** jsdom has no layout engine
 * and does not implement the Unicode bidirectional algorithm, so a component
 * test that mounts the chip and reads `textContent` sees the logical string in
 * every case — it passes identically whether the box is 112px or 46ch, and
 * whether the leading separator is reordered to the far end or not. Both bugs
 * this file exists for were invisible to jsdom while they were live.
 * `composerFocus.test.ts` is the precedent, for the same reason.
 *
 * **Measured in a real browser** (Chromium, 12px `--font-mono`, the real strip
 * markup, 141 pane widths from 900px down to 200px in 5px steps). Character
 * positions were read back with a `Range` per character and sorted by x, which
 * gives the VISUAL glyph order rather than the logical string:
 *
 *   before  `/Users/wgu/Downloads`  ->  `…wgu/Downloads/`   (phantom slash)
 *   after   `/Users/wgu/Downloads`  ->  `/Users/wgu/Downloads`  (fits whole)
 *   before  `C:\`                   ->  `\:C`               (fully reversed)
 *   after   `C:\`                   ->  `C:\`
 *   after   deep path               ->  `…trees/frosty-proskuriakova-fe9584/ui/desktop`
 *
 * And the widening costs the counts across the strip nothing: they were never
 * squeezed at any of the 141 widths in either spelling, and the strip first
 * overflows at the SAME 200px in both — far below any real pane — because
 * `flex-shrink-0` on the counts plus `min-w-0` all the way down this side means
 * a bigger cap only takes effect where there is room for it.
 */
const CSS = readFileSync(join(__dirname, 'main.css'), 'utf8');
const DIR_SWITCHER = readFileSync(
  join(__dirname, '../components/bottom_menu/DirSwitcher.tsx'),
  'utf8'
);

const RULE = /\.biorouter-dir-chip-path\s*\{([^}]*)\}/;
const ISOLATE_RULE = /\.biorouter-dir-chip-path\s*>\s*bdi\s*\{([^}]*)\}/;

describe('the working-directory chip while the directory is still choosable', () => {
  it('caps the path far wider than the locked chip, so an ordinary path fits', () => {
    const rule = CSS.match(RULE)?.[1];
    expect(rule).toBeTruthy();
    // `ch` on 12px mono: 46ch is ~332px, measured. `/Users/wgu/Downloads` is 20
    // characters and rendered at 145px — comfortably inside it.
    expect(rule).toMatch(/max-width:\s*46ch/);
    // Not the locked chip's cap: 112px truncates a 20-character path.
    expect(rule).not.toContain('112px');
  });

  /**
   * A percentage term (`min(46ch, 40%)`) was considered for the "never push the
   * counts off" job and declined on two independent grounds, both measured —
   * see the rule's own comment. This pins the decision so it is not quietly
   * reintroduced: this div's containing block is a content-sized flex item with
   * no definite width, which is exactly where a percentage max-width resolves
   * against an indefinite size and behaves as `none`.
   */
  it('bounds itself in `ch` and leaves narrow panes to flex, not to a percentage', () => {
    const rule = CSS.match(RULE)?.[1] ?? '';
    expect(rule).not.toContain('%');
    // The shrink chain is what actually protects the counts, so the chip must
    // keep the `min-w-0` that lets flex reach it.
    expect(DIR_SWITCHER).toMatch(/biorouter-dir-chip-path[^"]*min-w-0/);
  });

  it('clips the HEAD of the path, keeping the identifying final segments', () => {
    expect(CSS.match(RULE)?.[1]).toMatch(/direction:\s*rtl/);
    expect(DIR_SWITCHER).toMatch(/biorouter-dir-chip-path[^"]*truncate/);
  });

  /**
   * `/` and `\` are bidi-neutral (Unicode class CS). In an RTL paragraph the
   * bidi algorithm resolves a neutral sitting at a paragraph boundary to the
   * PARAGRAPH direction, which moved a path's leading separator to the visual
   * right end: `/Users/wgu/Downloads` rendered `…wgu/Downloads/` and `C:\`
   * rendered `\:C`. A chip showing a path that does not exist is worse than one
   * showing a truncated path that does.
   */
  it('isolates the path as LTR so its separators are not reordered', () => {
    const isolate = CSS.match(ISOLATE_RULE)?.[1];
    expect(isolate).toBeTruthy();
    expect(isolate).toMatch(/unicode-bidi:\s*isolate/);
    // `unicode-bidi: isolate` ALONE is not enough — an isolate inherits the RTL
    // base direction and reorders the boundary neutral exactly the same way.
    expect(isolate).toMatch(/direction:\s*ltr/);
    // The rule matches nothing without the element it selects.
    expect(DIR_SWITCHER).toMatch(/<bdi>\{workingDir\}<\/bdi>/);
  });

  /**
   * The RTL box and the LTR isolate are one mechanism: the box exists to clip
   * the head, and the isolate exists to stop that box corrupting the path.
   * Splitting them across a stylesheet and a Tailwind arbitrary value at the
   * call site is how the second half went missing. Authored CSS also cannot
   * silently fail to generate the way a newly written utility can (see
   * `composerFocus.test.ts`).
   */
  it('declares the RTL box and its LTR isolate together, as authored CSS', () => {
    expect(DIR_SWITCHER).not.toContain('[direction:rtl]');
    expect(DIR_SWITCHER).not.toContain('[unicode-bidi:');
    expect(CSS.indexOf('.biorouter-dir-chip-path')).toBeGreaterThan(-1);
  });
});

describe('the locked chip is left exactly as it was', () => {
  /**
   * #44: once the chat has messages the directory is immutable, and a decided
   * directory needs only its folder name. That is intended behaviour, not an
   * oversight the widening should have swept up — the two chips answer
   * different questions and are allowed to be different sizes.
   */
  it('still shows the basename at the narrow cap', () => {
    const locked = DIR_SWITCHER.match(/data-testid="dir-switcher-locked"[\s\S]*?<\/span>/)?.[0];
    expect(locked).toBeTruthy();
    expect(locked).toContain('max-w-[112px]');
    expect(locked).toContain('workingDirLabel(workingDir)');
    // The locked chip stays LTR: a basename has no leading separator to
    // reorder, and nothing here clips the head.
    expect(locked).not.toContain('biorouter-dir-chip-path');
  });

  it('no longer shares one cap with the unlocked chip', () => {
    expect(DIR_SWITCHER.match(/max-w-\[112px\]/g)).toHaveLength(1);
  });
});
