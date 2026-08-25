import { beforeEach, describe, expect, it, vi } from 'vitest';
import { agentRemoveExtension } from '../../../api';
import { userActionHeaders } from '../../../utils/userAction';
import { removeFromAgent } from './agent-api';

vi.mock('../../../api', () => ({
  agentAddExtension: vi.fn(),
  agentRemoveExtension: vi.fn(),
}));
vi.mock('../../../utils/userAction', () => ({
  userActionHeaders: vi.fn(),
}));
vi.mock('../../../toasts', () => ({
  toastService: {
    loading: vi.fn(),
    dismiss: vi.fn(),
    success: vi.fn(),
    error: vi.fn(),
  },
}));
vi.mock('../../../utils/crossAffiliationNotice', () => ({
  showCrossAffiliationNotice: vi.fn(),
}));
vi.mock('../../../utils/extensionErrorUtils', () => ({
  createExtensionRecoverHints: vi.fn(() => ''),
  formatExtensionErrorMessage: vi.fn((message: string) => message),
}));

describe('removeFromAgent', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('proves a user-requested extension removal for a child session', async () => {
    vi.mocked(userActionHeaders).mockResolvedValue({ 'X-User-Action': 'proof' });
    vi.mocked(agentRemoveExtension).mockResolvedValue({ data: {} } as never);

    await removeFromAgent('developer', 'child-session', false);

    expect(agentRemoveExtension).toHaveBeenCalledWith({
      headers: { 'X-User-Action': 'proof' },
      body: { session_id: 'child-session', name: 'developer' },
      throwOnError: true,
    });
  });

  it('does not invent proof when the user-action bridge is unavailable', async () => {
    vi.mocked(userActionHeaders).mockResolvedValue({});
    const refusal = { message: 'forbidden' };
    vi.mocked(agentRemoveExtension).mockRejectedValue(refusal);

    await expect(removeFromAgent('developer', 'child-session', false)).rejects.toBe(refusal);
    expect(agentRemoveExtension).toHaveBeenCalledWith(expect.objectContaining({ headers: {} }));
  });
});
