/**
 * ONE display surface for a generated artifact, on EVERY transcript surface.
 *
 * A `ui://` figure or app card is shown as a click-to-open card in the
 * transcript and rendered only in the artifact side panel — in the live chat,
 * in a saved session replay, and in a shared read-only session alike. The two
 * read-only surfaces used to differ: they passed no `onOpenArtifact`, so
 * `MCPUIResourceRenderer` fell back to an inline `@mcp-ui/client` frame served
 * through `/mcp-ui-proxy`. The same figure therefore looked like two different
 * things depending on where you opened it, with a second CSP, a second action
 * channel and a second resize behaviour that nothing kept in step with the
 * panel's.
 *
 * These specs are the regression guard for that divergence: on each read-only
 * surface, a figure must be a card, must never be an inline frame, and clicking
 * the card must mount the real panel on that surface.
 */
import React from 'react';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import SessionHistoryView from './SessionHistoryView';
import SharedSessionView from './SharedSessionView';
import { ThemeProvider } from '../../contexts/ThemeContext';
import { ARTIFACT_PANEL_ATTR } from '../../utils/tabCycle';
import type { Message, Session } from '../../api';
import type { SharedSessionDetails } from '../../sharedSessions';

// The mock exists to give the *absence* of an inline frame something to be an
// assertion about. A bare `UIResourceRenderer` emits nothing identifiable, so
// "no iframe rendered" would pass whether or not the inline path came back.
//
// `isUIResource` is re-exported from the REAL module via `importOriginal`
// rather than hand-rolled: ToolCallWithResponse routes a tool result to
// MCPUIResourceRenderer only when that predicate says yes, so a hand-rolled
// stand-in that happened to be more permissive than the shipped one would make
// this whole file assert against a card the app never actually draws.
vi.mock('@mcp-ui/client', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@mcp-ui/client')>();
  return {
    ...actual,
    UIResourceRenderer: vi.fn(() =>
      React.createElement('iframe', {
        'data-testid': 'mcp-ui-frame',
        title: 'mock mcp ui frame',
      })
    ),
  };
});

const listSpy = vi.hoisted(() => vi.fn<(chat: { sessionId: string }) => void>());

// A transparent spy, NOT a replacement. SessionHistoryView used to hand the
// transcript the literal string 'session-preview' — nobody's session — so every
// consumer that scopes work by id addressed a chat that does not exist. Reading
// the prop is the direct way to pin the real id; the real component still
// renders underneath, because "the panel mounts through the real chain" is the
// other half of what this file tests.
vi.mock('../ProgressiveMessageList', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../ProgressiveMessageList')>();
  const Real = actual.default;
  return {
    ...actual,
    default: (props: React.ComponentProps<typeof Real>) => {
      listSpy(props.chat as { sessionId: string });
      return React.createElement(Real, props);
    },
  };
});

vi.mock('../../utils/userAction', () => ({
  userActionHeaders: async () => ({ 'X-User-Action': 'test-key' }),
}));

const FIGURE_HTML = '<!doctype html><html><body><h1>Volcano</h1></body></html>';

// Copied from MCPUIResourceRenderer.test.tsx rather than invented: this exact
// shape is what `isUIResource` recognises and what `artifactSourceFromResource`
// turns into a `kind: 'html'` source titled "Chart Visualization".
const FIGURE_RESOURCE = {
  uri: 'ui://chart/visualization',
  mimeType: 'text/html',
  blob: btoa(FIGURE_HTML),
};

const CARD_LABEL = 'Open Chart Visualization in the artifact viewer';

const assistantWithFigureCall = {
  id: 'assistant-1',
  role: 'assistant',
  created: 1_700_000_000,
  metadata: { userVisible: true, agentVisible: true },
  content: [
    {
      type: 'toolRequest',
      id: 'tool-1',
      toolCall: {
        status: 'success',
        value: { name: 'autovisualiser__show_chart', arguments: {} },
      },
    },
  ],
} as unknown as Message;

const figureToolResponse = {
  id: 'user-1',
  role: 'user',
  created: 1_700_000_001,
  metadata: { userVisible: true, agentVisible: true },
  content: [
    {
      type: 'toolResponse',
      id: 'tool-1',
      toolResult: {
        status: 'success',
        value: { content: [{ type: 'resource', resource: FIGURE_RESOURCE }] },
      },
    },
  ],
} as unknown as Message;

const transcript: Message[] = [assistantWithFigureCall, figureToolResponse];
function linkedFileTranscript(fromSessionId?: string): Message[] {
  return [
    {
      id: 'file-prior',
      role: 'assistant',
      created: 1_700_000_002,
      metadata: {
        userVisible: true,
        agentVisible: true,
        ...(fromSessionId
          ? { provenance: { kind: 'agent_injection' as const, fromSessionId } }
          : {}),
      },
      content: [{ type: 'text', text: 'Created `/tmp/project/results/report.md`.' }],
    },
    {
      id: 'file-current',
      role: 'assistant',
      created: 1_700_000_003,
      metadata: { userVisible: true, agentVisible: true },
      content: [{ type: 'text', text: '[Open report](report.md)' }],
    },
  ];
}

/**
 * The panel reads a file and prepares HTML through the main process. Stubbed
 * from ArtifactViewer.test.tsx, because the real `ArtifactViewer` is mounted
 * here on purpose — "the panel actually mounts on this surface" is the thing
 * under test, so mocking it away would delete the assertion.
 */
function installElectronMock() {
  Object.defineProperty(window, 'electron', {
    configurable: true,
    value: {
      prepareArtifactHtml: vi.fn(async ({ html }: { html: string }) => ({ html })),
      readArtifactFile: vi.fn(async () => ({ kind: 'error', found: false, error: 'not used' })),
      openArtifactInBrowser: vi.fn(),
      openDirectoryInExplorer: vi.fn(),
      openExternal: vi.fn(),
      getSecretKey: vi.fn().mockResolvedValue('secret'),
      getBiorouterdHostPort: vi.fn().mockResolvedValue('http://localhost:8765'),
      broadcastThemeChange: vi.fn(),
      on: vi.fn().mockReturnValue(() => undefined),
    },
  });
}

function artifactPanel(): Element | null {
  return document.querySelector(`[${ARTIFACT_PANEL_ATTR}]`);
}

function sharedSession(
  messages: Message[] = transcript,
  shareToken = 'token-1'
): SharedSessionDetails {
  return {
    share_token: shareToken,
    created_at: 1_700_000_000,
    base_url: 'https://share.test',
    description: 'Cohort volcano plot',
    working_dir: '/tmp/project',
    messages,
    message_count: messages.length,
    total_tokens: null,
  };
}

function savedSession(over: Partial<Session> = {}): Session {
  return {
    id: '20260814_120000',
    name: 'Cohort volcano plot',
    created_at: '2026-08-14T12:00:00Z',
    updated_at: '2026-08-14T12:00:00Z',
    working_dir: '/tmp/project',
    message_count: transcript.length,
    extension_data: {},
    conversation: transcript,
    ...over,
  } as Session;
}

beforeEach(() => {
  vi.clearAllMocks();
  installElectronMock();
});

describe('a shared transcript', () => {
  function sharedView(messages: Message[] = transcript, shareToken = 'token-1') {
    return (
      <ThemeProvider>
        <MemoryRouter>
          <SharedSessionView
            session={sharedSession(messages, shareToken)}
            isLoading={false}
            error={null}
            onRetry={vi.fn()}
          />
        </MemoryRouter>
      </ThemeProvider>
    );
  }

  function renderShared(messages: Message[] = transcript, shareToken = 'token-1') {
    return render(sharedView(messages, shareToken));
  }

  it('shows a figure as a card and never as an inline frame', () => {
    renderShared();

    // The shared transcript is the one that leaves the machine, so it is the
    // highest-stakes place for a second renderer of the same document.
    expect(screen.queryByTestId('mcp-ui-frame')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: CARD_LABEL })).toBeInTheDocument();
  });

  it('opens the artifact panel on this read-only surface when the card is clicked', async () => {
    renderShared();

    // Nothing auto-opens on a read-only surface: the panel appears because the
    // reader asked for it, never because the page loaded.
    expect(artifactPanel()).toBeNull();

    fireEvent.click(screen.getByRole('button', { name: CARD_LABEL }));

    await waitFor(() => expect(artifactPanel()).not.toBeNull());
    // The panel renders the document in its own sandboxed srcdoc frame, so the
    // inline path stays absent even once the figure is on screen.
    expect(screen.queryByTestId('mcp-ui-frame')).not.toBeInTheDocument();
  });

  it('resolves a basename link from earlier same-session file provenance', async () => {
    renderShared(linkedFileTranscript());
    fireEvent.click(await screen.findByRole('button', { name: 'Open report' }));
    await waitFor(() =>
      expect(window.electron.readArtifactFile).toHaveBeenCalledWith(
        '/tmp/project/results/report.md'
      )
    );
  });

  it('does not reuse foreign-origin file provenance', async () => {
    renderShared(linkedFileTranscript('foreign-session'));
    fireEvent.click(await screen.findByRole('button', { name: 'Open report' }));
    await waitFor(() =>
      expect(window.electron.readArtifactFile).toHaveBeenCalledWith('/tmp/project/report.md')
    );
  });

  it('isolates provenance when the same transcript is rerendered under another share token', async () => {
    const messages = linkedFileTranscript('shared:token-1');
    const { rerender } = renderShared(messages, 'token-1');
    fireEvent.click(await screen.findByRole('button', { name: 'Open report' }));
    await waitFor(() =>
      expect(window.electron.readArtifactFile).toHaveBeenCalledWith(
        '/tmp/project/results/report.md'
      )
    );

    fireEvent.click(screen.getByRole('button', { name: 'Close preview panel' }));
    await waitFor(() => expect(artifactPanel()).toBeNull());
    vi.mocked(window.electron.readArtifactFile).mockClear();
    rerender(sharedView(messages, 'token-2'));
    fireEvent.click(await screen.findByRole('button', { name: 'Open report' }));
    await waitFor(() =>
      expect(window.electron.readArtifactFile).toHaveBeenCalledWith('/tmp/project/report.md')
    );
  });
});

describe('a saved transcript', () => {
  function renderSaved(over: Partial<Session> = {}) {
    return render(
      <ThemeProvider>
        <MemoryRouter>
          <SessionHistoryView
            session={savedSession(over)}
            isLoading={false}
            error={null}
            onBack={vi.fn()}
            onRetry={vi.fn()}
            showActionButtons={false}
          />
        </MemoryRouter>
      </ThemeProvider>
    );
  }

  it('shows a figure as a card and never as an inline frame', () => {
    renderSaved();

    expect(screen.queryByTestId('mcp-ui-frame')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: CARD_LABEL })).toBeInTheDocument();
  });

  it('opens the artifact panel on this read-only surface when the card is clicked', async () => {
    renderSaved();

    expect(artifactPanel()).toBeNull();

    fireEvent.click(screen.getByRole('button', { name: CARD_LABEL }));

    await waitFor(() => expect(artifactPanel()).not.toBeNull());
    expect(screen.queryByTestId('mcp-ui-frame')).not.toBeInTheDocument();
  });

  it('gives the transcript the real session id instead of a fabricated one', () => {
    const { container } = renderSaved();

    // The strong form: read the id the transcript was actually handed. An
    // id-shaped string that never reaches the DOM would be invisible to any
    // innerHTML check.
    expect(listSpy).toHaveBeenCalledWith(expect.objectContaining({ sessionId: '20260814_120000' }));
    expect(listSpy).not.toHaveBeenCalledWith(
      expect.objectContaining({ sessionId: 'session-preview' })
    );
    expect(container.innerHTML).not.toContain('session-preview');
  });

  it('resolves a basename link from earlier same-session file provenance', async () => {
    const messages = linkedFileTranscript('20260814_120000');
    renderSaved({ conversation: messages, message_count: messages.length });
    fireEvent.click(await screen.findByRole('button', { name: 'Open report' }));
    await waitFor(() =>
      expect(window.electron.readArtifactFile).toHaveBeenCalledWith(
        '/tmp/project/results/report.md'
      )
    );
  });
});
