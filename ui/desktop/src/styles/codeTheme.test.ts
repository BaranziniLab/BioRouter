import { describe, expect, it } from 'vitest';
import { CODE_BG, codePalettes, codePalettesAlma, codeThemeDark, codeThemeLight } from './codeTheme';

function luminance(hex: string): number {
  const channels = [1, 3, 5]
    .map((i) => parseInt(hex.slice(i, i + 2), 16) / 255)
    .map((v) => (v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4)));
  return 0.2126 * channels[0] + 0.7152 * channels[1] + 0.0722 * channels[2];
}

function contrast(fg: string, bg: string): number {
  const [a, b] = [luminance(fg), luminance(bg)];
  return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
}

describe('code theme', () => {
  it.each(['light', 'dark'] as const)('%s palette clears WCAG AA on its own ground', (theme) => {
    const bg = CODE_BG[theme];
    for (const [token, hex] of Object.entries(codePalettes[theme])) {
      expect(
        contrast(hex, bg),
        `${theme} "${token}" (${hex}) on ${bg} is ${contrast(hex, bg).toFixed(2)}:1`
      ).toBeGreaterThanOrEqual(4.5);
    }
  });

  it.each([
    ['light', codeThemeLight],
    ['dark', codeThemeDark],
  ] as const)('%s theme drives every colour through the mono font', (_theme, style) => {
    // A stock Prism theme would set a proportional font here. P6: code is monospace.
    expect(style['code[class*="language-"]'].fontFamily).toBe('var(--font-mono)');
    expect(style['pre[class*="language-"]'].fontFamily).toBe('var(--font-mono)');
  });

  it.each([
    ['light', codeThemeLight],
    ['dark', codeThemeDark],
  ] as const)('%s theme carries no text-shadow', (_theme, style) => {
    // Prism's defaults smear against a warm ground.
    expect(style['code[class*="language-"]'].textShadow).toBe('none');
  });

  it('uses a different hue per theme rather than sharing one', () => {
    expect(codePalettes.light.keyword).not.toBe(codePalettes.dark.keyword);
    expect(codePalettes.light.plain).not.toBe(codePalettes.dark.plain);
  });

  it.each(['light', 'dark'] as const)(
    'Alma Mater %s palette clears WCAG AA on its own ground',
    (theme) => {
      const { palette, bg } = codePalettesAlma[theme];
      for (const [token, hex] of Object.entries(palette)) {
        expect(
          contrast(hex, bg),
          `alma ${theme} "${token}" (${hex}) on ${bg} is ${contrast(hex, bg).toFixed(2)}:1`
        ).toBeGreaterThanOrEqual(4.5);
      }
    }
  );
});
