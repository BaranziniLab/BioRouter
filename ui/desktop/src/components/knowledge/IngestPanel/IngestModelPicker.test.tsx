import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { ProviderDetails } from '../../../api';
import { IngestModelPicker } from './IngestModelPicker';

const mocks = vi.hoisted(() => ({
  config: { getProviders: vi.fn(), getProviderModels: vi.fn() },
}));

// One stable object: `useConfig()` handing back a fresh one on every call would
// change the picker's effect dependencies on every render. Same reasoning as
// `IngestPanel.test.tsx`, which is the other suite that renders this component.
vi.mock('../../ConfigContext', () => ({
  useConfig: () => mocks.config,
}));

/**
 * A configured provider with a curated model list.
 *
 * `known_models` is deliberately non-empty: `fetchModelsForProviders` short-
 * circuits to the curated list when there is one, so a fixture built this way
 * exercises the real model-loading path without a second mock standing in for
 * it. `tier: 'public'` puts every fixture in one group, which keeps the
 * assertions about *which providers appear* from doubling as assertions about
 * `providerOrdering.ts`'s grouping.
 */
function provider(name: string, displayName: string, models: string[]): ProviderDetails {
  return {
    is_configured: true,
    name,
    provider_type: 'Builtin',
    metadata: {
      name,
      display_name: displayName,
      description: '',
      default_model: models[0] ?? '',
      model_doc_link: '',
      config_keys: [],
      known_models: models.map((model) => ({ name: model, context_limit: 128_000 })),
      tier: 'public',
    },
  };
}

const OPENAI = provider('openai', 'OpenAI', ['gpt-5.5']);
const OLLAMA = provider('ollama', 'Ollama', ['qwen3.6:latest']);
const CLAUDE_CODE = provider('claude_code', 'Claude Code', ['opus-5']);
const CODEX = provider('codex', 'Codex', ['gpt-5.4-codex']);

function renderPicker() {
  return render(
    <MemoryRouter>
      <IngestModelPicker value={null} onChange={vi.fn()} />
    </MemoryRouter>
  );
}

/** Open the popover and wait for the model rows to have loaded into it. */
async function openPicker() {
  fireEvent.click(await screen.findByTestId('knowledge-model-picker-trigger'));
  return screen.findByRole('listbox', { name: 'Knowledge models' });
}

beforeEach(() => {
  vi.clearAllMocks();
  mocks.config.getProviderModels.mockResolvedValue([]);
});

describe('IngestModelPicker provider exclusions', () => {
  it('does not offer a model that cannot make the tool calls a digest is made of', async () => {
    mocks.config.getProviders.mockResolvedValue([OPENAI, CLAUDE_CODE, CODEX, OLLAMA]);
    renderPicker();
    const list = await openPicker();

    // The two coding-agent providers reach `complete_with_model` with `tools`
    // dropped on the floor, so a digest run against one narrates its calls and
    // writes nothing. Neither the group heading nor the model row may appear.
    await waitFor(() => expect(list).toHaveTextContent('OpenAI'));
    expect(list).not.toHaveTextContent('Claude Code');
    expect(list).not.toHaveTextContent('opus-5');
    expect(list).not.toHaveTextContent('Codex');
    expect(list).not.toHaveTextContent('gpt-5.4-codex');
  });

  it('still offers every other configured provider', async () => {
    mocks.config.getProviders.mockResolvedValue([OPENAI, CLAUDE_CODE, CODEX, OLLAMA]);
    renderPicker();
    const list = await openPicker();

    // The exclusion has to be surgical. A filter that took the whole list with
    // it would pass the assertion above and leave the picker empty.
    await waitFor(() => expect(list).toHaveTextContent('gpt-5.5'));
    expect(list).toHaveTextContent('OpenAI');
    expect(list).toHaveTextContent('Ollama');
    expect(list).toHaveTextContent('qwen3.6:latest');
  });

  it('says which providers it left out, and that they still work in chat', async () => {
    mocks.config.getProviders.mockResolvedValue([OPENAI, CLAUDE_CODE, CODEX]);
    renderPicker();
    await openPicker();

    const note = await screen.findByTestId('knowledge-model-picker-excluded');
    expect(note).toHaveTextContent('Claude Code and Codex');
    expect(note).toHaveTextContent(/written nothing/i);
    expect(note).toHaveTextContent(/still work in chat/i);
  });

  it('stays silent when there was nothing to leave out', async () => {
    mocks.config.getProviders.mockResolvedValue([OPENAI, OLLAMA]);
    renderPicker();
    const list = await openPicker();

    // The note is an explanation for an absence. With no absence to explain it
    // is pure noise in a popover that has room for none.
    await waitFor(() => expect(list).toHaveTextContent('gpt-5.5'));
    expect(screen.queryByTestId('knowledge-model-picker-excluded')).toBeNull();
  });

  it('explains the absence even when nothing at all is left to offer', async () => {
    mocks.config.getProviders.mockResolvedValue([CLAUDE_CODE]);
    renderPicker();
    await openPicker();

    // The worst case for the old behaviour: the one configured provider is
    // excluded, so the picker falls to its "No models available / configure a
    // provider in Settings" empty state — which is a verdict on a configuration
    // that is in fact fine. The note has to survive that branch to correct it.
    expect(await screen.findByText('No models available')).toBeInTheDocument();
    expect(await screen.findByTestId('knowledge-model-picker-excluded')).toHaveTextContent(
      'Claude Code'
    );
  });
});

describe('IngestModelPicker trigger label', () => {
  it('does not blame the configuration when the model merely cannot digest', async () => {
    mocks.config.getProviders.mockResolvedValue([CLAUDE_CODE]);
    render(
      <MemoryRouter>
        <IngestModelPicker value={null} valueState="unsupported" onChange={vi.fn()} />
      </MemoryRouter>
    );

    // "No model configured" would send a user with a working chat model to
    // Settings to fix something that is not broken.
    const trigger = await screen.findByTestId('knowledge-model-picker-trigger');
    expect(trigger).toHaveTextContent(/can’t digest sources/);
    expect(trigger).not.toHaveTextContent('No model configured');
  });
});
