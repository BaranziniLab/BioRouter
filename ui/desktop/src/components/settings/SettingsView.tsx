import { ScrollArea } from '../ui/scroll-area';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '../ui/tabs';
import { View, ViewOptions } from '../../utils/navigationUtils';
import ModelsSection from './models/ModelsSection';
import AppSettingsSection from './app/AppSettingsSection';
import ConfigSettings from './config/ConfigSettings';
import { ExtensionConfig } from '../../api';
import { MainPanelLayout } from '../Layout/MainPanelLayout';
import { Brain, Monitor, MessageSquare } from '../icons/app-icons';
import { useState, useEffect } from 'react';
import ChatSettingsSection from './chat/ChatSettingsSection';
import { CONFIGURATION_ENABLED } from '../../updates';
import { ReadableContent } from '../Layout/ReadableContent';

export type SettingsViewOptions = {
  deepLinkConfig?: ExtensionConfig;
  showEnvVars?: boolean;
  section?: string;
};

export default function SettingsView({
  onClose,
  setView,
  viewOptions,
}: {
  onClose: () => void;
  setView: (view: View, viewOptions?: ViewOptions) => void;
  viewOptions: SettingsViewOptions;
}) {
  const [activeTab, setActiveTab] = useState('models');

  const handleTabChange = (tab: string) => {
    setActiveTab(tab);
  };

  // Determine initial tab based on section prop
  useEffect(() => {
    if (viewOptions.section) {
      // Map section names to tab values
      const sectionToTab: Record<string, string> = {
        update: 'app',
        models: 'models',
        modes: 'chat',
        styles: 'chat',
        tools: 'chat',
        app: 'app',
        chat: 'chat',
      };

      const targetTab = sectionToTab[viewOptions.section];
      if (targetTab) {
        setActiveTab(targetTab);
      }
    }
  }, [viewOptions.section]);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        onClose();
      }
    };

    document.addEventListener('keydown', handleKeyDown);

    return () => {
      document.removeEventListener('keydown', handleKeyDown);
    };
  }, [onClose]);

  return (
    <>
      <MainPanelLayout>
        <div className="flex-1 flex flex-col min-h-0">
          <ReadableContent className="px-8 pt-12 pb-6 border-b border-border-subtle flex-shrink-0">
            <h1 className="text-2xl font-semibold tracking-tight mb-1 page-transition">Settings</h1>
            <p className="text-sm text-text-muted">
              Manage models, chat behavior, and application preferences
            </p>
          </ReadableContent>

          <div className="flex-1 min-h-0 flex flex-col">
            <Tabs
              value={activeTab}
              onValueChange={handleTabChange}
              className="h-full flex flex-col"
            >
              <ReadableContent className="px-8 pt-4">
                <TabsList className="biorouter-settings-tabs justify-start gap-1 rounded-xl p-1 h-auto w-fit">
                  <TabsTrigger
                    value="models"
                    className="flex gap-2 rounded-lg bg-transparent px-3 py-2 text-sm data-[state=active]:bg-background-medium data-[state=active]:shadow-none"
                    data-testid="settings-models-tab"
                  >
                    <Brain className="h-4 w-4" />
                    Models
                  </TabsTrigger>
                  <TabsTrigger
                    value="chat"
                    className="flex gap-2 rounded-lg bg-transparent px-3 py-2 text-sm data-[state=active]:bg-background-medium data-[state=active]:shadow-none"
                    data-testid="settings-chat-tab"
                  >
                    <MessageSquare className="h-4 w-4" />
                    Chat
                  </TabsTrigger>
                  <TabsTrigger
                    value="app"
                    className="flex gap-2 rounded-lg bg-transparent px-3 py-2 text-sm data-[state=active]:bg-background-medium data-[state=active]:shadow-none"
                    data-testid="settings-app-tab"
                  >
                    <Monitor className="h-4 w-4" />
                    App
                  </TabsTrigger>
                </TabsList>
              </ReadableContent>

              <ScrollArea className="flex-1" paddingX={1}>
                <ReadableContent className="px-8 py-5">
                  <TabsContent value="models" className="mt-0 focus-visible:outline-none">
                    <ModelsSection setView={setView} />
                  </TabsContent>
                  <TabsContent value="chat" className="mt-0 focus-visible:outline-none">
                    <ChatSettingsSection />
                  </TabsContent>
                  <TabsContent value="app" className="mt-0 focus-visible:outline-none">
                    <div>
                      {CONFIGURATION_ENABLED && <ConfigSettings />}
                      <AppSettingsSection scrollToSection={viewOptions.section} />
                    </div>
                  </TabsContent>
                </ReadableContent>
              </ScrollArea>
            </Tabs>
          </div>
        </div>
      </MainPanelLayout>
    </>
  );
}
