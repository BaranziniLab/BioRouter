import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createSession } from './sessions';

const mocks = vi.hoisted(() => ({
  startAgent: vi.fn(),
  userActionHeaders: vi.fn(),
}));

vi.mock('./api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./api')>()),
  startAgent: mocks.startAgent,
}));
vi.mock('./utils/userAction', () => ({ userActionHeaders: mocks.userActionHeaders }));

describe('createSession user-action proof', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.startAgent.mockResolvedValue({ data: { id: 'new-session' } });
  });

  it('attaches renderer proof when starting a new chat', async () => {
    mocks.userActionHeaders.mockResolvedValue({ 'X-User-Action': 'proof-of-user' });

    await createSession('/tmp/workspace');

    expect(mocks.startAgent).toHaveBeenCalledWith({
      body: { working_dir: '/tmp/workspace' },
      headers: { 'X-User-Action': 'proof-of-user' },
      throwOnError: true,
    });
  });

  it('does not invent proof when the renderer bridge is unavailable', async () => {
    mocks.userActionHeaders.mockResolvedValue({});
    const refusal = { message: 'configured private provider requires user proof' };
    mocks.startAgent.mockRejectedValueOnce(refusal);

    await expect(createSession('/tmp/workspace')).rejects.toBe(refusal);

    expect(mocks.startAgent).toHaveBeenCalledWith(expect.objectContaining({ headers: {} }));
  });
});
