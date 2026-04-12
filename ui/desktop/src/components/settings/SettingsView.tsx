import { ScrollArea } from '../ui/scroll-area';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '../ui/tabs';
import { View, ViewOptions } from '../../utils/navigationUtils';
import ModelsSection from './models/ModelsSection';
import AppSettingsSection from './app/AppSettingsSection';
import ConfigSettings from './config/ConfigSettings';
import { ExtensionConfig } from '../../api';
import { MainPanelLayout } from '../Layout/MainPanelLayout';
import { Bot, Monitor, MessageSquare } from 'lucide-react';
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
          {/* Floating header card */}
          <div
            className="mx-4 mt-4 bg-background-default rounded-2xl mb-3 relative overflow-hidden"
            style={{ boxShadow: 'var(--shadow-default)' }}
          >
            <div className="px-8 pb-6 pt-12">
              <div className="flex flex-col page-transition">
                <div className="flex justify-between items-center mb-1">
                  <h1 className="text-4xl font-light">Settings</h1>
                </div>
              </div>
            </div>
          </div>

          {/* Tabs content card */}
          <div
            className="flex-1 min-h-0 mx-4 mb-4 rounded-2xl bg-background-muted overflow-hidden"
            style={{ boxShadow: 'var(--shadow-default)' }}
          >
            <div className="flex flex-col h-full px-4 pt-3 pb-0">
              <Tabs
                value={activeTab}
                onValueChange={handleTabChange}
                className="h-full flex flex-col"
              >
                <div className="px-1">
                  <TabsList className="w-full mb-2 justify-start bg-background-muted">
                    <TabsTrigger
                      value="models"
                      className="flex gap-2"
                      data-testid="settings-models-tab"
                    >
                      <Bot className="h-4 w-4" />
                      Models
                    </TabsTrigger>
                    <TabsTrigger value="chat" className="flex gap-2" data-testid="settings-chat-tab">
                      <MessageSquare className="h-4 w-4" />
                      Chat
                    </TabsTrigger>
                    <TabsTrigger value="app" className="flex gap-2" data-testid="settings-app-tab">
                      <Monitor className="h-4 w-4" />
                      App
                    </TabsTrigger>
                  </TabsList>
                </div>

                <ScrollArea className="flex-1" paddingX={1}>
                  <TabsContent
                    value="models"
                    className="mt-0 focus-visible:outline-none"
                  >
                    <ModelsSection setView={setView} />
                  </TabsContent>

                  <TabsContent
                    value="chat"
                    className="mt-0 focus-visible:outline-none"
                  >
                    <ChatSettingsSection />
                  </TabsContent>

                  <TabsContent
                    value="app"
                    className="mt-0 focus-visible:outline-none"
                  >
                    <div className="space-y-8">
                      {CONFIGURATION_ENABLED && <ConfigSettings />}
                      <AppSettingsSection scrollToSection={viewOptions.section} />
                    </div>
                  </TabsContent>
                </ScrollArea>
              </Tabs>
            </div>
          </div>
        </div>
      </MainPanelLayout>
    </>
  );
}
