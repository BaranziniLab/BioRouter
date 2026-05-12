export const ACCENT_PALETTE: readonly string[] = [
  '#14b8a6', // teal
  '#6366f1', // indigo
  '#f59e0b', // amber
  '#f43f5e', // rose
  '#84cc16', // lime
  '#0ea5e9', // sky
  '#8b5cf6', // violet
  '#fb7185', // coral
  '#10b981', // mint
  '#eab308', // gold
  '#d946ef', // magenta
  '#64748b', // slate
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
