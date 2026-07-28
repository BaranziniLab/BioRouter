import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { ModelRef } from '../../../api/types.gen';
import { IngestPanel } from './IngestPanel';

const mocks = vi.hoisted(() => ({
  knowledge: {
    primaryKbId: 'kb-1' as string | null,
    primaryKb: { id: 'kb-1', name: 'Notes', default_model: null as ModelRef | null },
    loading: false,
    basesError: null as string | null,
    refresh: vi.fn(),
    triggerGraphRefresh: vi.fn(),
  },
  modelAndProvider: {
    currentProvider: null as string | null,
    currentModel: null as string | null,
    modelConfigStatus: 'ready' as 'loading' | 'ready',
  },
  checkModel: vi.fn(),
  start: vi.fn(),
  startMultipart: vi.fn(),
  abort: vi.fn(),
  knowledgeFetch: vi.fn(),
  expandKnowledgePath: vi.fn(),
  config: { getProviders: vi.fn(), getProviderModels: vi.fn() },
  /** Every `value` the model picker has been rendered with, oldest first. */
  pickerValues: [] as (ModelRef | null)[],
  /** The picker's latest `onChange`, so a test can pick a model without a provider list. */
  pickModel: null as ((next: ModelRef) => void) | null,
}));

vi.mock('../KnowledgeContext', () => ({
  useKnowledge: () => mocks.knowledge,
}));

// The real picker still renders — this only records what it was handed on each
// commit, so a test can assert on the *sequence* of values and not just the
// settled one. A model that is only wrong for one commit is still a model the
// user can see and click Digest against.
vi.mock('./IngestModelPicker', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./IngestModelPicker')>();
  return {
    IngestModelPicker: (props: Parameters<typeof actual.IngestModelPicker>[0]) => {
      mocks.pickerValues.push(props.value);
      mocks.pickModel = props.onChange;
      return <actual.IngestModelPicker {...props} />;
    },
  };
});

vi.mock('../hooks/knowledgeRequest', () => ({
  knowledgeFetch: mocks.knowledgeFetch,
  expandKnowledgePath: mocks.expandKnowledgePath,
}));

vi.mock('../../ModelAndProviderContext', () => ({
  useModelAndProvider: () => mocks.modelAndProvider,
}));

// One stable object: `useConfig()` handing back a fresh one on every call would
// change the picker's effect dependencies on every render.
vi.mock('../../ConfigContext', () => ({
  useConfig: () => mocks.config,
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

function modelLabels(): string[] {
  return mocks.pickerValues.map((value) => (value ? `${value.provider} / ${value.model}` : 'none'));
}

beforeEach(() => {
  vi.clearAllMocks();
  mocks.knowledge.primaryKbId = 'kb-1';
  mocks.knowledge.primaryKb = { id: 'kb-1', name: 'Notes', default_model: null };
  mocks.knowledge.loading = false;
  mocks.knowledge.basesError = null;
  mocks.modelAndProvider.currentProvider = null;
  mocks.modelAndProvider.currentModel = null;
  mocks.modelAndProvider.modelConfigStatus = 'ready';
  mocks.checkModel.mockResolvedValue({ data: { ok: true } });
  mocks.start.mockResolvedValue({ status: 'done' });
  mocks.knowledgeFetch.mockResolvedValue({ ok: true, text: async () => '' });
  mocks.config.getProviders.mockResolvedValue([]);
  mocks.config.getProviderModels.mockResolvedValue([]);
  mocks.pickerValues.length = 0;
  mocks.pickModel = null;
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

describe('IngestPanel model loading state', () => {
  it('does not claim nothing is configured while the config is still being read', async () => {
    mocks.modelAndProvider.modelConfigStatus = 'loading';

    render(<IngestPanel />);
    stageSomeText();

    const trigger = await screen.findByTestId('knowledge-model-picker-trigger');
    expect(trigger).toHaveTextContent(/loading/i);
    expect(trigger.textContent).not.toMatch(/no model configured/i);

    // Still not dispatchable — but the reason offered is "wait", not "go and
    // configure a model", which is advice for a state we cannot see yet.
    const digest = screen.getByTestId('knowledge-digest-button');
    expect(digest).toHaveAttribute('aria-disabled', 'true');
    expect(screen.queryByText(/no model is configured/i)).not.toBeInTheDocument();

    fireEvent.click(digest);
    await waitFor(() => expect(mocks.checkModel).not.toHaveBeenCalled());
  });

  it('says nothing is configured once the read has finished with nothing', async () => {
    mocks.modelAndProvider.modelConfigStatus = 'ready';

    render(<IngestPanel />);

    const trigger = await screen.findByTestId('knowledge-model-picker-trigger');
    expect(trigger).toHaveTextContent(/no model configured/i);
    expect(screen.getByText(/no model is configured/i)).toBeInTheDocument();
  });

  it('uses a base default immediately, without waiting on the app config', async () => {
    mocks.modelAndProvider.modelConfigStatus = 'loading';
    mocks.knowledge.primaryKb = {
      id: 'kb-1',
      name: 'Notes',
      default_model: { provider: 'ollama', model: 'qwen3.6:latest' },
    };

    render(<IngestPanel />);
    stageSomeText();

    const trigger = await screen.findByTestId('knowledge-model-picker-trigger');
    expect(trigger).toHaveTextContent('ollama / qwen3.6:latest');

    fireEvent.click(screen.getByTestId('knowledge-digest-button'));
    await waitFor(() => expect(mocks.checkModel).toHaveBeenCalled());
    expect(mocks.checkModel).toHaveBeenCalledWith({
      body: { model: { provider: 'ollama', model: 'qwen3.6:latest' } },
    });
  });

  it('waits for the base itself before falling back to the app config', async () => {
    // The base's own default outranks the app config, so a base whose manifest
    // has not arrived yet has no resolved model — naming the app's one here
    // would both display and dispatch a model this base overrides.
    mocks.modelAndProvider.currentProvider = 'versa_azure';
    mocks.modelAndProvider.currentModel = 'gpt-5.5-2026-04-24';
    mocks.knowledge.primaryKbId = 'kb-1';
    mocks.knowledge.primaryKb = null as unknown as (typeof mocks.knowledge)['primaryKb'];
    mocks.knowledge.loading = true;

    render(<IngestPanel />);

    const trigger = await screen.findByTestId('knowledge-model-picker-trigger');
    expect(trigger).toHaveTextContent(/loading/i);
    expect(trigger.textContent).not.toContain('versa_azure');
  });
});

describe('IngestPanel model freshness', () => {
  it('never renders the previous base’s model after the primary base changes', async () => {
    mocks.modelAndProvider.currentProvider = 'versa_azure';
    mocks.modelAndProvider.currentModel = 'gpt-5.5-2026-04-24';
    mocks.knowledge.primaryKb = {
      id: 'kb-1',
      name: 'Notes',
      default_model: { provider: 'ollama', model: 'qwen3.6:latest' },
    };

    const { rerender } = render(<IngestPanel />);
    const trigger = await screen.findByTestId('knowledge-model-picker-trigger');
    await waitFor(() => expect(trigger).toHaveTextContent('ollama / qwen3.6:latest'));

    mocks.pickerValues.length = 0;
    mocks.knowledge.primaryKbId = 'kb-2';
    mocks.knowledge.primaryKb = { id: 'kb-2', name: 'Papers', default_model: null };
    rerender(<IngestPanel />);

    // Not "settles on the right model eventually" — the base that is no longer
    // primary must never be named once the switch has been committed.
    expect(modelLabels()).not.toContain('ollama / qwen3.6:latest');
    expect(trigger).toHaveTextContent('versa_azure / gpt-5.5-2026-04-24');
  });

  it('does not carry a model chosen for one base over to another', async () => {
    mocks.modelAndProvider.currentProvider = 'versa_azure';
    mocks.modelAndProvider.currentModel = 'gpt-5.5-2026-04-24';

    const { rerender } = render(<IngestPanel />);
    const trigger = await screen.findByTestId('knowledge-model-picker-trigger');
    await waitFor(() => expect(trigger).toHaveTextContent('versa_azure / gpt-5.5-2026-04-24'));

    // Pick a per-base model for kb-1. Neither base has a `default_model` in this
    // fixture, so nothing the resolver reads changes when the base does — the
    // choice must be scoped to kb-1 by construction, not by a dependency array.
    await act(async () => {
      mocks.pickModel?.({ provider: 'ollama', model: 'qwen3.6:latest' });
    });
    await waitFor(() => expect(trigger).toHaveTextContent('ollama / qwen3.6:latest'));

    mocks.knowledge.primaryKbId = 'kb-2';
    mocks.knowledge.primaryKb = { id: 'kb-2', name: 'Papers', default_model: null };
    rerender(<IngestPanel />);

    expect(trigger).toHaveTextContent('versa_azure / gpt-5.5-2026-04-24');

    stageSomeText();
    fireEvent.click(screen.getByTestId('knowledge-digest-button'));
    await waitFor(() => expect(mocks.checkModel).toHaveBeenCalled());
    expect(mocks.checkModel).toHaveBeenCalledWith({
      body: { model: { provider: 'versa_azure', model: 'gpt-5.5-2026-04-24' } },
    });
  });
});

// A primary id whose manifest never arrived. The list read is over — so
// "loading" no longer covers it — but `bases` does not describe the daemon's
// state, and this base's own `default_model` outranks the app's. Falling
// through to the app config here names, and dispatches, a model the base may
// override, at an id nothing has confirmed still exists.
describe('IngestPanel with an unresolvable knowledge base', () => {
  function unresolvedPrimary(basesError: string | null) {
    mocks.knowledge.primaryKbId = 'kb-1';
    mocks.knowledge.primaryKb = null as unknown as (typeof mocks.knowledge)['primaryKb'];
    mocks.knowledge.loading = false;
    mocks.knowledge.basesError = basesError;
    mocks.modelAndProvider.currentProvider = 'versa_azure';
    mocks.modelAndProvider.currentModel = 'gpt-5.5-2026-04-24';
  }

  it('does not fall back to the app model once the base list read has failed', async () => {
    unresolvedPrimary('daemon down');

    render(<IngestPanel />);

    const trigger = await screen.findByTestId('knowledge-model-picker-trigger');
    // Not "settles on nothing eventually" — the app's model must never be
    // named for a base whose own default is unknown, on any commit.
    expect(modelLabels()).not.toContain('versa_azure / gpt-5.5-2026-04-24');
    expect(trigger.textContent).not.toContain('versa_azure');
    expect(trigger).toHaveTextContent(/unavailable/i);
  });

  it('never dispatches a digest at an id whose base it could not resolve', async () => {
    unresolvedPrimary('daemon down');

    render(<IngestPanel />);
    stageSomeText();

    const digest = screen.getByTestId('knowledge-digest-button');
    expect(digest).toHaveAttribute('aria-disabled', 'true');
    await act(async () => {
      fireEvent.click(digest);
    });

    expect(mocks.checkModel).not.toHaveBeenCalled();
    expect(mocks.start).not.toHaveBeenCalled();
    expect(mocks.startMultipart).not.toHaveBeenCalled();
  });

  it('offers a retry instead of blaming the user’s configuration', async () => {
    unresolvedPrimary('daemon down');

    render(<IngestPanel />);
    await screen.findByTestId('knowledge-model-picker-trigger');

    // "No model is configured" is advice about a setup that is not broken.
    expect(screen.queryByText(/no model is configured/i)).not.toBeInTheDocument();
    expect(screen.getByText(/could not load your knowledge bases/i)).toBeInTheDocument();

    fireEvent.click(screen.getByTestId('knowledge-ingest-retry'));
    expect(mocks.knowledge.refresh).toHaveBeenCalled();
  });

  it('says the base is unavailable when the list arrived without it', async () => {
    unresolvedPrimary(null);

    render(<IngestPanel />);
    await screen.findByTestId('knowledge-model-picker-trigger');

    expect(screen.getByText(/knowledge base is unavailable/i)).toBeInTheDocument();
    expect(screen.queryByText(/no model is configured/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/could not load your knowledge bases/i)).not.toBeInTheDocument();
  });

  it('does not offer to save a default model to a base it cannot address', async () => {
    unresolvedPrimary('daemon down');

    render(<IngestPanel />);

    expect(await screen.findByTestId('knowledge-model-picker-trigger')).toBeDisabled();
  });

  it('still tells a genuinely unconfigured setup apart from an unavailable base', async () => {
    // Base resolved, nothing else configured: this *is* the user's setup, and
    // the unavailable wording must not swallow the one honest verdict.
    mocks.modelAndProvider.currentProvider = null;
    mocks.modelAndProvider.currentModel = null;

    render(<IngestPanel />);
    await screen.findByTestId('knowledge-model-picker-trigger');

    expect(screen.getByText(/no model is configured/i)).toBeInTheDocument();
    expect(screen.queryByTestId('knowledge-ingest-retry')).not.toBeInTheDocument();
  });
});
