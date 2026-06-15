import type { ExtensionConfig } from '../../../api/types.gen';
import { toastService } from '../../../toasts';

interface DeleteExtensionProps {
  name: string;
  removeFromConfig: (name: string) => Promise<void>;
  extensionConfig?: ExtensionConfig;
}

/**
 * Deletes an extension from config (will no longer be loaded in new sessions)
 */
export async function deleteExtension({ name, removeFromConfig }: DeleteExtensionProps) {
  try {
    await removeFromConfig(name);
  } catch (error) {
    console.error('Failed to remove extension from config:', error);
    throw error;
  }
}

interface ToggleExtensionDefaultProps {
  toggle: 'toggleOn' | 'toggleOff';
  extensionConfig: ExtensionConfig;
  addToConfig: (name: string, extensionConfig: ExtensionConfig, enabled: boolean) => Promise<void>;
}

export async function toggleExtensionDefault({
  toggle,
  extensionConfig,
  addToConfig,
}: ToggleExtensionDefaultProps) {
  const enabled = toggle === 'toggleOn';

  try {
    await addToConfig(extensionConfig.name, extensionConfig, enabled);
    toastService.success({
      title: extensionConfig.name,
      msg: enabled ? 'Extension enabled in defaults' : 'Extension removed from defaults',
    });
  } catch (error) {
    console.error('Failed to update extension default in config:', error);
    toastService.error({
      title: extensionConfig.name,
      msg: 'Failed to update extension default',
    });
    throw error;
  }
}

interface ActivateExtensionDefaultProps {
  addToConfig: (name: string, extensionConfig: ExtensionConfig, enabled: boolean) => Promise<void>;
  extensionConfig: ExtensionConfig;
}

export async function activateExtensionDefault({
  addToConfig,
  extensionConfig,
}: ActivateExtensionDefaultProps): Promise<void> {
  try {
    await addToConfig(extensionConfig.name, extensionConfig, true);
    toastService.success({
      title: extensionConfig.name,
      msg: 'Extension added as default',
    });
  } catch (error) {
    console.error('Failed to add extension to config:', error);
    toastService.error({
      title: extensionConfig.name,
      msg: 'Failed to add extension',
    });
    throw error;
  }
}
