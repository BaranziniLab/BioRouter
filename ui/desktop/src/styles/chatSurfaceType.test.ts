import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

/**
 * Every card in the chat transcript is set at the transcript's own size.
 *
 * `biorouter-message-content` marks a block that renders INSIDE the transcript,
 * shoulder to shoulder with the assistant's prose. That prose is 14px. The
 * document default is 16px, so a card that names no size class does not render
 * "at the default" — it renders two pixels larger than everything around it,
 * which is what the user saw and described as a banner sitting among the text.
 *
 * The bug is invisible to every component test in this repo, and not by
 * accident: jsdom has no layout engine and never runs Tailwind, so a card
 * rendered in a test reports the same (absent) computed size whether or not the
 * class is there. Nothing short of a real browser can measure it. So the
 * assertion is made where it CAN be made — over the sources — and it is made as
 * a rule about the marker rather than as a list of the six files that were
 * wrong on the day it was written.
 *
 * The rule: a `biorouter-message-content` element states its own type size.
 * Any of the semantic roles satisfies it, as does a raw Tailwind step; the
 * point is that the size is a decision, not an inheritance.
 */
const COMPONENTS = join(__dirname, '../components');
const MARKER = 'biorouter-message-content';
const SIZE = /\btext-(body|label|supporting|code|sm|xs|base|lg)\b/;

function sourceFiles(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) return sourceFiles(path);
    return entry.isFile() && entry.name.endsWith('.tsx') && !entry.name.endsWith('.test.tsx')
      ? [path]
      : [];
  });
}

/** Every line that puts the marker on an element, as `path:line — text`. */
function markerLines(): string[] {
  return sourceFiles(COMPONENTS).flatMap((path) =>
    readFileSync(path, 'utf8')
      .split('\n')
      .map((text, index) => ({ text, line: index + 1 }))
      .filter(({ text }) => text.includes(MARKER))
      .map(({ text, line }) => `${path.slice(COMPONENTS.length + 1)}:${line} — ${text.trim()}`)
  );
}

describe('chat-surface cards state their own type size', () => {
  it('finds the marker in the tree at all', () => {
    // Guards the guard: a renamed marker would empty the sweep below, and an
    // empty sweep passes. This is the assertion that fails instead.
    expect(markerLines().length).toBeGreaterThan(5);
  });

  it('never ships a card that inherits the 16px document default', () => {
    const unsized = markerLines().filter((line) => !SIZE.test(line));
    expect(unsized).toEqual([]);
  });
});
