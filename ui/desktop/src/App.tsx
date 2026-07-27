import { type ReactNode, useEffect, useState, useRef } from 'react';
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
import { TOAST_SURFACE_CLASS_NAME } from './components/alerts/NotificationSurface';
import { ExtensionInstallModal } from './components/ExtensionInstallModal';
import { ToastContainer } from 'react-toastify';
import AnnouncementModal from './components/AnnouncementModal';
import UpdateAvailableModal from './components/UpdateAvailableModal';
import ProviderGuard from './components/ProviderGuard';
import { createSession } from './sessions';

import { ChatType } from './types/chat';
import Hub from './components/Hub';
import { PairRouteState } from './components/Pair';
import { ChatGroupsProvider, useChatGroups } from './contexts/ChatGroupsContext';
import { requestNewTab } from './components/chatGroups/newTabRegistry';
import { runCloseActiveTabCommand } from './utils/closeActiveTabCommand';
import { isTerminalFocused, requestNewTerminalPane } from './utils/terminalFocus';
import { TerminalDockProvider } from './contexts/TerminalDockContext';
import ChatGroupsShell from './components/chatGroups/ChatGroupsShell';
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
import ApplicationsView from './components/applications/ApplicationsView';
import { View, ViewOptions } from './utils/navigationUtils';

import { useNavigation } from './hooks/useNavigation';
import { errorMessage } from './utils/conversionUtils';
import { getInitialWorkingDir } from './utils/workingDir';
import { ChatStreamProvider } from './hooks/chatStreamStore';
import { AppTooltipLayer } from './components/ui/AppTooltipLayer';

// Route Components
const HubRouteWrapper = () => {
  const setView = useNavigation();
  return <Hub setView={setView} />;
};

/**
 * TerminalDockProvider wraps /pair ONLY.
 *
 * It holds the PER-CHAT-TAB terminals, keyed by tab id, above the tab switch so
 * a tab's shell survives switching away from it (BaseChat, keyed by tab id,
 * remounts on every switch). Its scope is deliberately this route and no wider:
 * every other surface (/extensions' mini-chat,
 * the Hub) has no provider, so useTerminalDock() returns null there and BaseChat
 * keeps its own local per-chat dock exactly as it does today. Hoisting this to
 * the app root would let those surfaces share terminal state across windows.
 */
const PairRouteWrapper = ({ setChat }: { setChat: (chat: ChatType) => void }) => (
  <ChatGroupsProvider>
    <TerminalDockProvider>
      <PairRouteContent setChat={setChat} />
    </TerminalDockProvider>
  </ChatGroupsProvider>
);

/**
 * The URL adapter, and nothing else.
 *
 * This used to own /pair's session identity (and mirror it into the URL from an
 * effect of its own). ChatGroupsProvider is now the ONLY writer of
 * ?resumeSessionId= — two writers is precisely the mutual recursion R2 warns
 * about — so the old sync effect here is deleted. What remains are the entry
 * points that must create a session BEFORE anything can be opened: the Hub
 * composer's initialMessage, a workflow deeplink, and the sidebar's new-chat.
 * Each of them lands back here as a normal ?resumeSessionId= navigation, which
 * the provider consumes exactly once.
 */
const PairRouteContent = ({ setChat }: { setChat: (chat: ChatType) => void }) => {
  const { extensionsList } = useConfig();
  const location = useLocation();
  const navigate = useNavigate();
  const groups = useChatGroups();
  const routeState = (location.state as PairRouteState) || {};
  const [searchParams] = useSearchParams();
  const [isCreatingSession, setIsCreatingSession] = useState(false);

  const resumeSessionId = searchParams.get('resumeSessionId') ?? undefined;
  const workflowId = searchParams.get('workflowId') ?? undefined;
  const workflowDeeplinkFromConfig = window.appConfig?.get('workflowDeeplink') as
    | string
    | undefined;
  const isNewChat = routeState.newChat === true;

  const sessionIdFromState = routeState.resumeSessionId;
  // Identity comes from the URL and the route state only. The old
  // `|| chat.sessionId` fallback reached into the App-level singleton, which is
  // no longer /pair's identity: the focused tab is.
  const sessionId = isNewChat ? undefined : sessionIdFromState || resumeSessionId || undefined;
  const initialMessage = isNewChat ? undefined : routeState.initialMessage;
  const initialAttachments = isNewChat ? undefined : routeState.initialAttachments;

  const dispatch = groups?.dispatch;

  // The sidebar's new-chat button: open ONE empty tab per navigation. The tab
  // carries sessionId '' until BaseChat's pre-session submit creates a real
  // session; that navigation then ADOPTS this tab in place (see the reducer's
  // empty-tab branch) rather than orphaning it beside a second one.
  const newChatKeyRef = useRef<string | null>(null);
  useEffect(() => {
    if (!isNewChat || !dispatch) return;
    if (newChatKeyRef.current === location.key) return;
    newChatKeyRef.current = location.key;
    dispatch({ type: 'openTab', payload: { sessionId: '' } });
  }, [isNewChat, location.key, dispatch]);

  // Create a session when we have something to say but nothing to say it in.
  useEffect(() => {
    if (
      !isNewChat &&
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
        } finally {
          setIsCreatingSession(false);
        }
      })();
    }
  }, [
    initialMessage,
    initialAttachments,
    isNewChat,
    workflowId,
    workflowDeeplinkFromConfig,
    sessionId,
    isCreatingSession,
    extensionsList,
    navigate,
  ]);

  return <ChatGroupsShell onChatChange={setChat} />;
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
        onProviderLaunched={() => {
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
            await openSharedSessionFromDeepLink(
              `biorouter://sessions/${shareToken}`,
              setView,
              baseUrl
            );
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

  // Cmd+W (Ctrl+W off mac). Sent by the File menu's "Close Tab" item — see
  // main.ts, where the accelerator had to be taken off `role: 'close'` so it
  // stops closing the whole window.
  //
  // This listener lives at the ROOT, not in ChatGroupsProvider, because it must
  // answer everywhere: the provider is mounted only under /pair, and Cmd+W on
  // Settings must still close the window like any other macOS app. The ladder —
  // focused terminal pane, then chat tab, then window — lives in
  // runCloseActiveTabCommand so the registry-driven tests gate the exact code
  // this handler runs (the keystroke never reaches the DOM; the menu owns it).
  //
  // No text-input guard: Cmd+W has no native editing behaviour to steal, and the
  // key never reaches the DOM anyway — the menu consumes it.
  useEffect(
    () =>
      window.electron.on('close-active-tab', () => {
        runCloseActiveTabCommand(() => window.electron.closeWindow());
      }),
    []
  );

  // Cmd+T / Ctrl+T — a new tab. Sent by the Go menu's "New Chat" item; like
  // Cmd+W it can only be a menu item, because the menu already owned the key and
  // would have eaten any renderer listener.
  //
  // What "new tab" MEANS is focus-aware: when the cursor is in the in-app
  // terminal, Cmd+T adds a new terminal PANE (the reflex a terminal user has);
  // otherwise it opens a new CHAT tab. isTerminalFocused only sees the visible
  // dock (a hidden one is display:none and cannot hold focus), and
  // requestNewTerminalPane returns false when no terminal is open — either way
  // we fall through to the chat path, so a chat-focused Cmd+T is unchanged.
  //
  // The chat path keeps the same root-level reasoning as Cmd+W: the tab surface
  // is mounted only under /pair, so off that route requestNewTab() finds no
  // handler. It then REMEMBERS the request and we navigate — the mounting
  // provider consumes it and opens the tab. Cmd+T on Settings therefore lands
  // you on a fresh chat, as the key does in a browser from any page.
  useEffect(
    () =>
      window.electron.on('new-chat-tab', () => {
        if (isTerminalFocused() && requestNewTerminalPane()) return;
        if (requestNewTab()) return;
        navigate('/pair');
      }),
    [navigate]
  );

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
      <AppTooltipLayer />
      <ToastContainer
        aria-label="Toast notifications"
        toastClassName={() => TOAST_SURFACE_CLASS_NAME}
        style={{
          width: 'fit-content',
          minWidth: '280px',
          maxWidth: 'min(420px, calc(100vw - 32px))',
        }}
        className="mt-6"
        position="top-right"
        autoClose={3000}
        closeOnClick
        pauseOnHover
      />
      <ExtensionInstallModal addExtension={addExtension} setView={setView} />
      <div className="relative w-screen h-screen overflow-hidden bg-background-canvas flex flex-col">
        <div className="titlebar-drag-region" />
        <ChatStreamProvider>
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
                <Route path="pair" element={<PairRouteWrapper setChat={setChat} />} />
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
                <Route path="applications" element={<ApplicationsView />} />
                <Route path="sessions" element={<SessionsRoute />} />
                <Route path="schedules" element={<SchedulesRoute />} />
                <Route path="workflows" element={<WorkflowsRoute />} />
                <Route path="skills" element={<SkillsRoute />} />
                <Route path="knowledge" element={<KnowledgeRoute />} />
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
        </ChatStreamProvider>
      </div>
    </>
  );
}

export function ImmediateHashRouter({ children }: { children: ReactNode }) {
  return <HashRouter useTransitions={false}>{children}</HashRouter>;
}

export default function App() {
  return (
    <ThemeProvider>
      <ModelAndProviderProvider>
        <ImmediateHashRouter>
          <AppInner />
        </ImmediateHashRouter>
        <AnnouncementModal />
        <UpdateAvailableModal />
      </ModelAndProviderProvider>
    </ThemeProvider>
  );
}
