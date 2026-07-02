import { describe, expect, it } from 'vitest';
import type { DashboardWindow } from '../../contexts/DashboardContext';
import { getDashboardShortcutAction, getVisualWindowOrder } from './dashboardShortcuts';

function win(
  windowId: string,
  x: number,
  y: number,
  over: Partial<DashboardWindow> = {}
): DashboardWindow {
  return {
    windowId,
    sessionId: `session-${windowId}`,
    name: windowId,
    userSetName: false,
    badge: 1,
    accentColor: '#000',
    position: { x, y },
    size: { w: 100, h: 100 },
    isManuallyPlaced: true,
    lastInteraction: 1,
    unreadActivity: false,
    folded: false,
    isBusy: false,
    ...over,
  };
}

function event(
  over: Partial<globalThis.KeyboardEvent> & { target?: globalThis.EventTarget | null }
) {
  return {
    key: '',
    metaKey: true,
    ctrlKey: false,
    altKey: true,
    shiftKey: false,
    defaultPrevented: false,
    target: null,
    ...over,
  };
}

describe('dashboard shortcuts', () => {
  const windows = [win('top-left', 0, 0), win('top-right', 300, 0), win('bottom-left', 0, 300)];

  it('orders windows visually for number and bracket shortcuts', () => {
    expect(
      getVisualWindowOrder([windows[2], windows[1], windows[0]]).map((w) => w.windowId)
    ).toEqual(['top-left', 'top-right', 'bottom-left']);
  });

  it('keeps existing spawn and remove shortcuts', () => {
    expect(
      getDashboardShortcutAction(event({ key: 'n', altKey: false, shiftKey: true }), windows, null)
    ).toEqual({ type: 'spawn' });
    expect(
      getDashboardShortcutAction(
        event({ key: 'w', altKey: false, shiftKey: true }),
        windows,
        'top-left'
      )
    ).toEqual({ type: 'remove-focused' });
  });

  it('spawns from the dashboard mnemonic shortcut', () => {
    expect(getDashboardShortcutAction(event({ key: 'n' }), windows, null)).toEqual({
      type: 'spawn',
    });
  });

  it('keeps command shortcuts available from chat inputs', () => {
    const input = document.createElement('textarea');
    expect(getDashboardShortcutAction(event({ key: 'n', target: input }), windows, null)).toEqual({
      type: 'spawn',
    });
    expect(
      getDashboardShortcutAction(event({ key: 'o', target: input }), windows, 'top-left')
    ).toEqual({
      type: 'organize',
    });
  });

  it('focuses directionally with arrow keys', () => {
    expect(getDashboardShortcutAction(event({ key: 'ArrowRight' }), windows, 'top-left')).toEqual({
      type: 'focus-window',
      windowId: 'top-right',
    });
    expect(getDashboardShortcutAction(event({ key: 'ArrowDown' }), windows, 'top-left')).toEqual({
      type: 'focus-window',
      windowId: 'bottom-left',
    });
  });

  it('cycles focus with brackets', () => {
    expect(getDashboardShortcutAction(event({ key: ']' }), windows, 'top-left')).toEqual({
      type: 'focus-window',
      windowId: 'top-right',
    });
    expect(getDashboardShortcutAction(event({ key: '[' }), windows, 'top-left')).toEqual({
      type: 'focus-window',
      windowId: 'bottom-left',
    });
  });

  it('targets specific conversations by visual number', () => {
    expect(getDashboardShortcutAction(event({ key: '2' }), windows, 'top-left')).toEqual({
      type: 'focus-window',
      windowId: 'top-right',
    });
    expect(
      getDashboardShortcutAction(event({ key: '3', shiftKey: true }), windows, 'top-left')
    ).toEqual({
      type: 'toggle-window-fold',
      windowId: 'bottom-left',
    });
  });

  it('maps fold, expand, organize, and remove actions to mnemonic keys', () => {
    expect(getDashboardShortcutAction(event({ key: 'Enter' }), windows, 'top-left')).toEqual({
      type: 'toggle-focused-fold',
    });
    expect(getDashboardShortcutAction(event({ key: 'f' }), windows, 'top-left')).toEqual({
      type: 'toggle-fold-mode',
    });
    expect(getDashboardShortcutAction(event({ key: 'a' }), windows, 'top-left')).toEqual({
      type: 'fold-all',
    });
    expect(getDashboardShortcutAction(event({ key: 'e' }), windows, 'top-left')).toEqual({
      type: 'unfold-all',
    });
    expect(getDashboardShortcutAction(event({ key: 'o' }), windows, 'top-left')).toEqual({
      type: 'organize',
    });
    expect(getDashboardShortcutAction(event({ key: 'Backspace' }), windows, 'top-left')).toEqual({
      type: 'remove-focused',
    });
  });

  it('ignores shortcuts while typing in editable controls', () => {
    const input = document.createElement('input');
    expect(
      getDashboardShortcutAction(event({ key: 'Backspace', target: input }), windows, 'top-left')
    ).toBeNull();
  });
});
