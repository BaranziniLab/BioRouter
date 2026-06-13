import React from 'react';
import { Outlet, useNavigate, useLocation } from 'react-router-dom';
import AppSidebar from '../BioRouterSidebar/AppSidebar';
import { View, ViewOptions } from '../../utils/navigationUtils';
import { Plus, LayoutDashboard, Terminal } from '../icons/app-icons';
import { Button } from '../ui/button';
import { Sidebar, SidebarInset, SidebarProvider, SidebarTrigger, useSidebar } from '../ui/sidebar';
import { getInitialWorkingDir } from '../../utils/workingDir';
import DependencySetupModal from '../DependencySetupModal';

const AppLayoutContent: React.FC = () => {
  const navigate = useNavigate();
  const location = useLocation();
  const safeIsMacOS = (window?.electron?.platform || 'darwin') === 'darwin';
  const { isMobile, openMobile } = useSidebar();

  // Calculate padding based on sidebar state and macOS
  const headerPadding = safeIsMacOS ? 'pl-21' : 'pl-4';
  // const headerPadding = '';

  // Hide buttons when mobile sheet is showing
  const shouldHideButtons = isMobile && openMobile;

  const setView = (view: View, viewOptions?: ViewOptions) => {
    // Convert view-based navigation to route-based navigation
    switch (view) {
      case 'chat':
        navigate('/');
        break;
      case 'pair':
        navigate('/pair');
        break;
      case 'settings':
        navigate('/settings', { state: viewOptions });
        break;
      case 'extensions':
        navigate('/extensions', { state: viewOptions });
        break;
      case 'sessions':
        navigate('/sessions');
        break;
      case 'schedules':
        navigate('/schedules');
        break;
      case 'workflows':
        navigate('/workflows');
        break;
      case 'permission':
        navigate('/permission', { state: viewOptions });
        break;
      case 'ConfigureProviders':
        navigate('/configure-providers');
        break;
      case 'sharedSession':
        navigate('/shared-session', { state: viewOptions });
        break;
      case 'welcome':
        navigate('/welcome');
        break;
      default:
        navigate('/');
    }
  };

  const handleSelectSession = async (sessionId: string) => {
    // Navigate to chat with session data
    navigate('/', { state: { sessionId } });
  };

  const handleNewWindow = () => {
    window.electron.createChatWindow(undefined, getInitialWorkingDir());
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

  return (
    <div className="flex flex-1 w-full relative animate-fade-in">
      {!shouldHideButtons && (
        <div className={`${headerPadding} absolute top-3 z-100 flex items-center`}>
          <SidebarTrigger
            className={`no-drag hover:border-border-strong hover:text-text-default hover:!bg-background-medium hover:scale-105`}
          />
          <Button
            onClick={handleNewWindow}
            className="no-drag hover:!bg-background-medium"
            variant="ghost"
            size="xs"
            title="Start a new session in a new window"
          >
            <Plus className="w-4 h-4" />
          </Button>
          <Button
            onClick={() =>
              navigate(location.pathname === '/dashboard' ? '/' : '/dashboard')
            }
            className={`no-drag hover:!bg-background-medium ${
              location.pathname === '/dashboard' ? 'bg-background-medium' : ''
            }`}
            variant="ghost"
            size="xs"
            title={
              location.pathname === '/dashboard'
                ? 'Exit Dashboard'
                : 'Open Dashboard'
            }
          >
            <LayoutDashboard className="w-4 h-4" />
          </Button>
          <Button
            onClick={handleLaunchCli}
            className="no-drag hover:!bg-background-medium"
            variant="ghost"
            size="xs"
            title="Launch the Biorouter CLI in a terminal"
          >
            <Terminal className="w-4 h-4" />
          </Button>
        </div>
      )}
      <Sidebar variant="inset" collapsible="offcanvas">
        <AppSidebar
          onSelectSession={handleSelectSession}
          setView={setView}
          currentPath={location.pathname}
        />
      </Sidebar>
      <SidebarInset>
        <Outlet />
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
