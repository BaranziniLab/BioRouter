function withoutOuterQuotes(argument: string): string {
  const trimmed = argument.trim();
  if (trimmed.length >= 2) {
    const first = trimmed[0];
    const last = trimmed[trimmed.length - 1];
    if ((first === '"' && last === '"') || (first === "'" && last === "'")) {
      return trimmed.slice(1, -1);
    }
  }
  return trimmed;
}

export function isBrxtFile(argument: string): boolean {
  return withoutOuterQuotes(argument).toLowerCase().endsWith('.brxt');
}

export function findBrxtArgument(arguments_: readonly string[]): string | undefined {
  for (const argument of arguments_) {
    const normalized = withoutOuterQuotes(argument);
    if (normalized.toLowerCase().endsWith('.brxt')) return normalized;
  }
  return undefined;
}
