import { describe, it, expect } from 'vitest';

import { resolveNewSessionWorkingDir } from './BaseChat';

// #39 — the pre-session composer path. Choosing a directory on the /pair "New
// Session" screen must survive into the createSession call that the first
// message triggers; the app default applies only when nothing was chosen.
describe('resolveNewSessionWorkingDir (#39)', () => {
  it('prefers the directory the user picked before the session existed', () => {
    expect(resolveNewSessionWorkingDir('/Users/wgu/Desktop/data', '/default')).toBe(
      '/Users/wgu/Desktop/data'
    );
  });

  it('falls back to the app default when nothing was picked', () => {
    expect(resolveNewSessionWorkingDir(null, '/default')).toBe('/default');
    expect(resolveNewSessionWorkingDir(undefined, '/default')).toBe('/default');
  });

  it('treats empty and whitespace-only picks as not picked', () => {
    expect(resolveNewSessionWorkingDir('', '/default')).toBe('/default');
    expect(resolveNewSessionWorkingDir('   ', '/default')).toBe('/default');
  });
});
