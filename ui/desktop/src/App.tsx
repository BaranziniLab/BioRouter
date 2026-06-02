import { useEffect, useState } from 'react';
import { IpcRendererEvent } from 'electron';
import {
  HashRouter,
  Routes,
  Route,
  useNavigate,
  useLocation,
  useSearchParams,
} from 'react-router-dom';
import { openSharedSessionFromDeepLink } from './sessionLinks';
import { type SharedSessionDetails } from './sharedSessions';
import { ErrorUI } from './components/ErrorBoundary';
import { ExtensionInstallModal } from './components/ExtensionInstallModal';
import { ToastContainer } from 'react-toastify';
import AnnouncementModal from './components/AnnouncementModal';
import TelemetryOptOutModal from './components/TelemetryOptOutModal';
import UpdateAvailableModal from './components/UpdateAvailableModal';
import ProviderGuard from './components/ProviderGuard';
import { createSession } from './sessions';

import { ChatType } from './types/chat';
import Hub from './components/Hub';
import Pair, { PairRouteState } from './components/Pair';
import type { UserAttachment } from './types/message';
import SettingsView, { SettingsViewOptions } from './components/settings/SettingsView';
import SessionsView from './components/sessions/SessionsView';
import SharedSessionView from './components/sessions/SharedSessionView';
import SchedulesView from './components/schedule/SchedulesView';
import ProviderSettings from './components/settings/providers/ProviderSettingsPage';
import { AppLayout } from './components/Layout/AppLayout';
import { ChatProvider } from './contexts/ChatContext';
import LauncherView from './components/LauncherView';

import 'react-toastify/dist/ReactToastify.css';
import { useConfig } from './components/ConfigContext';
import { ModelAndProviderProvider } from './components/ModelAndProviderContext';
import { ThemeProvider } from './contexts/ThemeContext';
import PermissionSettingsView from './components/settings/permission/PermissionSetting';

import ExtensionsView, { ExtensionsViewOptions } from './components/extensions/ExtensionsView';
import WorkflowsView from './components/workflows/WorkflowsView';
import SkillsView from './components/skills/SkillsView';
import KnowledgeView from './components/knowledge/KnowledgeView';
import { KnowledgeProvider } from './components/knowledge/KnowledgeContext';
import AppsView from './components/apps/AppsView';
import StandaloneAppView from './components/apps/StandaloneAppView';
import { View, ViewOptions } from './utils/navigationUtils';

import { useNavigation } from './hooks/useNavigation';
import { DashboardProvider } from './components/Dashboard/DashboardProvider';
import { DashboardRoute } from './components/Dashboard/DashboardRoute';
import { errorMessage } from './utils/conversionUtils';
import { getInitialWorkingDir } from './utils/workingDir';
import { usePageViewTracking } from './hooks/useAnalytics';
import { trackOnboardingCompleted, trackErrorWithContext } from './utils/analytics';

function PageViewTracker() {
  usePageViewTracking();
  return null;
}

// Route Components
const HubRouteWrapper = () => {
  const setView = useNavigation();
  return <Hub setView={setView} />;
};

const PairRouteWrapper = ({
  chat,
  setChat,
}: {
  chat: ChatType;
  setChat: (chat: ChatType) => void;
}) => {
  const { extensionsList } = useConfig();
  const location = useLocation();
  const navigate = useNavigate();
  const routeState = (location.state as PairRouteState) || {};
  const [searchParams] = useSearchParams();
  const [isCreatingSession, setIsCreatingSession] = useState(false);

  // Capture initialMessage in local state to survive route state being cleared
  const [capturedInitialMessage, setCapturedInitialMessage] = useState<string | undefined>(
    undefined
  );
  const [capturedInitialAttachments, setCapturedInitialAttachments] = useState<
    UserAttachment[] | undefined
  >(undefined);

  const resumeSessionId = searchParams.get('resumeSessionId') ?? undefined;
  const workflowId = searchParams.get('workflowId') ?? undefined;
  const workflowDeeplinkFromConfig = window.appConfig?.get('workflowDeeplink') as string | undefined;

  // Session ID and initialMessage come from route state (Hub, fork) or URL params (refresh, deeplink)
  const sessionIdFromState = routeState.resumeSessionId;
  const sessionId = sessionIdFromState || resumeSessionId || chat.sessionId || undefined;

  // Use route state if available, otherwise use captured state
  const initialMessage = routeState.initialMessage || capturedInitialMessage;
  const initialAttachments = routeState.initialAttachments || capturedInitialAttachments;

  // Capture initialMessage when it comes from route state
  useEffect(() => {
    console.log(
      '[PairRouteWrapper] capture effect:',
      JSON.stringify({
        routeStateInitialMessage: routeState.initialMessage,
        routeStateInitialAttachmentsCount: routeState.initialAttachments?.length,
      })
    );
    if (routeState.initialMessage) {
      setCapturedInitialMessage(routeState.initialMessage);
    }
    if (routeState.initialAttachments) {
      setCapturedInitialAttachments(routeState.initialAttachments);
    }
  }, [routeState.initialMessage, routeState.initialAttachments]);

  // Create session if we have an initialMessage, workflowId, or workflowDeeplink but no sessionId
  useEffect(() => {
    if (
      (initialMessage || workflowId || workflowDeeplinkFromConfig) &&
      !sessionId &&
      !isCreatingSession
    ) {
      setIsCreatingSession(true);

      (async () => {
        try {
          const newSession = await createSession(getInitialWorkingDir(), {
            workflowId,
            workflowDeeplink: workflowDeeplinkFromConfig,
            allExtensions: extensionsList,
          });
          navigate(`/pair?resumeSessionId=${newSession.id}`, {
            replace: true,
            state: { resumeSessionId: newSession.id, initialMessage, initialAttachments },
          });
        } catch (error) {
          console.error('Failed to create session:', error);
          trackErrorWithContext(error, {
            component: 'PairRouteWrapper',
            action: 'create_session',
            recoverable: true,
          });
        } finally {
          setIsCreatingSession(false);
        }
      })();
    }
  }, [
    initialMessage,
    workflowId,
    workflowDeeplinkFromConfig,
    sessionId,
    isCreatingSession,
    extensionsList,
    navigate,
  ]);

  // Sync URL with session ID for refresh support (only if not already in URL)
  useEffect(() => {
    if (sessionId && sessionId !== resumeSessionId) {
      navigate(`/pair?resumeSessionId=${sessionId}`, {
        replace: true,
        state: { resumeSessionId: sessionIdFromState, initialMessage, initialAttachments },
      });
    }
  }, [sessionId, resumeSessionId, navigate, sessionIdFromState, initialMessage, initialAttachments]);

  // Clear captured initialMessage when session changes (to prevent re-sending on navigation)
  useEffect(() => {
    if (sessionId && capturedInitialMessage && sessionIdFromState) {
      const timer = setTimeout(() => {
        setCapturedInitialMessage(undefined);
      }, 100);
      return () => clearTimeout(timer);
    }
    return undefined;
  }, [sessionId, capturedInitialMessage, sessionIdFromState]);

  return (
    <Pair
      key={sessionId}
      setChat={setChat}
      sessionId={sessionId ?? ''}
      initialMessage={initialMessage}
      initialAttachments={initialAttachments}
    />
  );
};

const SettingsRoute = () => {
  const location = useLocation();
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const setView = useNavigation();

  // Get viewOptions from location.state, history.state, or URL search params
  const viewOptions =
    (location.state as SettingsViewOptions) || (window.history.state as SettingsViewOptions) || {};

  // If section is provided via URL search params, add it to viewOptions
  const sectionFromUrl = searchParams.get('section');
  if (sectionFromUrl) {
    viewOptions.section = sectionFromUrl;
  }

  return <SettingsView onClose={() => navigate('/')} setView={setView} viewOptions={viewOptions} />;
};

const SessionsRoute = () => {
  return <SessionsView />;
};

const SchedulesRoute = () => {
  const navigate = useNavigate();
  return <SchedulesView onClose={() => navigate('/')} />;
};

const WorkflowsRoute = () => {
  return <WorkflowsView />;
};

const SkillsRoute = () => <SkillsView />;
const KnowledgeRoute = () => <KnowledgeView />;

const PermissionRoute = () => {
  const location = useLocation();
  const navigate = useNavigate();
  const parentView = location.state?.parentView as View;
  const parentViewOptions = location.state?.parentViewOptions as ViewOptions;

  return (
    <PermissionSettingsView
      onClose={() => {
        // Navigate back to parent view with options
        switch (parentView) {
          case 'chat':
            navigate('/');
            break;
          case 'pair':
            navigate('/pair');
            break;
          case 'settings':
            navigate('/settings', { state: parentViewOptions });
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
          default:
            navigate('/');
        }
      }}
    />
  );
};

const ConfigureProvidersRoute = () => {
  const navigate = useNavigate();

  return (
    <div className="w-screen h-screen bg-background-default">
      <ProviderSettings
        onClose={() => navigate('/settings', { state: { section: 'models' } })}
        isOnboarding={false}
      />
    </div>
  );
};

interface WelcomeRouteProps {
  onSelectProvider: () => void;
}

const WelcomeRoute = ({ onSelectProvider }: WelcomeRouteProps) => {
  const navigate = useNavigate();

  return (
    <div className="w-screen h-screen bg-background-default">
      <ProviderSettings
        onClose={() => {
          navigate('/', { replace: true });
        }}
        isOnboarding={true}
        onProviderLaunched={(model?: string) => {
          trackOnboardingCompleted('other', model);
          onSelectProvider();
          navigate('/', { replace: true });
        }}
      />
    </div>
  );
};

// Wrapper component for SharedSessionRoute to access parent state
const SharedSessionRouteWrapper = ({
  isLoadingSharedSession,
  setIsLoadingSharedSession,
  sharedSessionError,
}: {
  isLoadingSharedSession: boolean;
  setIsLoadingSharedSession: (loading: boolean) => void;
  sharedSessionError: string | null;
}) => {
  const location = useLocation();
  const setView = useNavigation();

  const historyState = window.history.state;
  const sessionDetails = (location.state?.sessionDetails ||
    historyState?.sessionDetails) as SharedSessionDetails | null;
  const error = location.state?.error || historyState?.error || sharedSessionError;
  const shareToken = location.state?.shareToken || historyState?.shareToken;
  const baseUrl = location.state?.baseUrl || historyState?.baseUrl;

  return (
    <SharedSessionView
      session={sessionDetails}
      isLoading={isLoadingSharedSession}
      error={error}
      onRetry={async () => {
        if (shareToken && baseUrl) {
          setIsLoadingSharedSession(true);
          try {
            await openSharedSessionFromDeepLink(`biorouter://sessions/${shareToken}`, setView, baseUrl);
          } catch (error) {
            console.error('Failed to retry loading shared session:', error);
          } finally {
            setIsLoadingSharedSession(false);
          }
        }
      }}
    />
  );
};

const ExtensionsRoute = () => {
  const navigate = useNavigate();
  const location = useLocation();

  // Get viewOptions from location.state or history.state (for deep link extensions)
  const viewOptions =
    (location.state as ExtensionsViewOptions) ||
    (window.history.state as ExtensionsViewOptions) ||
    {};

  return (
    <ExtensionsView
      onClose={() => navigate(-1)}
      setView={(view, options) => {
        switch (view) {
          case 'chat':
            navigate('/');
            break;
          case 'pair':
            navigate('/pair', { state: options });
            break;
          case 'settings':
            navigate('/settings', { state: options });
            break;
          default:
            navigate('/');
        }
      }}
      viewOptions={viewOptions}
    />
  );
};

export function AppInner() {
  const [fatalError, setFatalError] = useState<string | null>(null);
  const [isLoadingSharedSession, setIsLoadingSharedSession] = useState(false);
  const [sharedSessionError, setSharedSessionError] = useState<string | null>(null);
  const [didSelectProvider, setDidSelectProvider] = useState<boolean>(false);

  const navigate = useNavigate();
  const setView = useNavigation();

  const [chat, setChat] = useState<ChatType>({
    sessionId: '',
    name: 'Pair Chat',
    messages: [],
    workflow: null,
  });

  const { addExtension } = useConfig();

  useEffect(() => {
    console.log('Sending reactReady signal to Electron');
    try {
      window.electron.reactReady();
    } catch (error) {
      console.error('Error sending reactReady:', error);
      setFatalError(
        `React ready notification failed: ${error instanceof Error ? error.message : 'Unknown error'}`
      );
    }
  }, []);

  useEffect(() => {
    const handleOpenSharedSession = async (_event: IpcRendererEvent, ...args: unknown[]) => {
      const link = args[0] as string;
      window.electron.logInfo(`Opening shared session from deep link ${link}`);
      setIsLoadingSharedSession(true);
      setSharedSessionError(null);
      try {
        await openSharedSessionFromDeepLink(link, (_view: View, options?: ViewOptions) => {
          navigate('/shared-session', { state: options });
        });
      } catch (error) {
        console.error('Unexpected error opening shared session:', error);
        trackErrorWithContext(error, {
          component: 'AppInner',
          action: 'open_shared_session',
          recoverable: true,
        });
        // Navigate to shared session view with error
        const shareToken = link.replace('biorouter://sessions/', '');
        const options = {
          sessionDetails: null,
          error: error instanceof Error ? error.message : 'Unknown error',
          shareToken,
        };
        navigate('/shared-session', { state: options });
      } finally {
        setIsLoadingSharedSession(false);
      }
    };
    return window.electron.on('open-shared-session', handleOpenSharedSession);
  }, [navigate]);

  useEffect(() => {
    console.log('Setting up keyboard shortcuts');
    const handleKeyDown = (event: KeyboardEvent) => {
      const isMac = window.electron.platform === 'darwin';
      if ((isMac ? event.metaKey : event.ctrlKey) && event.key === 'n') {
        event.preventDefault();
        try {
          window.electron.createChatWindow(undefined, getInitialWorkingDir());
        } catch (error) {
          console.error('Error creating new window:', error);
        }
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => {
      window.removeEventListener('keydown', handleKeyDown);
    };
  }, []);

  // Prevent default drag and drop behavior globally to avoid opening files in new windows
  // but allow our React components to handle drops in designated areas
  useEffect(() => {
    const preventDefaults = (e: globalThis.DragEvent) => {
      // Only prevent default if we're not over a designated drop zone
      const target = e.target as HTMLElement;
      const isOverDropZone = target.closest('[data-drop-zone="true"]') !== null;

      if (!isOverDropZone) {
        e.preventDefault();
        e.stopPropagation();
      }
    };

    const handleDragOver = (e: globalThis.DragEvent) => {
      // Always prevent default for dragover to allow dropping
      e.preventDefault();
      e.stopPropagation();
    };

    const handleDrop = (e: globalThis.DragEvent) => {
      // Only prevent default if we're not over a designated drop zone
      const target = e.target as HTMLElement;
      const isOverDropZone = target.closest('[data-drop-zone="true"]') !== null;

      if (!isOverDropZone) {
        e.preventDefault();
        e.stopPropagation();
      }
    };

    // Add event listeners to document to catch drag events
    document.addEventListener('dragenter', preventDefaults, false);
    document.addEventListener('dragleave', preventDefaults, false);
    document.addEventListener('dragover', handleDragOver, false);
    document.addEventListener('drop', handleDrop, false);

    return () => {
      document.removeEventListener('dragenter', preventDefaults, false);
      document.removeEventListener('dragleave', preventDefaults, false);
      document.removeEventListener('dragover', handleDragOver, false);
      document.removeEventListener('drop', handleDrop, false);
    };
  }, []);

  useEffect(() => {
    const handleFatalError = (_event: IpcRendererEvent, ...args: unknown[]) => {
      const errorMessage = args[0] as string;
      console.error('Encountered a fatal error:', errorMessage);
      setFatalError(errorMessage);
    };
    return window.electron.on('fatal-error', handleFatalError);
  }, []);

  useEffect(() => {
    const handleOpenBrxtFile = (_event: IpcRendererEvent, ...args: unknown[]) => {
      const filePath = args[0];
      if (typeof filePath !== 'string') return;
      navigate('/extensions', { state: { brxtFilePath: filePath } });
    };
    return window.electron.on('open-brxt-file', handleOpenBrxtFile);
  }, [navigate]);

  useEffect(() => {
    const handleSetView = (_event: IpcRendererEvent, ...args: unknown[]) => {
      const newView = args[0] as View;
      const section = args[1] as string | undefined;
      console.log(
        `Received view change request to: ${newView}${section ? `, section: ${section}` : ''}`
      );

      if (section && newView === 'settings') {
        navigate(`/settings?section=${section}`);
      } else {
        navigate(`/${newView}`);
      }
    };

    return window.electron.on('set-view', handleSetView);
  }, [navigate]);

  useEffect(() => {
    const handleFocusInput = (_event: IpcRendererEvent, ..._args: unknown[]) => {
      const inputField = document.querySelector('input[type="text"], textarea') as HTMLInputElement;
      if (inputField) {
        inputField.focus();
      }
    };
    return window.electron.on('focus-input', handleFocusInput);
  }, []);

  // Handle initial message from launcher
  useEffect(() => {
    const handleSetInitialMessage = async (_event: IpcRendererEvent, ...args: unknown[]) => {
      const initialMessage = args[0] as string;
      if (initialMessage) {
        console.log('Received initial message from launcher:', initialMessage);
        try {
          const session = await createSession(getInitialWorkingDir(), {});
          navigate('/pair', {
            state: {
              initialMessage,
              resumeSessionId: session.id,
            },
          });
        } catch (error) {
          console.error('Failed to create session for launcher message:', error);
        }
      }
    };
    return window.electron.on('set-initial-message', handleSetInitialMessage);
  }, [navigate]);

  if (fatalError) {
    return <ErrorUI error={errorMessage(fatalError)} />;
  }

  return (
    <>
      <PageViewTracker />
      <ToastContainer
        aria-label="Toast notifications"
        toastClassName={() =>
          `relative min-h-16 mb-4 p-3 rounded-xl
               flex justify-between overflow-hidden cursor-pointer
               text-white bg-neutral-800/95 backdrop-blur-md
               border border-white/10 shadow-lg
              `
        }
        style={{ width: '450px' }}
        className="mt-6"
        position="top-right"
        autoClose={3000}
        closeOnClick
        pauseOnHover
      />
      <ExtensionInstallModal addExtension={addExtension} setView={setView} />
      <div className="relative w-screen h-screen overflow-hidden bg-background-muted flex flex-col">
        <div className="titlebar-drag-region" />
        <DashboardProvider>
          <KnowledgeProvider sessionId={chat.sessionId || null}>
            <Routes>
            <Route path="launcher" element={<LauncherView />} />
            <Route
              path="welcome"
              element={<WelcomeRoute onSelectProvider={() => setDidSelectProvider(true)} />}
            />
            <Route path="configure-providers" element={<ConfigureProvidersRoute />} />
            <Route path="standalone-app" element={<StandaloneAppView />} />
            <Route
              path="/"
              element={
                <ProviderGuard didSelectProvider={didSelectProvider}>
                  <ChatProvider chat={chat} setChat={setChat} contextKey="hub">
                    <AppLayout />
                  </ChatProvider>
                </ProviderGuard>
              }
            >
              <Route index element={<HubRouteWrapper />} />
              <Route path="pair" element={<PairRouteWrapper chat={chat} setChat={setChat} />} />
              <Route path="settings" element={<SettingsRoute />} />
              <Route
                path="extensions"
                element={
                  <ChatProvider chat={chat} setChat={setChat} contextKey="extensions">
                    <ExtensionsRoute />
                  </ChatProvider>
                }
              />
              <Route path="apps" element={<AppsView />} />
              <Route path="sessions" element={<SessionsRoute />} />
              <Route path="schedules" element={<SchedulesRoute />} />
              <Route path="workflows" element={<WorkflowsRoute />} />
              <Route path="skills" element={<SkillsRoute />} />
              <Route path="knowledge" element={<KnowledgeRoute />} />
              <Route path="dashboard" element={<DashboardRoute />} />
              <Route
                path="shared-session"
                element={
                  <SharedSessionRouteWrapper
                    isLoadingSharedSession={isLoadingSharedSession}
                    setIsLoadingSharedSession={setIsLoadingSharedSession}
                    sharedSessionError={sharedSessionError}
                  />
                }
              />
              <Route path="permission" element={<PermissionRoute />} />
            </Route>
            </Routes>
          </KnowledgeProvider>
        </DashboardProvider>
      </div>
    </>
  );
}

export default function App() {
  return (
    <ThemeProvider>
      <ModelAndProviderProvider>
        <HashRouter>
          <AppInner />
        </HashRouter>
        <AnnouncementModal />
        <TelemetryOptOutModal controlled={false} />
        <UpdateAvailableModal />
      </ModelAndProviderProvider>
    </ThemeProvider>
  );
}
