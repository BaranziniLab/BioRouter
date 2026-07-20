import { describe, it, expect, vi } from 'vitest';
import {
  DEFAULT_MAX_TERMINAL_SESSIONS_PER_OWNER,
  TerminalSessionRegistry,
  maxTerminalSessionsPerOwner,
  terminalSessionLimitMessage,
  type RegisteredTerminalSession,
} from './terminalSessionRegistry';

type FakeSession = RegisteredTerminalSession & { killed: boolean };

function makeRegistry() {
  const registry = new TerminalSessionRegistry<FakeSession>();
  let nextId = 0;
  const open = (ownerId: number): string => {
    const id = `session-${(nextId += 1)}`;
    const session: FakeSession = {
      ownerId,
      killed: false,
      dispose: () => {
        session.killed = true;
      },
      removeOwnerDestroyedListener: vi.fn(),
    };
    registry.add(id, session);
    return id;
  };
  return { registry, open };
}

describe('terminal session limit', () => {
  it('defaults to a rail far above what a person opens by hand', () => {
    expect(maxTerminalSessionsPerOwner({})).toBe(DEFAULT_MAX_TERMINAL_SESSIONS_PER_OWNER);
    // The old hardcoded 8 was low enough that ordinary use hit it.
    expect(DEFAULT_MAX_TERMINAL_SESSIONS_PER_OWNER).toBeGreaterThanOrEqual(32);
  });

  it('honours BIOROUTER_MAX_TERMINAL_SESSIONS', () => {
    expect(maxTerminalSessionsPerOwner({ BIOROUTER_MAX_TERMINAL_SESSIONS: '200' })).toBe(200);
  });

  it('ignores a nonsense override rather than disabling the rail', () => {
    for (const raw of ['0', '-4', 'lots', '']) {
      expect(maxTerminalSessionsPerOwner({ BIOROUTER_MAX_TERMINAL_SESSIONS: raw })).toBe(
        DEFAULT_MAX_TERMINAL_SESSIONS_PER_OWNER
      );
    }
  });

  it('names both the limit and the knob in the refusal', () => {
    const message = terminalSessionLimitMessage(64);
    expect(message).toContain('64');
    expect(message).toContain('BIOROUTER_MAX_TERMINAL_SESSIONS');
  });
});

describe('TerminalSessionRegistry', () => {
  it('counts only the sessions a given window owns', () => {
    const { registry, open } = makeRegistry();
    open(1);
    open(1);
    open(2);

    expect(registry.countForOwner(1)).toBe(2);
    expect(registry.countForOwner(2)).toBe(1);
    expect(registry.countForOwner(3)).toBe(0);
  });

  it('refuses to hand a session to a window that does not own it', () => {
    const { registry, open } = makeRegistry();
    const id = open(1);

    expect(registry.getOwned(id, 1)).toBeDefined();
    expect(registry.getOwned(id, 2)).toBeUndefined();
  });

  it('frees the slot when a session is released', () => {
    const { registry, open } = makeRegistry();
    const id = open(1);
    const session = registry.get(id)!;

    expect(registry.release(id)).toBe(true);
    expect(session.killed).toBe(true);
    expect(registry.countForOwner(1)).toBe(0);
    // Releasing twice is a no-op, not a double-kill.
    expect(registry.release(id)).toBe(false);
  });

  /**
   * THE REGRESSION.
   *
   * A renderer reload (Cmd+R, View > Reload, the `reload-app` IPC) replaces the
   * document without destroying the webContents, so React never runs its effect
   * cleanups and the renderer never sends `terminal:dispose`. Before
   * `releaseOwner` existed there was no path that freed those slots, so every
   * reload permanently burned a window's terminal budget — the cap then fired
   * with no terminals visibly open.
   */
  it('releases every session a window owns when its document is replaced', () => {
    const { registry, open } = makeRegistry();
    const mine = [open(7), open(7), open(7)];
    const theirs = open(9);
    const survivor = registry.get(theirs)!;

    expect(registry.releaseOwner(7)).toBe(3);

    expect(registry.countForOwner(7)).toBe(0);
    for (const id of mine) expect(registry.get(id)).toBeUndefined();
    // Another window's shells are untouched.
    expect(registry.countForOwner(9)).toBe(1);
    expect(survivor.killed).toBe(false);
  });

  it('lets a window reopen its full budget after a reload — the ceiling does not decay', () => {
    const { registry, open } = makeRegistry();
    const limit = maxTerminalSessionsPerOwner({ BIOROUTER_MAX_TERMINAL_SESSIONS: '8' });

    // Three reload cycles. Each fills the window to the rail, then reloads.
    for (let cycle = 0; cycle < 3; cycle += 1) {
      let opened = 0;
      while (registry.countForOwner(1) < limit) {
        open(1);
        opened += 1;
      }
      // Every cycle must be able to open the SAME number of terminals. Without
      // releaseOwner this is 8, then 0, then 0.
      expect(opened).toBe(limit);
      registry.releaseOwner(1);
      expect(registry.countForOwner(1)).toBe(0);
    }
  });

  it('releaseOwner on a window holding nothing is a no-op', () => {
    const { registry, open } = makeRegistry();
    open(1);
    expect(registry.releaseOwner(42)).toBe(0);
    expect(registry.size).toBe(1);
  });

  it('forget drops a self-exited shell without killing it again', () => {
    const { registry, open } = makeRegistry();
    const id = open(1);
    const session = registry.get(id)!;

    expect(registry.forget(id)).toBe(session);
    expect(session.killed).toBe(false);
    expect(registry.countForOwner(1)).toBe(0);
  });

  it('keeps releasing the rest when one shell throws on the way down', () => {
    const onDisposeError = vi.fn();
    const registry = new TerminalSessionRegistry<FakeSession>(onDisposeError);
    const bad: FakeSession = {
      ownerId: 1,
      killed: false,
      dispose: () => {
        throw new Error('pty already gone');
      },
      removeOwnerDestroyedListener: vi.fn(),
    };
    const good: FakeSession = {
      ownerId: 1,
      killed: false,
      dispose: () => {
        good.killed = true;
      },
      removeOwnerDestroyedListener: vi.fn(),
    };
    registry.add('bad', bad);
    registry.add('good', good);

    expect(registry.releaseOwner(1)).toBe(2);
    expect(good.killed).toBe(true);
    expect(registry.size).toBe(0);
    expect(onDisposeError).toHaveBeenCalledTimes(1);
  });
});
