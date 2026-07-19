import React, { useEffect, useMemo, useState } from 'react';
import { Home, Plus, Settings } from '../icons/app-icons';
import { ENTITY_ICONS } from '../icons/entity-icons';
import { useNavigate, useSearchParams } from 'react-router-dom';
import {
  SidebarContent,
  SidebarFooter,
  SidebarMenu,
  SidebarMenuItem,
  SidebarMenuButton,
  SidebarGroup,
  SidebarGroupContent,
} from '../ui/sidebar';
import { BioRouterWordmark } from '../icons/BioRouterWordmark';
import { ViewOptions, View, navigateWithViewTransition } from '../../utils/navigationUtils';
import { useChatContext } from '../../contexts/ChatContext';
import { DEFAULT_CHAT_TITLE } from '../../contexts/ChatContext';
import EnvironmentBadge from './EnvironmentBadge';
import { listApps } from '../../api';
import { useRunningChats } from '../../hooks/chatStreamStore';
import { preloadSessionList } from '../../utils/sessionListCache';
import { preloadHomeActivity } from '../../utils/homeInsightsCache';
import SidebarUpdateButton from './SidebarUpdateButton';
import RecentChats from './RecentChats';
import useSidebarSessions from './useSidebarSessions';

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

const settingsItem: NavigationItem = {
  type: 'item',
  path: '/settings',
  label: 'Settings',
  icon: Settings,
  tooltip: 'Configure Biorouter settings',
};

const menuItems: NavigationItem[] = [
  {
    type: 'item',
    path: '/pair',
    label: 'New Session',
    icon: Plus,
    tooltip: 'Start a new session',
  },
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
    icon: ENTITY_ICONS.workflow,
    tooltip: 'Browse your saved workflows',
  },
  {
    type: 'item',
    path: '/schedules',
    label: 'Scheduler',
    icon: ENTITY_ICONS.schedule,
    tooltip: 'Manage scheduled runs',
  },
  {
    type: 'item',
    path: '/extensions',
    label: 'Extensions',
    icon: ENTITY_ICONS.extension,
    tooltip: 'Manage your extensions',
  },
  {
    type: 'item' as const,
    path: '/skills',
    label: 'Skills',
    icon: ENTITY_ICONS.skill,
    tooltip: 'Manage reusable instruction skills',
  },
  {
    type: 'item' as const,
    path: '/knowledge',
    label: 'Knowledge',
    icon: ENTITY_ICONS.knowledge,
    tooltip: 'Personal knowledge bases',
  },
  {
    type: 'item' as const,
    path: '/applications',
    label: 'Applications',
    icon: ENTITY_ICONS.application,
    tooltip: 'Biorouter apps you built with Agent Drafter',
  },
  {
    type: 'item',
    path: '/apps',
    label: 'Apps',
    icon: ENTITY_ICONS.mcpApp,
    tooltip: 'Browse and launch MCP apps',
  },
];

const AppSidebar: React.FC<SidebarProps> = ({ currentPath }) => {
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const chatContext = useChatContext();
  const runningChats = useRunningChats();
  const { sessions, hasMore, isLoading, loadMore } = useSidebarSessions();
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

  // A click opens the chat as its own tab; if it is already open anywhere the
  // reducer dedupes and just activates it. This is a cross-route entry, so it
  // keeps today's PUSH navigation — only in-strip tab clicks use replace, which
  // is what keeps Back returning you to wherever you came from INTO /pair.
  // `title` rides along in route state so the tab opens already named. It is
  // only ever a HINT: the URL alone still opens the chat (a deep link, a
  // reload, an external nav carry no state), and BaseChat's load still renames
  // the tab authoritatively — including the `userSetName` flag, which the
  // session LIST does not carry. This removes the flash, not the correction.
  const handleOpenChat = (sessionId: string, title?: string) => {
    // NB: the 3rd arg of navigateWithViewTransition IS the route state itself,
    // not an options bag — `{ state: … }` here would nest it one level deep and
    // silently do nothing.
    navigateWithViewTransition(
      navigate,
      `/pair?resumeSessionId=${sessionId}`,
      title ? { title } : undefined
    );
  };

  const renderMenuItem = (entry: NavigationItem) => {
    const IconComponent = entry.icon;
    const isActive =
      entry.path === '/pair'
        ? currentPath === '/pair' && !currentSessionId
        : isActivePath(entry.path);

    return (
      <SidebarMenuItem key={entry.path}>
        <SidebarMenuButton
          data-testid={`sidebar-${entry.label.toLowerCase().replace(/\s+/g, '-')}-button`}
          onClick={() => handleNavigation(entry.path)}
          onFocus={entry.path === '/' ? preloadHome : undefined}
          onPointerEnter={entry.path === '/' ? preloadHome : undefined}
          isActive={isActive}
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
      <SidebarContent className="gap-0 overflow-hidden">
        {/* The 52px titlebar band: traffic lights and the floating TitlebarControls
            strip live over this space, so the sidebar only reserves it. Its bottom
            hairline continues the chat/preview header hairline, giving the window
            one continuous top edge.
            `-mt-2` cancels SidebarContent's 8px top padding so the band starts at
            the window's top edge (y=0), exactly like the chat/preview header —
            otherwise the band sat 8px low and its hairline fell ~9px below the tab
            strip's, breaking the "one continuous top edge". */}
        <div
          data-testid="sidebar-titlebar-band"
          aria-hidden="true"
          className="-mt-2 h-13 shrink-0 border-b border-sidebar-border"
        />

        <div className="shrink-0">
          {/* Brand row. The wordmark IS the lockup now — "Bio" navy + "Router"
              coral over the split underline (D-39), replacing the old mono glyph
              + plain "Biorouter" text. Its left edge lands on the same 44px text
              edge as every nav label. The component recolours navy -> UCSF teal
              on a dark surface on its own. */}
          <div className="px-2 pt-2">
            <div
              data-testid="sidebar-biorouter-wordmark"
              className="flex h-8 items-center gap-2 px-3"
            >
              <BioRouterWordmark
                data-testid="sidebar-biorouter-mark"
                // h-[22px] so the wordmark's cap height reads at the same size as
                // the text-sm nav labels beside it — measured against "New Session"
                // in the calibration harness. (The SVG box includes the underline,
                // so the letters are smaller than the box; 17px looked undersized.)
                className="h-[22px] w-auto shrink-0"
              />
              <EnvironmentBadge />
            </div>
          </div>

          <SidebarGroup className="px-2 pb-1">
            <div data-testid="sidebar-menu-label" className="flex h-8 shrink-0 items-center px-3">
              <span className="text-[11px] font-semibold uppercase tracking-[0.08em] text-text-subtle">
                Menu
              </span>
            </div>
            <SidebarGroupContent>
              <SidebarMenu className="gap-0">{visibleMenuItems.map(renderMenuItem)}</SidebarMenu>
            </SidebarGroupContent>
          </SidebarGroup>
        </div>

        {/* Full-width inset rule — the menu and the history are separate zones. */}
        <div
          data-testid="sidebar-nav-divider"
          role="separator"
          aria-orientation="horizontal"
          className="mx-3.5 my-2 h-px shrink-0 bg-sidebar-border"
        />
        <RecentChats
          sessions={sessions}
          activeSessionId={currentSessionId}
          runningSessionIds={runningSessionIds}
          hasMore={hasMore}
          isLoadingMore={isLoading}
          onLoadMore={loadMore}
          onOpen={handleOpenChat}
          onViewAll={() => navigateWithViewTransition(navigate, '/sessions')}
        />
      </SidebarContent>

      {/* Matches the menu/history divider above rather than SidebarSeparator's
          32px sliver — two rules ~100px apart at different widths read as an
          accident, not a system. */}
      <div
        data-testid="sidebar-footer-divider"
        role="separator"
        className="mx-3.5 my-2 h-px shrink-0 bg-sidebar-border"
      />
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
