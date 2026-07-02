import type { DashboardWindow } from '../../contexts/DashboardContext';

type Direction = 'left' | 'right' | 'up' | 'down';

export type DashboardShortcutAction =
  | { type: 'spawn' }
  | { type: 'remove-focused' }
  | { type: 'focus-window'; windowId: string }
  | { type: 'toggle-window-fold'; windowId: string }
  | { type: 'toggle-focused-fold' }
  | { type: 'toggle-fold-mode' }
  | { type: 'fold-all' }
  | { type: 'unfold-all' }
  | { type: 'organize' };

interface ShortcutEventLike {
  key: string;
  metaKey: boolean;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  defaultPrevented?: boolean;
  target?: globalThis.EventTarget | null;
}

function isEditableTarget(target: globalThis.EventTarget | null | undefined): boolean {
  if (!(target instanceof globalThis.HTMLElement)) return false;
  if (target.isContentEditable) return true;
  if (target.closest('[contenteditable="true"]')) return true;
  if (target.closest('input, textarea, select, [role="textbox"]')) return true;
  return false;
}

function center(win: DashboardWindow): { x: number; y: number } {
  return {
    x: win.position.x + win.size.w / 2,
    y: win.position.y + win.size.h / 2,
  };
}

export function getVisualWindowOrder(windows: readonly DashboardWindow[]): DashboardWindow[] {
  return [...windows].sort((a, b) => {
    const ac = center(a);
    const bc = center(b);
    return ac.y - bc.y || ac.x - bc.x || a.lastInteraction - b.lastInteraction;
  });
}

function relativeWindowId(
  windows: readonly DashboardWindow[],
  focusedWindowId: string | null,
  delta: number
): string | null {
  if (windows.length === 0) return null;
  const ordered = getVisualWindowOrder(windows);
  const current = focusedWindowId ? ordered.findIndex((w) => w.windowId === focusedWindowId) : -1;
  const index = current >= 0 ? current : delta > 0 ? -1 : 0;
  const next = (index + delta + ordered.length) % ordered.length;
  return ordered[next]?.windowId ?? null;
}

function numberedWindowId(windows: readonly DashboardWindow[], key: string): string | null {
  if (!/^[1-9]$/.test(key)) return null;
  return getVisualWindowOrder(windows)[Number(key) - 1]?.windowId ?? null;
}

function directionalWindowId(
  windows: readonly DashboardWindow[],
  focusedWindowId: string | null,
  direction: Direction
): string | null {
  if (windows.length === 0) return null;
  const focused = focusedWindowId ? windows.find((w) => w.windowId === focusedWindowId) : null;
  if (!focused) return getVisualWindowOrder(windows)[0]?.windowId ?? null;

  const fc = center(focused);
  const others = windows.filter((w) => w.windowId !== focused.windowId);
  if (others.length === 0) return focused.windowId;

  const candidates = others
    .map((win) => {
      const c = center(win);
      const dx = c.x - fc.x;
      const dy = c.y - fc.y;
      const primary =
        direction === 'left' ? -dx : direction === 'right' ? dx : direction === 'up' ? -dy : dy;
      const secondary = direction === 'left' || direction === 'right' ? Math.abs(dy) : Math.abs(dx);
      return { win, primary, secondary };
    })
    .filter((item) => item.primary > 0)
    .sort((a, b) => a.primary * 4 + a.secondary - (b.primary * 4 + b.secondary));

  if (candidates[0]) return candidates[0].win.windowId;

  const wrapped = [...others].sort((a, b) => {
    const ac = center(a);
    const bc = center(b);
    if (direction === 'left') return bc.x - ac.x || ac.y - bc.y;
    if (direction === 'right') return ac.x - bc.x || ac.y - bc.y;
    if (direction === 'up') return bc.y - ac.y || ac.x - bc.x;
    return ac.y - bc.y || ac.x - bc.x;
  });
  return wrapped[0]?.windowId ?? null;
}

export function getDashboardShortcutAction(
  event: ShortcutEventLike,
  windows: readonly DashboardWindow[],
  focusedWindowId: string | null
): DashboardShortcutAction | null {
  if (event.defaultPrevented) return null;
  const editable = isEditableTarget(event.target);
  const mod = event.metaKey || event.ctrlKey;
  if (!mod) return null;

  const key = event.key.length === 1 ? event.key.toLowerCase() : event.key;

  if (!event.altKey && event.shiftKey) {
    if (key === 'n') return { type: 'spawn' };
    if (key === 'w' && focusedWindowId && !editable) return { type: 'remove-focused' };
    return null;
  }

  if (!event.altKey) return null;

  const numbered = numberedWindowId(windows, event.key);
  if (numbered) {
    return event.shiftKey
      ? { type: 'toggle-window-fold', windowId: numbered }
      : { type: 'focus-window', windowId: numbered };
  }

  if (event.shiftKey) return null;

  if (key === '[') {
    const windowId = relativeWindowId(windows, focusedWindowId, -1);
    return windowId ? { type: 'focus-window', windowId } : null;
  }
  if (key === ']') {
    const windowId = relativeWindowId(windows, focusedWindowId, 1);
    return windowId ? { type: 'focus-window', windowId } : null;
  }
  if (
    event.key === 'ArrowLeft' ||
    event.key === 'ArrowRight' ||
    event.key === 'ArrowUp' ||
    event.key === 'ArrowDown'
  ) {
    const direction = event.key.replace('Arrow', '').toLowerCase() as Direction;
    const windowId = directionalWindowId(windows, focusedWindowId, direction);
    return windowId ? { type: 'focus-window', windowId } : null;
  }
  if (event.key === 'Enter' && focusedWindowId) return { type: 'toggle-focused-fold' };
  if ((event.key === 'Backspace' || event.key === 'Delete') && focusedWindowId && !editable) {
    return { type: 'remove-focused' };
  }
  if (key === 'n') return { type: 'spawn' };
  if (key === 'f') return { type: 'toggle-fold-mode' };
  if (key === 'a') return { type: 'fold-all' };
  if (key === 'e') return { type: 'unfold-all' };
  if (key === 'o') return { type: 'organize' };

  return null;
}
