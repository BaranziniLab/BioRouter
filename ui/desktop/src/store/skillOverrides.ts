const CONFIG_PATH = '~/.config/biorouter/skills-config.json';

type SkillOverrides = Map<string, boolean>;

const state: { overrides: SkillOverrides } = { overrides: new Map() };
let saveQueue: Promise<void> = Promise.resolve();

export async function loadSkillOverrides(): Promise<boolean> {
  try {
    const result = await window.electron.readFile(CONFIG_PATH);
    const next = new Map<string, boolean>();
    if (result.found && result.file) {
      const config = JSON.parse(result.file) as { disabled?: string[] };
      const disabled = config.disabled ?? [];
      disabled.forEach((name) => next.set(name, false));
    }
    state.overrides = next;
    return true;
  } catch {
    return false;
  }
}

export function saveSkillOverrides(): Promise<void> {
  const disabled = Array.from(state.overrides.entries())
    .filter(([, enabled]) => !enabled)
    .map(([name]) => name);
  const content = JSON.stringify({ disabled }, null, 2);
  const save = saveQueue
    .catch(() => undefined)
    .then(async () => {
      const written = await window.electron.writeFile(CONFIG_PATH, content);
      if (!written) throw new Error('Could not save skill preferences');
    });
  saveQueue = save;
  return save;
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
