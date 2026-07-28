import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { useState } from 'react';
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
  const { config, refreshConfig, upsert } = useConfig();
  const [refreshError, setRefreshError] = useState('none');
  const [upsertResult, setUpsertResult] = useState('idle');

  return (
    <div>
      <div data-testid="provider">{String(config.BIOROUTER_PROVIDER ?? 'none')}</div>
      <div data-testid="refresh-error">{refreshError}</div>
      <div data-testid="upsert-result">{upsertResult}</div>
      <button
        type="button"
        onClick={() => {
          setRefreshError('none');
          refreshConfig().catch((error: unknown) =>
            setRefreshError(error instanceof Error ? error.message : String(error))
          );
        }}
      >
        Refresh
      </button>
      <button
        type="button"
        onClick={() => {
          setUpsertResult('pending');
          upsert('BIOROUTER_MODE', 'smart_approve', false).then(
            () => setUpsertResult('ok'),
            (error: unknown) =>
              setUpsertResult(`failed: ${error instanceof Error ? error.message : String(error)}`)
          );
        }}
      >
        Upsert
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
  mocks.upsertConfig.mockResolvedValue({ data: {} });
});

function renderProbe() {
  return render(
    <ConfigProvider>
      <ConfigProbe />
    </ConfigProvider>
  );
}

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

// The re-read added for #52 is the only thing that keeps the cache honest, so
// every way it can go wrong has to leave the cache no worse than it found it.
// The two ways it went wrong: a failed read replaced the whole cache with `{}`
// (the generated client does not throw by default, so an HTTP 500 *resolves*
// with no `data`), and two overlapping reads could apply in either order, so a
// read issued before a write could land after the refresh that followed it.
describe('ConfigContext cache integrity', () => {
  it('reads with throwOnError, so a failed read cannot arrive as an empty config', async () => {
    renderProbe();

    await waitFor(() => expect(screen.getByTestId('provider')).toHaveTextContent('ollama'));

    expect(mocks.readAllConfig).toHaveBeenCalledWith({ throwOnError: true });
    expect(mocks.readAllConfig).not.toHaveBeenCalledWith();
  });

  it('keeps the cached config when a re-read resolves without data', async () => {
    renderProbe();
    await waitFor(() => expect(screen.getByTestId('provider')).toHaveTextContent('ollama'));

    // What an HTTP 500 looks like through the non-throwing client: a resolved
    // response carrying an error and no config at all.
    mocks.readAllConfig.mockResolvedValue({
      data: undefined,
      error: { message: 'internal server error' },
      response: { status: 500 },
    });

    fireEvent.click(screen.getByRole('button', { name: 'Refresh' }));

    await waitFor(() => expect(mocks.readAllConfig).toHaveBeenCalledTimes(2));
    await waitFor(() =>
      expect(screen.getByTestId('refresh-error')).toHaveTextContent(/no configuration/i)
    );
    expect(screen.getByTestId('provider')).toHaveTextContent('ollama');
  });

  it('keeps the cached config when a re-read rejects, and reports the failure', async () => {
    renderProbe();
    await waitFor(() => expect(screen.getByTestId('provider')).toHaveTextContent('ollama'));

    mocks.readAllConfig.mockRejectedValue(new Error('daemon unreachable'));

    fireEvent.click(screen.getByRole('button', { name: 'Refresh' }));

    await waitFor(() =>
      expect(screen.getByTestId('refresh-error')).toHaveTextContent('daemon unreachable')
    );
    expect(screen.getByTestId('provider')).toHaveTextContent('ollama');
  });

  it('ignores a read that was issued earlier but landed later', async () => {
    let releaseMountRead: () => void = () => {};
    const mountRead = new Promise<void>((resolve) => {
      releaseMountRead = resolve;
    });

    mocks.readAllConfig
      .mockImplementationOnce(async () => {
        await mountRead;
        return { data: { config: { BIOROUTER_PROVIDER: 'ollama' } } };
      })
      .mockImplementation(async () => ({
        data: { config: { BIOROUTER_PROVIDER: 'versa_azure' } },
      }));

    renderProbe();

    // The mount read is still in flight when a provider write lands and asks
    // for a refresh. The refresh answers first.
    fireEvent.click(screen.getByRole('button', { name: 'Refresh' }));
    await waitFor(() => expect(screen.getByTestId('provider')).toHaveTextContent('versa_azure'));

    // Now the older read finally answers — with the pre-write snapshot.
    await act(async () => {
      releaseMountRead();
    });

    expect(screen.getByTestId('provider')).toHaveTextContent('versa_azure');
  });

  it('still applies an in-flight read when a newer read fails', async () => {
    let releaseMountRead: () => void = () => {};
    const mountRead = new Promise<void>((resolve) => {
      releaseMountRead = resolve;
    });

    mocks.readAllConfig
      .mockImplementationOnce(async () => {
        await mountRead;
        return { data: { config: { BIOROUTER_PROVIDER: 'ollama' } } };
      })
      .mockRejectedValue(new Error('daemon unreachable'));

    renderProbe();

    fireEvent.click(screen.getByRole('button', { name: 'Refresh' }));
    await waitFor(() =>
      expect(screen.getByTestId('refresh-error')).toHaveTextContent('daemon unreachable')
    );

    // The refresh failed, so it never became the cache's contents. Discarding
    // the older read on its account would leave the cache empty for good.
    await act(async () => {
      releaseMountRead();
    });

    expect(screen.getByTestId('provider')).toHaveTextContent('ollama');
  });

  it('still loads providers and extensions when the initial config read fails', async () => {
    mocks.readAllConfig.mockRejectedValue(new Error('daemon still starting'));

    renderProbe();

    await waitFor(() => expect(mocks.providers).toHaveBeenCalled());
    await waitFor(() => expect(mocks.getExtensions).toHaveBeenCalled());
    expect(screen.getByTestId('provider')).toHaveTextContent('none');
  });

  it('does not fail a write because the re-read that follows it failed', async () => {
    renderProbe();
    await waitFor(() => expect(screen.getByTestId('provider')).toHaveTextContent('ollama'));

    mocks.readAllConfig.mockRejectedValue(new Error('daemon unreachable'));

    fireEvent.click(screen.getByRole('button', { name: 'Upsert' }));

    // The write landed. Reporting it as failed because a *cache* could not be
    // re-read would be wrong in the more alarming direction, and callers that
    // write several keys in sequence would stop partway through.
    await waitFor(() => expect(screen.getByTestId('upsert-result')).toHaveTextContent('ok'));
    expect(mocks.upsertConfig).toHaveBeenCalledTimes(1);
    expect(screen.getByTestId('provider')).toHaveTextContent('ollama');
  });
});
