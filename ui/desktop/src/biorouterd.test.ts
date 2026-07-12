import type { App } from 'electron';
import { afterAll, beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  logInfo: vi.fn(),
  logError: vi.fn(),
  spawn: vi.fn(),
}));

vi.mock('./utils/logger', () => ({
  default: {
    info: mocks.logInfo,
    error: mocks.logError,
  },
}));

vi.mock('child_process', async (importOriginal) => {
  const actual = await importOriginal<typeof import('child_process')>();
  const mocked = {
    ...actual,
    spawn: mocks.spawn,
  };
  return {
    ...mocked,
    default: mocked,
  };
});

import { startBiorouterd } from './biorouterd';

describe('startBiorouterd logging', () => {
  const inheritedKey = 'BIOROUTER_TEST_INHERITED_VALUE';
  const inheritedValue = 'inherited-value-sentinel';
  const overrideValue = 'override-value-sentinel';
  const serverSecret = 'server-secret-sentinel';
  const previousInheritedValue = process.env[inheritedKey];

  beforeEach(() => {
    process.env[inheritedKey] = inheritedValue;
    mocks.logInfo.mockClear();
    mocks.logError.mockClear();
    mocks.spawn.mockReset();
    mocks.spawn.mockReturnValue({
      stdout: { on: vi.fn() },
      stderr: { on: vi.fn() },
      on: vi.fn(),
      kill: vi.fn(),
      unref: vi.fn(),
    });
  });

  afterAll(() => {
    if (previousInheritedValue === undefined) {
      delete process.env[inheritedKey];
    } else {
      process.env[inheritedKey] = previousInheritedValue;
    }
  });

  it('passes environment values to the child without writing them to logs', async () => {
    const app = {
      isPackaged: false,
      on: vi.fn(),
    } as unknown as App;

    await startBiorouterd({
      app,
      serverSecret,
      dir: process.cwd(),
      env: { BIOROUTER_TEST_OVERRIDE_VALUE: overrideValue },
    });

    const spawnOptions = mocks.spawn.mock.calls[0]?.[2];
    expect(spawnOptions?.env?.[inheritedKey]).toBe(inheritedValue);
    expect(spawnOptions?.env?.BIOROUTER_TEST_OVERRIDE_VALUE).toBe(overrideValue);
    expect(spawnOptions?.env?.BIOROUTER_SERVER__SECRET_KEY).toBe(serverSecret);

    const logged = JSON.stringify(mocks.logInfo.mock.calls);
    expect(logged).not.toContain(inheritedValue);
    expect(logged).not.toContain(overrideValue);
    expect(logged).not.toContain(serverSecret);
  });
});
