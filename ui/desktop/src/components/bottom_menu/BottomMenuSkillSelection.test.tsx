import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { BottomMenuSkillSelection } from './BottomMenuSkillSelection';

const mocks = vi.hoisted(() => ({
  overrides: new Map<string, boolean>(),
  saveSkillOverrides: vi.fn(async () => undefined),
  loadSkillOverrides: vi.fn(async () => true),
  loadSkillsFromDirs: vi.fn(async () => ({
    singles: [
      {
        folderPath: '/skills/example',
        sourceDir: '/skills',
        name: 'Example Skill',
        description: 'Example description',
        content: '# Example',
      },
    ],
    bundles: [],
  })),
  toastError: vi.fn(),
}));

vi.mock('../../store/skillOverrides', () => ({
  loadSkillOverrides: mocks.loadSkillOverrides,
  saveSkillOverrides: mocks.saveSkillOverrides,
  setSkillOverride: (name: string, enabled: boolean) => mocks.overrides.set(name, enabled),
  isSkillEnabled: (name: string) => mocks.overrides.get(name) ?? true,
  getSkillOverrides: () => mocks.overrides,
}));

vi.mock('../skills/skillUtils', () => ({
  ALL_SKILL_DIRS: ['/skills'],
  loadSkillsFromDirs: mocks.loadSkillsFromDirs,
  isBuiltinSkill: () => false,
}));

vi.mock('../../toasts', () => ({
  toastService: { success: vi.fn(), error: mocks.toastError },
}));

describe('BottomMenuSkillSelection', () => {
  beforeEach(() => {
    mocks.overrides.clear();
    vi.clearAllMocks();
  });

  it('applies rapid hub choices immediately and leaves the final choice enabled', async () => {
    render(<BottomMenuSkillSelection sessionId={null} />);
    const trigger = screen.getByLabelText(/Manage skills/);
    expect(trigger).not.toHaveAttribute('title');
    fireEvent.pointerDown(trigger, { button: 0, ctrlKey: false });

    const toggle = await screen.findByRole('menuitemcheckbox');
    expect(toggle).toHaveAttribute('aria-checked', 'true');

    fireEvent.click(toggle);
    await waitFor(() => expect(toggle).toHaveAttribute('aria-checked', 'false'));
    expect(toggle).not.toBeDisabled();

    fireEvent.click(toggle);
    await waitFor(() => expect(toggle).toHaveAttribute('aria-checked', 'true'));

    expect(mocks.overrides.get('Example Skill')).toBe(true);
  });

  it('restores the persisted state when the latest disk write fails', async () => {
    let restoreFromDisk = false;
    mocks.loadSkillOverrides.mockImplementation(async () => {
      if (restoreFromDisk) mocks.overrides.delete('Example Skill');
      return true;
    });
    render(<BottomMenuSkillSelection sessionId={null} />);
    fireEvent.pointerDown(screen.getByLabelText(/Manage skills/), {
      button: 0,
      ctrlKey: false,
    });

    const toggle = await screen.findByRole('menuitemcheckbox');
    restoreFromDisk = true;
    mocks.saveSkillOverrides.mockRejectedValueOnce(new Error('disk full'));
    fireEvent.click(toggle);

    await waitFor(() => expect(mocks.toastError).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(toggle).toHaveAttribute('aria-checked', 'true'));
  });
});
