import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { useState } from 'react';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { ConfigProvider, useConfig } from './ConfigContext';
import { ContextWindowGauge } from './ContextWindowIndicator';

// Issue #52. `ConfigContext.config` is a cache, and the only thing that keeps it
// honest is a re-read after every write. A component that reaches past the
// context and POSTs to `/config/upsert` itself leaves the cache serving the
// pre-write value to every other consumer — silently, for as long as it takes
// some unrelated write to refresh it.
//
// The auto-compact threshold is not a secret, so it really does live in that
// cache. Nothing reads it from there *today*, which is exactly why this needs a
// test: the next consumer that does would inherit the bug with no signal.
//
// These tests drive the real `ConfigProvider` over a fake config store, so
// "the cache agrees with what was written" is asserted end to end rather than
// by spying on which function the component happened to call.

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

const THRESHOLD_KEY = 'BIOROUTER_AUTO_COMPACT_THRESHOLD';

let store: Record<string, unknown> = {};

beforeAll(() => {
  vi.stubGlobal(
    'ResizeObserver',
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    }
  );
});

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

/** Renders whatever the cache currently believes the threshold to be. */
function CachedThreshold() {
  const { config } = useConfig();
  return <div data-testid="cached-threshold">{String(config[THRESHOLD_KEY] ?? 'unset')}</div>;
}

/** The gauge, plus a way to unmount it while the provider stays mounted — the
 * gauge flushes a pending drag from its own unmount cleanup. */
function Harness() {
  const [mounted, setMounted] = useState(true);
  return (
    <ConfigProvider>
      <CachedThreshold />
      <button type="button" onClick={() => setMounted(false)}>
        Close gauge
      </button>
      {mounted ? (
        <ContextWindowGauge
          totalTokens={24_000}
          tokenLimit={1_100_000}
          isTokenLimitLoaded
          onCompact={vi.fn()}
        />
      ) : null}
    </ConfigProvider>
  );
}

async function renderHarness() {
  render(<Harness />);
  // Wait for both the provider's mount read and the gauge's own read.
  await waitFor(() => expect(screen.getByTestId('cached-threshold')).toHaveTextContent('0.8'));
  await waitFor(() => expect(screen.getByRole('slider')).toHaveAttribute('aria-valuenow', '80'));
}

describe('auto-compact threshold writes keep the config cache consistent (#52)', () => {
  it('refreshes the cache when the slider is adjusted', async () => {
    await renderHarness();

    fireEvent.keyDown(screen.getByRole('slider'), { key: 'ArrowRight' });

    await waitFor(() => expect(store[THRESHOLD_KEY]).toBeCloseTo(0.81));
    await waitFor(() =>
      expect(screen.getByTestId('cached-threshold')).toHaveTextContent(/^0\.81$/)
    );
  });

  it('refreshes the cache when a pending drag is flushed on unmount', async () => {
    const rect = vi
      .spyOn(Element.prototype, 'getBoundingClientRect')
      .mockReturnValue({ left: 0, top: 0, width: 200, height: 4 } as DOMRect);
    try {
      await renderHarness();

      // Press on the bar at the halfway point and never release: the value is
      // pending, and only the unmount cleanup persists it.
      fireEvent.mouseDown(screen.getByRole('slider'), { clientX: 100 });
      await waitFor(() =>
        expect(screen.getByRole('slider')).toHaveAttribute('aria-valuenow', '50')
      );
      expect(mocks.upsertConfig).not.toHaveBeenCalled();

      fireEvent.click(screen.getByRole('button', { name: 'Close gauge' }));

      await waitFor(() => expect(store[THRESHOLD_KEY]).toBeCloseTo(0.5));
      await waitFor(() =>
        expect(screen.getByTestId('cached-threshold')).toHaveTextContent(/^0\.5$/)
      );
    } finally {
      rect.mockRestore();
    }
  });
});
