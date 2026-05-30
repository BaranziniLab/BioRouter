import { useEffect, useState, useCallback, useMemo } from 'react';
import { Button } from '../../ui/button';
import { Plus } from '../../icons/app-icons';
import { GPSIcon } from '../../ui/icons';
import { useConfig, FixedExtensionEntry } from '../../ConfigContext';
import ExtensionList from './subcomponents/ExtensionList';
import ExtensionModal from './modal/ExtensionModal';
import {
  createExtensionConfig,
  ExtensionFormData,
  extensionToFormData,
  getDefaultFormData,
} from './utils';

import { activateExtensionDefault, deleteExtension, toggleExtensionDefault } from './index';
import { toastService } from '../../../toasts';
import type { ExtensionConfig } from '../../../api/types.gen';
import { BrxtInstallModal } from '../../BrxtInstallModal';

interface ExtensionSectionProps {
  deepLinkConfig?: ExtensionConfig;
  showEnvVars?: boolean;
  hideButtons?: boolean;
  disableConfiguration?: boolean;
  customToggle?: (extension: FixedExtensionEntry) => Promise<boolean | void>;
  selectedExtensions?: string[]; // Add controlled state
  onModalClose?: (extensionName: string) => void;
  searchTerm?: string;
}

export default function ExtensionsSection({
  deepLinkConfig,
  showEnvVars,
  hideButtons,
  disableConfiguration,
  customToggle,
  selectedExtensions = [],
  onModalClose,
  searchTerm = '',
}: ExtensionSectionProps) {
  const { getExtensions, addExtension, removeExtension, extensionsList } = useConfig();
  const [selectedExtension, setSelectedExtension] = useState<FixedExtensionEntry | null>(null);
  const [isModalOpen, setIsModalOpen] = useState(false);
  const [isAddModalOpen, setIsAddModalOpen] = useState(false);
  const [isBrxtModalOpen, setIsBrxtModalOpen] = useState(false);
  const [deepLinkConfigStateVar, setDeepLinkConfigStateVar] = useState<
    ExtensionConfig | undefined | null
  >(deepLinkConfig);
  const [showEnvVarsStateVar, setShowEnvVarsStateVar] = useState<boolean | undefined | null>(
    showEnvVars
  );

  useEffect(() => {
    setDeepLinkConfigStateVar(deepLinkConfig);
    setShowEnvVarsStateVar(showEnvVars);
  }, [deepLinkConfig, showEnvVars]);

  const extensions = useMemo(() => {
    if (extensionsList.length === 0) return [];

    return [...extensionsList]
      .sort((a, b) => {
        // First sort by builtin
        if (a.type === 'builtin' && b.type !== 'builtin') return -1;
        if (a.type !== 'builtin' && b.type === 'builtin') return 1;

        // Then sort by bundled (handle null/undefined cases)
        const aBundled = 'bundled' in a && a.bundled === true;
        const bBundled = 'bundled' in b && b.bundled === true;
        if (aBundled && !bBundled) return -1;
        if (!aBundled && bBundled) return 1;

        // Finally sort alphabetically within each group
        return a.name.localeCompare(b.name);
      })
      .map((ext) => ({
        ...ext,
        // Use selectedExtensions to determine enabled state in workflow editor
        enabled: disableConfiguration ? selectedExtensions.includes(ext.name) : ext.enabled,
      }));
  }, [extensionsList, disableConfiguration, selectedExtensions]);

  const fetchExtensions = useCallback(async () => {
    await getExtensions(true); // Force refresh - this will update the context
  }, [getExtensions]);

  const handleExtensionToggle = async (extensionConfig: FixedExtensionEntry) => {
    if (customToggle) {
      await customToggle(extensionConfig);
      return true;
    }

    const toggleDirection = extensionConfig.enabled ? 'toggleOff' : 'toggleOn';

    await toggleExtensionDefault({
      toggle: toggleDirection,
      extensionConfig: extensionConfig,
      addToConfig: addExtension,
    });

    await fetchExtensions();
    return true;
  };

  const handleConfigureClick = (extension: FixedExtensionEntry) => {
    setSelectedExtension(extension);
    setIsModalOpen(true);
  };

  const handleAddExtension = async (formData: ExtensionFormData) => {
    // Close the modal immediately
    handleModalClose();

    const extensionConfig = createExtensionConfig(formData);
    try {
      await activateExtensionDefault({
        addToConfig: addExtension,
        extensionConfig: extensionConfig,
      });
    } catch (error) {
      console.error('Failed to add extension:', error);
    } finally {
      await fetchExtensions();
      if (onModalClose) {
        setTimeout(() => {
          onModalClose(formData.name);
        }, 200);
      }
    }
  };

  const handleUpdateExtension = async (formData: ExtensionFormData) => {
    if (!selectedExtension) {
      console.error('No selected extension for update');
      return;
    }

    // Close the modal immediately
    handleModalClose();

    let extensionConfig: ExtensionConfig;
    if (selectedExtension.type === 'builtin') {
      // Built-in extensions only expose timeout for editing — preserve every other field.
      extensionConfig = {
        type: 'builtin',
        name: selectedExtension.name,
        description: selectedExtension.description,
        display_name: selectedExtension.display_name,
        bundled: selectedExtension.bundled,
        available_tools: selectedExtension.available_tools,
        timeout: formData.timeout,
      };
    } else {
      extensionConfig = createExtensionConfig(formData);
    }

    const originalName = selectedExtension.name;

    try {
      if (originalName !== extensionConfig.name) {
        await removeExtension(originalName);
      }
      await addExtension(extensionConfig.name, extensionConfig, formData.enabled);
    } catch (error) {
      console.error('Failed to update extension:', error);
    } finally {
      await fetchExtensions();
    }
  };

  const handleDeleteExtension = async (name: string) => {
    handleModalClose();

    // Detect .brxt-installed extensions by their --directory arg pointing to extensions/
    const config = extensionsList.find((e) => e.name === name);
    const isBrxtInstalled =
      config?.type === 'stdio' &&
      Array.isArray(config.args) &&
      config.args[0] === 'uv' &&
      config.args[1] === 'run' &&
      config.args[2] === '--directory' &&
      typeof config.args[3] === 'string' &&
      config.args[3].includes('biorouter/extensions/');

    try {
      if (isBrxtInstalled) {
        const uninstallResult = await window.electron.uninstallBrxtExtension(name);
        if ('error' in uninstallResult) {
          toastService.error({
            title: name,
            msg: `Failed to remove extension files: ${uninstallResult.error}`,
          });
          return;
        }
      }
      await deleteExtension({
        name,
        removeFromConfig: removeExtension,
        extensionConfig: config,
      });
      if (isBrxtInstalled) {
        toastService.success({ title: name, msg: 'Extension and its skills removed' });
      }
    } catch (error) {
      console.error('Failed to delete extension:', error);
    } finally {
      await fetchExtensions();
    }
  };

  const handleModalClose = () => {
    setDeepLinkConfigStateVar(null);
    setShowEnvVarsStateVar(null);

    setIsModalOpen(false);
    setIsAddModalOpen(false);
    setSelectedExtension(null);

    // Clear any navigation state that might be cached
    if (window.history.state?.deepLinkConfig) {
      window.history.replaceState({}, '', window.location.hash);
    }
  };

  return (
    <section id="extensions">
      <div className="">
        <ExtensionList
          extensions={extensions}
          onToggle={handleExtensionToggle}
          onConfigure={handleConfigureClick}
          disableConfiguration={disableConfiguration}
          searchTerm={searchTerm}
        />

        {!hideButtons && (
          <div className="flex gap-4 pt-4 w-full">
            <Button
              className="flex items-center gap-2 justify-center"
              variant="default"
              onClick={() => setIsBrxtModalOpen(true)}
            >
              <Plus className="h-4 w-4" />
              Add Extension
            </Button>
            <Button
              className="flex items-center gap-2 justify-center"
              variant="outline"
              onClick={() =>
                window.open('https://baranzinilab.github.io/biorouter-landing/baam.html', '_blank')
              }
            >
              <GPSIcon size={12} />
              Browse Extensions
            </Button>
            <Button
              className="flex items-center gap-2 justify-center"
              variant="outline"
              onClick={() => setIsAddModalOpen(true)}
            >
              <Plus className="h-4 w-4" />
              Add Custom Extension
            </Button>
          </div>
        )}

        {/* Modal for updating an existing extension */}
        {isModalOpen && selectedExtension && (
          <ExtensionModal
            title="Update Extension"
            initialData={extensionToFormData(selectedExtension)}
            onClose={handleModalClose}
            onSubmit={handleUpdateExtension}
            onDelete={handleDeleteExtension}
            submitLabel="Save Changes"
            modalType={'edit'}
          />
        )}

        {/* Modal for adding a new extension */}
        {isAddModalOpen && (
          <ExtensionModal
            title="Add custom extension"
            initialData={getDefaultFormData()}
            onClose={handleModalClose}
            onSubmit={handleAddExtension}
            submitLabel="Add Extension"
            modalType={'add'}
          />
        )}

        {/* Modal for adding extension from deeplink*/}
        {deepLinkConfigStateVar && showEnvVarsStateVar && (
          <ExtensionModal
            title="Add custom extension"
            initialData={extensionToFormData({
              ...deepLinkConfig,
              enabled: true,
            } as FixedExtensionEntry)}
            onClose={handleModalClose}
            onSubmit={handleAddExtension}
            submitLabel="Add Extension"
            modalType={'add'}
          />
        )}
      </div>
      {isBrxtModalOpen && (
        <BrxtInstallModal
          onClose={() => setIsBrxtModalOpen(false)}
          onInstalled={fetchExtensions}
        />
      )}
    </section>
  );
}
