import React, { useEffect, useRef, useState } from 'react';
import {
  Clock,
  Home,
  Layers,
  Puzzle,
  History,
  AppWindow,
  MessageSquare,
  Pipeline,
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
import { BioRouter } from '../icons/BioRouter';
import { ViewOptions, View, navigateWithViewTransition } from '../../utils/navigationUtils';
import { useChatContext } from '../../contexts/ChatContext';
import { DEFAULT_CHAT_TITLE } from '../../contexts/ChatContext';
import EnvironmentBadge from './EnvironmentBadge';
import { listApps } from '../../api';
import { useRunningChats, RunningChatEntry } from '../../hooks/chatStreamStore';

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

interface NavigationSeparator {
  type: 'separator';
}

type NavigationEntry = NavigationItem | NavigationSeparator;

const menuItems: NavigationEntry[] = [
  {
    type: 'item',
    path: '/',
    label: 'Home',
    icon: Home,
    tooltip: 'Go back to the main chat screen',
  },
  { type: 'separator' },
  {
    type: 'item',
    path: '/pair',
    label: 'Chat',
    icon: MessageSquare,
    tooltip: 'Start pairing with BioRouter',
  },
  {
    type: 'item',
    path: '/sessions',
    label: 'History',
    icon: History,
    tooltip: 'View your session history',
  },
  { type: 'separator' },
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
  { type: 'separator' },
  {
    type: 'item',
    path: '/settings',
    label: 'Settings',
    icon: Settings,
    tooltip: 'Configure BioRouter settings',
  },
];

function RunningChatItem({
  entry,
  onOpen,
}: {
  entry: RunningChatEntry;
  onOpen: (sessionId: string) => void;
}) {
  const completed = Boolean(entry.completedAt);

  return (
    <button
      type="button"
      onClick={() => onOpen(entry.sessionId)}
      className={`w-full min-w-0 flex items-center gap-2 rounded-md px-2 py-1 text-left text-xs transition-[background-color,opacity,transform] duration-[var(--motion-base)] ease-[var(--ease-out)] hover:bg-sidebar-hover ${
        completed ? 'opacity-0 translate-y-1' : 'opacity-100 translate-y-0'
      }`}
      title={entry.title}
    >
      <span
        aria-hidden="true"
        className="relative flex h-4 w-4 flex-shrink-0 items-center justify-center text-text-default/80"
      >
        {!completed && (
          <>
            <span className="absolute h-4 w-4 rounded-full border border-current animate-[biorouter-working-ring_1.8s_ease-out_infinite]" />
            <span className="absolute h-2.5 w-2.5 rounded-full bg-current opacity-20 animate-[biorouter-working-glow_1.8s_ease-in-out_infinite]" />
          </>
        )}
        <span className="h-1.5 w-1.5 rounded-full bg-current opacity-70" />
      </span>
      <span className="min-w-0 flex-1 truncate font-medium text-text-default">{entry.title}</span>
    </button>
  );
}

const AppSidebar: React.FC<SidebarProps> = ({ currentPath }) => {
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const chatContext = useChatContext();
  const runningChats = useRunningChats();
  const lastSessionIdRef = useRef<string | null>(null);
  const currentSessionId = currentPath === '/pair' ? searchParams.get('resumeSessionId') : null;
  const [hasApps, setHasApps] = useState(false);

  useEffect(() => {
    if (currentSessionId) {
      lastSessionIdRef.current = currentSessionId;
    }
  }, [currentSessionId]);

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
    const currentItem = menuItems.find(
      (item) => item.type === 'item' && item.path === currentPath
    ) as NavigationItem | undefined;

    const titleBits = ['Biorouter'];

    if (
      currentPath === '/pair' &&
      chatContext?.chat?.name &&
      chatContext.chat.name !== DEFAULT_CHAT_TITLE
    ) {
      titleBits.push(chatContext.chat.name);
    } else if (currentPath !== '/' && currentItem) {
      titleBits.push(currentItem.label);
    }

    document.title = titleBits.join(' - ');
  }, [currentPath, chatContext?.chat?.name]);

  const isActivePath = (path: string) => {
    return currentPath === path;
  };

  const handleNavigation = (path: string) => {
    // For /pair, preserve the current session if one exists
    // Priority: current URL param > last known session > context
    const sessionId = currentSessionId || lastSessionIdRef.current || chatContext?.chat?.sessionId;
    // Route through the View Transitions crossfade (same path Hub/useNavigation
    // uses) so a top-level view switch orients the user instead of hard-cutting.
    // The helper falls back to a plain navigate under reduced-motion / no support.
    if (path === '/pair' && sessionId && sessionId.length > 0) {
      navigateWithViewTransition(navigate, `/pair?resumeSessionId=${sessionId}`);
    } else {
      navigateWithViewTransition(navigate, path);
    }
  };

  const handleOpenRunningChat = (sessionId: string) => {
    lastSessionIdRef.current = sessionId;
    navigateWithViewTransition(navigate, `/pair?resumeSessionId=${sessionId}`);
  };

  const renderMenuItem = (entry: NavigationEntry, index: number) => {
    if (entry.type === 'separator') {
      return <SidebarSeparator key={index} />;
    }

    const IconComponent = entry.icon;

    return (
      <SidebarGroup key={entry.path}>
        <SidebarGroupContent className="space-y-1">
          <div className="sidebar-item">
            <SidebarMenuItem>
              <SidebarMenuButton
                data-testid={`sidebar-${entry.label.toLowerCase()}-button`}
                onClick={() => handleNavigation(entry.path)}
                isActive={isActivePath(entry.path)}
                tooltip={entry.tooltip}
                className="w-full justify-start px-3 py-2 rounded-lg text-sm hover:bg-sidebar-hover transition-colors duration-150 data-[active=true]:bg-sidebar-active data-[active=true]:font-medium"
              >
                <IconComponent className="w-4 h-4" />
                <span>{entry.label}</span>
              </SidebarMenuButton>
            </SidebarMenuItem>
            {entry.path === '/sessions' && runningChats.length > 0 && (
              <div className="ml-7 mt-0.5 mb-1 space-y-0.5">
                {runningChats.map((running) => (
                  <RunningChatItem
                    key={running.sessionId}
                    entry={running}
                    onOpen={handleOpenRunningChat}
                  />
                ))}
              </div>
            )}
          </div>
        </SidebarGroupContent>
      </SidebarGroup>
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
      <SidebarContent className="pt-16">
        <SidebarMenu>
          {visibleMenuItems.map((entry, index) => renderMenuItem(entry, index))}
        </SidebarMenu>
      </SidebarContent>

      <SidebarFooter className="pb-4 px-4">
        <div className="flex items-center gap-2">
          <BioRouter className="size-7 biorouter-icon-animation flex-shrink-0" />
          <span className="text-sm font-semibold leading-none">Biorouter</span>
          <EnvironmentBadge />
        </div>
      </SidebarFooter>
    </>
  );
};

export default AppSidebar;
