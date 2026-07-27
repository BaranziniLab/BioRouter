import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import LlamaServerInlineCard from './LlamaServerInlineCard';
import {
  llamaServerStore,
  resetLlamaServerStoreForTests,
  LLAMA_SERVER_POLL_INTERVAL_MS,
} from '../settings/models/llamaServerStore';

const mockLlamacppStatus = vi.fn();
const mockLlamacppEnsure = vi.fn();
const mockLlamacppWarmup = vi.fn();

vi.mock('../../api', () => ({
  llamacppStatus: (...args: unknown[]) => mockLlamacppStatus(...args),
  llamacppEnsure: (...args: unknown[]) => mockLlamacppEnsure(...args),
  llamacppWarmup: (...args: unknown[]) => mockLlamacppWarmup(...args),
}));

vi.mock('react-router-dom', () => ({
  useNavigate: () => vi.fn(),
}));

const mockUpsert = vi.fn();
vi.mock('../ConfigContext', () => ({
  useConfig: () => ({ upsert: mockUpsert }),
}));

const mockToastError = vi.fn();
const mockToastSuccess = vi.fn();
vi.mock('../../toasts', () => ({
  toastService: {
    error: (...args: unknown[]) => mockToastError(...args),
    success: (...args: unknown[]) => mockToastSuccess(...args),
  },
}));

const catalogEntry = (overrides: Record<string, unknown> = {}) => ({
  name: 'gemma4',
  display_name: 'Gemma 4 E4B',
  family: 'Gemma 4',
  ollama_name: 'gemma4:latest',
  official_url: 'https://ollama.com/library/gemma4',
  hf_spec: 'google/gemma-4-E4B-it-qat-q4_0-gguf:Q4_0',
  download_size: '9.6 GB',
  description: 'Laptop default',
  min_gpu_memory_gib: 16,
  recommended_gpu_memory_gib: 16,
  context_limit: 131072,
  active_params_b: 4,
  speed_hint: 'Fast — ~4B active parameters',
  is_default: true,
  downloaded: false,
  download_status: 'not_downloaded',
  download_source: 'none',
  fallback_downloaded: false,
  fallback_download_status: 'not_downloaded',
  model_path: null,
  suitable: true,
  suitability_status: 'suitable',
  suitability_message: 'Recommended for this machine.',
  ...overrides,
});

const statusResponse = (sidecar: Record<string, unknown> = {}) => ({
  data: {
    sidecar: {
      state: 'stopped',
      warmed: false,
      build: 'test',
      detail: null,
      model: null,
      ...sidecar,
    },
    catalog: [
      catalogEntry(),
      catalogEntry({
        name: 'gemma4-12b',
        display_name: 'Gemma 4 12B',
        download_size: '7.6 GB',
        active_params_b: 12,
        speed_hint: 'Fast — dense 12B, quick to load',
        is_default: false,
      }),
    ],
    system: {
      os: 'macos',
      total_memory_gib: 64,
      accelerator_memory_gib: 64,
      accelerator_memory_kind: 'apple_unified',
      default_context_size: 131072,
      model_cache_dir: '/tmp/models',
      model_cache_layout: 'test',
    },
  },
});

describe('LlamaServerInlineCard', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetLlamaServerStoreForTests();
    mockLlamacppStatus.mockResolvedValue(statusResponse());
  });

  afterEach(() => {
    resetLlamaServerStoreForTests();
    vi.useRealTimers();
  });

  it('preselects the default catalog model', async () => {
    render(<LlamaServerInlineCard onSuccess={vi.fn()} />);
    const select = await screen.findByTestId('llamacpp-model-select');
    await waitFor(() => expect(select).toHaveValue('gemma4'));
  });

  it('shows download size and expected speed for the selected model (#35)', async () => {
    render(<LlamaServerInlineCard onSuccess={vi.fn()} />);
    const info = await screen.findByTestId('llamacpp-size-speed');
    expect(info).toHaveTextContent('9.6 GB download · Fast — ~4B active parameters');
  });

  it('restores the live progress box when remounting during an in-flight download', async () => {
    // A download started earlier (possibly from another surface) is still
    // running in the shared store while this card was unmounted.
    mockLlamacppStatus.mockResolvedValue(
      statusResponse({ state: 'starting', model: 'gemma4', detail: 'downloading 42%' })
    );
    llamaServerStore.beginOperation('start', 'gemma4', 'downloading 42%');

    const first = render(<LlamaServerInlineCard onSuccess={vi.fn()} />);
    expect(await first.findByTestId('llamacpp-progress')).toHaveTextContent(
      'Preparing gemma4. Loading or downloading on first use…'
    );
    first.unmount();

    // The store keeps the operation; a fresh mount shows progress immediately.
    render(<LlamaServerInlineCard onSuccess={vi.fn()} />);
    expect(await screen.findByTestId('llamacpp-progress')).toHaveTextContent('downloading 42%');
    expect(llamaServerStore.getSnapshot().operation).not.toBeNull();
  });

  it('runs the start flow through the shared store and connects on success', async () => {
    mockUpsert.mockResolvedValue(undefined);
    mockLlamacppEnsure.mockResolvedValue(statusResponse({ state: 'starting', model: 'gemma4' }));
    mockLlamacppWarmup.mockResolvedValue({
      data: {
        output: 'OK',
        sidecar: { state: 'ready', warmed: true, build: 'test', model: 'gemma4', detail: null },
      },
    });
    const onSuccess = vi.fn();
    render(<LlamaServerInlineCard onSuccess={onSuccess} />);

    const start = await screen.findByTestId('llamacpp-start');
    start.click();

    await waitFor(() => expect(onSuccess).toHaveBeenCalled());
    expect(mockLlamacppEnsure).toHaveBeenCalledWith({
      body: { model: 'gemma4' },
      throwOnError: true,
    });
    expect(mockLlamacppWarmup).toHaveBeenCalledWith({
      body: { model: 'gemma4' },
      throwOnError: true,
    });
    expect(mockUpsert).toHaveBeenCalledWith('BIOROUTER_MODEL', 'gemma4', false);
    // The operation ended, so the shared poll interval is stopped.
    expect(llamaServerStore.getSnapshot().operation).toBeNull();
  });

  it('never writes config or navigates from an unmounted card (finding 7)', async () => {
    mockUpsert.mockResolvedValue(undefined);
    mockLlamacppEnsure.mockResolvedValue(statusResponse({ state: 'starting', model: 'gemma4' }));
    let resolveWarmup!: (value: unknown) => void;
    mockLlamacppWarmup.mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveWarmup = resolve;
        })
    );
    const onSuccess = vi.fn();
    const view = render(<LlamaServerInlineCard onSuccess={onSuccess} />);

    fireEvent.click(await view.findByTestId('llamacpp-start'));
    await waitFor(() => expect(mockLlamacppWarmup).toHaveBeenCalled());

    // The user navigates away mid warm-up; the flow keeps running.
    view.unmount();
    resolveWarmup({
      data: {
        output: 'OK',
        sidecar: { state: 'ready', warmed: true, build: 'test', model: 'gemma4', detail: null },
      },
    });

    // The flow still ends the shared operation cleanly...
    await waitFor(() => expect(llamaServerStore.getSnapshot().operation).toBeNull());
    // ...but the dead card must not configure the provider or navigate.
    expect(mockUpsert).not.toHaveBeenCalled();
    expect(onSuccess).not.toHaveBeenCalled();
  });

  it('surfaces a polled sidecar error immediately; the stale flow cannot double-toast or kill a retry (finding 9)', async () => {
    mockUpsert.mockResolvedValue(undefined);
    mockLlamacppEnsure.mockResolvedValue(statusResponse({ state: 'starting', model: 'gemma4' }));
    let rejectWarmup!: (err: Error) => void;
    mockLlamacppWarmup.mockImplementation(
      () =>
        new Promise((_resolve, reject) => {
          rejectWarmup = reject;
        })
    );
    render(<LlamaServerInlineCard onSuccess={vi.fn()} />);
    const start = await screen.findByTestId('llamacpp-start');

    vi.useFakeTimers();
    fireEvent.click(start);
    // Let the ensure call settle and the shared poll loop start.
    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });

    // The next poll tick reports a terminal sidecar error while the warm-up
    // HTTP call is still hanging.
    mockLlamacppStatus.mockResolvedValue(
      statusResponse({ state: 'error', model: 'gemma4', detail: 'model blew up' })
    );
    await act(async () => {
      await vi.advanceTimersByTimeAsync(LLAMA_SERVER_POLL_INTERVAL_MS);
    });

    // Toasted immediately from the retained store error — NOT deferred until
    // the warm-up request eventually rejects.
    expect(mockToastError).toHaveBeenCalledTimes(1);
    expect(mockToastError.mock.calls[0][0]).toMatchObject({ msg: 'model blew up' });
    expect(llamaServerStore.getSnapshot().operation).toBeNull();

    // The user retries; when the stale flow's warm-up finally rejects it
    // must neither toast a second time nor end the retry's operation.
    const retryOp = llamaServerStore.beginOperation('start', 'gemma4');
    await act(async () => {
      rejectWarmup(new Error('502 Bad Gateway'));
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(mockToastError).toHaveBeenCalledTimes(1);
    expect(llamaServerStore.getSnapshot().operation).toMatchObject({ id: retryOp });
    llamaServerStore.endOperation(retryOp);
  });
});
