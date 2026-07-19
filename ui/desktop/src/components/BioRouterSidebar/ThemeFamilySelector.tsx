import React from 'react';
import { GENERATED_THEMES, THEME_FAMILY_IDS } from '../../styles/themes.generated';
import { Button } from '../ui/button';
import { useTheme, type ThemeFamily } from '../../contexts/ThemeContext';

interface ThemeFamilySelectorProps {
  className?: string;
  hideTitle?: boolean;
  horizontal?: boolean;
}

/**
 * Selects the theme *family* (Parchment / Alma Mater / Roche Limit) — the second
 * axis beside the light/dark ThemeSelector. Both render the same segmented-button
 * look, so they read as siblings in the Appearance settings. The small swatch is
 * the family's accent (terracotta for Parchment, UCSF teal for Alma Mater,
 * Jupyter-adjacent orange for Roche Limit).
 */
/**
 * Tailwind generates utilities by scanning source for literal class names, so
 * an interpolated `grid-cols-${n}` would silently produce an unstyled grid.
 * These literals are what make a new family's column appear.
 */
const GRID_COLS: Record<number, string> = {
  1: 'grid-cols-1', 2: 'grid-cols-2', 3: 'grid-cols-3',
  4: 'grid-cols-4', 5: 'grid-cols-5', 6: 'grid-cols-6',
};

const FAMILIES: { id: ThemeFamily; label: string; swatch: string }[] = THEME_FAMILY_IDS.map(
  (id) => ({
    id,
    label: GENERATED_THEMES[id].label,
    swatch: GENERATED_THEMES[id].swatch,
  })
);

const ThemeFamilySelector: React.FC<ThemeFamilySelectorProps> = ({
  className = '',
  hideTitle = false,
  horizontal = false,
}) => {
  const { themeFamily, setThemeFamily } = useTheme();

  return (
    <div className={`${!horizontal ? 'px-1 py-2 space-y-2' : ''} ${className}`}>
      {!hideTitle && <div className="text-xs text-text-default px-3">Palette</div>}
      <div
        className={`${horizontal ? 'flex' : `grid ${GRID_COLS[FAMILIES.length] ?? 'grid-cols-3'}`} gap-1 ${!horizontal ? 'px-3' : ''}`}
      >
        {FAMILIES.map((family) => {
          const active = themeFamily === family.id;
          return (
            <Button
              key={family.id}
              data-testid={`theme-family-${family.id}-button`}
              onClick={() => setThemeFamily(family.id)}
              className={`flex items-center justify-center gap-2 p-2 rounded-md border transition-colors text-xs ${
                active
                  ? 'bg-background-accent text-text-on-accent border-border-accent hover:!bg-background-accent hover:!text-text-on-accent'
                  : 'border-border-default hover:!bg-background-muted text-text-muted hover:text-text-default'
              }`}
              variant="ghost"
              size="sm"
            >
              <span
                aria-hidden
                className="h-2.5 w-2.5 rounded-full flex-none"
                style={{
                  backgroundColor: active ? 'currentColor' : family.swatch,
                  boxShadow: active ? 'none' : 'inset 0 0 0 1px rgba(0, 0, 0, 0.15)',
                }}
              />
              <span>{family.label}</span>
            </Button>
          );
        })}
      </div>
    </div>
  );
};

export default ThemeFamilySelector;
