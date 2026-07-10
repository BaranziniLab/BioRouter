import React, { createContext, useContext, useEffect, useState, useCallback } from 'react';

type ThemePreference = 'light' | 'dark' | 'system';
type ResolvedTheme = 'light' | 'dark';
/**
 * The theme *family* — orthogonal to light/dark. `parchment` is the warm
 * default; `alma-mater` is the UCSF brand palette. Written to `data-theme` on
 * <html>; main.css re-colours the same semantic tokens per family.
 */
export type ThemeFamily = 'parchment' | 'alma-mater';

interface ThemeContextValue {
  userThemePreference: ThemePreference;
  setUserThemePreference: (pref: ThemePreference) => void;
  resolvedTheme: ResolvedTheme;
  themeFamily: ThemeFamily;
  setThemeFamily: (family: ThemeFamily) => void;
}

const ThemeContext = createContext<ThemeContextValue | null>(null);

function getSystemTheme(): ResolvedTheme {
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

function resolveTheme(preference: ThemePreference): ResolvedTheme {
  if (preference === 'system') {
    return getSystemTheme();
  }
  return preference;
}

function loadThemePreference(): ThemePreference {
  const useSystemTheme = localStorage.getItem('use_system_theme');
  if (useSystemTheme === 'true') {
    return 'system';
  }

  const savedTheme = localStorage.getItem('theme');
  if (savedTheme === 'dark') {
    return 'dark';
  }

  return 'light';
}

function saveThemePreference(preference: ThemePreference): void {
  if (preference === 'system') {
    localStorage.setItem('use_system_theme', 'true');
  } else {
    localStorage.setItem('use_system_theme', 'false');
    localStorage.setItem('theme', preference);
  }
}

function applyThemeToDocument(theme: ResolvedTheme): void {
  const toRemove = theme === 'dark' ? 'light' : 'dark';
  document.documentElement.classList.add(theme);
  document.documentElement.classList.remove(toRemove);
}

function loadThemeFamily(): ThemeFamily {
  return localStorage.getItem('theme_family') === 'alma-mater' ? 'alma-mater' : 'parchment';
}

function saveThemeFamily(family: ThemeFamily): void {
  localStorage.setItem('theme_family', family);
}

function applyFamilyToDocument(family: ThemeFamily): void {
  // `data-theme` sits alongside the `.dark`/`.light` class; main.css scopes the
  // Alma Mater token overrides to `[data-theme='alma-mater']`. Parchment has no
  // matching rules, so the bare `:root`/`.dark` defaults render.
  document.documentElement.setAttribute('data-theme', family);
}

interface ThemeProviderProps {
  children: React.ReactNode;
}

export function ThemeProvider({ children }: ThemeProviderProps) {
  const [userThemePreference, setUserThemePreferenceState] =
    useState<ThemePreference>(loadThemePreference);
  const [resolvedTheme, setResolvedTheme] = useState<ResolvedTheme>(() =>
    resolveTheme(loadThemePreference())
  );
  const [themeFamily, setThemeFamilyState] = useState<ThemeFamily>(loadThemeFamily);

  const setUserThemePreference = useCallback(
    (preference: ThemePreference) => {
      setUserThemePreferenceState(preference);
      saveThemePreference(preference);

      const resolved = resolveTheme(preference);
      setResolvedTheme(resolved);

      // Broadcast to other windows via Electron (carry the family so windows converge)
      window.electron?.broadcastThemeChange({
        mode: resolved,
        useSystemTheme: preference === 'system',
        theme: resolved,
        themeFamily,
      });
    },
    [themeFamily]
  );

  const setThemeFamily = useCallback(
    (family: ThemeFamily) => {
      setThemeFamilyState(family);
      saveThemeFamily(family);
      applyFamilyToDocument(family);

      window.electron?.broadcastThemeChange({
        mode: resolvedTheme,
        useSystemTheme: userThemePreference === 'system',
        theme: resolvedTheme,
        themeFamily: family,
      });
    },
    [resolvedTheme, userThemePreference]
  );

  // Listen for system theme changes when preference is 'system'
  useEffect(() => {
    if (userThemePreference !== 'system') return;

    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');

    const handleChange = () => {
      setResolvedTheme(getSystemTheme());
    };

    mediaQuery.addEventListener('change', handleChange);
    return () => mediaQuery.removeEventListener('change', handleChange);
  }, [userThemePreference]);

  // Listen for theme changes from other windows (via Electron IPC)
  useEffect(() => {
    if (!window.electron) return;

    const handleThemeChanged = (_event: unknown, ...args: unknown[]) => {
      const themeData = args[0] as {
        useSystemTheme: boolean;
        theme: string;
        themeFamily?: string;
      };
      const newPreference: ThemePreference = themeData.useSystemTheme
        ? 'system'
        : themeData.theme === 'dark'
          ? 'dark'
          : 'light';

      setUserThemePreferenceState(newPreference);
      saveThemePreference(newPreference);
      setResolvedTheme(resolveTheme(newPreference));

      // The native app menu broadcasts without a family; only converge when present.
      if (themeData.themeFamily === 'alma-mater' || themeData.themeFamily === 'parchment') {
        setThemeFamilyState(themeData.themeFamily);
        saveThemeFamily(themeData.themeFamily);
        applyFamilyToDocument(themeData.themeFamily);
      }
    };

    return window.electron.on('theme-changed', handleThemeChanged);
  }, []);

  // Apply theme to document whenever resolvedTheme changes
  useEffect(() => {
    applyThemeToDocument(resolvedTheme);
  }, [resolvedTheme]);

  // Apply the theme family (data-theme) whenever it changes. The pre-hydration
  // script in index.html sets it first so there is no flash on load.
  useEffect(() => {
    applyFamilyToDocument(themeFamily);
  }, [themeFamily]);

  const value: ThemeContextValue = {
    userThemePreference,
    setUserThemePreference,
    resolvedTheme,
    themeFamily,
    setThemeFamily,
  };

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}

export function useTheme(): ThemeContextValue {
  const context = useContext(ThemeContext);
  if (!context) {
    throw new Error('useTheme must be used within a ThemeProvider');
  }
  return context;
}

/// The resolved theme, or `light` when rendered outside a `ThemeProvider`.
///
/// For leaf components (e.g. a syntax-highlighted code block) that want to match
/// the theme but must not force every caller — including tests and standalone
/// renders — to wrap themselves in a provider.
export function useResolvedTheme(): 'light' | 'dark' {
  return useContext(ThemeContext)?.resolvedTheme ?? 'light';
}

/// The active theme family, or `parchment` when rendered outside a `ThemeProvider`.
///
/// Non-throwing (like `useResolvedTheme`) so leaf components — the code block, the
/// artifact preview, and the standalone harness — can match the family without
/// forcing every caller into a provider.
export function useThemeFamily(): ThemeFamily {
  return useContext(ThemeContext)?.themeFamily ?? 'parchment';
}
