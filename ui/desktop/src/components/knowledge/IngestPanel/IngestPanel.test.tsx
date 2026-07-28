import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { ModelRef } from '../../../api/types.gen';
import { IngestPanel } from './IngestPanel';

const mocks = vi.hoisted(() => ({
  knowledge: {
    primaryKbId: 'kb-1' as string | null,
    primaryKb: { id: 'kb-1', name: 'Notes', default_model: null as ModelRef | null },
    refresh: vi.fn(),
    triggerGraphRefresh: vi.fn(),
  },
  modelAndProvider: {
    currentProvider: null as string | null,
    currentModel: null as string | null,
  },
  checkModel: vi.fn(),
  start: vi.fn(),
  startMultipart: vi.fn(),
  abort: vi.fn(),
}));

vi.mock('../KnowledgeContext', () => ({
  useKnowledge: () => mocks.knowledge,
}));

vi.mock('../../ModelAndProviderContext', () => ({
  useModelAndProvider: () => mocks.modelAndProvider,
}));

vi.mock('../../ConfigContext', () => ({
  useConfig: () => ({
    getProviders: vi.fn().mockResolvedValue([]),
    getProviderModels: vi.fn().mockResolvedValue([]),
  }),
}));

vi.mock('../../../api/sdk.gen', () => ({
  checkModel: mocks.checkModel,
}));

vi.mock('../hooks/useIngestStream', () => ({
  useIngestStream: () => ({
    events: [],
    status: 'idle',
    finalResult: null,
    error: undefined,
    start: mocks.start,
    startMultipart: mocks.startMultipart,
    abort: mocks.abort,
  }),
}));

vi.mock('../DispatchProgress', () => ({
  DispatchProgress: () => null,
}));

vi.mock('../../../toasts', () => ({
  toastError: vi.fn(),
  toastSuccess: vi.fn(),
}));

function stageSomeText() {
  fireEvent.click(screen.getByTestId('knowledge-ingest-paste-text'));
  fireEvent.change(screen.getByPlaceholderText(/Paste knowledge/i), {
    target: { value: 'some knowledge' },
  });
  fireEvent.click(screen.getByRole('button', { name: 'Stage' }));
}

beforeEach(() => {
  vi.clearAllMocks();
  mocks.knowledge.primaryKbId = 'kb-1';
  mocks.knowledge.primaryKb = { id: 'kb-1', name: 'Notes', default_model: null };
  mocks.modelAndProvider.currentProvider = null;
  mocks.modelAndProvider.currentModel = null;
  mocks.checkModel.mockResolvedValue({ data: { ok: true } });
  mocks.start.mockResolvedValue({ status: 'done' });
});

describe('IngestPanel model selection', () => {
  it("preselects the app's configured provider and model, not a hardcoded vendor", async () => {
    mocks.modelAndProvider.currentProvider = 'versa_azure';
    mocks.modelAndProvider.currentModel = 'gpt-5.5-2026-04-24';

    render(<IngestPanel />);

    const trigger = await screen.findByTestId('knowledge-model-picker-trigger');
    await waitFor(() => expect(trigger).toHaveTextContent('versa_azure / gpt-5.5-2026-04-24'));
    expect(trigger.textContent).not.toMatch(/anthropic|claude/i);
  });

  it('dispatches digestion to the configured model', async () => {
    mocks.modelAndProvider.currentProvider = 'versa_azure';
    mocks.modelAndProvider.currentModel = 'gpt-5.5-2026-04-24';

    render(<IngestPanel />);
    stageSomeText();
    fireEvent.click(screen.getByTestId('knowledge-digest-button'));

    await waitFor(() => expect(mocks.checkModel).toHaveBeenCalled());
    expect(mocks.checkModel).toHaveBeenCalledWith({
      body: { model: { provider: 'versa_azure', model: 'gpt-5.5-2026-04-24' } },
    });
  });

  it("prefers the knowledge base's own default model when it has one", async () => {
    mocks.modelAndProvider.currentProvider = 'versa_azure';
    mocks.modelAndProvider.currentModel = 'gpt-5.5-2026-04-24';
    mocks.knowledge.primaryKb = {
      id: 'kb-1',
      name: 'Notes',
      default_model: { provider: 'ollama', model: 'qwen3.6:latest' },
    };

    render(<IngestPanel />);

    const trigger = await screen.findByTestId('knowledge-model-picker-trigger');
    await waitFor(() => expect(trigger).toHaveTextContent('ollama / qwen3.6:latest'));
  });

  it('says so and blocks digestion when no model can be resolved', async () => {
    render(<IngestPanel />);
    stageSomeText();

    const trigger = await screen.findByTestId('knowledge-model-picker-trigger');
    expect(trigger).toHaveTextContent(/no model configured/i);
    expect(trigger.textContent).not.toMatch(/anthropic|claude/i);

    const digest = screen.getByTestId('knowledge-digest-button');
    expect(digest).toHaveAttribute('aria-disabled', 'true');
    expect(screen.getByText(/choose a model/i)).toBeInTheDocument();

    fireEvent.click(digest);
    await waitFor(() => expect(mocks.checkModel).not.toHaveBeenCalled());
    expect(mocks.start).not.toHaveBeenCalled();
  });
});
