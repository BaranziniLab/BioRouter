import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import LocalModelInventory from './LocalModelInventory';
import { llamaServerStore, resetLlamaServerStoreForTests } from './llamaServerStore';

const mockLlamacppStatus = vi.fn();

vi.mock('../../../api', () => ({
  llamacppStatus: (...args: unknown[]) => mockLlamacppStatus(...args),
  llamacppEnsure: vi.fn(),
  llamacppWarmup: vi.fn(),
  llamacppDelete: vi.fn(),
}));

vi.mock('../../../utils/ollamaDetection', () => ({
  checkOllamaStatus: vi.fn().mockResolvedValue({ isRunning: false }),
  deleteOllamaModel: vi.fn(),
  pullOllamaModel: vi.fn(),
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

const statusResponse = () => ({
  data: {
    sidecar: {
      state: 'stopped',
      warmed: false,
      build: 'test',
      detail: null,
      model: null,
    },
    catalog: [catalogEntry()],
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

describe('LocalModelInventory', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetLlamaServerStoreForTests();
    mockLlamacppStatus.mockResolvedValue(statusResponse());
  });

  afterEach(() => {
    resetLlamaServerStoreForTests();
  });

  it('shows the expected-speed hint next to the download size in each row (#35)', async () => {
    render(<LocalModelInventory />);
    const row = await screen.findByText(
      /Gemma 4 · 9\.6 GB · Fast — ~4B active parameters · 131,072 context/
    );
    expect(row).toBeInTheDocument();
  });

  it('shows the expected-speed detail row in the model info dialog (#35)', async () => {
    render(<LocalModelInventory />);
    fireEvent.click(await screen.findByText('View Info'));

    expect(await screen.findByText('Expected speed')).toBeInTheDocument();
    expect(screen.getAllByText(/Fast — ~4B active parameters/).length).toBeGreaterThanOrEqual(2);
  });

  it('renders the busy row for an operation started elsewhere (#34)', async () => {
    llamaServerStore.beginOperation('start', 'gemma4', 'downloading 42%', { poll: false });
    render(<LocalModelInventory />);

    expect(await screen.findByText('downloading 42%')).toBeInTheDocument();
  });
});
