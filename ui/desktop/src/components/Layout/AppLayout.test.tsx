import { describe, expect, it } from 'vitest';
import { sidebarAutoCollapseAction } from './AppLayout';

/**
 * The user-reported bug: "the responsiveness of the sidebar collapse button is
 * not working well once the side bar is collapsed (cant bring it back)".
 *
 * Measured in the real app before the fix (Electron, real layout):
 *   1400px, expanded → click toggle → collapsed (railX -240)   ok
 *   shrink to 1000px → still collapsed                          ok
 *   click toggle to reopen → STILL COLLAPSED (railX -240)       BUG
 *
 * Cause: the width watcher had `open` in its deps, so it re-ran as a
 * reconciliation on the user's own toggle and immediately undid it. These tests
 * pin the rule that makes that impossible — auto-collapse fires ONLY on a width
 * crossing. The geometry is verified by driving the app; what is unit-testable
 * here is the decision, which is where the bug actually lived.
 */
describe('sidebarAutoCollapseAction', () => {
  it('does nothing when the width did not cross the threshold', () => {
    // THE REGRESSION. Narrow window, sidebar closed, ref never armed; the user
    // clicks to reopen (open=true) and the watcher re-runs. Before the fix this
    // returned 'collapse' and swallowed the click.
    expect(
      sidebarAutoCollapseAction({
        wasCompact: true,
        isCompact: true,
        open: true,
        autoCollapsed: false,
      })
    ).toBe('none');
  });

  it('never fights a toggle at a stable width, in any ref/open combination', () => {
    for (const compact of [true, false]) {
      for (const open of [true, false]) {
        for (const autoCollapsed of [true, false]) {
          expect(
            sidebarAutoCollapseAction({
              wasCompact: compact,
              isCompact: compact,
              open,
              autoCollapsed,
            })
          ).toBe('none');
        }
      }
    }
  });

  it('collapses when crossing INTO compact while the sidebar is showing', () => {
    expect(
      sidebarAutoCollapseAction({
        wasCompact: false,
        isCompact: true,
        open: true,
        autoCollapsed: false,
      })
    ).toBe('collapse');
  });

  it('collapses on the very first sample in a compact window', () => {
    expect(
      sidebarAutoCollapseAction({
        wasCompact: null,
        isCompact: true,
        open: true,
        autoCollapsed: false,
      })
    ).toBe('collapse');
  });

  it('does not collapse a sidebar the user had already closed', () => {
    expect(
      sidebarAutoCollapseAction({
        wasCompact: false,
        isCompact: true,
        open: false,
        autoCollapsed: false,
      })
    ).toBe('none');
  });

  it('restores on crossing OUT of compact only if it auto-collapsed it', () => {
    expect(
      sidebarAutoCollapseAction({
        wasCompact: true,
        isCompact: false,
        open: false,
        autoCollapsed: true,
      })
    ).toBe('restore');
  });

  it('leaves a hand-closed sidebar closed when the window grows again', () => {
    expect(
      sidebarAutoCollapseAction({
        wasCompact: true,
        isCompact: false,
        open: false,
        autoCollapsed: false,
      })
    ).toBe('none');
  });

  it('replays the exact reported sequence without ever swallowing the toggle', () => {
    // Model the refs the effect keeps, and drive the real sequence.
    let wasCompact: boolean | null = null;
    let autoCollapsed = false;
    let open = true;

    const sample = (isCompact: boolean) => {
      const action = sidebarAutoCollapseAction({ wasCompact, isCompact, open, autoCollapsed });
      wasCompact = isCompact;
      if (action === 'collapse') {
        autoCollapsed = true;
        open = false;
      } else if (action === 'restore') {
        autoCollapsed = false;
        open = true;
      }
      return action;
    };
    // The watcher re-runs after any state change; at a stable width it must
    // always be a no-op.
    const settle = (isCompact: boolean) => sample(isCompact);

    sample(false); // 1. load wide, sidebar open
    expect(open).toBe(true);

    open = false; // 2. user collapses by hand
    expect(settle(false)).toBe('none');
    expect(open).toBe(false);

    expect(sample(true)).toBe('none'); // 3. shrink under the threshold
    expect(open).toBe(false);

    open = true; // 4. user clicks the toggle to bring it back
    expect(settle(true)).toBe('none'); // <-- was 'collapse' before the fix
    expect(open).toBe(true); // the rail STAYS open on the first click
  });
});
