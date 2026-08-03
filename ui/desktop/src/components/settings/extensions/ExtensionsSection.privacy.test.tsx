import { render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  extensionsList: [] as Array<Record<string, unknown>>,
  addExtension: vi.fn(async () => undefined),
  removeExtension: vi.fn(async () => undefined),
  getExtensions: vi.fn(async () => []),
  read: vi.fn(async () => 'anthropic'),
  getProviders: vi.fn(async () => [
    {
      name: 'anthropic',
      is_configured: true,
      provider_type: 'Builtin',
      metadata: { name: 'anthropic', display_name: 'Anthropic', tier: 'public' },
    },
    {
      name: 'versa_azure',
      is_configured: true,
      provider_type: 'Builtin',
      metadata: { name: 'versa_azure', display_name: 'Versa', tier: 'private' },
    },
  ]),
  toggleExtensionDefault: vi.fn(async () => undefined),
  activateExtensionDefault: vi.fn(async () => undefined),
  deleteExtension: vi.fn(async () => undefined),
}));

vi.mock('../../ConfigContext', () => ({
  useConfig: () => ({
    extensionsList: mocks.extensionsList,
    addExtension: mocks.addExtension,
    removeExtension: mocks.removeExtension,
    getExtensions: mocks.getExtensions,
    read: mocks.read,
    getProviders: mocks.getProviders,
  }),
}));

vi.mock('./index', () => ({
  toggleExtensionDefault: mocks.toggleExtensionDefault,
  activateExtensionDefault: mocks.activateExtensionDefault,
  deleteExtension: mocks.deleteExtension,
}));

vi.mock('../../../toasts', () => ({
  toastService: { success: vi.fn(), error: vi.fn() },
}));

import ExtensionsSection from './ExtensionsSection';

const omop = { type: 'stdio', name: 'ucsfomopagent', description: 'UCSF OMOP', enabled: true };
// NOT `developer` — that key is a shipped capability, which `ExtensionsSection`
// filters out of this list entirely (`isCapabilityExtension`), so a fixture
// built on it renders nothing and every assertion about it is vacuous.
const spoke = { type: 'stdio', name: 'spokeagent', description: 'SPOKE graph', enabled: true };

describe('ExtensionsSection — the pairing state Settings can actually compute', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.extensionsList = [omop, spoke];
    mocks.read.mockResolvedValue('anthropic');
  });

  it('the Settings extension card states which provider it judged against', async () => {
    render(<ExtensionsSection hideButtons />);

    expect(
      await screen.findByText(/unavailable in new chats \(default model is public\)/i)
    ).toBeInTheDocument();
    // ⚠ "The focused session" does not exist here: Settings has no session
    // awareness, and with tabs and splits there is no single focused chat once
    // the user has navigated away. So the card must name the thing it DID judge
    // against — the global default provider.
    expect(screen.getByText(/Anthropic/)).toBeInTheDocument();
  });

  it('says nothing about a private extension when the default provider is private', async () => {
    mocks.read.mockResolvedValue('versa_azure');
    render(<ExtensionsSection hideButtons />);

    await screen.findByText(/UCSF OMOP/);
    expect(screen.queryByText(/unavailable in new chats/i)).toBeNull();
  });

  it('says nothing about a public extension under a public default provider', async () => {
    mocks.extensionsList = [spoke];
    render(<ExtensionsSection hideButtons />);

    await screen.findByText(/SPOKE graph/);
    expect(screen.queryByText(/unavailable in new chats/i)).toBeNull();
  });
});
