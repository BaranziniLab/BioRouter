import { describe, expect, it, vi } from 'vitest';
import type { ExtensionConfig } from '../../../api/types.gen';
import { syncBundledExtensions } from './bundled-extensions';

describe('syncBundledExtensions', () => {
  it('installs bundled capabilities with the requested fresh-install defaults', async () => {
    const addExtension = vi.fn(
      async (_name: string, _config: ExtensionConfig, _enabled: boolean) => undefined
    );

    await syncBundledExtensions([], addExtension);

    const enabledByName = Object.fromEntries(
      addExtension.mock.calls.map(([name, _config, enabled]) => [name, enabled])
    );
    expect(enabledByName).toEqual({
      developer: true,
      computercontroller: true,
      autovisualiser: true,
      memory: true,
      tutorial: false,
      knowledge: true,
      agent_drafter: true,
    });
  });
});
