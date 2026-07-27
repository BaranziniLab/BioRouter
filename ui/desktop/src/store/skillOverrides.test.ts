import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  isSkillEnabled,
  loadSkillOverrides,
  saveSkillOverrides,
  setSkillOverride,
} from './skillOverrides';

const writeFile = vi.fn();
const readFile = vi.fn();

beforeEach(async () => {
  writeFile.mockReset();
  readFile.mockReset();
  readFile.mockResolvedValue({ found: false });
  Object.defineProperty(window, 'electron', {
    configurable: true,
    value: {
      readFile,
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

  it('preserves unknown fields written by other surfaces on save', async () => {
    // The CLI (and future versions) store additional fields in the same
    // file; the GUI's read-modify-write must carry them through instead of
    // replacing the file with a bare {disabled}.
    writeFile.mockResolvedValue(true);
    readFile.mockResolvedValue({
      found: true,
      file: JSON.stringify({
        disabled: ['from-disk'],
        future: { nested: true },
        note: 'cli forward-compat',
      }),
    });

    setSkillOverride('analysis', false);
    await saveSkillOverrides();

    expect(writeFile).toHaveBeenCalledTimes(1);
    const written = JSON.parse(writeFile.mock.calls[0][1] as string);
    expect(written.future).toEqual({ nested: true });
    expect(written.note).toBe('cli forward-compat');
    // The disabled array reflects THIS surface's current overrides.
    expect(written.disabled).toEqual(['analysis']);
  });

  it('falls back to a bare object when the on-disk config is unreadable', async () => {
    writeFile.mockResolvedValue(true);
    readFile.mockResolvedValue({ found: true, file: '{ not json' });

    setSkillOverride('analysis', false);
    await saveSkillOverrides();

    const written = JSON.parse(writeFile.mock.calls[0][1] as string);
    expect(written).toEqual({ disabled: ['analysis'] });
  });
});
