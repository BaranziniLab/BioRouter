import { describe, it, expect } from 'vitest';
import {
  initialUpdaterState,
  reduceUpdaterEvent,
  shouldShowUpdateModal,
  stateFromSnapshot,
  isNewerVersion,
  normalizeVersion,
  type UpdaterState,
} from './updaterState';

function fold(events: Array<{ event: string; data?: unknown }>): UpdaterState {
  return events.reduce(reduceUpdaterEvent, initialUpdaterState);
}

describe('normalizeVersion', () => {
  it('strips leading v and whitespace', () => {
    expect(normalizeVersion('v1.85.4')).toBe('1.85.4');
    expect(normalizeVersion('  1.85.4 ')).toBe('1.85.4');
    expect(normalizeVersion('V2.0.0')).toBe('2.0.0');
    expect(normalizeVersion(undefined)).toBe('');
    expect(normalizeVersion(null)).toBe('');
  });

  it('tolerates version-like values from runtime bridges', () => {
    expect(normalizeVersion({ version: 'v1.86.1' })).toBe('1.86.1');
    expect(normalizeVersion({ version: 2 })).toBe('2');
    expect(normalizeVersion({ raw: 'v1.86.1' })).toBe('');
  });
});

describe('isNewerVersion', () => {
  it('detects newer patch/minor/major', () => {
    expect(isNewerVersion('1.85.5', '1.85.4')).toBe(true);
    expect(isNewerVersion('1.86.0', '1.85.4')).toBe(true);
    expect(isNewerVersion('2.0.0', '1.85.4')).toBe(true);
  });
  it('rejects equal or older', () => {
    expect(isNewerVersion('1.85.4', '1.85.4')).toBe(false);
    expect(isNewerVersion('1.85.3', '1.85.4')).toBe(false);
    expect(isNewerVersion('1.9.0', '1.10.0')).toBe(false); // numeric, not lexical
  });
  it('tolerates the v prefix and uneven segment counts', () => {
    expect(isNewerVersion('v1.85.4', '1.85.4')).toBe(false);
    expect(isNewerVersion('1.85.4.1', '1.85.4')).toBe(true);
    expect(isNewerVersion('1.85', '1.85.0')).toBe(false);
  });
});

describe('reduceUpdaterEvent — happy path', () => {
  it('walks idle → checking → available(+progress) → downloaded', () => {
    let s = reduceUpdaterEvent(initialUpdaterState, { event: 'checking-for-update' });
    expect(s.phase).toBe('checking');

    s = reduceUpdaterEvent(s, { event: 'update-available', data: { version: '1.86.0' } });
    expect(s.phase).toBe('available');
    expect(s.latestVersion).toBe('1.86.0');
    expect(s.percent).toBe(0);

    s = reduceUpdaterEvent(s, { event: 'download-progress', data: { percent: 42 } });
    expect(s.phase).toBe('available');
    expect(s.percent).toBe(42);

    s = reduceUpdaterEvent(s, { event: 'update-downloaded', data: { version: '1.86.0' } });
    expect(s.phase).toBe('downloaded');
    expect(s.percent).toBe(100);
    expect(s.latestVersion).toBe('1.86.0');
  });

  it('normalizes a v-prefixed version from event data', () => {
    const s = fold([{ event: 'update-available', data: { version: 'v1.86.0' } }]);
    expect(s.latestVersion).toBe('1.86.0');
  });
});

describe('reduceUpdaterEvent — progress is monotonic', () => {
  it('never rewinds the progress bar', () => {
    let s = fold([
      { event: 'update-available', data: { version: '1.86.0' } },
      { event: 'download-progress', data: { percent: 80 } },
    ]);
    s = reduceUpdaterEvent(s, { event: 'download-progress', data: { percent: 5 } });
    expect(s.percent).toBe(80);
  });

  it('clamps out-of-range percents', () => {
    const s = fold([
      { event: 'update-available' },
      { event: 'download-progress', data: { percent: 150 } },
    ]);
    expect(s.percent).toBe(100);
  });
});

describe('reduceUpdaterEvent — downloaded is sticky', () => {
  const downloaded = fold([
    { event: 'update-available', data: { version: '1.86.0' } },
    { event: 'update-downloaded', data: { version: '1.86.0' } },
  ]);

  it('survives a later background re-check', () => {
    const s = reduceUpdaterEvent(downloaded, { event: 'checking-for-update' });
    expect(s.phase).toBe('downloaded');
  });

  it('survives a later error (still installable)', () => {
    const s = reduceUpdaterEvent(downloaded, { event: 'error', data: 'network blip' });
    expect(s.phase).toBe('downloaded');
    expect(s.error).toBeUndefined();
  });

  it('survives update-not-available', () => {
    const s = reduceUpdaterEvent(downloaded, { event: 'update-not-available' });
    expect(s.phase).toBe('downloaded');
  });

  it('keeps version on a re-fired update-available', () => {
    const s = reduceUpdaterEvent(downloaded, { event: 'update-available', data: {} });
    expect(s.phase).toBe('downloaded');
    expect(s.latestVersion).toBe('1.86.0');
  });
});

describe('reduceUpdaterEvent — errors', () => {
  it('captures a string error message', () => {
    const s = reduceUpdaterEvent(initialUpdaterState, { event: 'error', data: 'boom' });
    expect(s.phase).toBe('error');
    expect(s.error).toBe('boom');
  });
  it('captures an Error instance message', () => {
    const s = reduceUpdaterEvent(initialUpdaterState, {
      event: 'error',
      data: new Error('kaboom'),
    });
    expect(s.error).toBe('kaboom');
  });
  it('falls back to a generic message', () => {
    const s = reduceUpdaterEvent(initialUpdaterState, { event: 'error', data: { weird: true } });
    expect(s.error).toMatch(/failed/i);
  });
});

describe('reduceUpdaterEvent — purity & unknown events', () => {
  it('does not mutate the previous state', () => {
    const prev = { ...initialUpdaterState };
    const snapshot = JSON.stringify(prev);
    reduceUpdaterEvent(prev, { event: 'update-available', data: { version: '9.9.9' } });
    expect(JSON.stringify(prev)).toBe(snapshot);
  });
  it('ignores unknown events', () => {
    const s = reduceUpdaterEvent(initialUpdaterState, { event: 'totally-unknown' });
    expect(s).toEqual(initialUpdaterState);
  });
});

describe('shouldShowUpdateModal', () => {
  it('shows for available / downloaded / error, hides otherwise', () => {
    expect(shouldShowUpdateModal({ ...initialUpdaterState, phase: 'available' })).toBe(true);
    expect(shouldShowUpdateModal({ ...initialUpdaterState, phase: 'downloaded' })).toBe(true);
    expect(shouldShowUpdateModal({ ...initialUpdaterState, phase: 'error' })).toBe(true);
    expect(shouldShowUpdateModal(initialUpdaterState)).toBe(false);
    expect(shouldShowUpdateModal({ ...initialUpdaterState, phase: 'checking' })).toBe(false);
    expect(shouldShowUpdateModal({ ...initialUpdaterState, phase: 'up-to-date' })).toBe(false);
  });
});

describe('stateFromSnapshot', () => {
  it('returns initial state for null', () => {
    expect(stateFromSnapshot(null)).toEqual(initialUpdaterState);
  });
  it('recovers a downloaded snapshot', () => {
    const s = stateFromSnapshot({ status: 'downloaded', latestVersion: 'v1.86.0', percent: 100 });
    expect(s.phase).toBe('downloaded');
    expect(s.latestVersion).toBe('1.86.0');
    expect(s.percent).toBe(100);
  });
  it('recovers an in-flight download with progress', () => {
    const s = stateFromSnapshot({ status: 'available', latestVersion: '1.86.0', percent: 33 });
    expect(s.phase).toBe('available');
    expect(s.percent).toBe(33);
  });
  it('maps legacy updateAvailable boolean to available', () => {
    const s = stateFromSnapshot({ updateAvailable: true, latestVersion: '1.86.0' });
    expect(s.phase).toBe('available');
  });
  it('carries the fallback flag', () => {
    const s = stateFromSnapshot({ status: 'downloaded', usingFallback: true });
    expect(s.usingFallback).toBe(true);
  });
});
