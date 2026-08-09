/**
 * Type surface for the hand-authored theme files.
 *
 * `themes/<id>.theme.mjs` is the single source of truth for a theme family, and
 * `npm run themes` generates `main.css`, `themes.generated.ts`, `index.html`'s
 * boot block and the picker from it. The `.mjs` extension is load-bearing: the
 * generator is a plain Node script, so the theme files have to stay runnable by
 * Node without a build step.
 *
 * ⚠ Without this declaration `tsc --noEmit` fails with TS7016 on every test that
 * imports a theme file, which breaks `lint:check` and therefore CI. That is
 * exactly what happened when `themeNeutrals.test.ts` landed: it imports all
 * three theme files to assert they share one set of neutrals, which is the only
 * way to check a rule nothing else enforces, and the type error came with it.
 *
 * The shape is deliberately loose. Pinning the full theme schema here would put
 * a second definition of it in the tree, and the generator already validates the
 * real one; a duplicate would drift and start rejecting valid themes.
 */
declare module '*.theme.mjs' {
  /**
   * Only the two things a consumer actually reaches for: the light and dark
   * blocks, and the token map inside each. Everything else stays `unknown` on
   * purpose, so this file cannot become a second, drifting copy of the schema.
   */
  interface ThemeModeBlock {
    tokens: Record<string, string>;
    [key: string]: unknown;
  }
  const theme: {
    light: ThemeModeBlock;
    dark: ThemeModeBlock;
    [key: string]: unknown;
  };
  export default theme;
}
