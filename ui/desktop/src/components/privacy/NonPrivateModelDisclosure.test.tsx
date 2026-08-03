import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

/**
 * Task 30A (issue #56, DR-17 requirement 3) — the non-private-model disclosure.
 *
 * ⚠ **The copy in this file is a FIXTURE, never the product's words.** The one
 * definition lives in `crates/biorouter/src/privacy/disclosure.rs` and arrives
 * over `GET /privacy/disclosure`; a component that carried its own English is
 * the drift this whole task exists to prevent, and it is invisible until the two
 * disagree. The fixture is deliberately *not* the real sentence, so Step 5's
 * gate (1) — which counts definitions of the real sentence across `crates/` and
 * `ui/desktop/src/` and expects exactly one — still sees one.
 */
const SERVED = {
  titleTemplate: '{provider} is not hosted by your institution.',
  long: 'SERVED-COPY-MARKER — it can read files on this computer.',
  short: 'SERVED-SHORT-MARKER',
};

const mocks = vi.hoisted(() => ({
  getPrivacyDisclosure: vi.fn(),
  ackPrivacyDisclosure: vi.fn(),
  getProviders: vi.fn(),
  sendTurn: vi.fn(),
}));

vi.mock('../../api', () => ({
  getPrivacyDisclosure: mocks.getPrivacyDisclosure,
  ackPrivacyDisclosure: mocks.ackPrivacyDisclosure,
}));

vi.mock('../ConfigContext', () => ({
  useConfig: () => ({ getProviders: mocks.getProviders }),
}));

vi.mock('../../utils/userAction', () => ({
  userActionHeaders: async () => ({ 'X-User-Action': 'test-key' }),
}));

import { NonPrivateModelDisclosure } from './NonPrivateModelDisclosure';
import { NonPrivateModelDisclosureGate } from './NonPrivateModelDisclosureGate';

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

afterEach(cleanup);

beforeEach(() => {
  vi.clearAllMocks();
  mocks.getPrivacyDisclosure.mockResolvedValue({
    data: {
      title_template: SERVED.titleTemplate,
      long: SERVED.long,
      short: SERVED.short,
      acknowledged: false,
    },
  });
  mocks.ackPrivacyDisclosure.mockResolvedValue({ data: undefined });
  mocks.getProviders.mockResolvedValue([
    provider('openai', 'public'),
    provider('llamacpp', 'private'),
  ]);
});

describe('NonPrivateModelDisclosure — the blocking dialog', () => {
  it('the renderer does not carry its own copy of the sentence', () => {
    // The dialog renders server-supplied text. A hardcoded English string here
    // is the drift this task exists to prevent, and it is invisible until the
    // two disagree.
    render(
      <NonPrivateModelDisclosure
        open
        providerDisplayName="OpenAI"
        copy={SERVED}
        onAcknowledge={vi.fn()}
      />
    );
    expect(screen.getByText(/SERVED-COPY-MARKER/)).toBeInTheDocument();
  });

  it('the dialog cannot be dismissed by Escape or an overlay click', async () => {
    const user = userEvent.setup();
    const onAck = vi.fn();
    render(
      <NonPrivateModelDisclosure
        open
        providerDisplayName="OpenAI"
        copy={SERVED}
        onAcknowledge={onAck}
      />
    );

    await user.keyboard('{Escape}');
    expect(screen.getByRole('dialog')).toBeVisible();

    const overlay = document.querySelector<HTMLElement>('[data-slot="dialog-overlay"]');
    expect(overlay).not.toBeNull();
    fireEvent.pointerDown(overlay!);
    expect(screen.getByRole('dialog')).toBeVisible();
    expect(onAck).not.toHaveBeenCalled();

    // …and there is no close button to click either.
    expect(screen.queryByRole('button', { name: 'Close' })).not.toBeInTheDocument();
  });

  it('no key acknowledges it — Task 29’s discipline, on a dialog with one button', async () => {
    const user = userEvent.setup();
    const onAck = vi.fn();
    render(
      <NonPrivateModelDisclosure
        open
        providerDisplayName="OpenAI"
        copy={SERVED}
        onAcknowledge={onAck}
      />
    );
    await user.keyboard('{Enter}');
    expect(onAck).not.toHaveBeenCalled();
    // The one control is reachable, and clicking it is the only way through.
    await user.click(screen.getByRole('button', { name: /I understand/i }));
    expect(onAck).toHaveBeenCalledTimes(1);
  });

  it('names the provider in the heading, from the served template', () => {
    render(
      <NonPrivateModelDisclosure
        open
        providerDisplayName="OpenAI"
        copy={SERVED}
        onAcknowledge={vi.fn()}
      />
    );
    expect(
      screen.getByRole('dialog', { name: /not hosted by your institution/i })
    ).toBeVisible();
    expect(screen.getByRole('dialog')).toHaveTextContent(/OpenAI/);
  });
});

describe('NonPrivateModelDisclosureGate — when it is shown', () => {
  it('a public model gets the dialog before the first turn', async () => {
    render(
      <>
        <NonPrivateModelDisclosureGate providerName="openai" />
        <button type="button" onClick={mocks.sendTurn}>
          Send
        </button>
      </>
    );

    expect(
      await screen.findByRole('dialog', { name: /not hosted by your institution/i })
    ).toBeVisible();
    // BEFORE, not after: an acknowledgement collected once the transcript
    // already went out is a receipt, not a disclosure. The dialog is modal, so
    // the composer behind it is inert — Radix marks the rest of the page
    // `aria-hidden` and traps focus — and nothing has been sent.
    expect(mocks.sendTurn).not.toHaveBeenCalled();
    // …and it is genuinely modal, not merely on top: the Send control is still
    // in the DOM but has left the accessibility tree entirely, which is what
    // "the composer cannot be reached" means to a screen reader and to
    // Testing Library alike.
    expect(document.body.textContent).toContain('Send');
    expect(screen.queryByRole('button', { name: 'Send' })).toBeNull();
  });

  it('a local model never does', async () => {
    render(<NonPrivateModelDisclosureGate providerName="llamacpp" />);
    await waitFor(() => expect(mocks.getProviders).toHaveBeenCalled());
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('an already-acknowledged install is never asked again', async () => {
    mocks.getPrivacyDisclosure.mockResolvedValue({
      data: {
        title_template: SERVED.titleTemplate,
        long: SERVED.long,
        short: SERVED.short,
        acknowledged: true,
      },
    });
    render(<NonPrivateModelDisclosureGate providerName="openai" />);
    await waitFor(() => expect(mocks.getProviders).toHaveBeenCalled());
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('acknowledging sends the proof-of-user and closes the dialog', async () => {
    const user = userEvent.setup();
    render(<NonPrivateModelDisclosureGate providerName="openai" />);
    await screen.findByRole('dialog');

    await user.click(screen.getByRole('button', { name: /I understand/i }));

    await waitFor(() =>
      expect(mocks.ackPrivacyDisclosure).toHaveBeenCalledWith(
        expect.objectContaining({ headers: { 'X-User-Action': 'test-key' } })
      )
    );
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
  });

  it('a provider it cannot classify is disclosed, not waved through', async () => {
    // Fail-safe means fail TOWARDS disclosing. A metadata entry that resolved
    // without a tier is Public by the daemon's own polarity, and a name with no
    // entry at all is a provider Biorouter cannot vouch for.
    mocks.getProviders.mockResolvedValue([
      { ...provider('mystery', 'public'), metadata: { ...provider('mystery', 'public').metadata, tier: undefined } },
    ]);
    render(<NonPrivateModelDisclosureGate providerName="mystery" />);
    expect(await screen.findByRole('dialog')).toBeVisible();
  });

  it('says nothing at all until a provider is bound', async () => {
    render(<NonPrivateModelDisclosureGate providerName={null} />);
    await waitFor(() => expect(mocks.getPrivacyDisclosure).not.toHaveBeenCalled());
    expect(screen.queryByRole('dialog')).toBeNull();
  });
});
