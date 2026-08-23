import { render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import ProviderGuard from './ProviderGuard';
import { BROWSER_SURFACE_MARKER } from '../utils/surface';

/**
 * SD-1's dead end: a fresh browser user lands in onboarding, which is exactly
 * where provider selection happens, and every card there writes
 * `BIOROUTER_PROVIDER` — a write the browser-served daemon refuses with a 409
 * whose body is addressed to an AI agent and tells the reader to open the
 * desktop application they do not have.
 */

const mocks = vi.hoisted(() => ({
  read: vi.fn(),
  upsert: vi.fn(),
  navigate: vi.fn(),
}));

vi.mock('./ConfigContext', () => ({
  useConfig: () => ({ read: mocks.read, upsert: mocks.upsert }),
}));

vi.mock('react-router-dom', () => ({
  useNavigate: () => mocks.navigate,
}));

// Each card gets a distinguishable marker, because the assertion that matters
// most here is an ABSENCE — a stub rendering `null` (as the sibling suite's do)
// could not tell "not rendered" from "rendered and empty".
vi.mock('./onboarding/LlamaServerInlineCard', () => ({
  default: () => <div>CARD:llama</div>,
}));
vi.mock('./onboarding/OllamaInlineCard', () => ({ default: () => <div>CARD:ollama</div> }));
vi.mock('./onboarding/InstitutionalSetupCard', () => ({
  default: () => <div>CARD:institutional</div>,
}));
vi.mock('./onboarding/CodingAgentInlineCard', () => ({
  default: () => <div>CARD:coding-agent</div>,
}));
vi.mock('./onboarding/CommercialSetupCard', () => ({
  default: () => <div>CARD:commercial</div>,
}));

vi.mock('./settings/models/subcomponents/SwitchModelModal', () => ({
  SwitchModelModal: () => <div>SWITCH-MODEL-MODAL</div>,
}));

const CARD_MARKERS = [
  'CARD:llama',
  'CARD:ollama',
  'CARD:institutional',
  'CARD:coding-agent',
  'CARD:commercial',
];

function renderGuard() {
  return render(
    <ProviderGuard didSelectProvider={false}>
      <div>Application</div>
    </ProviderGuard>
  );
}

describe('ProviderGuard on a browser-served surface', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.read.mockResolvedValue('');
    mocks.upsert.mockResolvedValue(undefined);
  });

  afterEach(() => {
    delete document.documentElement.dataset.biorouterSurface;
  });

  /**
   * ⚠ **Fails against today's code**, which renders all five cards on every
   * surface: `queryByText('CARD:llama')` finds one, and there is no panel for
   * `findByTestId` to resolve. The user reaches a picker whose every path ends
   * in a refusal.
   */
  it('replaces the provider cards with what to run on the host', async () => {
    document.documentElement.dataset.biorouterSurface = BROWSER_SURFACE_MARKER;
    renderGuard();

    const panel = await screen.findByTestId('host-managed-model-panel');
    expect(panel).toHaveTextContent('biorouter configure');
    expect(panel).toHaveTextContent('biorouter serve');
    // Named, so a reader knows the choice is on the host rather than missing.
    expect(screen.getByText('Choose a model on the host')).toBeInTheDocument();

    for (const marker of CARD_MARKERS) {
      expect(screen.queryByText(marker)).toBeNull();
    }
  });

  /**
   * The reason, not just the instruction. SD-1 is a privacy boundary, and a
   * screen that said only "run this on the host" would read as a limitation
   * rather than as the thing keeping a private conversation off a public model.
   *
   * ⚠ Fails against today's code for the same reason as above — there is no
   * panel to carry the sentence.
   */
  it('says why a browser tab cannot choose, not only that it cannot', async () => {
    document.documentElement.dataset.biorouterSurface = BROWSER_SURFACE_MARKER;
    renderGuard();

    const panel = await screen.findByTestId('host-managed-model-panel');
    expect(panel.textContent).toMatch(/private conversation/i);
    expect(panel.textContent).toMatch(/public model/i);
  });

  /**
   * ⚠ **The control, and the one test here that passes both before and after.**
   * Its job is to catch over-reach: a helper that answered `browser` whenever
   * `window.electron` looked unusual — or one that read a module-level snapshot
   * — would strip the desktop application's own onboarding down to a panel
   * telling the user to go and run a command in a terminal.
   */
  it('leaves the desktop onboarding exactly as it was', async () => {
    renderGuard();

    for (const marker of CARD_MARKERS) {
      expect(await screen.findByText(marker)).toBeInTheDocument();
    }
    expect(screen.queryByTestId('host-managed-model-panel')).toBeNull();
  });

  /**
   * A browser session whose host *has* a provider is a working session, and
   * must not be diverted. The panel is for the unconfigured case only.
   *
   * ⚠ Fails against a plausible wrong implementation that renders the panel
   * whenever the surface is a browser, rather than only inside the
   * no-provider branch.
   */
  it('does not divert a browser session whose host is already configured', async () => {
    document.documentElement.dataset.biorouterSurface = BROWSER_SURFACE_MARKER;
    mocks.read.mockResolvedValue('versa_azure');
    renderGuard();

    await waitFor(() => expect(screen.getByText('Application')).toBeInTheDocument());
    expect(screen.queryByTestId('host-managed-model-panel')).toBeNull();
  });
});
