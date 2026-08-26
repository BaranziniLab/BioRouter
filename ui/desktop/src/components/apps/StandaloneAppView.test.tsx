import { render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import StandaloneAppView from './StandaloneAppView';

const mocks = vi.hoisted(() => ({
  listApps: vi.fn(),
  resumeAgent: vi.fn(),
  startAgent: vi.fn(),
  stopAgent: vi.fn(),
  userActionHeaders: vi.fn(),
}));

vi.mock('../../api', () => ({
  listApps: mocks.listApps,
  resumeAgent: mocks.resumeAgent,
  startAgent: mocks.startAgent,
  stopAgent: mocks.stopAgent,
}));
vi.mock('../../utils/userAction', () => ({ userActionHeaders: mocks.userActionHeaders }));
vi.mock('../McpApps/McpAppRenderer', () => ({
  default: ({ sessionId }: { sessionId: string | null }) => (
    <div data-testid="standalone-app">{sessionId}</div>
  ),
}));

describe('StandaloneAppView session initialization', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.listApps.mockResolvedValue({ data: { apps: [] } });
    mocks.startAgent.mockResolvedValue({ data: { id: 'app-session' } });
    mocks.resumeAgent.mockResolvedValue({ data: {} });
    mocks.stopAgent.mockResolvedValue({ data: {} });
    mocks.userActionHeaders.mockResolvedValue({ 'X-User-Action': 'proof-of-user' });
  });

  it('proves both start and resume requests came from the renderer user', async () => {
    render(
      <MemoryRouter
        initialEntries={[
          '/standalone-app?resourceUri=ui%3A%2F%2Fapp&extensionName=demo&workingDir=%2Ftmp%2Fworkspace',
        ]}
      >
        <StandaloneAppView />
      </MemoryRouter>
    );

    expect(await screen.findByTestId('standalone-app')).toHaveTextContent('app-session');
    expect(mocks.startAgent).toHaveBeenCalledWith({
      body: { working_dir: '/tmp/workspace' },
      headers: { 'X-User-Action': 'proof-of-user' },
      throwOnError: true,
    });
    await waitFor(() => {
      expect(mocks.resumeAgent).toHaveBeenCalledWith({
        body: { session_id: 'app-session', load_model_and_extensions: true },
        headers: { 'X-User-Action': 'proof-of-user' },
        throwOnError: true,
      });
    });
    expect(mocks.userActionHeaders).toHaveBeenCalledTimes(2);
  });
});
