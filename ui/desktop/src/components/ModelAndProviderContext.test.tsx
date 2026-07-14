/**
 * @vitest-environment jsdom
 */
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ModelAndProviderProvider, useModelAndProvider } from './ModelAndProviderContext';
import type Model from './settings/models/modelInterface';

const mocks = vi.hoisted(() => ({
  llamacppStatus: vi.fn(),
  llamacppWarmup: vi.fn(),
  setConfigProvider: vi.fn(),
  updateAgentProvider: vi.fn(),
  read: vi.fn(),
  getProviders: vi.fn(),
  toastError: vi.fn(),
  toastSuccess: vi.fn(),
}));

vi.mock('../api', () => ({
  llamacppStatus: mocks.llamacppStatus,
  llamacppWarmup: mocks.llamacppWarmup,
  setConfigProvider: mocks.setConfigProvider,
  updateAgentProvider: mocks.updateAgentProvider,
}));

vi.mock('../toasts', () => ({
  toastError: mocks.toastError,
  toastSuccess: mocks.toastSuccess,
}));

vi.mock('./ConfigContext', () => ({
  useConfig: () => ({
    read: mocks.read,
    getProviders: mocks.getProviders,
  }),
}));

const llamaModel: Model = {
  name: 'qwen3.6',
  provider: 'llamacpp',
  alias: 'Qwen3.6',
  subtext: 'Llama Server',
  context_limit: 131072,
};

const sidecar = {
  state: 'stopped',
  model: null,
  hf_spec: null,
  ollama_name: null,
  model_path: null,
  model_source: null,
  port: 11543,
  binary_path: '/opt/homebrew/bin/llama-server',
  build: 'test',
  detail: null,
  context_size: null,
  warmed: false,
};

const statusResponse = {
  data: {
    sidecar,
    catalog: [
      {
        name: 'qwen3.6',
        display_name: 'Qwen3.6 (Ollama library)',
        ollama_name: 'qwen3.6:latest',
        hf_spec: 'unsloth/Qwen3.6-35B-A3B-GGUF:UD-Q4_K_M',
        download_size: '23 GB',
        description: 'Large Qwen3.6 MoE model from Ollama library.',
        min_gpu_memory_gib: 48,
        recommended_gpu_memory_gib: 64,
        context_limit: 262144,
        downloaded: false,
        download_status: 'not_downloaded',
        download_source: 'none',
        fallback_downloaded: false,
        fallback_download_status: 'not_downloaded',
        model_path: null,
        suitable: false,
        suitability_status: 'above_recommendation',
        suitability_message:
          'This model recommends 64 GiB of GPU-addressable memory; this machine reports 8 GiB vram.',
        is_default: true,
      },
    ],
    system: {
      os: 'windows',
      total_memory_gib: 32,
      accelerator_memory_gib: 8,
      accelerator_memory_kind: 'vram',
      default_context_size: 32768,
      model_cache_dir: '/Users/test/.ollama/models',
      model_cache_layout: 'Ollama manifests/blobs; Hugging Face fallback cache under the same root',
    },
  },
};

function SwitchHarness() {
  const { changeModel } = useModelAndProvider();
  return (
    <button type="button" onClick={() => void changeModel(null, llamaModel)}>
      Switch to local
    </button>
  );
}

function renderHarness() {
  return render(
    <ModelAndProviderProvider>
      <SwitchHarness />
    </ModelAndProviderProvider>
  );
}

describe('ModelAndProviderProvider Llama Server warm-up', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.read.mockImplementation(async (key: string) => {
      if (key === 'BIOROUTER_MODEL') return 'gpt-4o';
      if (key === 'BIOROUTER_PROVIDER') return 'openai';
      return null;
    });
    mocks.getProviders.mockResolvedValue([]);
    mocks.llamacppStatus.mockResolvedValue(statusResponse);
    mocks.llamacppWarmup.mockResolvedValue({
      data: {
        output: 'pong',
        sidecar: {
          ...sidecar,
          state: 'ready',
          model: llamaModel.name,
          warmed: true,
          context_size: 32768,
        },
      },
    });
    mocks.setConfigProvider.mockResolvedValue(undefined);
    mocks.updateAgentProvider.mockResolvedValue(undefined);
  });

  it('keeps the previous model when the warm-up prompt is declined', async () => {
    renderHarness();

    fireEvent.click(screen.getByRole('button', { name: 'Switch to local' }));

    expect(await screen.findByText('Warm up local model')).toBeInTheDocument();
    expect(screen.getAllByText(/8 GiB VRAM/).length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText('Needs download')).toBeInTheDocument();
    expect(screen.getByText('Llama Server fallback')).toBeInTheDocument();
    expect(screen.getByText('Fallback may download')).toBeInTheDocument();
    expect(screen.getByText('Ollama model')).toBeInTheDocument();
    expect(screen.getByText('qwen3.6:latest')).toBeInTheDocument();
    expect(screen.getByText('Model store')).toBeInTheDocument();
    expect(screen.getByText('/Users/test/.ollama/models')).toBeInTheDocument();
    expect(screen.getByText(/32,768 tokens/)).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Keep previous model' }));

    await waitFor(() => {
      expect(screen.queryByText('Warm up local model')).not.toBeInTheDocument();
    });
    expect(mocks.llamacppWarmup).not.toHaveBeenCalled();
    expect(mocks.setConfigProvider).not.toHaveBeenCalled();
    expect(mocks.updateAgentProvider).not.toHaveBeenCalled();
  });

  it('switches to Llama Server only after a non-empty warm-up output', async () => {
    renderHarness();

    fireEvent.click(screen.getByRole('button', { name: 'Switch to local' }));
    fireEvent.click(await screen.findByRole('button', { name: 'Warm up model' }));

    await waitFor(() => {
      expect(mocks.llamacppWarmup).toHaveBeenCalledWith({
        body: { model: llamaModel.name },
        throwOnError: true,
      });
      expect(mocks.setConfigProvider).toHaveBeenCalledWith({
        body: {
          provider: 'llamacpp',
          model: llamaModel.name,
        },
        throwOnError: true,
      });
    });
  });

  it('does not switch models when warm-up returns an empty completion', async () => {
    mocks.llamacppWarmup.mockResolvedValueOnce({
      data: {
        output: '  ',
        sidecar: {
          ...sidecar,
          state: 'ready',
          model: llamaModel.name,
          warmed: false,
        },
      },
    });
    renderHarness();

    fireEvent.click(screen.getByRole('button', { name: 'Switch to local' }));
    fireEvent.click(await screen.findByRole('button', { name: 'Warm up model' }));

    await waitFor(() => {
      expect(mocks.toastError).toHaveBeenCalledWith(
        expect.objectContaining({ title: 'Llama Server warm-up failed' })
      );
    });
    expect(mocks.setConfigProvider).not.toHaveBeenCalled();
  });
});
