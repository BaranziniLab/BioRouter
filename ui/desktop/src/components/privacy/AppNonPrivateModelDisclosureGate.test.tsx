/**
 * @vitest-environment jsdom
 */
import { act, cleanup, render, screen, waitFor } from '@testing-library/react';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

/**
 * Task 30A (issue #56, DR-17 requirement 3) — **the conjunction**, not the
 * predicate.
 *
 * `NonPrivateModelDisclosure.test.tsx` proves the predicate: given a bound
 * public provider, disclose. Every one of its cases hands the gate a literal
 * (`"openai"`, `"llamacpp"`, `null`), so between them they say nothing about the
 * question the requirement actually asks — **on a fresh install, before the
 * first turn, is a provider bound at all?**
 *
 * It was not. The gate's only mount was `BaseChat`, keyed on
 * `session?.provider_name`, and a session row exists only after `/agent/resume`
 * has one to return — i.e. after the first turn has already gone out. Green
 * predicate tests, unmet requirement: the disclosure arrived as a receipt.
 *
 * So this suite deliberately mounts **no chat, no session and no literal**. It
 * renders the real `ModelAndProviderProvider` over a mocked config read and lets
 * the app resolve its own configured provider, which is the path a fresh install
 * actually takes.
 */
const SERVED = {
  titleTemplate: '{provider} is not hosted by your institution.',
  long: 'SERVED-COPY-MARKER — it can read files on this computer.',
  short: 'SERVED-SHORT-MARKER',
};

const mocks = vi.hoisted(() => ({
  getPrivacyDisclosure: vi.fn(),
  ackPrivacyDisclosure: vi.fn(),
  setConfigProvider: vi.fn(),
  updateAgentProvider: vi.fn(),
  llamacppStatus: vi.fn(),
  llamacppWarmup: vi.fn(),
  read: vi.fn(),
  getProviders: vi.fn(),
  refreshConfig: vi.fn(),
  toastError: vi.fn(),
  toastSuccess: vi.fn(),
}));

vi.mock('../../api', () => ({
  getPrivacyDisclosure: mocks.getPrivacyDisclosure,
  ackPrivacyDisclosure: mocks.ackPrivacyDisclosure,
  setConfigProvider: mocks.setConfigProvider,
  updateAgentProvider: mocks.updateAgentProvider,
  llamacppStatus: mocks.llamacppStatus,
  llamacppWarmup: mocks.llamacppWarmup,
}));

vi.mock('../../toasts', () => ({
  toastError: mocks.toastError,
  toastSuccess: mocks.toastSuccess,
}));

vi.mock('../ConfigContext', () => ({
  useConfig: () => ({
    read: mocks.read,
    getProviders: mocks.getProviders,
    refreshConfig: mocks.refreshConfig,
  }),
}));

vi.mock('../../utils/userAction', () => ({
  userActionHeaders: async () => ({ 'X-User-Action': 'test-key' }),
}));

import { ModelAndProviderProvider } from '../ModelAndProviderContext';
import { AppNonPrivateModelDisclosureGate } from './AppNonPrivateModelDisclosureGate';
import { __resetDisclosureStoreForTests } from './disclosureCopy';

/** A provider entry shaped like `GET /config/providers` serves one. */
const provider = (name: string, tier: 'private' | 'public') => ({
  name,
  is_configured: true,
  provider_type: 'Builtin',
  metadata: {
    config_keys: [],
    default_model: '',
    description: '',
    display_name: name,
    known_models: [],
    model_doc_link: '',
    name,
    tier,
    runs_locally: tier === 'private',
  },
});

/** vitest runs with `ui/desktop` as its root. */
const readSource = (...p: string[]) => readFileSync(path.join(process.cwd(), ...p), 'utf8');

const configured = (providerName: string | null) => {
  mocks.read.mockImplementation(async (key: string) => {
    if (key === 'BIOROUTER_MODEL') return providerName ? 'a-model' : null;
    if (key === 'BIOROUTER_PROVIDER') return providerName;
    return null;
  });
};

afterEach(cleanup);

beforeEach(() => {
  vi.clearAllMocks();
  __resetDisclosureStoreForTests();
  // `getFallbackModelAndProvider` reads the bundled defaults off this when the
  // config holds nothing; empty here, so "nothing configured" stays nothing.
  Object.defineProperty(window, 'appConfig', {
    writable: true,
    value: { get: () => undefined },
  });
  mocks.getPrivacyDisclosure.mockResolvedValue({
    data: {
      title_template: SERVED.titleTemplate,
      long: SERVED.long,
      short: SERVED.short,
      acknowledged: false,
    },
  });
  mocks.ackPrivacyDisclosure.mockResolvedValue({ data: undefined });
  mocks.setConfigProvider.mockResolvedValue({ data: undefined });
  mocks.getProviders.mockResolvedValue([
    provider('openai', 'public'),
    provider('llamacpp', 'private'),
  ]);
});

const renderGate = () =>
  render(
    <ModelAndProviderProvider>
      <AppNonPrivateModelDisclosureGate />
    </ModelAndProviderProvider>
  );

describe('the install-wide disclosure gate — before the first turn means before the first chat', () => {
  it('discloses on a fresh profile whose configured provider is public, with no session anywhere', async () => {
    configured('openai');

    renderGate();

    expect(
      await screen.findByRole('dialog', { name: /not hosted by your institution/i })
    ).toBeVisible();
    // The point of the whole suite: nothing session-shaped was consulted to get
    // here. No chat is mounted, no `/agent/resume` has run, and there is no
    // `session.provider_name` to read — which is the state of an install on
    // which nothing has been sent yet.
    expect(mocks.updateAgentProvider).not.toHaveBeenCalled();
  });

  it('never discloses when the configured provider runs on this computer', async () => {
    configured('llamacpp');

    renderGate();

    await waitFor(() => expect(mocks.getProviders).toHaveBeenCalled());
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('says nothing while the configured provider is still being read, rather than guessing', async () => {
    // A null provider means two different things — "not read yet" and "nothing
    // is configured" — and a gate that treated the first as the second would
    // flash a modal at every launch before the config had answered.
    let release: () => void = () => {};
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    mocks.read.mockImplementation(async (key: string) => {
      await gate;
      if (key === 'BIOROUTER_MODEL') return 'a-model';
      if (key === 'BIOROUTER_PROVIDER') return 'openai';
      return null;
    });

    renderGate();

    expect(screen.queryByRole('dialog')).toBeNull();
    expect(mocks.getPrivacyDisclosure).not.toHaveBeenCalled();

    await act(async () => {
      release();
    });

    expect(await screen.findByRole('dialog')).toBeVisible();
  });

  it('says nothing on an install with no provider configured at all', async () => {
    configured(null);

    renderGate();

    await waitFor(() => expect(mocks.read).toHaveBeenCalled());
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  /**
   * The structural half. `App` cannot be mounted here — its own suite replaces
   * a dozen of its providers with stubs to get it on screen at all — but the
   * one thing it owes this task is checkable: the gate has to be mounted at the
   * app level, inside `ModelAndProviderProvider` so the hook above resolves,
   * and it must take no chat-scoped argument. A gate that exists and is
   * rendered nowhere is precisely the defect this fixup pass is for, one level
   * up.
   */
  it('App mounts it, with no chat-scoped argument', () => {
    const source = readSource('src', 'App.tsx');
    expect(source).toMatch(/<AppNonPrivateModelDisclosureGate\s*\/>/);
    const shell = /<ModelAndProviderProvider>[\s\S]*?<\/ModelAndProviderProvider>/.exec(source);
    expect(shell, 'App no longer renders ModelAndProviderProvider').not.toBeNull();
    expect(shell![0]).toMatch(/<AppNonPrivateModelDisclosureGate\s*\/>/);
  });
});
