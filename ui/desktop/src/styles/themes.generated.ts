/**
 * GENERATED — do not edit. Run `npm run themes` after changing a theme
 * definition in themes/*.theme.mjs.
 *
 * Carries the theme values that cannot live in CSS: xterm paints to a canvas
 * and react-syntax-highlighter takes a JS object, so neither can read a
 * custom property. Everything here is derived from the same definition that
 * produced the CSS token blocks, which is what keeps the two in step — these
 * values were previously hand-copied and drifted silently.
 */

export type SyntaxPalette = {
  plain: string;
  comment: string;
  keyword: string;
  string: string;
  number: string;
  func: string;
  type: string;
  operator: string;
  deleted: string;
  inserted: string;
};

export type TerminalPalette = {
  background: string;
  foreground: string;
  cursor: string;
  cursorAccent: string;
  selectionBackground: string;
  black: string;
  red: string;
  green: string;
  yellow: string;
  blue: string;
  magenta: string;
  cyan: string;
  white: string;
  brightBlack: string;
  brightRed: string;
  brightGreen: string;
  brightYellow: string;
  brightBlue: string;
  brightMagenta: string;
  brightCyan: string;
  brightWhite: string;
};

export type ThemeModeData = {
  /**
   * The brand mark's two inks for this family and mode. The boot splash paints
   * these before React exists, and BioRouterMark paints them after — reading
   * both from here is what stops the two disagreeing. Roche Limit used to flash
   * an orange mark that React then re-rendered in Parchment's coral.
   */
  mark: { navy: string; coral: string };
  syntax: SyntaxPalette;
  /** The surface the syntax palette is measured against (--background-code). */
  codeGround: string;
  /** Which token the terminal dock paints. Families genuinely differ. */
  terminalGround: string;
  terminal: TerminalPalette;
};

export type GeneratedTheme = {
  id: string;
  label: string;
  swatch: string;
  light: ThemeModeData;
  dark: ThemeModeData;
};

export const GENERATED_THEMES = {
  parchment: {
    id: 'parchment',
    label: 'Parchment',
    swatch: '#cf6d47',
    light: {
      mark: { navy: '#052049', coral: '#b85a32' },
      syntax: {
        plain: '#2a2520',
        comment: '#6f6659',
        keyword: '#a94f2a',
        string: '#22784f',
        number: '#8a5a00',
        func: '#255fb5',
        type: '#7847b8',
        operator: '#6e6760',
        deleted: '#b3261e',
        inserted: '#1f7a3d',
      },
      codeGround: '#faf8f3',
      terminalGround: '--background-muted',
      terminal: {
        background: '#faf8f3',
        cursorAccent: '#faf8f3',
        foreground: '#2d2a26',
        cursor: '#b85a32',
        selectionBackground: '#e4d9c3',
        black: '#2d2a26',
        red: '#b63f3f',
        green: '#22784f',
        yellow: '#9b6818',
        blue: '#255fb5',
        magenta: '#7847b8',
        cyan: '#107e89',
        white: '#574f46',
        brightBlack: '#6f6659',
        brightRed: '#d45252',
        brightGreen: '#1f7a3d',
        brightYellow: '#8a5a00',
        brightBlue: '#2f75d6',
        brightMagenta: '#9462d6',
        brightCyan: '#1f9aa6',
        brightWhite: '#2d2a26',
      },
    },
    dark: {
      mark: { navy: '#18a3ac', coral: '#b85a32' },
      syntax: {
        plain: '#e8e1d2',
        comment: '#8d8266',
        keyword: '#e8895f',
        string: '#7fbf6a',
        number: '#d9a441',
        func: '#8fb8e8',
        type: '#b98ad6',
        operator: '#b0a892',
        deleted: '#f07575',
        inserted: '#7ac87c',
      },
      codeGround: '#16120c',
      terminalGround: '--background-code',
      terminal: {
        background: '#16120c',
        cursorAccent: '#16120c',
        foreground: '#e8e1d2',
        cursor: '#e8895f',
        selectionBackground: '#403928',
        black: '#3a3324',
        red: '#e2665c',
        green: '#7fbf6a',
        yellow: '#d9a441',
        blue: '#6f9fd8',
        magenta: '#b98ad6',
        cyan: '#5fb8b8',
        white: '#d4cab6',
        brightBlack: '#8d8266',
        brightRed: '#f0857b',
        brightGreen: '#9ad686',
        brightYellow: '#ecc063',
        brightBlue: '#8fb8e8',
        brightMagenta: '#d0a6e8',
        brightCyan: '#7fd0d0',
        brightWhite: '#e8e1d2',
      },
    },
  },
  'alma-mater': {
    id: 'alma-mater',
    label: 'Alma Mater',
    swatch: '#14828c',
    light: {
      mark: { navy: '#052049', coral: '#16a0ac' },
      syntax: {
        plain: '#052049',
        comment: '#586780',
        keyword: '#0f388a',
        string: '#007242',
        number: '#8a5a00',
        func: '#6c247c',
        type: '#0e5258',
        operator: '#506380',
        deleted: '#c40d3e',
        inserted: '#007242',
      },
      codeGround: '#f2f3f4',
      terminalGround: '--background-muted',
      terminal: {
        background: '#f2f3f4',
        cursorAccent: '#f2f3f4',
        foreground: '#052049',
        cursor: '#0e5258',
        selectionBackground: '#d8d9da',
        black: '#052049',
        red: '#c40d3e',
        green: '#007242',
        yellow: '#8a5a00',
        blue: '#0f388a',
        magenta: '#6c247c',
        cyan: '#0e5258',
        white: '#506380',
        brightBlack: '#586780',
        brightRed: '#d0143f',
        brightGreen: '#1f7a3d',
        brightYellow: '#8a5a00',
        brightBlue: '#255fb5',
        brightMagenta: '#8a1fa0',
        brightCyan: '#106a72',
        brightWhite: '#052049',
      },
    },
    dark: {
      mark: { navy: '#18a3ac', coral: '#16a0ac' },
      syntax: {
        plain: '#e1e3e5',
        comment: '#8a93a6',
        keyword: '#7fb3e6',
        string: '#6fc084',
        number: '#e0a44a',
        func: '#c58ad6',
        type: '#5cc6d0',
        operator: '#b4b9bf',
        deleted: '#f5768a',
        inserted: '#5fbf74',
      },
      codeGround: '#08213f',
      terminalGround: '--background-muted',
      terminal: {
        background: '#0d2a50',
        cursorAccent: '#0d2a50',
        foreground: '#e1e3e5',
        cursor: '#8ae0e8',
        selectionBackground: '#163864',
        black: '#1e477f',
        red: '#f5768a',
        green: '#5fbf74',
        yellow: '#feb80a',
        blue: '#7fb3e6',
        magenta: '#c58ad6',
        cyan: '#5cc6d0',
        white: '#b4b9bf',
        brightBlack: '#909aa6',
        brightRed: '#ff8fa0',
        brightGreen: '#7fd08f',
        brightYellow: '#ffca4a',
        brightBlue: '#a3c9f0',
        brightMagenta: '#d7a5e8',
        brightCyan: '#7fd8e0',
        brightWhite: '#f2f3f4',
      },
    },
  },
  'roche-limit': {
    id: 'roche-limit',
    label: 'Roche Limit',
    swatch: '#ee6c1a',
    light: {
      mark: { navy: '#1f1e1c', coral: '#ee6c1a' },
      syntax: {
        plain: '#1f1e1c',
        comment: '#3f6e6e',
        keyword: '#0a7a32',
        string: '#b02121',
        number: '#0f6e38',
        func: '#1849b8',
        type: '#0f6e38',
        operator: '#7024b0',
        deleted: '#c4232b',
        inserted: '#12805c',
      },
      codeGround: '#f5f5f3',
      terminalGround: '--background-muted',
      terminal: {
        background: '#f4f4f2',
        cursorAccent: '#f4f4f2',
        foreground: '#1f1e1c',
        cursor: '#d95b08',
        selectionBackground: '#fadbbb',
        black: '#1f1e1c',
        red: '#c4232b',
        green: '#0f7150',
        yellow: '#6e6300',
        blue: '#0a69bc',
        magenta: '#7024b0',
        cyan: '#3f6e6e',
        white: '#69675f',
        brightBlack: '#5c5a55',
        brightRed: '#a8161e',
        brightGreen: '#0a5e42',
        brightYellow: '#5a5100',
        brightBlue: '#08579c',
        brightMagenta: '#5c1d91',
        brightCyan: '#2f5a5a',
        brightWhite: '#1f1e1c',
      },
    },
    dark: {
      mark: { navy: '#ededea', coral: '#ee6c1a' },
      syntax: {
        plain: '#ededea',
        comment: '#7fa3a3',
        keyword: '#6fcb78',
        string: '#ff8f8f',
        number: '#84d089',
        func: '#7fbef7',
        type: '#84d089',
        operator: '#d9a0ff',
        deleted: '#ff9592',
        inserted: '#3dd68c',
      },
      codeGround: '#1b1b19',
      terminalGround: '--background-muted',
      terminal: {
        background: '#232320',
        cursorAccent: '#232320',
        foreground: '#ededea',
        cursor: '#ee6c1a',
        selectionBackground: '#452201',
        black: '#3a3a36',
        red: '#ff9592',
        green: '#3dd68c',
        yellow: '#f2e06b',
        blue: '#70b8ff',
        magenta: '#d9a0ff',
        cyan: '#7fa3a3',
        white: '#a5a39d',
        brightBlack: '#9c9a93',
        brightRed: '#ffb3b0',
        brightGreen: '#6fe5a6',
        brightYellow: '#f7ec9a',
        brightBlue: '#9ccdff',
        brightMagenta: '#e6bcff',
        brightCyan: '#a0c0c0',
        brightWhite: '#ffffff',
      },
    },
  },
} as const satisfies Record<string, GeneratedTheme>;

export type ThemeFamilyId = keyof typeof GENERATED_THEMES;

/** Every family id, in definition order. The one list. */
export const THEME_FAMILY_IDS = Object.keys(GENERATED_THEMES) as ThemeFamilyId[];
