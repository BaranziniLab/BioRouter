import React, { useEffect, useRef, useState } from 'react';
import { Clock, Home, Layers, Puzzle, History, AppWindow, MessageSquare, Pipeline, Settings, KnowledgeIcon, Terminal } from '../icons/app-icons';
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
import { ViewOptions, View } from '../../utils/navigationUtils';
import { useChatContext } from '../../contexts/ChatContext';
import { DEFAULT_CHAT_TITLE } from '../../contexts/ChatContext';
import EnvironmentBadge from './EnvironmentBadge';
import { listApps } from '../../api';

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

interface NavigationAction {
  type: 'action';
  action: 'launch-cli';
  label: string;
  icon: React.ComponentType<{ className?: string }>;
  tooltip: string;
}

interface NavigationSeparator {
  type: 'separator';
}

type NavigationEntry = NavigationItem | NavigationAction | NavigationSeparator;

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
    type: 'action',
    action: 'launch-cli',
    label: 'Terminal',
    icon: Terminal,
    tooltip: 'Launch the Biorouter CLI in a terminal',
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

const AppSidebar: React.FC<SidebarProps> = ({ currentPath }) => {
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const chatContext = useChatContext();
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
    if (path === '/pair' && sessionId && sessionId.length > 0) {
      navigate(`/pair?resumeSessionId=${sessionId}`);
    } else {
      navigate(path);
    }
  };

  const handleLaunchCli = async () => {
    try {
      const status = await window.electron.cliStatus();
      if (status?.onPath) {
        const res = await window.electron.launchCli();
        if (!res.success) {
          window.dispatchEvent(new CustomEvent('biorouter:open-cli-install'));
        }
      } else {
        // Not installed yet — surface the same install card the startup
        // check shows. Once installed, this button launches the CLI.
        window.dispatchEvent(new CustomEvent('biorouter:open-cli-install'));
      }
    } catch {
      window.dispatchEvent(new CustomEvent('biorouter:open-cli-install'));
    }
  };

  const renderMenuItem = (entry: NavigationEntry, index: number) => {
    if (entry.type === 'separator') {
      return <SidebarSeparator key={index} />;
    }

    const IconComponent = entry.icon;

    if (entry.type === 'action') {
      return (
        <SidebarGroup key={entry.action}>
          <SidebarGroupContent className="space-y-1">
            <div className="sidebar-item">
              <SidebarMenuItem>
                <SidebarMenuButton
                  data-testid={`sidebar-${entry.label.toLowerCase()}-button`}
                  onClick={handleLaunchCli}
                  tooltip={entry.tooltip}
                  className="w-full justify-start px-3 py-2 rounded-lg text-sm hover:bg-background-medium transition-colors duration-150"
                >
                  <IconComponent className="w-4 h-4" />
                  <span>{entry.label}</span>
                </SidebarMenuButton>
              </SidebarMenuItem>
            </div>
          </SidebarGroupContent>
        </SidebarGroup>
      );
    }

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
                className="w-full justify-start px-3 py-2 rounded-lg text-sm hover:bg-background-medium transition-colors duration-150 data-[active=true]:bg-background-strong data-[active=true]:font-medium"
              >
                <IconComponent className="w-4 h-4" />
                <span>{entry.label}</span>
              </SidebarMenuButton>
            </SidebarMenuItem>
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
