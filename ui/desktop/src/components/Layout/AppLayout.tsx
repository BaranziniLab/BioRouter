import React from 'react';
import { Outlet, useNavigate, useLocation } from 'react-router-dom';
import AppSidebar from '../BioRouterSidebar/AppSidebar';
import { View, ViewOptions } from '../../utils/navigationUtils';
import { useNavigation } from '../../hooks/useNavigation';
import { Sidebar, SidebarInset, SidebarProvider, useSidebar } from '../ui/sidebar';
import { getInitialWorkingDir } from '../../utils/workingDir';
import DependencySetupModal from '../DependencySetupModal';
import {
  getTitlebarControlReserve,
  TitlebarControls,
  TITLEBAR_CONTROL_RESERVE_PROPERTY,
} from './TitlebarControls';

const SIDEBAR_AUTO_COLLAPSE_WIDTH = 1120;

const AppLayoutContent: React.FC = () => {
  const navigate = useNavigate();
  const location = useLocation();
  const safeIsMacOS = (window?.electron?.platform || 'darwin') === 'darwin';
  const { isMobile, open, openMobile, setOpen } = useSidebar();
  const autoCollapsedSidebarRef = React.useRef(false);
  const resizeSettlingTimerRef = React.useRef<number | null>(null);

  // Hide buttons when mobile sheet is showing
  const shouldHideButtons = isMobile && openMobile;

  React.useEffect(() => {
    const isChatRoute = location.pathname === '/' || location.pathname === '/pair';
    document.body.classList.toggle('biorouter-chat-route-active', isChatRoute);
    return () => document.body.classList.remove('biorouter-chat-route-active');
  }, [location.pathname]);

  React.useEffect(() => {
    const updateSidebarMode = () => {
      const shouldCollapse = window.innerWidth < SIDEBAR_AUTO_COLLAPSE_WIDTH;
      document.body.classList.toggle('biorouter-sidebar-compact', shouldCollapse && !isMobile);

      if (shouldCollapse && open && !autoCollapsedSidebarRef.current) {
        autoCollapsedSidebarRef.current = true;
        setOpen(false);
        return;
      }

      if (!shouldCollapse && autoCollapsedSidebarRef.current) {
        autoCollapsedSidebarRef.current = false;
        setOpen(true);
      }
    };

    updateSidebarMode();
    window.addEventListener('resize', updateSidebarMode);
    return () => {
      document.body.classList.remove('biorouter-sidebar-compact');
      window.removeEventListener('resize', updateSidebarMode);
    };
  }, [isMobile, open, setOpen]);

  React.useEffect(() => {
    const markWindowResizing = () => {
      document.body.classList.add('biorouter-window-resizing');
      if (resizeSettlingTimerRef.current !== null) {
        window.clearTimeout(resizeSettlingTimerRef.current);
      }
      resizeSettlingTimerRef.current = window.setTimeout(() => {
        resizeSettlingTimerRef.current = null;
        document.body.classList.remove('biorouter-window-resizing');
      }, 180);
    };

    window.addEventListener('resize', markWindowResizing);
    return () => {
      window.removeEventListener('resize', markWindowResizing);
      if (resizeSettlingTimerRef.current !== null) {
        window.clearTimeout(resizeSettlingTimerRef.current);
      }
      document.body.classList.remove('biorouter-window-resizing');
    };
  }, []);

  // Keep every top-level route on the shared navigation mapping.
  const navigateToView = useNavigation();
  const setView = (view: View, viewOptions?: ViewOptions) => navigateToView(view, viewOptions);

  const handleSelectSession = async (sessionId: string) => {
    // Navigate to chat with session data
    navigate('/', { state: { sessionId } });
  };

  const handleNewWindow = () => {
    window.electron.createChatWindow(undefined, getInitialWorkingDir());
  };

  return (
    <div
      className="relative flex w-full flex-1 animate-fade-in"
      style={
        {
          [TITLEBAR_CONTROL_RESERVE_PROPERTY]: `${getTitlebarControlReserve(safeIsMacOS)}px`,
        } as React.CSSProperties
      }
    >
      <TitlebarControls
        hidden={shouldHideButtons}
        isMacOS={safeIsMacOS}
        onNewWindow={handleNewWindow}
      />
      <Sidebar variant="inset" collapsible="offcanvas">
        <AppSidebar
          onSelectSession={handleSelectSession}
          setView={setView}
          currentPath={location.pathname}
        />
      </Sidebar>
      <SidebarInset>
        <main
          className="route-container biorouter-route-surface relative z-[60] flex h-full min-h-0 min-w-0 flex-1 flex-col"
          data-route-path={location.pathname}
        >
          <Outlet />
        </main>
      </SidebarInset>
    </div>
  );
};

export const AppLayout: React.FC = () => {
  return (
    <SidebarProvider>
      <AppLayoutContent />
      <DependencySetupModal />
    </SidebarProvider>
  );
};
