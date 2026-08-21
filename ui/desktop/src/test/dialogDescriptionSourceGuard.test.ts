import { readFileSync, readdirSync } from 'node:fs';
import { join, relative } from 'node:path';
import { describe, expect, it } from 'vitest';
import { findUnsettledDialogSurfaces } from './dialogDescriptionSourceGuard';

function productionTsxFiles(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) return productionTsxFiles(path);
    return entry.isFile() && entry.name.endsWith('.tsx') && !entry.name.endsWith('.test.tsx')
      ? [path]
      : [];
  });
}

describe('dialog description source guard', () => {
  it('checks each call site when one file contains both settled and unsettled dialogs', () => {
    const source = `
      import { DialogContent, DialogDescription } from './dialog';
      export const Example = () => <>
        <DialogContent><DialogDescription>Settled</DialogDescription></DialogContent>
        <DialogContent />
      </>;
    `;

    expect(findUnsettledDialogSurfaces(source)).toEqual([
      expect.objectContaining({ component: 'DialogContent', line: 5 }),
    ]);
  });

  it('does not let a nested surface lend its description to its parent or child', () => {
    const source = `
      import { DialogContent, DialogDescription } from './dialog';
      export const Example = () => <>
        <DialogContent>
          <DialogContent><DialogDescription>Child only</DialogDescription></DialogContent>
        </DialogContent>
        <DialogContent>
          <DialogDescription>Parent only</DialogDescription>
          <DialogContent />
        </DialogContent>
      </>;
    `;

    expect(findUnsettledDialogSurfaces(source).map(({ line }) => line)).toEqual([4, 9]);
  });

  it('recognizes aliased primitives and an explicit opt-out', () => {
    const source = `
      import { SheetContent as Surface, SheetDescription as Description } from './sheet';
      export const Example = () => <>
        <Surface aria-describedby={undefined} />
        <Surface><Description>Details</Description></Surface>
      </>;
    `;

    expect(findUnsettledDialogSurfaces(source)).toEqual([]);
  });

  it('finds no unsettled production DialogContent or SheetContent call sites', () => {
    const root = join(__dirname, '..');
    const files = productionTsxFiles(root);
    const callSites = files.flatMap((path) =>
      findUnsettledDialogSurfaces(readFileSync(path, 'utf8'), relative(root, path))
    );

    expect(
      callSites,
      `Each DialogContent and SheetContent must own a matching description or pass ` +
        `aria-describedby={undefined}. A description inside a nested surface does not settle its parent.`
    ).toEqual([]);
    expect(files.length).toBeGreaterThan(100);
  });
});
