import { render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({ loadRegistry: vi.fn() }));

vi.mock('./registry', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./registry')>()),
  loadRegistry: mocks.loadRegistry,
}));
vi.mock('./installSkill', () => ({ installRegistrySkill: vi.fn() }));
vi.mock('../../toasts', () => ({ toastSuccess: vi.fn(), toastError: vi.fn() }));

import BrowseSkillsModal from './BrowseSkillsModal';

const skill = {
  id: 'scientific-research',
  name: 'scientific-research',
  category: 'Core' as const,
  // A real registry value: prose, not a token. See landing/registry.json.
  type: 'User-invocable · /scientific-research',
  description: 'Research workflows',
  tags: [],
  keywords: [],
  download: 'https://example.com/scientific-research.zip',
  filename: 'scientific-research.zip',
};

beforeEach(() => {
  vi.clearAllMocks();
  mocks.loadRegistry.mockResolvedValue({
    registry: { version: 1, skills: [skill] },
    live: true,
    fetchedAt: '2026-09-02T00:00:00Z',
  });
});

/// `skill.type` is an English phrase, not a machine token. The real registry
/// ships values like "5 skills · auto-applied" and
/// "User-invocable · /scientific-research" — and this span sits inline, on the
/// same row, beside `skill.name` in the body font. Monospace here was the
/// reverse defect: prose set in the code face.
///
/// jsdom never runs Tailwind, so asserting a computed font would pass whatever
/// the class says. This asserts the CLASS, and walks the ancestors because
/// `font-mono` on a parent is inherited.
describe('BrowseSkillsModal — a classification phrase is prose, not a token', () => {
  it('sets the skill type in the body font, not monospace', async () => {
    render(<BrowseSkillsModal onClose={vi.fn()} onInstalled={vi.fn()} installedIds={new Set()} />);

    const type = await screen.findByText(skill.type);
    expect(type.className).not.toMatch(/font-mono/);
    for (let node = type.parentElement; node; node = node.parentElement) {
      expect(node.className ?? '').not.toMatch(/font-mono/);
      if (node.tagName === 'BODY') break;
    }
  });
});
