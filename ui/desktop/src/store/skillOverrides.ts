const CONFIG_PATH = '~/.config/biorouter/skills-config.json';

type SkillOverrides = Map<string, boolean>;

const state: { overrides: SkillOverrides } = { overrides: new Map() };

export async function loadSkillOverrides(): Promise<void> {
  try {
    const result = await window.electron.readFile(CONFIG_PATH);
    if (result.found && result.file) {
      const config = JSON.parse(result.file) as { disabled?: string[] };
      const disabled = config.disabled ?? [];
      state.overrides.clear();
      disabled.forEach((name) => state.overrides.set(name, false));
    }
  } catch {
    // file doesn't exist yet — all skills enabled by default
  }
}

export async function saveSkillOverrides(): Promise<void> {
  const disabled = Array.from(state.overrides.entries())
    .filter(([, enabled]) => !enabled)
    .map(([name]) => name);
  await window.electron.writeFile(CONFIG_PATH, JSON.stringify({ disabled }, null, 2));
}

export function setSkillOverride(name: string, enabled: boolean): void {
  state.overrides.set(name, enabled);
}

export function getSkillOverride(name: string): boolean | undefined {
  return state.overrides.get(name);
}

export function getSkillOverrides(): SkillOverrides {
  return state.overrides;
}

export function isSkillEnabled(name: string): boolean {
  const override = state.overrides.get(name);
  return override === undefined ? true : override;
}
