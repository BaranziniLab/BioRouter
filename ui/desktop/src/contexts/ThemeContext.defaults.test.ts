import { describe, expect, it, beforeEach } from 'vitest';
import { loadThemePreferenceForTest } from './ThemeContext';

/**
 * The default theme is AUTO (follow the OS), not light.
 *
 * This is a behaviour change, but it also closes a real bug: index.html's
 * pre-hydration script already resolves an unset preference to the system
 * theme (`savedTheme ? savedTheme === 'dark' : systemPrefersDark`), while this
 * module returned a hard-coded 'light'. On a dark-mode machine the window
 * painted DARK and then React hydrated and forced it LIGHT — a flash on every
 * launch, because the two answered the same question differently.
 *
 * These cases must stay in lockstep with that script. If you change one,
 * change both.
 */
describe('loadThemePreference', () => {
  beforeEach(() => localStorage.clear());

  it('defaults a fresh install to the system theme', () => {
    expect(loadThemePreferenceForTest()).toBe('system');
  });

  it('honours an explicit system choice', () => {
    localStorage.setItem('use_system_theme', 'true');
    expect(loadThemePreferenceForTest()).toBe('system');
  });

  it.each(['dark', 'light'] as const)('honours an explicit %s choice', (theme) => {
    localStorage.setItem('use_system_theme', 'false');
    localStorage.setItem('theme', theme);
    expect(loadThemePreferenceForTest()).toBe(theme);
  });

  // The upgrade path that matters: someone who picked a theme on an older
  // build, which wrote `theme` alone. They must NOT be dragged back to auto.
  it.each(['dark', 'light'] as const)(
    'honours a legacy %s choice stored without use_system_theme',
    (theme) => {
      localStorage.setItem('theme', theme);
      expect(loadThemePreferenceForTest()).toBe(theme);
    }
  );

  it('falls back to system when the stored theme is junk', () => {
    localStorage.setItem('theme', 'chartreuse');
    expect(loadThemePreferenceForTest()).toBe('system');
  });
});
