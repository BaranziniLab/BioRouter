import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  SIDEBAR_DEFAULT_WIDTH,
  SIDEBAR_MAX_WIDTH,
  SIDEBAR_MIN_WIDTH,
  SIDEBAR_WIDTH_KEYBOARD_STEP,
  SIDEBAR_WIDTH_STORAGE_KEY,
} from './sidebarWidth';

const isMobile = vi.hoisted(() => ({ value: false }));
vi.mock('../../hooks/use-mobile', () => ({
  useIsMobile: () => isMobile.value,
}));

import { Sidebar, SidebarProvider, SidebarTrigger } from './sidebar';

beforeEach(() => {
  localStorage.clear();
});

afterEach(() => {
  isMobile.value = false;
});

describe('responsive Sidebar', () => {
  beforeEach(() => {
    isMobile.value = true;
  });

  it('keeps the mobile drawer at the canonical width instead of sizing to its content', () => {
    render(
      <SidebarProvider>
        <Sidebar>
          <div>A conversation title long enough to exceed the sidebar width</div>
        </Sidebar>
        <SidebarTrigger />
      </SidebarProvider>
    );

    fireEvent.click(screen.getByRole('button', { name: 'Toggle sidebar' }));

    const drawer = screen.getByRole('dialog');
    expect(drawer).toHaveAttribute('data-mobile', 'true');
    expect(drawer).toHaveClass(
      'w-(--sidebar-width)',
      'min-w-(--sidebar-width)',
      'max-w-(--sidebar-width)'
    );
    expect(drawer).not.toHaveClass('!w-fit', '!max-w-none');
    // ⚠ A LITERAL, exactly as this line read `'15rem'` before the sidebar became
    // resizable. Writing `` `${SIDEBAR_DEFAULT_WIDTH}px` `` here reads as the
    // same assertion and is not one: the drawer's width IS that constant, so
    // both sides move together and the expectation can never fail because the
    // shipped width changed. The number is load-bearing outside this file — the
    // OS window's `minWidth` is derived from it (`main.ts`, pinned in
    // `styles/measures.test.ts`) — so moving it must be a deliberate act that
    // trips a test, not a silent one.
    expect(drawer.style.getPropertyValue('--sidebar-width')).toBe('288px');
    expect(SIDEBAR_DEFAULT_WIDTH).toBe(288);
  });

  /**
   * The drawer is a sheet over a narrow window, not a column beside content:
   * there is no edge to drag, and a width chosen on a wide desktop layout is a
   * decision about a layout that is not on screen. So the mobile width stays
   * canonical even after the user has resized the desktop sidebar.
   */
  it('does not follow a width the user chose on the desktop layout', () => {
    localStorage.setItem(SIDEBAR_WIDTH_STORAGE_KEY, String(SIDEBAR_MAX_WIDTH));

    render(
      <SidebarProvider>
        <Sidebar>
          <div>chats</div>
        </Sidebar>
        <SidebarTrigger />
      </SidebarProvider>
    );
    fireEvent.click(screen.getByRole('button', { name: 'Toggle sidebar' }));

    expect(screen.getByRole('dialog').style.getPropertyValue('--sidebar-width')).toBe(
      `${SIDEBAR_DEFAULT_WIDTH}px`
    );
  });
});

/**
 * The resizable sidebar, tested as a STATE MACHINE.
 *
 * ⚠ jsdom computes no layout, so none of this observes a width — it observes the
 * `--sidebar-width` variable the width is published through, and the persistence
 * behind it. That distinction is the point: the bugs this area produces are
 * state bugs (a handler that never wires up, a drag that never commits, a stored
 * value that escapes the bounds), and those are decidable here. Whether the
 * column actually moves is a browser question, verified by driving the app.
 */
describe('the resizable sidebar', () => {
  const renderSidebar = () =>
    render(
      <SidebarProvider>
        <Sidebar>
          <div>chats</div>
        </Sidebar>
      </SidebarProvider>
    );

  const widthVariable = () => {
    const wrapper = document.querySelector<HTMLElement>('[data-slot="sidebar-wrapper"]');
    if (!wrapper) throw new Error('the sidebar wrapper did not render');
    return wrapper.style.getPropertyValue('--sidebar-width');
  };

  const handle = () => screen.getByRole('separator', { name: 'Resize sidebar' });

  it('opens at the default width', () => {
    renderSidebar();
    expect(widthVariable()).toBe(`${SIDEBAR_DEFAULT_WIDTH}px`);
  });

  it('opens at the width the user last chose', () => {
    localStorage.setItem(SIDEBAR_WIDTH_STORAGE_KEY, '324');
    renderSidebar();
    expect(widthVariable()).toBe('324px');
  });

  /**
   * The read-side clamp, exercised through the component rather than only
   * through the pure module — this is the path a real stale value takes.
   */
  it('clamps a stored width from an earlier build into the current bounds', () => {
    localStorage.setItem(SIDEBAR_WIDTH_STORAGE_KEY, '15');
    renderSidebar();
    expect(widthVariable()).toBe(`${SIDEBAR_MIN_WIDTH}px`);
  });

  it('exposes its bounds on the handle, so the control is reachable without a pointer', () => {
    renderSidebar();
    expect(handle()).toHaveAttribute('aria-valuemin', String(SIDEBAR_MIN_WIDTH));
    expect(handle()).toHaveAttribute('aria-valuemax', String(SIDEBAR_MAX_WIDTH));
    expect(handle()).toHaveAttribute('aria-valuenow', String(SIDEBAR_DEFAULT_WIDTH));
    expect(handle()).toHaveAttribute('tabindex', '0');
  });

  it('moves the edge with the arrow keys and persists each step', () => {
    renderSidebar();

    fireEvent.keyDown(handle(), { key: 'ArrowRight' });
    expect(widthVariable()).toBe(`${SIDEBAR_DEFAULT_WIDTH + SIDEBAR_WIDTH_KEYBOARD_STEP}px`);
    expect(localStorage.getItem(SIDEBAR_WIDTH_STORAGE_KEY)).toBe(
      String(SIDEBAR_DEFAULT_WIDTH + SIDEBAR_WIDTH_KEYBOARD_STEP)
    );

    fireEvent.keyDown(handle(), { key: 'ArrowLeft' });
    fireEvent.keyDown(handle(), { key: 'ArrowLeft' });
    expect(widthVariable()).toBe(`${SIDEBAR_DEFAULT_WIDTH - SIDEBAR_WIDTH_KEYBOARD_STEP}px`);
  });

  it('jumps to either bound and cannot be pushed past it', () => {
    renderSidebar();

    fireEvent.keyDown(handle(), { key: 'End' });
    expect(widthVariable()).toBe(`${SIDEBAR_MAX_WIDTH}px`);
    fireEvent.keyDown(handle(), { key: 'ArrowRight' });
    expect(widthVariable()).toBe(`${SIDEBAR_MAX_WIDTH}px`);

    fireEvent.keyDown(handle(), { key: 'Home' });
    expect(widthVariable()).toBe(`${SIDEBAR_MIN_WIDTH}px`);
    fireEvent.keyDown(handle(), { key: 'ArrowLeft' });
    expect(widthVariable()).toBe(`${SIDEBAR_MIN_WIDTH}px`);
  });

  it('restores the default on a double-click', () => {
    localStorage.setItem(SIDEBAR_WIDTH_STORAGE_KEY, String(SIDEBAR_MAX_WIDTH));
    renderSidebar();
    expect(widthVariable()).toBe(`${SIDEBAR_MAX_WIDTH}px`);

    fireEvent.doubleClick(handle());
    expect(widthVariable()).toBe(`${SIDEBAR_DEFAULT_WIDTH}px`);
    expect(localStorage.getItem(SIDEBAR_WIDTH_STORAGE_KEY)).toBe(String(SIDEBAR_DEFAULT_WIDTH));
  });

  /**
   * The drag itself. Widths land a frame late (the move handler batches through
   * rAF, so a fast pointer cannot queue one setState per event), which is why
   * the assertions here are after the commit on pointerup rather than mid-move.
   */
  it('widens as the pointer moves right and persists once, at the end', () => {
    renderSidebar();

    fireEvent.pointerDown(handle(), { pointerId: 1, clientX: SIDEBAR_DEFAULT_WIDTH });
    // Committing on pointerup uses the latest sampled width directly, so the
    // result does not depend on whether a rAF happened to run first.
    fireEvent.pointerMove(window, { pointerId: 1, clientX: SIDEBAR_DEFAULT_WIDTH + 40 });
    expect(localStorage.getItem(SIDEBAR_WIDTH_STORAGE_KEY)).toBeNull();

    fireEvent.pointerUp(window, { pointerId: 1, clientX: SIDEBAR_DEFAULT_WIDTH + 40 });
    expect(widthVariable()).toBe(`${SIDEBAR_DEFAULT_WIDTH + 40}px`);
    expect(localStorage.getItem(SIDEBAR_WIDTH_STORAGE_KEY)).toBe(
      String(SIDEBAR_DEFAULT_WIDTH + 40)
    );
  });

  it('narrows as the pointer moves left, and stops at the floor', () => {
    renderSidebar();

    fireEvent.pointerDown(handle(), { pointerId: 1, clientX: 500 });
    fireEvent.pointerMove(window, { pointerId: 1, clientX: 0 });
    fireEvent.pointerUp(window, { pointerId: 1, clientX: 0 });

    expect(widthVariable()).toBe(`${SIDEBAR_MIN_WIDTH}px`);
  });

  /**
   * Every exit path funnels through one `finishResize`, and this is the one that
   * gets forgotten: a pointer released outside the window fires no `pointerup`
   * on the handle. Without the window-level listeners the body would keep
   * `col-resize` painted on it and the move listener would stay live.
   */
  it('ends the drag cleanly when the window loses focus mid-drag', () => {
    renderSidebar();

    // The pointer's DISPLACEMENT is what moves the edge, not its position: the
    // grab point is wherever the user took hold of the handle, so a drag that
    // starts at 300 and ends at 330 widens the sidebar by 30 from whatever it
    // already was.
    const settled = SIDEBAR_DEFAULT_WIDTH + 30;

    fireEvent.pointerDown(handle(), { pointerId: 1, clientX: 300 });
    expect(document.body.classList.contains('biorouter-sidebar-resizing')).toBe(true);
    expect(document.body.style.cursor).toBe('col-resize');

    fireEvent.pointerMove(window, { pointerId: 1, clientX: 330 });
    fireEvent.blur(window);

    expect(document.body.classList.contains('biorouter-sidebar-resizing')).toBe(false);
    expect(document.body.style.cursor).toBe('');
    expect(document.body.style.userSelect).toBe('');
    expect(localStorage.getItem(SIDEBAR_WIDTH_STORAGE_KEY)).toBe(String(settled));

    // The listener is gone: further movement must not move the edge.
    fireEvent.pointerMove(window, { pointerId: 1, clientX: 200 });
    expect(localStorage.getItem(SIDEBAR_WIDTH_STORAGE_KEY)).toBe(String(settled));
  });

  it('leaves no body styling behind when unmounted mid-drag', () => {
    const view = renderSidebar();

    fireEvent.pointerDown(handle(), { pointerId: 1, clientX: 300 });
    view.unmount();

    expect(document.body.classList.contains('biorouter-sidebar-resizing')).toBe(false);
    expect(document.body.style.cursor).toBe('');
    // A width the user was mid-way through choosing is not a width they chose.
    expect(localStorage.getItem(SIDEBAR_WIDTH_STORAGE_KEY)).toBeNull();
  });
});

/**
 * ⚠ Asserted AT THE SOURCE, and it has to be.
 *
 * The handle's whole affordance is a hover hairline and `cursor: col-resize`.
 * jsdom applies no stylesheet and never runs Tailwind, so a component test that
 * hovers the handle and reads its style sees nothing either way — and a Tailwind
 * utility that failed to generate would leave an invisible, apparently-dead edge
 * that reads as a missing feature rather than a build one. Same lesson, and the
 * same remedy, as `styles/composerFocus.test.ts`.
 */
describe('the resize handle is styled by authored CSS, not a generated utility', () => {
  const CSS = readFileSync(join(__dirname, '../../styles/main.css'), 'utf8');
  const SOURCE = readFileSync(join(__dirname, 'sidebar.tsx'), 'utf8');

  it('declares the cursor and the hover hairline in main.css', () => {
    expect(CSS).toMatch(/\.biorouter-sidebar-resize-handle\s*\{[^}]*cursor:\s*col-resize/);
    expect(CSS).toMatch(/\.biorouter-sidebar-resize-handle:hover::after/);
  });

  it('hides the handle while the sidebar is collapsed', () => {
    expect(CSS).toMatch(
      /\[data-slot='sidebar'\]\[data-state='collapsed'\]\s+\.biorouter-sidebar-resize-handle\s*\{\s*display:\s*none/
    );
  });

  /**
   * The drag must not be eased. `sidebar-gap` carries `transition-[width]` at
   * --motion-slow, so without this rule the column trails the pointer by a third
   * of a second and the sidebar feels detached from the hand moving it.
   */
  it('kills the width transition for the duration of a drag', () => {
    expect(CSS).toContain("body.biorouter-sidebar-resizing [data-slot='sidebar-gap']");
    expect(SOURCE).toContain("'biorouter-sidebar-resizing'");
  });
});
