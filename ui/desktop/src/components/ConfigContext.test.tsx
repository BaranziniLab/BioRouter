import { render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ConfigProvider, useConfig } from './ConfigContext';

// Issue #52 — the cached `config` object was only ever re-read when a write
// went through this context's own `upsert`. Every key written straight to the
// API (`setConfigProvider`, notably) left the cache serving whatever was loaded
// when the provider mounted, with no way for a consumer to ask for a re-read:
// the reload function existed but was private.

const mocks = vi.hoisted(() => ({
  readAllConfig: vi.fn(),
  readConfig: vi.fn(),
  removeConfig: vi.fn(),
  upsertConfig: vi.fn(),
  getExtensions: vi.fn(),
  addExtension: vi.fn(),
  removeExtension: vi.fn(),
  providers: vi.fn(),
  getProviderModels: vi.fn(),
  syncBundledExtensions: vi.fn(),
}));

vi.mock('../api', () => ({
  readAllConfig: mocks.readAllConfig,
  readConfig: mocks.readConfig,
  removeConfig: mocks.removeConfig,
  upsertConfig: mocks.upsertConfig,
  getExtensions: mocks.getExtensions,
  addExtension: mocks.addExtension,
  removeExtension: mocks.removeExtension,
  providers: mocks.providers,
  getProviderModels: mocks.getProviderModels,
}));

vi.mock('./settings/extensions', () => ({
  syncBundledExtensions: mocks.syncBundledExtensions,
}));

function ConfigProbe() {
  const { config, refreshConfig } = useConfig();
  return (
    <div>
      <div data-testid="provider">{String(config.BIOROUTER_PROVIDER ?? 'none')}</div>
      <button type="button" onClick={() => void refreshConfig()}>
        Refresh
      </button>
    </div>
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  mocks.readAllConfig.mockResolvedValue({ data: { config: { BIOROUTER_PROVIDER: 'ollama' } } });
  mocks.getExtensions.mockResolvedValue({
    data: { extensions: [], warnings: [] },
    response: { status: 200 },
  });
  mocks.providers.mockResolvedValue({ data: [] });
  mocks.syncBundledExtensions.mockResolvedValue(undefined);
});

describe('ConfigContext refreshConfig (#52)', () => {
  it('re-reads the backing config on demand', async () => {
    render(
      <ConfigProvider>
        <ConfigProbe />
      </ConfigProvider>
    );

    await waitFor(() => expect(screen.getByTestId('provider')).toHaveTextContent('ollama'));

    // Someone else wrote the config — a model switch through `setConfigProvider`,
    // the CLI, another window. Nothing went through `upsert`, so the cache
    // cannot know.
    mocks.readAllConfig.mockResolvedValue({
      data: { config: { BIOROUTER_PROVIDER: 'versa_azure' } },
    });

    screen.getByRole('button', { name: 'Refresh' }).click();

    await waitFor(() => expect(screen.getByTestId('provider')).toHaveTextContent('versa_azure'));
  });
});
