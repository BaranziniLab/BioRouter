import { View, ViewOptions } from '../../utils/navigationUtils';
import ExtensionsSection from '../settings/extensions/ExtensionsSection';
import { ExtensionConfig } from '../../api';
import { MainPanelLayout } from '../Layout/MainPanelLayout';
import { Button } from '../ui/button';
import { Plus } from 'lucide-react';
import { GPSIcon } from '../ui/icons';
import { useState, useEffect } from 'react';
import kebabCase from 'lodash/kebabCase';
import ExtensionModal from '../settings/extensions/modal/ExtensionModal';
import {
  getDefaultFormData,
  ExtensionFormData,
  createExtensionConfig,
} from '../settings/extensions/utils';
import { activateExtensionDefault } from '../settings/extensions';
import { useConfig } from '../ConfigContext';
import { SearchView } from '../conversation/SearchView';
import { getSearchShortcutText } from '../../utils/keyboardShortcuts';

export type ExtensionsViewOptions = {
  deepLinkConfig?: ExtensionConfig;
  showEnvVars?: boolean;
};

export default function ExtensionsView({
  viewOptions,
}: {
  onClose: () => void;
  setView: (view: View, viewOptions?: ViewOptions) => void;
  viewOptions: ExtensionsViewOptions;
}) {
  const [isAddModalOpen, setIsAddModalOpen] = useState(false);
  const [refreshKey, setRefreshKey] = useState(0);
  const [searchTerm, setSearchTerm] = useState('');
  const { addExtension } = useConfig();

  // Only trigger refresh when deep link config changes AND we don't need to show env vars
  useEffect(() => {
    if (viewOptions.deepLinkConfig && !viewOptions.showEnvVars) {
      setRefreshKey((prevKey) => prevKey + 1);
    }
  }, [viewOptions.deepLinkConfig, viewOptions.showEnvVars]);

  const scrollToExtension = (extensionName: string) => {
    setTimeout(() => {
      const element = document.getElementById(`extension-${kebabCase(extensionName)}`);
      if (element) {
        element.scrollIntoView({
          behavior: 'smooth',
          block: 'center',
        });
        // Add a subtle highlight effect
        element.style.boxShadow = '0 0 0 2px rgba(59, 130, 246, 0.5)';
        setTimeout(() => {
          element.style.boxShadow = '';
        }, 2000);
      }
    }, 200);
  };

  // Scroll to extension whenever extensionId is provided (after refresh)
  useEffect(() => {
    if (viewOptions.deepLinkConfig?.name && refreshKey > 0) {
      scrollToExtension(viewOptions.deepLinkConfig?.name);
    }
  }, [viewOptions.deepLinkConfig?.name, refreshKey]);

  const handleModalClose = () => {
    setIsAddModalOpen(false);
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
      // Trigger a refresh of the extensions list
      setRefreshKey((prevKey) => prevKey + 1);
    } catch (error) {
      console.error('Failed to activate extension:', error);
      setRefreshKey((prevKey) => prevKey + 1);
    }
  };

  return (
    <MainPanelLayout>
      <div
        className="flex flex-col min-w-0 flex-1 overflow-y-auto relative"
        data-search-scroll-area
      >
        {/* Flat page header */}
        <div className="px-8 pt-12 pb-6 flex-shrink-0 border-b border-border-subtle">
          <div className="flex flex-col page-transition">
            <h1 className="text-2xl font-semibold tracking-tight mb-1">Extensions</h1>
            <p className="text-sm text-text-muted mb-1">
              These extensions use the Model Context Protocol (MCP). They can expand BioRouter's
              capabilities using three main components: Prompts, Resources, and Tools.{' '}
              {getSearchShortcutText()} to search.
            </p>
            <p className="text-sm text-text-muted mb-4">
              Extensions enabled here are used as the default for new chats. You can also toggle
              active extensions during chat.
            </p>
            <div className="flex gap-3">
              <Button
                className="flex items-center gap-2"
                variant="default"
                onClick={() => setIsAddModalOpen(true)}
              >
                <Plus className="h-4 w-4" />
                Add custom extension
              </Button>
              <Button
                className="flex items-center gap-2"
                variant="outline"
                onClick={() =>
                  window.open('https://baranzinilab.github.io/biorouter-landing/baam.html', '_blank')
                }
              >
                <GPSIcon size={12} />
                Browse extensions
              </Button>
            </div>
          </div>
        </div>

        <div className="px-4 pb-4">
          <SearchView onSearch={(term) => setSearchTerm(term)} placeholder="Search extensions...">
            <ExtensionsSection
              key={refreshKey}
              deepLinkConfig={viewOptions.deepLinkConfig}
              showEnvVars={viewOptions.showEnvVars}
              hideButtons={true}
              searchTerm={searchTerm}
              onModalClose={(extensionName: string) => {
                scrollToExtension(extensionName);
              }}
            />
          </SearchView>
        </div>
      </div>

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
    </MainPanelLayout>
  );
}
