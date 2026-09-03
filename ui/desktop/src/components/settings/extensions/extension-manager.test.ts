import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { ExtensionConfig } from '../../../api/types.gen';

const mocks = vi.hoisted(() => ({
  success: vi.fn(),
  error: vi.fn(),
}));

vi.mock('../../../toasts', () => ({
  toastService: {
    success: mocks.success,
    error: mocks.error,
  },
}));

import { toggleExtensionDefault } from './extension-manager';

const capability: ExtensionConfig = {
  type: 'platform',
  name: 'skills',
  description: 'Reusable skills',
};

describe('toggleExtensionDefault terminology', () => {
  beforeEach(() => vi.clearAllMocks());

  it('describes capability defaults as applying to new chats', async () => {
    const addToConfig = vi.fn(async () => undefined);

    await toggleExtensionDefault({
      toggle: 'toggleOn',
      extensionConfig: capability,
      addToConfig,
      itemKind: 'capability',
    });

    expect(mocks.success).toHaveBeenCalledWith({
      title: 'skills',
      msg: 'Capability enabled for new chats',
    });
  });

  it('keeps extension terminology for extension defaults', async () => {
    const addToConfig = vi.fn(async () => undefined);

    await toggleExtensionDefault({
      toggle: 'toggleOff',
      extensionConfig: capability,
      addToConfig,
    });

    expect(mocks.success).toHaveBeenCalledWith({
      title: 'skills',
      msg: 'Extension disabled for new chats',
    });
  });
});
