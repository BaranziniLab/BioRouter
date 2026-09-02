import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

import { BRAND_SANS } from '../components/icons/brandFont';

/**
 * The font stacks that are written out in more than one place.
 *
 * Nearly every face in the app is late-bound through a token, so it cannot
 * drift. Two cannot be, and each is a documented exception rather than an
 * oversight:
 *
 *  - the terminal's, because **xterm measures glyph widths itself** and so
 *    resolves no `var()`. Its stack has to be a literal string in TypeScript.
 *  - the brand mark's, because it is drawn into an SVG `fontFamily` attribute.
 *    That one is now a single exported constant, so the assertion below is
 *    about it not silently becoming two again.
 *
 * A comment saying "keep this identical to X" is not a guard, and until this
 * file existed that comment was the only thing holding the terminal stack to
 * `--font-mono`. The consequence of drift is quiet and specific: a command
 * copied out of a chat code block would render in a different face from the
 * terminal it is pasted into, at the same nominal size, with different glyph
 * widths — which reads as a rendering bug in the terminal rather than as two
 * stacks disagreeing.
 */
const CSS = readFileSync(join(__dirname, 'main.css'), 'utf8');
const TERMINAL_DOCK = readFileSync(join(__dirname, '../components/InAppTerminalDock.tsx'), 'utf8');

/** A font stack as a comparable list: quoting and whitespace are not identity. */
function families(stack: string): string[] {
  return stack
    .replace(/\s+/g, ' ')
    .split(',')
    .map((family) => family.trim().replace(/^['"]|['"]$/g, ''))
    .filter(Boolean);
}

function cssToken(name: string): string {
  const match = CSS.match(new RegExp(`--${name}:\\s*([^;]+);`));
  if (!match) throw new Error(`--${name} is not declared in main.css`);
  return match[1];
}

describe('font stacks written out more than once', () => {
  it('sets the terminal in exactly the app monospace face', () => {
    const declared = TERMINAL_DOCK.match(/const TERMINAL_FONT =\s*\n?\s*'([^']+)'/);
    if (!declared) throw new Error('TERMINAL_FONT is no longer a single-quoted literal');

    // Family for family, in order. A stack that merely SHARES its first family
    // is not the same stack: the fallbacks decide what a machine without SF
    // Mono actually renders, which is most of them.
    expect(families(declared[1])).toEqual(families(cssToken('font-mono')));
  });

  it('keeps one brand face rather than a copy per mark', () => {
    const marks = ['BioRouterMark.tsx', 'BioRouterWordmark.tsx'].map((file) =>
      readFileSync(join(__dirname, '../components/icons', file), 'utf8')
    );
    for (const source of marks) {
      expect(source).toContain("from './brandFont'");
      expect(source).not.toMatch(/const SANS =\s*\n?\s*'Inter/);
    }
    expect(families(BRAND_SANS)[0]).toBe('Inter');
  });
});
