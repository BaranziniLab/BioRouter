import React, { useEffect, useMemo, useState } from 'react';
import {
  Clock,
  Home,
  Layers,
  Puzzle,
  AppWindow,
  Pipeline,
  Plus,
  Settings,
  KnowledgeIcon,
} from '../icons/app-icons';
import { useNavigate, useSearchParams } from 'react-router-dom';
import {
  SidebarContent,
  SidebarFooter,
  SidebarMenu,
  SidebarMenuItem,
  SidebarMenuButton,
  SidebarGroup,
  SidebarGroupContent,
  SidebarSeparator,
} from '../ui/sidebar';
import { ViewOptions, View, navigateWithViewTransition } from '../../utils/navigationUtils';
import { useChatContext } from '../../contexts/ChatContext';
import { DEFAULT_CHAT_TITLE } from '../../contexts/ChatContext';
import EnvironmentBadge from './EnvironmentBadge';
import { listApps, type Session } from '../../api';
import { useRunningChats } from '../../hooks/chatStreamStore';
import {
  getCachedSessionList,
  preloadSessionList,
  refreshSessionList,
  subscribeSessionList,
} from '../../utils/sessionListCache';
import { preloadHomeActivity } from '../../utils/homeInsightsCache';
import SidebarUpdateButton from './SidebarUpdateButton';
import { subscribeSessionNameChanges } from '../../utils/sessionNameSync';
import RecentChats from './RecentChats';

function preloadHome(): void {
  preloadHomeActivity();
  preloadSessionList();
}

interface SidebarProps {
  onSelectSession: (sessionId: string) => void;
  refreshTrigger?: number;
  children?: React.ReactNode;
  setView?: (view: View, viewOptions?: ViewOptions) => void;
  currentPath?: string;
}

interface NavigationItem {
  type: 'item';
  path: string;
  label: string;
  icon: React.ComponentType<{ className?: string }>;
  tooltip: string;
}

const newChatItem: NavigationItem = {
  type: 'item',
  path: '/pair',
  label: 'New Chat',
  icon: Plus,
  tooltip: 'Start a new chat',
};

const settingsItem: NavigationItem = {
  type: 'item',
  path: '/settings',
  label: 'Settings',
  icon: Settings,
  tooltip: 'Configure BioRouter settings',
};

const menuItems: NavigationItem[] = [
  {
    type: 'item',
    path: '/',
    label: 'Home',
    icon: Home,
    tooltip: 'Go back to the main chat screen',
  },
  {
    type: 'item',
    path: '/workflows',
    label: 'Workflows',
    icon: Pipeline,
    tooltip: 'Browse your saved workflows',
  },
  {
    type: 'item',
    path: '/schedules',
    label: 'Scheduler',
    icon: Clock,
    tooltip: 'Manage scheduled runs',
  },
  {
    type: 'item',
    path: '/extensions',
    label: 'Extensions',
    icon: Puzzle,
    tooltip: 'Manage your extensions',
  },
  {
    type: 'item' as const,
    path: '/skills',
    label: 'Skills',
    icon: Layers,
    tooltip: 'Manage reusable instruction skills',
  },
  {
    type: 'item' as const,
    path: '/knowledge',
    label: 'Knowledge',
    icon: KnowledgeIcon,
    tooltip: 'Personal knowledge bases',
  },
  {
    type: 'item' as const,
    path: '/applications',
    label: 'Applications',
    icon: AppWindow,
    tooltip: 'BioRouter apps you built with Agent Drafter',
  },
  {
    type: 'item',
    path: '/apps',
    label: 'Apps',
    icon: AppWindow,
    tooltip: 'Browse and launch MCP apps',
  },
];

function useSidebarSessions(): Session[] {
  const [sessions, setSessions] = useState<Session[]>(() => getCachedSessionList() ?? []);

  useEffect(() => {
    const syncCache = () => setSessions(getCachedSessionList() ?? []);
    const refresh = () => {
      void refreshSessionList().catch(() => undefined);
    };
    let refreshTimer: number | undefined;
    const scheduleRefresh = () => {
      if (refreshTimer !== undefined) window.clearTimeout(refreshTimer);
      refreshTimer = window.setTimeout(() => {
        refreshTimer = undefined;
        refresh();
      }, 250);
    };

    const unsubscribe = subscribeSessionList(syncCache);
    const unsubscribeNames = subscribeSessionNameChanges(({ sessionId, name, userSetName }) => {
      setSessions((current) =>
        current.map((session) =>
          session.id === sessionId ? { ...session, name, user_set_name: userSetName } : session
        )
      );
    });

    refresh();
    window.addEventListener('session-created', scheduleRefresh);
    window.addEventListener('message-stream-finished', scheduleRefresh);

    return () => {
      unsubscribe();
      unsubscribeNames();
      if (refreshTimer !== undefined) window.clearTimeout(refreshTimer);
      window.removeEventListener('session-created', scheduleRefresh);
      window.removeEventListener('message-stream-finished', scheduleRefresh);
    };
  }, []);

  return sessions;
}

const AppSidebar: React.FC<SidebarProps> = ({ currentPath }) => {
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const chatContext = useChatContext();
  const runningChats = useRunningChats();
  const sessions = useSidebarSessions();
  const currentSessionId = currentPath === '/pair' ? searchParams.get('resumeSessionId') : null;
  const [hasApps, setHasApps] = useState(false);
  const runningSessionIds = useMemo(
    () =>
      new Set(runningChats.filter((entry) => !entry.completedAt).map((entry) => entry.sessionId)),
    [runningChats]
  );

  useEffect(() => {
    const checkApps = async () => {
      try {
        const response = await listApps({
          throwOnError: true,
        });
        setHasApps((response.data?.apps || []).length > 0);
      } catch (err) {
        console.warn('Failed to check for apps:', err);
      }
    };

    checkApps();
  }, [currentPath]);

  useEffect(() => {
    const currentItem = [...menuItems, settingsItem].find((item) => item.path === currentPath);

    const titleBits = ['Biorouter'];

    if (
      currentPath === '/pair' &&
      chatContext?.chat?.name &&
      chatContext.chat.name !== DEFAULT_CHAT_TITLE
    ) {
      titleBits.push(chatContext.chat.name);
    } else if (currentPath === '/sessions') {
      titleBits.push('Chat history');
    } else if (currentPath !== '/' && currentItem) {
      titleBits.push(currentItem.label);
    }

    document.title = titleBits.join(' - ');
  }, [currentPath, chatContext?.chat?.name]);

  const isActivePath = (path: string) => {
    return currentPath === path;
  };

  const handleNavigation = (path: string) => {
    if (path === '/pair') {
      chatContext?.resetChat();
      navigateWithViewTransition(navigate, '/pair', { newChat: true });
      return;
    }

    navigateWithViewTransition(navigate, path);
  };

  const handleOpenChat = (sessionId: string) => {
    navigateWithViewTransition(navigate, `/pair?resumeSessionId=${sessionId}`);
  };

  const renderMenuItem = (entry: NavigationItem) => {
    const IconComponent = entry.icon;

    return (
      <SidebarMenuItem key={entry.path}>
        <SidebarMenuButton
          data-testid={`sidebar-${entry.label.toLowerCase().replace(/\s+/g, '-')}-button`}
          onClick={() => handleNavigation(entry.path)}
          onFocus={entry.path === '/' ? preloadHome : undefined}
          onPointerEnter={entry.path === '/' ? preloadHome : undefined}
          isActive={isActivePath(entry.path)}
          tooltip={entry.tooltip}
          className="relative h-8 w-full justify-start rounded-lg px-3 py-2 text-sm transition-colors duration-150 before:absolute before:inset-y-2 before:left-0 before:w-0.5 before:rounded-full before:bg-transparent hover:bg-sidebar-hover data-[active=true]:bg-sidebar-active data-[active=true]:font-medium data-[active=true]:before:bg-accent-bar"
        >
          <IconComponent className="h-4 w-4" />
          <span>{entry.label}</span>
        </SidebarMenuButton>
      </SidebarMenuItem>
    );
  };

  const visibleMenuItems = menuItems.filter((entry) => {
    if (entry.type === 'item' && entry.path === '/apps') {
      return hasApps;
    }
    return true;
  });

  return (
    <>
      <SidebarContent className="gap-0 overflow-hidden pt-10">
        <div className="shrink-0">
          <div
            data-testid="sidebar-biorouter-wordmark"
            className="flex h-9 items-center gap-2 px-5"
          >
            <span className="text-sm font-semibold leading-none">Biorouter</span>
            <EnvironmentBadge />
          </div>

          <SidebarGroup className="px-2 pb-2 pt-1">
            <SidebarGroupContent>
              <SidebarMenu>
                <SidebarMenuItem>
                  <SidebarMenuButton
                    data-testid="sidebar-new-chat-button"
                    onClick={() => handleNavigation(newChatItem.path)}
                    onFocus={preloadSessionList}
                    onPointerEnter={preloadSessionList}
                    isActive={currentPath === '/pair' && !currentSessionId}
                    tooltip={newChatItem.tooltip}
                    className="relative h-10 w-full justify-start rounded-lg border border-sidebar-border bg-background-default/55 px-2.5 text-[13px] font-semibold text-text-default transition-colors duration-[var(--motion-base)] before:absolute before:inset-y-2 before:left-0 before:w-0.5 before:rounded-full before:bg-transparent hover:bg-background-default data-[active=true]:bg-sidebar-active data-[active=true]:before:bg-accent-bar"
                  >
                    <Plus className="size-4" />
                    <span>{newChatItem.label}</span>
                  </SidebarMenuButton>
                </SidebarMenuItem>
              </SidebarMenu>
            </SidebarGroupContent>
          </SidebarGroup>

          <SidebarGroup className="px-2 pb-2">
            <SidebarGroupContent>
              <SidebarMenu>{visibleMenuItems.map(renderMenuItem)}</SidebarMenu>
            </SidebarGroupContent>
          </SidebarGroup>
        </div>

        <SidebarSeparator data-testid="sidebar-nav-divider" className="shrink-0" />
        <RecentChats
          sessions={sessions}
          activeSessionId={currentSessionId}
          runningSessionIds={runningSessionIds}
          onOpen={handleOpenChat}
          onViewAll={() => navigateWithViewTransition(navigate, '/sessions')}
        />
      </SidebarContent>

      <SidebarSeparator data-testid="sidebar-footer-divider" className="shrink-0" />
      <SidebarFooter className="gap-1 p-2">
        <SidebarUpdateButton />
        <SidebarMenu className="gap-0">
          <SidebarMenuItem>
            <SidebarMenuButton
              data-testid="sidebar-settings-button"
              onClick={() => handleNavigation(settingsItem.path)}
              isActive={isActivePath(settingsItem.path)}
              tooltip={settingsItem.tooltip}
              className="relative h-8 w-full justify-start rounded-lg px-3 py-2 text-sm transition-colors duration-150 before:absolute before:inset-y-2 before:left-0 before:w-0.5 before:rounded-full before:bg-transparent hover:bg-sidebar-hover data-[active=true]:bg-sidebar-active data-[active=true]:font-medium data-[active=true]:before:bg-accent-bar"
            >
              <Settings className="h-4 w-4" />
              <span>{settingsItem.label}</span>
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarFooter>
    </>
  );
};

export default AppSidebar;
