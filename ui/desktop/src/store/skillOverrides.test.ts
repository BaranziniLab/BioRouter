import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  isSkillEnabled,
  loadSkillOverrides,
  saveSkillOverrides,
  setSkillOverride,
} from './skillOverrides';

const writeFile = vi.fn();

beforeEach(async () => {
  writeFile.mockReset();
  Object.defineProperty(window, 'electron', {
    configurable: true,
    value: {
      readFile: vi.fn(async () => ({ found: false })),
      writeFile,
    },
  });
  await loadSkillOverrides();
});

describe('skillOverrides persistence', () => {
  it('treats a false write result as a failed save', async () => {
    writeFile.mockResolvedValue(false);
    setSkillOverride('analysis', false);

    await expect(saveSkillOverrides()).rejects.toThrow('Could not save skill preferences');
  });

  it('serializes rapid writes so the latest preference reaches disk last', async () => {
    let finishFirst: ((written: boolean) => void) | undefined;
    writeFile
      .mockImplementationOnce(() => new Promise<boolean>((resolve) => (finishFirst = resolve)))
      .mockResolvedValueOnce(true);

    setSkillOverride('analysis', false);
    const first = saveSkillOverrides();
    setSkillOverride('analysis', true);
    const second = saveSkillOverrides();

    await vi.waitFor(() => expect(writeFile).toHaveBeenCalledTimes(1));
    finishFirst?.(true);
    await first;
    await second;

    expect(writeFile).toHaveBeenCalledTimes(2);
    expect(writeFile.mock.calls[0][1]).toContain('analysis');
    expect(writeFile.mock.calls[1][1]).not.toContain('analysis');
    expect(isSkillEnabled('analysis')).toBe(true);
  });
});
