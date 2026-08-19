import { readFileSync, readdirSync } from 'node:fs';
import { join, relative } from 'node:path';
import { cleanup, render } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from './sheet';

/**
 * The description contract for drawers.
 *
 * Radix writes `aria-describedby` onto every `SheetContent` unconditionally,
 * pointing at the id a `SheetDescription` would claim. Render no description and
 * the attribute dangles at nothing: the accessibility tree carries a broken
 * reference, and Radix logs *"Missing `Description` or
 * `aria-describedby={undefined}` for {DialogContent}"* on every open.
 *
 * ⚠ **`npm run test:run` cannot see the warning on its own.** `src/test/setup.ts`
 * replaces `console.warn` with a `vi.fn()` for the whole suite, so the message is
 * swallowed rather than absent — which is why both Knowledge drawers shipped
 * warning in a real browser while every test passed. These tests read that mock
 * back deliberately, and assert the DOM attribute as well, so neither half can
 * rot alone.
 */
const warned = () =>
  vi
    .mocked(console.warn)
    .mock.calls.some((call) => String(call[0]).includes('Missing `Description`'));

const surface = () => document.querySelector('[data-slot="sheet-content"]')!;

beforeEach(() => {
  vi.mocked(console.warn).mockClear();
});
afterEach(cleanup);

describe('SheetContent — settling the description', () => {
  it('links a rendered SheetDescription, and says nothing', () => {
    render(
      <Sheet open>
        <SheetContent>
          <SheetHeader>
            <SheetTitle>Sidebar</SheetTitle>
            <SheetDescription>Displays the mobile sidebar.</SheetDescription>
          </SheetHeader>
        </SheetContent>
      </Sheet>
    );

    const id = surface().getAttribute('aria-describedby');
    expect(id).toBeTruthy();
    expect(document.getElementById(id!)).toHaveTextContent('Displays the mobile sidebar.');
    expect(warned()).toBe(false);
  });

  it('drops the attribute entirely on the explicit opt-out', () => {
    render(
      <Sheet open>
        <SheetContent aria-describedby={undefined}>
          <SheetHeader>
            <SheetTitle>Change log</SheetTitle>
          </SheetHeader>
        </SheetContent>
      </Sheet>
    );

    // Not "points at an empty description" — absent. A dangling id is the defect.
    expect(surface().hasAttribute('aria-describedby')).toBe(false);
    expect(warned()).toBe(false);
  });

  // Without this case the two above would pass against a primitive that could
  // never warn, and the guard below would be policing nothing.
  it('does dangle, and does warn, when a call site settles neither', () => {
    render(
      <Sheet open>
        <SheetContent>
          <SheetHeader>
            <SheetTitle>Change log</SheetTitle>
          </SheetHeader>
        </SheetContent>
      </Sheet>
    );

    const id = surface().getAttribute('aria-describedby');
    expect(id).toBeTruthy();
    expect(document.getElementById(id!)).toBeNull();
    expect(warned()).toBe(true);
  });
});

/**
 * Every drawer in the app, checked at the source.
 *
 * A runtime test only covers the drawers someone remembered to write a test for;
 * this covers the ones they did not. It is file-granular on purpose — the two
 * spellings are far enough apart that a file containing neither is unambiguously
 * a call site that forgot.
 *
 * `*.test.tsx` is excluded because the third case above must render the broken
 * spelling to prove the warning is real.
 */
function tsxFilesUnder(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) return tsxFilesUnder(path);
    return entry.isFile() && entry.name.endsWith('.tsx') && !entry.name.endsWith('.test.tsx')
      ? [path]
      : [];
  });
}

describe('every SheetContent call site', () => {
  it('either renders a description or opts out explicitly', () => {
    const root = join(__dirname, '../..');
    const offenders = tsxFilesUnder(root)
      .map((path) => ({ path, source: readFileSync(path, 'utf8') }))
      .filter(({ source }) => source.includes('<SheetContent'))
      .filter(
        ({ source }) =>
          !source.includes('SheetDescription') && !source.includes('aria-describedby={undefined}')
      )
      .map(({ path }) => relative(root, path));

    expect(
      offenders,
      `These drawers leave Radix an aria-describedby that resolves to nothing, and warn on every ` +
        `open. Render a SheetDescription, or pass aria-describedby={undefined} if the drawer ` +
        `genuinely has no description — see the doc comment on SheetContent in ui/sheet.tsx.`
    ).toEqual([]);
  });

  // Cheap proof the sweep is actually reaching source files.
  it('is a check that found the drawers it is meant to police', () => {
    const root = join(__dirname, '../..');
    const callSites = tsxFilesUnder(root).filter((path) =>
      readFileSync(path, 'utf8').includes('<SheetContent')
    );
    expect(callSites.length).toBeGreaterThanOrEqual(3);
  });
});
