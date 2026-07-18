import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { SIDEBAR_COMPACT_WIDTH } from './yieldLadder';
import { SIDEBAR_COMPACT_TITLE_WIDTH } from './TitlebarControls';

/**
 * 1120 has exactly one home.
 *
 * It had three: `AppLayout` (rung 1's auto-collapse threshold),
 * `TitlebarControls` (the titlebar reserve), and a third copy re-declared
 * inside `BaseChat` for the compact session title. All three were 1120, so
 * nothing was broken — which is precisely why it was worth fixing before it
 * broke.
 *
 * These three must agree by construction, not by coincidence. Rung 1 of the
 * yield ladder fires on this width and rungs 2–4 inherit the room it frees, so
 * a copy that drifted would desynchronise the ladder from the chrome **with
 * nothing failing**: the layout would simply be wrong at one width, silently.
 * That is the same shape as the traffic-light reserve, which had already
 * drifted once for this reason — and the same shape as the bug the visual sweep
 * found, where a green test guarded a property while the behaviour was broken.
 *
 * A value test alone would not catch the regression that matters: someone
 * re-declaring a local `const FOO = 1120` and using that instead. So this file
 * also greps the source for a fourth copy. That is deliberately a source-text
 * assertion — the defect is textual (a duplicate literal), and no runtime
 * assertion can see a constant that was never imported.
 */
describe('SIDEBAR_COMPACT_WIDTH — one number, one home', () => {
  it('is 1120', () => {
    expect(SIDEBAR_COMPACT_WIDTH).toBe(1120);
  });

  it("TitlebarControls' public name resolves to the same constant", () => {
    // Re-exported rather than re-declared, so existing importers are unchanged.
    expect(SIDEBAR_COMPACT_TITLE_WIDTH).toBe(SIDEBAR_COMPACT_WIDTH);
  });

  it('is declared exactly once across the layout + chat sources', () => {
    const files = [
      'Layout/yieldLadder.ts',
      'Layout/AppLayout.tsx',
      'Layout/TitlebarControls.tsx',
      'BaseChat.tsx',
      'chatGroups/ChatGroupsShell.tsx',
    ];

    const declarations = files.flatMap((file) => {
      const source = readFileSync(join(__dirname, '..', file), 'utf8');
      // A declaration, not a usage: `const NAME = 1120`. A reference to the
      // imported constant is fine and is the whole point.
      const hits = source.match(/const\s+\w+\s*=\s*1120\b/g) ?? [];
      return hits.map((hit) => `${file}: ${hit}`);
    });

    expect(declarations).toEqual(['Layout/yieldLadder.ts: const SIDEBAR_COMPACT_WIDTH = 1120']);
  });
});
