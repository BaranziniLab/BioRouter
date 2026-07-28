import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ConfigProvider, useConfig } from '../../ConfigContext';
import { AlertBox } from '../AlertBox';
import { Alert, AlertType } from '../types';

// Issue #52. The alert's threshold editor already reads through `ConfigContext`,
// but wrote straight to the API, so the write never invalidated the cache the
// read came from. `BIOROUTER_AUTO_COMPACT_THRESHOLD` is not a secret, so it does
// populate that cache: any consumer reading it from there would keep seeing the
// pre-edit value.
//
// Driven against the real `ConfigProvider` over a fake config store, so the
// assertion is "the cache agrees with what was written", not "some particular
// function was called".

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

vi.mock('../../../api', () => ({
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

vi.mock('../../settings/extensions', () => ({
  syncBundledExtensions: mocks.syncBundledExtensions,
}));

const THRESHOLD_KEY = 'BIOROUTER_AUTO_COMPACT_THRESHOLD';

let store: Record<string, unknown> = {};

beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  store = { [THRESHOLD_KEY]: 0.8 };

  mocks.readAllConfig.mockImplementation(async () => ({ data: { config: { ...store } } }));
  mocks.readConfig.mockImplementation(async ({ body }: { body: { key: string } }) => ({
    data: store[body.key] ?? null,
  }));
  mocks.upsertConfig.mockImplementation(
    async ({ body }: { body: { key: string; value: unknown } }) => {
      store[body.key] = body.value;
      return { data: {} };
    }
  );
  mocks.getExtensions.mockResolvedValue({
    data: { extensions: [], warnings: [] },
    response: { status: 200 },
  });
  mocks.providers.mockResolvedValue({ data: [] });
  mocks.syncBundledExtensions.mockResolvedValue(undefined);
});

function CachedThreshold() {
  const { config } = useConfig();
  return <div data-testid="cached-threshold">{String(config[THRESHOLD_KEY] ?? 'unset')}</div>;
}

describe('AlertBox threshold editing keeps the config cache consistent (#52)', () => {
  it('refreshes the cache after saving a new auto-compact threshold', async () => {
    const alert: Alert = {
      type: AlertType.Info,
      message: 'Context window',
      progress: { current: 50, total: 100 },
    };

    const { container } = render(
      <ConfigProvider>
        <CachedThreshold />
        <AlertBox alert={alert} />
      </ConfigProvider>
    );

    await waitFor(() => expect(screen.getByTestId('cached-threshold')).toHaveTextContent(/^0\.8$/));
    await waitFor(() => expect(screen.getByText('Auto compact at 80%')).toBeInTheDocument());

    // Icon-only controls: the sole button is "edit", and once editing it is
    // replaced by "save".
    fireEvent.click(container.querySelector('button')!);
    const input = await screen.findByRole('spinbutton');
    fireEvent.change(input, { target: { value: '55' } });
    fireEvent.mouseDown(container.querySelector('button')!);

    await waitFor(() => expect(store[THRESHOLD_KEY]).toBeCloseTo(0.55));
    await waitFor(() =>
      expect(screen.getByTestId('cached-threshold')).toHaveTextContent(/^0\.55$/)
    );
  });
});
