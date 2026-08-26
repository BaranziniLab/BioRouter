import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import SkillsView from './SkillsView';
import type { CatalogBundle, CatalogSkill, CatalogView } from '../../api';

const mocks = vi.hoisted(() => ({
  skillCatalogHandler: vi.fn(),
  refreshSkillCatalog: vi.fn(),
  setSessionSkills: vi.fn(),
  removeSkillPackage: vi.fn(),
  saveSkillOverrides: vi.fn(async () => undefined),
  loadSkillOverrides: vi.fn(async () => true),
  overrides: new Map<string, boolean>(),
  toastSuccess: vi.fn(),
  toastError: vi.fn(),
}));

vi.mock('../../api', () => ({
  skillCatalogHandler: (...a: unknown[]) => mocks.skillCatalogHandler(...a),
  refreshSkillCatalog: (...a: unknown[]) => mocks.refreshSkillCatalog(...a),
  setSessionSkills: (...a: unknown[]) => mocks.setSessionSkills(...a),
  removeSkillPackage: (...a: unknown[]) => mocks.removeSkillPackage(...a),
}));

vi.mock('../../store/skillOverrides', () => ({
  loadSkillOverrides: mocks.loadSkillOverrides,
  saveSkillOverrides: mocks.saveSkillOverrides,
  setSkillOverride: (name: string, enabled: boolean) => mocks.overrides.set(name, enabled),
  isSkillEnabled: (name: string) => mocks.overrides.get(name) ?? true,
  getSkillOverrides: () => mocks.overrides,
}));

vi.mock('../../toasts', () => ({
  toastSuccess: mocks.toastSuccess,
  toastError: mocks.toastError,
}));

vi.mock('../Layout/MainPanelLayout', () => ({
  MainPanelLayout: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
}));
vi.mock('../Layout/ReadableContent', () => ({
  ReadableContent: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
}));
vi.mock('../conversation/SearchView', () => ({
  SearchView: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
}));
vi.mock('../baam/BrowseSkillsModal', () => ({ default: () => null }));
vi.mock('./AddSkillModal', () => ({ default: () => null }));
vi.mock('./CustomSkillModal', () => ({ default: () => null }));

const state = {
  machineEnabled: true,
  session: 'default' as const,
  sessionViaBundle: false,
  hiddenContext: false,
  effective: true,
};

function skill(name: string, overrides: Partial<CatalogSkill> = {}): CatalogSkill {
  return {
    name,
    description: `${name} does things`,
    slug: name,
    directory: `/skills/${name}`,
    sourceRoot: '/skills',
    source: { kind: 'biorouter', extension: null, label: 'Biorouter' },
    bundle: null,
    builtin: false,
    state,
    ...overrides,
  };
}

function bundle(
  name: string,
  members: string[],
  overrides: Partial<CatalogBundle> = {}
): CatalogBundle {
  return {
    name,
    displayName: name,
    directory: `/skills/${name}`,
    sourceRoot: '/skills',
    source: { kind: 'biorouter', extension: null, label: 'Biorouter' },
    skills: members,
    package: null,
    builtin: false,
    state,
    ...overrides,
  };
}

function serve(view: Partial<CatalogView>) {
  const full: CatalogView = { generation: 1, roots: [], skills: [], bundles: [], ...view };
  mocks.skillCatalogHandler.mockResolvedValue({ data: full });
  mocks.refreshSkillCatalog.mockResolvedValue({ data: full });
}

beforeEach(() => {
  vi.clearAllMocks();
  mocks.overrides.clear();
  // @ts-expect-error test shim
  window.electron = { openDirectoryInExplorer: vi.fn(), readFile: vi.fn() };
  serve({ skills: [skill('my-skill')] });
});

describe('SkillsView', () => {
  /**
   * The visible half of #113 root cause 2: BiorOffice's bundled skills were
   * loaded by the model and had no row here, because this view scanned three
   * roots against the backend's seven.
   */
  it('lists extension-bundled skills in their own group', async () => {
    serve({
      skills: [
        skill('my-skill'),
        skill('word', {
          sourceRoot: '/extensions/BiorOffice/skills',
          source: { kind: 'extension', extension: 'BiorOffice', label: 'BiorOffice' },
        }),
      ],
    });
    render(<SkillsView />);
    expect(await screen.findByText('From BiorOffice (1)')).toBeInTheDocument();
    expect(screen.getByText('Biorouter Skills (1)')).toBeInTheDocument();
  });

  /**
   * A skill an extension supplies is not the user's to delete — the extension
   * would put it back. Same lesson as the built-in badge, second case.
   */
  it('offers no Delete for a skill an installed extension supplies', async () => {
    serve({
      skills: [
        skill('word', {
          sourceRoot: '/extensions/BiorOffice/skills',
          source: { kind: 'extension', extension: 'BiorOffice', label: 'BiorOffice' },
        }),
      ],
    });
    render(<SkillsView />);
    await screen.findByText('word');
    expect(screen.queryByLabelText('Delete word')).not.toBeInTheDocument();
  });

  it('shows a package as one expandable row, and opens to its components', async () => {
    serve({
      skills: [
        skill('hyperframes', { bundle: 'hyperframes', slug: 'hyperframes/hyperframes' }),
        skill('media-use', { bundle: 'hyperframes', slug: 'hyperframes/media-use' }),
      ],
      bundles: [
        bundle('hyperframes', ['hyperframes', 'media-use'], {
          displayName: 'HyperFrames',
          package: {
            id: 'hyperframes',
            displayName: 'HyperFrames',
            version: '0.8.12',
            entryPoint: 'hyperframes',
            sourceUrl: null,
            sourceRef: null,
            resolvedCommit: null,
            installer: null,
            installedAt: null,
            groups: { core: ['hyperframes'], 'on-demand': ['media-use'] },
          },
        }),
      ],
    });
    render(<SkillsView />);

    expect(await screen.findByText('HyperFrames')).toBeInTheDocument();
    expect(screen.getByText('Biorouter Skills (1)')).toBeInTheDocument();
    expect(screen.getByText('entry point: hyperframes')).toBeInTheDocument();

    // Collapsed, the row summarises; expanded, it details.
    fireEvent.click(screen.getByLabelText('Expand HyperFrames'));
    const list = await screen.findByRole('list');
    const items = within(list).getAllByRole('listitem');
    expect(items).toHaveLength(2);
    expect(items[1]).toHaveTextContent('media-use');
    expect(items[1]).toHaveTextContent('[on-demand]');
    expect(items[0]).toHaveTextContent('→');
  });

  it('removes a package through the importer rather than deleting a directory', async () => {
    serve({
      skills: [skill('alpha', { bundle: 'pack', slug: 'pack/alpha' })],
      bundles: [bundle('pack', ['alpha'])],
    });
    mocks.removeSkillPackage.mockResolvedValue({ data: { id: 'pack' } });
    render(<SkillsView />);

    fireEvent.click(await screen.findByLabelText('Delete skill package pack'));
    fireEvent.click(await screen.findByRole('button', { name: 'Delete Package' }));

    await waitFor(() => expect(mocks.removeSkillPackage).toHaveBeenCalledTimes(1));
    expect(mocks.removeSkillPackage.mock.calls[0][0].body).toEqual({
      id: 'pack',
      sourceRoot: '/skills',
    });
  });

  /**
   * A skill's directory name and its declared name are allowed to differ, and
   * the frontmatter is what wins for identity — so removal must use the
   * INSTALLED directory or it would miss the folder entirely.
   */
  it('removes a single skill by its installed directory, not its declared name', async () => {
    serve({
      skills: [skill('gwas-pipeline', { slug: 'run-gwas', directory: '/skills/run-gwas' })],
    });
    mocks.removeSkillPackage.mockResolvedValue({ data: { id: 'run-gwas' } });
    render(<SkillsView />);

    fireEvent.click(await screen.findByLabelText('Delete gwas-pipeline'));
    fireEvent.click(await screen.findByRole('button', { name: 'Delete' }));

    await waitFor(() => expect(mocks.removeSkillPackage).toHaveBeenCalledTimes(1));
    expect(mocks.removeSkillPackage.mock.calls[0][0].body.id).toBe('run-gwas');
  });

  it('reports a catalog it could not read instead of claiming there are no skills', async () => {
    mocks.skillCatalogHandler.mockRejectedValue(new Error('daemon is down'));
    mocks.refreshSkillCatalog.mockRejectedValue(new Error('daemon is down'));
    render(<SkillsView />);
    expect(await screen.findByText(/Could not read the skill catalog/)).toBeInTheDocument();
  });
});

/**
 * ⚠ **A seeded BUNDLE must offer no Delete, and its own field is the only
 * thing that can say so.** `SkillItem` gates its Trash on `CatalogSkill.builtin`
 * — the daemon's answer, not a list in the renderer — but a bundle row is a
 * different control over a different directory, and it had no such gate. So a
 * shipped bundle rendered a working Trash: `removeSkillPackage` succeeded, the
 * toast confirmed it, and the next startup rewrote the folder. That is exactly
 * regression 1 of #77, one level up from where it was fixed.
 *
 * `CatalogBundle.builtin` is `is_shipped_entry_name` on the Rust side, so this
 * cannot drift from the seeder.
 */
describe('SkillsView built-in bundles', () => {
  it('offers no Delete on a bundle the daemon says it shipped', async () => {
    serve({
      skills: [skill('member', { bundle: 'shipped-bundle', builtin: true })],
      bundles: [bundle('shipped-bundle', ['member'], { builtin: true })],
    });
    render(<SkillsView />);

    const row = (await screen.findByText('shipped-bundle')).closest('.biorouter-list-row')!;
    expect(
      within(row as HTMLElement).queryByLabelText(/Delete skill package/)
    ).not.toBeInTheDocument();
    expect(within(row as HTMLElement).getByText('Built-in')).toBeInTheDocument();
  });

  /**
   * The control is present for an installed package, so the assertion above is
   * about built-in-ness and not about bundle rows in general.
   */
  it('still offers Delete on an installed package', async () => {
    serve({
      skills: [skill('media-use', { bundle: 'hyperframes' })],
      bundles: [bundle('hyperframes', ['media-use'])],
    });
    render(<SkillsView />);

    const row = (await screen.findByText('hyperframes')).closest('.biorouter-list-row')!;
    expect(
      within(row as HTMLElement).getByLabelText(/Delete skill package/)
    ).toBeInTheDocument();
  });
});
