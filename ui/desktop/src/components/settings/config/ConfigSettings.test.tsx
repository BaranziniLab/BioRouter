import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import type { ConfigData } from '../../../types/config';

// #50 — Settings → App named a stale provider ("current settings for ollama"
// while the active provider was versa_azure). ConfigContext's `config` is only
// refetched when a write goes THROUGH that context; switching model/provider
// writes via `setConfigProvider` on the API instead, so the cached copy keeps
// naming whatever was configured when the app started. The live provider is
// the one ModelAndProviderContext tracks.

const configSnapshot: ConfigData = {
  BIOROUTER_PROVIDER: 'ollama', // stale: what the app booted with
  BIOROUTER_MODEL: 'qwen3.6',
  OLLAMA_HOST: 'localhost',
};

const modelAndProvider = {
  currentProvider: null as string | null,
  refreshCurrentModelAndProvider: vi.fn(async () => {}),
};

const refreshConfig = vi.fn(async () => {});

vi.mock('../../ConfigContext', () => ({
  useConfig: () => ({ config: configSnapshot, upsert: vi.fn(), refreshConfig }),
}));

vi.mock('../../ModelAndProviderContext', () => ({
  useModelAndProvider: () => modelAndProvider,
}));

import ConfigSettings from './ConfigSettings';

beforeEach(() => {
  vi.clearAllMocks();
  modelAndProvider.currentProvider = 'versa_azure';
});

describe('ConfigSettings provider label (#50)', () => {
  it('names the live provider, not the one cached in the config context', () => {
    render(<ConfigSettings />);

    expect(
      screen.getByText(/Edit your Biorouter configuration settings \(current settings for/)
    ).toHaveTextContent('(current settings for versa_azure)');
    expect(screen.queryByText(/current settings for ollama/)).not.toBeInTheDocument();
  });

  it('updates the label when the active provider changes', () => {
    const { rerender } = render(<ConfigSettings />);

    modelAndProvider.currentProvider = 'databricks';
    rerender(<ConfigSettings />);

    expect(screen.getByText(/current settings for/)).toHaveTextContent(
      '(current settings for databricks)'
    );
  });

  it('re-reads the active provider when the section mounts', async () => {
    render(<ConfigSettings />);

    await waitFor(() =>
      expect(modelAndProvider.refreshCurrentModelAndProvider).toHaveBeenCalledTimes(1)
    );
  });

  // #52 — this page renders BIOROUTER_PROVIDER/BIOROUTER_MODEL as editable
  // fields straight out of the cached config. A model switch writes them
  // through the API, so opening Settings after one showed the pre-switch
  // values in the editor beside a correct live label.
  it('re-reads the cached config when the section opens', async () => {
    render(<ConfigSettings />);

    await waitFor(() => expect(refreshConfig).toHaveBeenCalledTimes(1));
  });

  it('falls back to the config value while the live provider is still loading', () => {
    modelAndProvider.currentProvider = null;
    render(<ConfigSettings />);

    expect(screen.getByText(/current settings for/)).toHaveTextContent(
      '(current settings for ollama)'
    );
  });
});
