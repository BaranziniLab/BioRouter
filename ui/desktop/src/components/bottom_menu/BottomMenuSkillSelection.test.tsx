import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { BottomMenuSkillSelection } from './BottomMenuSkillSelection';
import type { CatalogBundle, CatalogSkill, CatalogView } from '../../api';

const mocks = vi.hoisted(() => ({
  overrides: new Map<string, boolean>(),
  saveSkillOverrides: vi.fn(async () => undefined),
  loadSkillOverrides: vi.fn(async () => true),
  skillCatalogHandler: vi.fn(),
  refreshSkillCatalog: vi.fn(),
  setSessionSkills: vi.fn(),
  toastError: vi.fn(),
  toastSuccess: vi.fn(),
}));

vi.mock('../../store/skillOverrides', () => ({
  loadSkillOverrides: mocks.loadSkillOverrides,
  saveSkillOverrides: mocks.saveSkillOverrides,
  setSkillOverride: (name: string, enabled: boolean) => mocks.overrides.set(name, enabled),
  isSkillEnabled: (name: string) => mocks.overrides.get(name) ?? true,
  getSkillOverrides: () => mocks.overrides,
}));

vi.mock('../../api', () => ({
  skillCatalogHandler: (...args: unknown[]) => mocks.skillCatalogHandler(...args),
  refreshSkillCatalog: (...args: unknown[]) => mocks.refreshSkillCatalog(...args),
  setSessionSkills: (...args: unknown[]) => mocks.setSessionSkills(...args),
}));

vi.mock('../../toasts', () => ({
  toastService: { success: mocks.toastSuccess, error: mocks.toastError },
}));

function skill(name: string, extras: Partial<CatalogSkill> = {}): CatalogSkill {
  return {
    name,
    description: `${name} description`,
    slug: name,
    directory: `/skills/${name}`,
    sourceRoot: '/skills',
    source: { kind: 'biorouter', extension: null, label: 'Biorouter' },
    bundle: null,
    builtin: false,
    state: {
      machineEnabled: true,
      session: 'default',
      sessionViaBundle: false,
      hiddenContext: false,
      effective: true,
    },
    ...extras,
  };
}

function bundle(name: string, members: string[], extras: Partial<CatalogBundle> = {}): CatalogBundle {
  return {
    name,
    displayName: name,
    directory: `/skills/${name}`,
    sourceRoot: '/skills',
    source: { kind: 'biorouter', extension: null, label: 'Biorouter' },
    skills: members,
    package: null,
    state: {
      machineEnabled: true,
      session: 'default',
      sessionViaBundle: false,
      hiddenContext: false,
      effective: true,
    },
    ...extras,
  };
}

function view(overrides: Partial<CatalogView> = {}): CatalogView {
  return {
    generation: 1,
    roots: [],
    skills: [skill('example-skill')],
    bundles: [],
    ...overrides,
  };
}

function serve(next: CatalogView) {
  mocks.skillCatalogHandler.mockResolvedValue({ data: next });
  mocks.refreshSkillCatalog.mockResolvedValue({ data: next });
}

async function openMenu() {
  fireEvent.pointerDown(screen.getByLabelText(/Manage skills/), { button: 0, ctrlKey: false });
  return screen.findAllByRole('menuitemcheckbox');
}

describe('BottomMenuSkillSelection', () => {
  beforeEach(() => {
    mocks.overrides.clear();
    vi.clearAllMocks();
    serve(view());
  });

  // ---------------------------------------------------------------- #113 (a)

  /**
   * The defect this component existed to have: the session branch wrote React
   * state, raised a green toast, and called nothing. Asserting the REQUEST is
   * what makes that irreproducible — a re-introduced local-only path would
   * still flip the switch and still pass any test that only reads the switch.
   */
  it('sends a per-chat toggle to the daemon and scopes it to the session', async () => {
    mocks.setSessionSkills.mockResolvedValue({
      data: {
        catalog: view({
          skills: [
            skill('example-skill', {
              state: {
                machineEnabled: true,
                session: 'removed',
                sessionViaBundle: false,
                hiddenContext: false,
                effective: false,
              },
            }),
          ],
        }),
        sessionAdd: [],
        sessionRemove: ['example-skill'],
      },
    });

    render(<BottomMenuSkillSelection sessionId="20260824_1" />);
    const [toggle] = await openMenu();
    fireEvent.click(toggle);

    await waitFor(() => expect(mocks.setSessionSkills).toHaveBeenCalledTimes(1));
    expect(mocks.setSessionSkills.mock.calls[0][0].body).toEqual({
      sessionId: '20260824_1',
      add: [],
      remove: ['example-skill'],
    });
    // ...and it must not have gone anywhere near the machine-wide file.
    expect(mocks.saveSkillOverrides).not.toHaveBeenCalled();
    await waitFor(() => expect(toggle).toHaveAttribute('aria-checked', 'false'));
  });

  it('raises a success toast only after the backend confirms, and rolls back when it refuses', async () => {
    mocks.setSessionSkills.mockRejectedValue(new Error('session not found'));

    render(<BottomMenuSkillSelection sessionId="20260824_1" />);
    const [toggle] = await openMenu();
    fireEvent.click(toggle);

    await waitFor(() => expect(mocks.toastError).toHaveBeenCalledTimes(1));
    expect(mocks.toastError.mock.calls[0][0].msg).toContain('session not found');
    expect(mocks.toastSuccess).not.toHaveBeenCalled();
    await waitFor(() => expect(toggle).toHaveAttribute('aria-checked', 'true'));
  });

  it('says "for this chat" only when there is a chat', async () => {
    mocks.setSessionSkills.mockResolvedValue({
      data: { catalog: view(), sessionAdd: [], sessionRemove: ['example-skill'] },
    });
    render(<BottomMenuSkillSelection sessionId="20260824_1" />);
    const [toggle] = await openMenu();
    fireEvent.click(toggle);
    await waitFor(() => expect(mocks.toastSuccess).toHaveBeenCalledTimes(1));
    expect(mocks.toastSuccess.mock.calls[0][0].msg).toContain('for this chat');
  });

  // ---------------------------------------------------------------- #113 (b)

  /**
   * BiorOffice and MarkItDown ship skills inside their extension directory.
   * The backend has always loaded them; this picker's own three-root scan never
   * saw them, so they were active for the model and had no row here.
   */
  it('lists a skill bundled inside an installed extension, and names the extension', async () => {
    serve(
      view({
        skills: [
          skill('example-skill'),
          skill('word', {
            sourceRoot: '/extensions/BiorOffice/skills',
            source: { kind: 'extension', extension: 'BiorOffice', label: 'BiorOffice' },
          }),
        ],
      })
    );
    render(<BottomMenuSkillSelection sessionId={null} />);
    await openMenu();
    expect(await screen.findByText('word')).toBeInTheDocument();
    expect(screen.getByText('BiorOffice')).toBeInTheDocument();
  });

  it('asks the daemon to rescan when the menu opens, so an install made elsewhere shows up', async () => {
    render(<BottomMenuSkillSelection sessionId={null} />);
    await openMenu();
    await waitFor(() => expect(mocks.refreshSkillCatalog).toHaveBeenCalled());
  });

  // ------------------------------------------------------------------ bundles

  it('toggles a bundle by its own name rather than expanding its members', async () => {
    serve(view({ skills: [], bundles: [bundle('hyperframes', ['hyperframes', 'media-use'])] }));
    mocks.setSessionSkills.mockResolvedValue({
      data: {
        catalog: view({ skills: [], bundles: [bundle('hyperframes', ['hyperframes', 'media-use'])] }),
        sessionAdd: [],
        sessionRemove: ['hyperframes'],
      },
    });

    render(<BottomMenuSkillSelection sessionId="20260824_1" />);
    const [toggle] = await openMenu();
    fireEvent.click(toggle);

    await waitFor(() => expect(mocks.setSessionSkills).toHaveBeenCalledTimes(1));
    expect(mocks.setSessionSkills.mock.calls[0][0].body.remove).toEqual(['hyperframes']);
  });

  it('shows a package bundle as one row with its member count and entry point', async () => {
    serve(
      view({
        skills: [],
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
              groups: {},
            },
          }),
        ],
      })
    );
    render(<BottomMenuSkillSelection sessionId={null} />);
    const items = await openMenu();
    expect(items).toHaveLength(1);
    expect(screen.getByText('HyperFrames')).toBeInTheDocument();
    expect(screen.getByText('2 skills')).toBeInTheDocument();
    expect(screen.getByText(/entry point: hyperframes/)).toBeInTheDocument();
  });

  // -------------------------------------------------------------- hub (machine)

  it('applies rapid hub choices in order and leaves the final choice enabled', async () => {
    render(<BottomMenuSkillSelection sessionId={null} />);
    const [toggle] = await openMenu();
    expect(toggle).toHaveAttribute('aria-checked', 'true');

    serve(
      view({
        skills: [
          skill('example-skill', {
            state: {
              machineEnabled: false,
              session: 'default',
              sessionViaBundle: false,
              hiddenContext: false,
              effective: false,
            },
          }),
        ],
      })
    );
    fireEvent.click(toggle);
    await waitFor(() => expect(toggle).toHaveAttribute('aria-checked', 'false'));

    serve(view());
    fireEvent.click(toggle);
    await waitFor(() => expect(toggle).toHaveAttribute('aria-checked', 'true'));
    expect(mocks.overrides.get('example-skill')).toBe(true);
  });

  it('restores the persisted state when the machine-wide write fails', async () => {
    render(<BottomMenuSkillSelection sessionId={null} />);
    const [toggle] = await openMenu();

    mocks.saveSkillOverrides.mockRejectedValueOnce(new Error('disk full'));
    fireEvent.click(toggle);

    await waitFor(() => expect(mocks.toastError).toHaveBeenCalledTimes(1));
    expect(mocks.toastSuccess).not.toHaveBeenCalled();
    // The in-memory store is re-read, or the next save would persist the edit
    // whose write just failed.
    expect(mocks.loadSkillOverrides).toHaveBeenCalled();
    await waitFor(() => expect(toggle).toHaveAttribute('aria-checked', 'true'));
  });

  it('reports a catalog it could not read instead of claiming there are no skills', async () => {
    mocks.skillCatalogHandler.mockRejectedValue(new Error('daemon is down'));
    mocks.refreshSkillCatalog.mockRejectedValue(new Error('daemon is down'));
    render(<BottomMenuSkillSelection sessionId={null} />);
    fireEvent.pointerDown(screen.getByLabelText(/Manage skills/), { button: 0, ctrlKey: false });
    expect(await screen.findByText(/Could not read the skill catalog/)).toBeInTheDocument();
  });
});
