export const ACCENT_PALETTE: readonly string[] = [
  '#7fae9f', // sage
  '#8b8fc4', // dusk
  '#c4a47a', // wheat
  '#c49096', // clay-rose
  '#a8b884', // olive
  '#8ab0c4', // dust-blue
  '#a89ac4', // lilac
  '#c49a96', // shell
  '#8fb8a3', // seafoam
  '#bdb084', // sand
  '#b894b4', // mauve
  '#8a96a3', // smoke
] as const;

export function pickAccentColor(usedColors: readonly string[]): string {
  for (const color of ACCENT_PALETTE) {
    if (!usedColors.includes(color)) return color;
  }
  // All used — pick by ring buffer
  const ringIndex = usedColors.length % ACCENT_PALETTE.length;
  return ACCENT_PALETTE[ringIndex];
}

// Default name for a freshly-spawned conversation. The LLM rewrites this
// after the first message exchange (see useChatStream's session-name polling),
// at which point disambiguation logic appends " 2", " 3" etc. if the
// LLM-assigned name collides with an existing session in history.
//
// The `index` parameter is unused but kept so callers don't have to change.
export function generateName(_index: number): string {
  void _index;
  return 'New Session';
}
