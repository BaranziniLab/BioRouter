/**
 * The Electron main process runs window compositing, IPC and input handling on
 * one thread. A synchronous child process there freezes the entire app for its
 * full duration — which is what issue #88 was: `spawnSync('biorouter doctor')`
 * with a 15 s budget, fired 4 s after launch, showing the macOS wait cursor and
 * an unresponsive window.
 *
 * The fix is structural (every probe is now `execFile`-based), so the guard has
 * to be structural too: the modules on the startup path may not reach for a
 * synchronous spawn again. A behavioural test cannot catch this — a reintroduced
 * `spawnSync` still returns the right answer, it just blocks while doing it.
 */
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'fs';
import * as path from 'path';
import {
  STARTUP_SCHEDULE,
  MIN_STARTUP_TASK_GAP_MS,
  STARTUP_UPDATER_SETUP_DELAY_MS,
  STARTUP_DEPENDENCY_CHECK_DELAY_MS,
  STARTUP_EXTENSION_CHECK_DELAY_MS,
} from './startupSchedule';

const SRC = path.join(__dirname, '..');

// Everything reachable from app-ready, or from a timer armed during app-ready.
const STARTUP_PATH_MODULES = [
  'main.ts',
  'biorouterd.ts',
  'utils/dependencyChecker.ts',
  'utils/extensionUpdater.ts',
  'utils/autoUpdater.ts',
  'utils/githubUpdater.ts',
  'utils/updateCheckSchedule.ts',
];

// `spawnSync` et al. block the event loop until the child exits.
const BLOCKING_SPAWN = /\b(spawnSync|execSync|execFileSync)\s*\(/;

function sourceOf(rel: string): string {
  return readFileSync(path.join(SRC, rel), 'utf8');
}

/** Strip block and line comments so a mention in prose isn't read as a call. */
function stripComments(src: string): string {
  return src.replace(/\/\*[\s\S]*?\*\//g, '').replace(/^\s*\/\/.*$/gm, '');
}

describe('startup path never blocks the main thread', () => {
  for (const rel of STARTUP_PATH_MODULES) {
    it(`${rel} contains no synchronous child-process call`, () => {
      const code = stripComments(sourceOf(rel));
      const offenders = code
        .split('\n')
        .map((line, i) => ({ line: line.trim(), n: i + 1 }))
        .filter(({ line }) => BLOCKING_SPAWN.test(line));

      expect(
        offenders,
        `${rel} reintroduces a blocking spawn on the Electron main thread (#88). ` +
          `Use runProbe() from utils/dependencyChecker, or child_process.spawn.\n` +
          offenders.map((o) => `  line ${o.n}: ${o.line}`).join('\n')
      ).toEqual([]);
    });
  }

  it('does not import a synchronous spawn helper', () => {
    for (const rel of STARTUP_PATH_MODULES) {
      const code = stripComments(sourceOf(rel));
      const imports = code.match(/import\s+\{[^}]*\}\s+from\s+'child_process'/g) ?? [];
      for (const imp of imports) {
        expect(imp, `${rel} imports a blocking spawn (#88)`).not.toMatch(
          /spawnSync|execSync|execFileSync/
        );
      }
    }
  });
});

describe('startup schedule', () => {
  it('runs its tasks in a fixed, non-overlapping order', () => {
    const delays = STARTUP_SCHEDULE.map((t) => t.delayMs);
    expect(delays).toEqual([...delays].sort((a, b) => a - b));

    for (let i = 1; i < STARTUP_SCHEDULE.length; i++) {
      const gap = STARTUP_SCHEDULE[i].delayMs - STARTUP_SCHEDULE[i - 1].delayMs;
      expect(
        gap,
        `${STARTUP_SCHEDULE[i - 1].name} → ${STARTUP_SCHEDULE[i].name} is only ${gap}ms apart`
      ).toBeGreaterThanOrEqual(MIN_STARTUP_TASK_GAP_MS);
    }
  });

  it('keeps every task out of the renderer first-paint window', () => {
    for (const task of STARTUP_SCHEDULE) {
      expect(task.delayMs, `${task.name} fires too early`).toBeGreaterThanOrEqual(2_000);
    }
  });

  it('orders updater setup before the dependency check before the extension check', () => {
    expect(STARTUP_UPDATER_SETUP_DELAY_MS).toBeLessThan(STARTUP_DEPENDENCY_CHECK_DELAY_MS);
    expect(STARTUP_DEPENDENCY_CHECK_DELAY_MS).toBeLessThan(STARTUP_EXTENSION_CHECK_DELAY_MS);
  });

  it('main.ts wires the schedule constants rather than inline numbers', () => {
    const main = sourceOf('main.ts');
    expect(main).toContain('STARTUP_UPDATER_SETUP_DELAY_MS');
    expect(main).toContain('setupDependencyChecker(STARTUP_DEPENDENCY_CHECK_DELAY_MS)');
    expect(main).toContain('scheduleExtensionUpdateCheck(STARTUP_EXTENSION_CHECK_DELAY_MS)');
  });
});
