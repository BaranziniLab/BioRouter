import { ScrollArea } from '../ui/scroll-area';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '../ui/tabs';
import { View, ViewOptions } from '../../utils/navigationUtils';
import ModelsSection from './models/ModelsSection';
import AppSettingsSection from './app/AppSettingsSection';
import ConfigSettings from './config/ConfigSettings';
import { ExtensionConfig } from '../../api';
import { MainPanelLayout } from '../Layout/MainPanelLayout';
import { Brain, Monitor, MessageSquare } from '../icons/app-icons';
import { useState, useEffect, useRef } from 'react';
import ChatSettingsSection from './chat/ChatSettingsSection';
import { CONFIGURATION_ENABLED } from '../../updates';
import { trackSettingsTabViewed } from '../../utils/analytics';

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
  const hasTrackedInitialTab = useRef(false);

  const handleTabChange = (tab: string) => {
    setActiveTab(tab);
    trackSettingsTabViewed(tab);
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
    if (!hasTrackedInitialTab.current) {
      trackSettingsTabViewed(activeTab);
      hasTrackedInitialTab.current = true;
    }
  }, [activeTab]);

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
          {/* Flat page header */}
          <div className="px-8 pt-12 pb-0 flex-shrink-0">
            <h1 className="text-2xl font-semibold tracking-tight page-transition">Settings</h1>
          </div>

          {/* Tabs directly on canvas — no card wrapper */}
          <div className="flex-1 min-h-0 flex flex-col">
            <Tabs
              value={activeTab}
              onValueChange={handleTabChange}
              className="h-full flex flex-col"
            >
              <div className="px-8 pt-4 border-b border-border-subtle">
                <TabsList className="justify-start bg-transparent gap-1 p-0 h-auto mb-0">
                  <TabsTrigger
                    value="models"
                    className="flex gap-2 rounded-none border-b-2 border-transparent data-[state=active]:border-text-default data-[state=active]:bg-transparent bg-transparent px-3 pb-3 text-sm"
                    data-testid="settings-models-tab"
                  >
                    <Brain className="h-4 w-4" />
                    Models
                  </TabsTrigger>
                  <TabsTrigger
                    value="chat"
                    className="flex gap-2 rounded-none border-b-2 border-transparent data-[state=active]:border-text-default data-[state=active]:bg-transparent bg-transparent px-3 pb-3 text-sm"
                    data-testid="settings-chat-tab"
                  >
                    <MessageSquare className="h-4 w-4" />
                    Chat
                  </TabsTrigger>
                  <TabsTrigger
                    value="app"
                    className="flex gap-2 rounded-none border-b-2 border-transparent data-[state=active]:border-text-default data-[state=active]:bg-transparent bg-transparent px-3 pb-3 text-sm"
                    data-testid="settings-app-tab"
                  >
                    <Monitor className="h-4 w-4" />
                    App
                  </TabsTrigger>
                </TabsList>
              </div>

              <ScrollArea className="flex-1" paddingX={1}>
                <div className="px-8 py-6">
                  <TabsContent value="models" className="mt-0 focus-visible:outline-none">
                    <ModelsSection setView={setView} />
                  </TabsContent>
                  <TabsContent value="chat" className="mt-0 focus-visible:outline-none">
                    <ChatSettingsSection />
                  </TabsContent>
                  <TabsContent value="app" className="mt-0 focus-visible:outline-none">
                    <div className="space-y-10">
                      {CONFIGURATION_ENABLED && <ConfigSettings />}
                      <AppSettingsSection scrollToSection={viewOptions.section} />
                    </div>
                  </TabsContent>
                </div>
              </ScrollArea>
            </Tabs>
          </div>
        </div>
      </MainPanelLayout>
    </>
  );
}
