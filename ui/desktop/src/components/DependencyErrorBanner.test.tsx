import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { DependencyErrorBanner } from './DependencyErrorBanner';

const createChatWindow = vi.fn();
const dependencyEnvironment = vi.fn();

beforeEach(() => {
  createChatWindow.mockReset();
  dependencyEnvironment.mockReset().mockResolvedValue({
    platform: 'darwin',
    arch: 'arm64',
    appVersion: '1.89.1',
    augmentedPath: '/opt/homebrew/bin:/usr/bin',
    inheritedPath: '/usr/bin',
  });
  // @ts-expect-error — partial stub, only what the banner touches.
  window.electron = { createChatWindow, dependencyEnvironment };
});

const failure = {
  kind: 'extension' as const,
  name: 'spokeagent',
  displayName: 'SPOKE Agent',
  command: 'uv sync',
};

describe('DependencyErrorBanner', () => {
  it('shows the error the user was already going to see', () => {
    render(<DependencyErrorBanner error="uv sync failed: no such file" failure={failure} />);
    expect(screen.getByText('uv sync failed: no such file')).toBeInTheDocument();
  });

  it('opens a NEW window, not the current chat', async () => {
    render(<DependencyErrorBanner error="uv sync failed" failure={failure} />);
    await userEvent.click(screen.getByRole('button', { name: /debug with biorouter/i }));
    await waitFor(() => expect(createChatWindow).toHaveBeenCalledTimes(1));
  });

  it('briefs the session with the error, the command and the machine', async () => {
    render(<DependencyErrorBanner error="uv sync failed: no such file" failure={failure} />);
    await userEvent.click(screen.getByRole('button', { name: /debug with biorouter/i }));
    await waitFor(() => expect(createChatWindow).toHaveBeenCalled());

    const prompt = createChatWindow.mock.calls[0][0] as string;
    expect(prompt).toContain('SPOKE Agent');
    expect(prompt).toContain('uv sync failed: no such file');
    expect(prompt).toContain('uv sync');
    expect(prompt).toContain('darwin');
    expect(prompt).toContain('1.89.1');
    expect(prompt).toContain('Ask me first');
  });

  it('still opens a session when the environment probe fails', async () => {
    dependencyEnvironment.mockRejectedValue(new Error('nope'));
    render(<DependencyErrorBanner error="uv sync failed" failure={failure} />);
    await userEvent.click(screen.getByRole('button', { name: /debug with biorouter/i }));

    await waitFor(() => expect(createChatWindow).toHaveBeenCalled());
    // A briefing without the machine section beats no briefing at all.
    expect(createChatWindow.mock.calls[0][0]).toContain('uv sync failed');
  });

  it('can be rendered without the action where debugging cannot help', () => {
    render(<DependencyErrorBanner error="Refused" failure={failure} hideDebugAction />);
    expect(screen.queryByRole('button', { name: /debug with biorouter/i })).toBeNull();
    expect(screen.getByText('Refused')).toBeInTheDocument();
  });
});
