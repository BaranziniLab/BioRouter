import React, {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import { useLocation, useNavigate, useSearchParams } from 'react-router-dom';
import { SearchView } from './conversation/SearchView';
import LoadingBioRouter from './LoadingBioRouter';
import ProgressiveMessageList from './ProgressiveMessageList';
import { MainPanelLayout } from './Layout/MainPanelLayout';
import ChatInput from './ChatInput';
import { ScrollArea, ScrollAreaHandle } from './ui/scroll-area';
import { useFileDrop } from '../hooks/useFileDrop';
import { Message } from '../api';
import type { UserAttachment } from '../types/message';
import { getProviderMetadata } from './settings/models/modelInterface';
import { ChatState } from '../types/chatState';
import { ChatType } from '../types/chat';
import { useIsMobile } from '../hooks/use-mobile';
import { useSidebar } from './ui/sidebar';
import { cn } from '../utils';
import { useChatStream } from '../hooks/useChatStream';
import { useNavigation } from '../hooks/useNavigation';
import { WorkflowHeader } from './WorkflowHeader';
import { WorkflowWarningModal } from './ui/WorkflowWarningModal';
import { scanWorkflow } from '../workflow';
import { useCostTracking } from '../hooks/useCostTracking';
import WorkflowActivities from './workflows/WorkflowActivities';
import { useToolCount } from './alerts/useToolCount';
import { getThinkingMessage, getTextContent } from '../types/message';
import ParameterInputModal from './ParameterInputModal';
import { substituteParameters } from '../utils/providerUtils';
import CreateWorkflowFromSessionModal from './workflows/CreateWorkflowFromSessionModal';
import { toastSuccess } from '../toasts';
import { Workflow } from '../workflow';
import { createSession } from '../sessions';
import { getInitialWorkingDir } from '../utils/workingDir';
import { useConfig } from './ConfigContext';
import { SessionNamePill } from './Dashboard/SessionNamePill';
import { announceSessionName, renameSession } from '../utils/sessionNameSync';
import { toastError } from '../toasts';
import { errorMessage } from '../utils/conversionUtils';
import { Greeting } from './common/Greeting';
import { navigateWithViewTransition } from '../utils/navigationUtils';

// Context for sharing current model info
const CurrentModelContext = createContext<{ model: string; mode: string } | null>(null);
export const useCurrentModelInfo = () => useContext(CurrentModelContext);

interface BaseChatProps {
  setChat: (chat: ChatType) => void;
  onMessageSubmit?: (message: string) => void;
  renderHeader?: () => React.ReactNode;
  customChatInputProps?: Record<string, unknown>;
  customMainLayoutProps?: Record<string, unknown>;
  contentClassName?: string;
  disableSearch?: boolean;
  showPopularTopics?: boolean;
  suppressEmptyState: boolean;
  sessionId: string;
  initialMessage?: string;
  initialAttachments?: UserAttachment[];
  /** Render messages + input as a single coherent surface (default true). */
  coherent?: boolean;
  /** Optional: overrides the default rename behavior (which calls biorouterd updateSessionName). */
  onRenameSession?: (newName: string) => void;
  /** Notify parent when the underlying session object changes (e.g., biorouterd renamed it). */
  onSessionUpdate?: (session: { id: string; name: string; userSetName: boolean } | null) => void;
  /** Optional accent dot color (dashboard windows pass theirs). */
  accentColor?: string;
  /** Hide the SessionNamePill at the top of the chat. Dashboard windows pass this
   * because their own WindowTitleBar already shows the editable name. */
  hideSessionNamePill?: boolean;
  /** When true, the ChatInput's secondary picker controls (cost, model, mode,
   * workflow, diagnostics) live behind a chevron popover. When false (chat
   * tab default), they render inline. Dashboard windows pass true. */
  compactPicker?: boolean;
  /** Fires when the inner chat transitions between idle and any non-idle state
   * (streaming, thinking, tool-running, etc.). Used by DashboardContext to
   * drive the per-window busy indicator on folded cards. */
  onBusyChange?: (busy: boolean) => void;
  /** Fires whenever the last assistant message text changes (including
   * mid-stream). Receives a tail-truncated string for hover previews on
   * dashboard folded cards. Null when there's no assistant message yet. */
  onLatestMessage?: (text: string | null) => void;
  /** Monotonically increments when the parent wants the chat input refocused
   * and the conversation scrolled back to the bottom. Used by dashboard
   * ChatWindow on unfold, because BaseChat stays mounted while folded so
   * mount-time autofocus / auto-scroll never re-fire on visibility change. */
  focusTrigger?: number;
}

function BaseChatContent({
  setChat,
  renderHeader,
  customChatInputProps = {},
  customMainLayoutProps = {},
  sessionId,
  initialMessage,
  initialAttachments,
  coherent = true,
  onRenameSession,
  onSessionUpdate,
  accentColor,
  hideSessionNamePill = false,
  compactPicker = false,
  onBusyChange,
  onLatestMessage,
  focusTrigger,
}: BaseChatProps) {
  const location = useLocation();
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const scrollRef = useRef<ScrollAreaHandle>(null);
  const { extensionsList, getProviders } = useConfig();
  // Per-session vision capability. The global ModelAndProviderContext tracks
  // the user's default model, but each chat session (especially in dashboard
  // mode) can be bound to a different provider/model. Look up vision support
  // against the session's own provider/model so attach gating reflects what
  // the session will actually use.
  const [sessionSupportsVision, setSessionSupportsVision] = React.useState<boolean | null>(null);

  const disableAnimation = location.state?.disableAnimation || false;
  const [hasStartedUsingWorkflow, setHasStartedUsingWorkflow] = React.useState(false);
  const [hasNotAcceptedWorkflow, setHasNotAcceptedWorkflow] = useState<boolean>();
  const [hasWorkflowSecurityWarnings, setHasWorkflowSecurityWarnings] = useState(false);
  const [isCreatingSession, setIsCreatingSession] = useState(false);
  const composerMotionRef = useRef<HTMLDivElement>(null);
  const pendingComposerRectRef = useRef<DOMRect | null>(null);

  const isMobile = useIsMobile();
  const { state: sidebarState } = useSidebar();
  const setView = useNavigation();

  const contentClassName = cn('pr-1 pb-10', (isMobile || sidebarState === 'collapsed') && 'pt-11');

  // Use shared file drop
  const { droppedFiles, setDroppedFiles, handleDrop, handleDragOver } = useFileDrop();

  const onStreamFinish = useCallback(() => {}, []);

  const [isCreateWorkflowModalOpen, setIsCreateWorkflowModalOpen] = useState(false);
  const hasAutoSubmittedRef = useRef(false);

  // Reset auto-submit flag when session changes
  useEffect(() => {
    hasAutoSubmittedRef.current = false;
  }, [sessionId]);

  const {
    session,
    messages,
    chatState,
    setChatState,
    handleSubmit,
    submitElicitationResponse,
    stopStreaming,
    sessionLoadError,
    setWorkflowUserParams,
    tokenState,
    notifications: toolCallNotifications,
    onMessageUpdate,
  } = useChatStream({
    sessionId,
    onStreamFinish,
  });

  const stageComposerMotion = useCallback(() => {
    const rect = composerMotionRef.current?.getBoundingClientRect();
    if (rect) {
      pendingComposerRectRef.current = rect;
    }
  }, []);

  // Pipe chatState transitions to the parent (dashboard window). Busy = any
  // non-idle state (Thinking, Streaming, WaitingForUserInput, Compacting, etc.).
  // ChatState.LoadingConversation counts as busy too — the session is still
  // resolving and the user should see that as activity.
  useEffect(() => {
    if (!onBusyChange) return;
    onBusyChange(chatState !== ChatState.Idle);
  }, [chatState, onBusyChange]);

  // Propagate a tail of the most recent assistant message to the parent so
  // dashboard folded cards can show a hover preview. Updates on every message
  // mutation — including mid-stream — so the user can watch the AI work
  // through the popup without expanding the card.
  useEffect(() => {
    if (!onLatestMessage) return;
    const lastAssistant = [...messages].reverse().find((m) => m.role === 'assistant');
    if (!lastAssistant) {
      onLatestMessage(null);
      return;
    }
    const text = getTextContent(lastAssistant).trim();
    if (!text) {
      onLatestMessage(null);
      return;
    }
    // Keep the tail short — the popup is ~6 lines of small text.
    const TAIL = 220;
    const tail = text.length > TAIL ? '…' + text.slice(-TAIL) : text;
    onLatestMessage(tail);
  }, [messages, onLatestMessage]);

  // Generate command history from user messages (most recent first)
  const commandHistory = useMemo(() => {
    return messages
      .reduce<string[]>((history, message) => {
        if (message.role === 'user') {
          const text = getTextContent(message).trim();
          if (text) {
            history.push(text);
          }
        }
        return history;
      }, [])
      .reverse();
  }, [messages]);

  useEffect(() => {
    if (!session || hasAutoSubmittedRef.current) {
      return;
    }

    const shouldStartAgent = searchParams.get('shouldStartAgent') === 'true';

    if (initialMessage) {
      hasAutoSubmittedRef.current = true;
      handleSubmit(initialMessage, initialAttachments);
      // Clear initialMessage + attachments from navigation state to prevent re-sending on refresh
      navigate(location.pathname + location.search, {
        replace: true,
        state: { ...location.state, initialMessage: undefined, initialAttachments: undefined },
      });
    } else if (shouldStartAgent) {
      hasAutoSubmittedRef.current = true;
      handleSubmit('');
    }
  }, [session, initialMessage, searchParams, handleSubmit, navigate, location]);

  // Resolve session-scoped vision capability whenever the session's bound
  // provider or model changes. Falls back to null (== use global context) if
  // either piece isn't loaded yet.
  React.useEffect(() => {
    const sessionProvider = session?.provider_name;
    const sessionModel = session?.model_config?.model_name;
    if (!sessionProvider || !sessionModel) {
      setSessionSupportsVision(null);
      return;
    }
    let cancelled = false;
    (async () => {
      try {
        const metadata = await getProviderMetadata(sessionProvider, getProviders);
        const info = metadata.known_models.find((m) => m.name === sessionModel);
        if (!cancelled) setSessionSupportsVision(info?.supports_vision === true);
      } catch {
        if (!cancelled) setSessionSupportsVision(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [session?.provider_name, session?.model_config?.model_name, getProviders]);

  const handleFormSubmit = async (e: React.FormEvent) => {
    const customEvent = e as unknown as CustomEvent;
    const textValue = customEvent.detail?.value || '';
    const attachments = customEvent.detail?.attachments ?? [];
    if (textValue.trim() || (Array.isArray(attachments) && attachments.length > 0)) {
      stageComposerMotion();
    }

    // If no session exists, create one and navigate with the initial message
    if (!session && !sessionId && textValue.trim() && !isCreatingSession) {
      setIsCreatingSession(true);
      try {
        const newSession = await createSession(getInitialWorkingDir(), {
          allExtensions: extensionsList,
        });
        navigateWithViewTransition(
          navigate,
          `/pair?resumeSessionId=${newSession.id}`,
          {
            resumeSessionId: newSession.id,
            initialMessage: textValue,
            initialAttachments: attachments,
          },
          { replace: true }
        );
      } catch {
        setIsCreatingSession(false);
      }
      return;
    }

    if (workflow && textValue.trim()) {
      setHasStartedUsingWorkflow(true);
    }
    handleSubmit(textValue, attachments);
  };

  const { sessionCosts } = useCostTracking({
    sessionInputTokens: session?.accumulated_input_tokens || 0,
    sessionOutputTokens: session?.accumulated_output_tokens || 0,
    localInputTokens: 0,
    localOutputTokens: 0,
    session,
  });

  const workflow = session?.workflow;

  useEffect(() => {
    if (!workflow) return;

    (async () => {
      const accepted = await window.electron.hasAcceptedWorkflowBefore(workflow);
      setHasNotAcceptedWorkflow(!accepted);

      if (!accepted) {
        const scanResult = await scanWorkflow(workflow);
        setHasWorkflowSecurityWarnings(scanResult.has_security_warnings);
      }
    })();
  }, [workflow]);

  const handleWorkflowAccept = async (accept: boolean) => {
    if (workflow && accept) {
      await window.electron.recordWorkflowHash(workflow);
      setHasNotAcceptedWorkflow(false);
    } else {
      setView('chat');
    }
  };

  // Track if this is the initial render for session resuming
  const initialRenderRef = useRef(true);

  // Auto-scroll when messages are loaded (for session resuming)
  const handleRenderingComplete = React.useCallback(() => {
    // Only force scroll on the very first render
    if (initialRenderRef.current && messages.length > 0) {
      initialRenderRef.current = false;
      if (scrollRef.current?.scrollToBottom) {
        scrollRef.current.scrollToBottom();
      }
    } else if (scrollRef.current?.isFollowing) {
      if (scrollRef.current?.scrollToBottom) {
        scrollRef.current.scrollToBottom();
      }
    }
  }, [messages.length]);

  const toolCount = useToolCount(sessionId);

  // Listen for global scroll-to-bottom requests (e.g., from MCP UI prompt actions)
  useEffect(() => {
    const handleGlobalScrollRequest = () => {
      // Add a small delay to ensure content has been rendered
      setTimeout(() => {
        if (scrollRef.current?.scrollToBottom) {
          scrollRef.current.scrollToBottom();
        }
      }, 200);
    };

    window.addEventListener('scroll-chat-to-bottom', handleGlobalScrollRequest);
    return () => window.removeEventListener('scroll-chat-to-bottom', handleGlobalScrollRequest);
  }, []);

  // When the parent bumps focusTrigger (e.g. dashboard card unfolded), wait
  // one animation frame for the display:none→display:flex transition to
  // settle, then scroll the chat to bottom and ask ChatInput to refocus.
  // BaseChat stays mounted while folded, so its mount-time autofocus and
  // auto-scroll-to-bottom never re-fire — without this hook the input would
  // stay unfocused and the conversation would stay at its pre-fold scroll
  // position (often leaving the input row visually cut off).
  useEffect(() => {
    if (!focusTrigger) return;
    const raf = requestAnimationFrame(() => {
      scrollRef.current?.scrollToBottom?.();
      window.dispatchEvent(new CustomEvent('focus-chat-input', { detail: { sessionId } }));
    });
    return () => cancelAnimationFrame(raf);
  }, [focusTrigger, sessionId]);

  useEffect(() => {
    const handleMakeAgent = () => {
      setIsCreateWorkflowModalOpen(true);
    };

    window.addEventListener('make-agent-from-chat', handleMakeAgent);
    return () => window.removeEventListener('make-agent-from-chat', handleMakeAgent);
  }, []);

  useEffect(() => {
    const handleSessionForked = (event: Event) => {
      const customEvent = event as CustomEvent<{
        newSessionId: string;
        shouldStartAgent?: boolean;
        editedMessage?: string;
      }>;
      const { newSessionId, shouldStartAgent, editedMessage } = customEvent.detail;

      const params = new URLSearchParams();
      params.set('resumeSessionId', newSessionId);
      if (shouldStartAgent) {
        params.set('shouldStartAgent', 'true');
      }

      navigate(`/pair?${params.toString()}`, {
        state: {
          disableAnimation: true,
          initialMessage: editedMessage,
        },
      });
    };

    window.addEventListener('session-forked', handleSessionForked);

    return () => {
      window.removeEventListener('session-forked', handleSessionForked);
    };
  }, [location.pathname, navigate]);

  const handleWorkflowCreated = (workflow: Workflow) => {
    toastSuccess({
      title: 'Workflow created successfully!',
      msg: `"${workflow.title}" has been saved and is ready to use.`,
    });
  };

  const chat: ChatType = {
    messages,
    workflow,
    sessionId,
    name: session?.name || 'No Session',
  };

  // Update the global chat context when session name changes
  const lastSetNameRef = useRef<string>('');

  useEffect(() => {
    const currentSessionName = session?.name;
    if (currentSessionName && currentSessionName !== lastSetNameRef.current) {
      lastSetNameRef.current = currentSessionName;
      setChat({
        messages,
        workflow,
        sessionId,
        name: currentSessionName,
      });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [session?.name, setChat]);

  // Keep the latest onSessionUpdate in a ref so changes to its identity (e.g., a
  // new arrow on every render) don't refire this effect. The effect must fire
  // only when the session id/name actually changes — otherwise we'd loop through
  // setState in parent, re-render this child, get a new callback identity, fire
  // again, ad infinitum.
  const onSessionUpdateRef = useRef(onSessionUpdate);
  useEffect(() => {
    onSessionUpdateRef.current = onSessionUpdate;
  }, [onSessionUpdate]);
  useEffect(() => {
    if (!session) return;
    onSessionUpdateRef.current?.({
      id: session.id,
      name: session.name,
      userSetName: session.user_set_name ?? false,
    });
  }, [session?.id, session?.name, session?.user_set_name]);

  const handleRename = async (newName: string) => {
    if (onRenameSession) {
      onRenameSession(newName);
      return;
    }
    if (!sessionId) return;
    // Optimistic announce so the pill, the chat-context display, history, and
    // any other open window snap to the new name immediately. `renameSession`
    // will re-announce on API success (idempotent — no-op if name matches).
    const previous = session;
    announceSessionName({
      sessionId,
      name: newName,
      userSetName: true,
      origin: 'user',
    });
    try {
      await renameSession(sessionId, newName, 'user');
    } catch (err) {
      // Roll back to whatever the session held before the click. Using
      // `sync` as the origin so listeners treat it as authoritative.
      if (previous?.name) {
        announceSessionName({
          sessionId,
          name: previous.name,
          userSetName: previous.user_set_name ?? false,
          origin: 'sync',
        });
      }
      toastError({
        title: 'Failed to rename session',
        msg: errorMessage(err),
      });
    }
  };

  // Only use initialMessage for the prompt if it hasn't been submitted yet
  // If we have a workflow prompt and user workflow values, substitute parameters
  let workflowPrompt = '';
  if (messages.length === 0 && workflow?.prompt) {
    workflowPrompt = session?.user_workflow_values
      ? substituteParameters(workflow.prompt, session.user_workflow_values)
      : workflow.prompt;
  }

  const initialPrompt = workflowPrompt;
  const isCleanConversation =
    messages.length === 0 &&
    !workflow &&
    !initialPrompt &&
    chatState === ChatState.Idle &&
    !isCreatingSession;

  useLayoutEffect(() => {
    if (isCleanConversation) return;

    const from = pendingComposerRectRef.current;
    const element = composerMotionRef.current;
    pendingComposerRectRef.current = null;

    const isReducedMotion =
      typeof window.matchMedia === 'function' &&
      window.matchMedia('(prefers-reduced-motion: reduce)').matches;

    if (!from || !element || isReducedMotion) {
      return;
    }

    const to = element.getBoundingClientRect();
    const deltaX = from.left - to.left;
    const deltaY = from.top - to.top;
    const scaleX = to.width > 0 ? from.width / to.width : 1;

    if (Math.abs(deltaX) < 1 && Math.abs(deltaY) < 1 && Math.abs(scaleX - 1) < 0.01) {
      return;
    }

    const animation = element.animate(
      [
        {
          opacity: 0.96,
          transform: `translate3d(${deltaX}px, ${deltaY}px, 0) scaleX(${scaleX})`,
        },
        { opacity: 1, transform: 'translate3d(0, 0, 0) scaleX(1)' },
      ],
      {
        duration: 420,
        easing: 'cubic-bezier(0.22, 1, 0.36, 1)',
      }
    );

    return () => animation.cancel();
  }, [isCleanConversation]);

  const renderChatInput = () => (
    <div
      ref={composerMotionRef}
      data-composer-shell="true"
      className={cn(
        'w-full max-w-[760px] mx-auto biorouter-composer-motion',
        !compactPicker && 'biorouter-composer-view-transition'
      )}
    >
      <ChatInput
        sessionId={sessionId}
        handleSubmit={handleFormSubmit}
        chatState={chatState}
        setChatState={setChatState}
        onStop={stopStreaming}
        commandHistory={commandHistory}
        initialValue={initialPrompt}
        setView={setView}
        totalTokens={tokenState?.totalTokens ?? session?.total_tokens ?? undefined}
        accumulatedInputTokens={
          tokenState?.accumulatedInputTokens ?? session?.accumulated_input_tokens ?? undefined
        }
        accumulatedOutputTokens={
          tokenState?.accumulatedOutputTokens ?? session?.accumulated_output_tokens ?? undefined
        }
        droppedFiles={droppedFiles}
        onFilesProcessed={() => setDroppedFiles([])} // Clear dropped files after processing
        messages={messages}
        disableAnimation={disableAnimation}
        sessionCosts={sessionCosts}
        workflow={workflow}
        workflowAccepted={!hasNotAcceptedWorkflow}
        initialPrompt={initialPrompt}
        toolCount={toolCount || 0}
        compactPicker={compactPicker}
        supportsVisionOverride={sessionSupportsVision ?? undefined}
        {...customChatInputProps}
      />
    </div>
  );

  const renderWorkingStatus = () => {
    if (chatState === ChatState.Idle) return null;

    return (
      <div className="w-full max-w-[760px] mx-auto mb-2.5 pl-2 pointer-events-none">
        <LoadingBioRouter
          chatState={chatState}
          message={
            messages.length > 0 ? getThinkingMessage(messages[messages.length - 1]) : undefined
          }
        />
      </div>
    );
  };

  if (sessionLoadError) {
    return (
      <div className="h-full flex flex-col min-h-0">
        <MainPanelLayout
          backgroundColor={'bg-background-muted'}
          removeTopPadding={true}
          {...customMainLayoutProps}
        >
          {renderHeader && renderHeader()}
          <div className="flex flex-col flex-1 mb-0.5 min-h-0 relative">
            <div className="flex-1 bg-background-default rounded-b-2xl flex items-center justify-center">
              <div className="flex flex-col items-center justify-center p-8">
                <div className="text-text-danger bg-background-danger/10 border border-border-danger/40 p-4 rounded-lg mb-4 max-w-md">
                  <h3 className="font-semibold mb-2">Failed to Load Session</h3>
                  <p className="text-sm">{sessionLoadError}</p>
                </div>
                <button
                  onClick={() => {
                    setView('chat');
                  }}
                  className="px-4 py-2 text-center cursor-pointer text-text-default border border-border-subtle hover:bg-background-medium rounded-lg transition-all duration-150"
                >
                  Go home
                </button>
              </div>
            </div>
          </div>
        </MainPanelLayout>
      </div>
    );
  }

  return (
    <div className="h-full flex flex-col min-h-0">
      <MainPanelLayout
        backgroundColor={'bg-background-muted'}
        removeTopPadding={true}
        {...customMainLayoutProps}
      >
        {/* Custom header */}
        {renderHeader && renderHeader()}

        {/* Chat container with sticky workflow header */}
        <div
          className={
            coherent
              ? 'flex flex-col flex-1 min-h-0 relative rounded-t-2xl overflow-hidden bg-background-default'
              : 'flex flex-col flex-1 mx-4 mt-4 mb-3 min-h-0 relative rounded-2xl overflow-hidden'
          }
        >
          {!hideSessionNamePill && (
            // Wrapper sits above the fixed `.titlebar-drag-region` (z-50,
            // top 32px) so the pill can receive clicks despite overlapping
            // the OS title-bar drag zone. It explicitly opts INTO drag, so
            // the wrapper area outside the pill itself still drags the
            // window — only the pill's own (inline-flex) bounding box is
            // marked no-drag (done inside SessionNamePill).
            <div
              className="flex-shrink-0 px-4 pt-3 relative z-[60]"
              style={{ WebkitAppRegion: 'drag' } as React.CSSProperties}
            >
              <SessionNamePill
                name={session?.name || 'New Session'}
                onRename={handleRename}
                accentColor={accentColor}
              />
            </div>
          )}
          {isCleanConversation ? (
            <div
              className="flex-1 min-h-0 flex items-center justify-center px-4 py-10 sm:px-6 sm:py-16"
              onDrop={handleDrop}
              onDragOver={handleDragOver}
              data-drop-zone="true"
            >
              <div className="w-full max-w-[760px] flex flex-col items-center gap-6">
                <Greeting
                  key={sessionId}
                  className={cn(
                    'text-center font-semibold tracking-tight text-text-default animate-in fade-in duration-300',
                    compactPicker ? 'text-xl' : 'text-2xl'
                  )}
                />
                {renderChatInput()}
              </div>
            </div>
          ) : (
            <ScrollArea
              ref={scrollRef}
              className={
                coherent
                  ? `flex-1 min-h-0 relative ${contentClassName}`
                  : `flex-1 bg-background-default rounded-2xl min-h-0 relative ${contentClassName}`
              }
              autoScroll
              onDrop={handleDrop}
              onDragOver={handleDragOver}
              data-drop-zone="true"
              paddingX={6}
              paddingY={0}
            >
              {workflow?.title && (
                <div className="sticky top-0 z-10 bg-background-default px-0 -mx-6 mb-4 pt-2">
                  <WorkflowHeader title={workflow.title} />
                </div>
              )}

              {workflow && (
                <div className={hasStartedUsingWorkflow ? 'mb-6' : ''}>
                  <WorkflowActivities
                    append={(text: string) => handleSubmit(text)}
                    activities={Array.isArray(workflow.activities) ? workflow.activities : null}
                    title={workflow.title}
                    parameterValues={session?.user_workflow_values || {}}
                  />
                </div>
              )}

              {messages.length > 0 || workflow ? (
                <>
                  <SearchView>
                    <ProgressiveMessageList
                      messages={messages}
                      chat={{ sessionId }}
                      toolCallNotifications={toolCallNotifications}
                      append={(text: string) => handleSubmit(text)}
                      isUserMessage={(m: Message) => m.role === 'user'}
                      isStreamingMessage={chatState !== ChatState.Idle}
                      onRenderingComplete={handleRenderingComplete}
                      onMessageUpdate={onMessageUpdate}
                      submitElicitationResponse={submitElicitationResponse}
                    />
                  </SearchView>

                  <div className="block h-8" />
                </>
              ) : null}
            </ScrollArea>
          )}
        </div>

        {!isCleanConversation && (
          <div
            className={
              coherent
                ? 'flex-shrink-0 px-4 sm:px-6 pb-4 pt-2 bg-background-default'
                : `px-4 sm:px-6 pb-4 pt-2 flex-shrink-0 ${disableAnimation ? '' : 'animate-[fadein_400ms_ease-in_forwards]'}`
            }
          >
            {renderWorkingStatus()}
            {renderChatInput()}
          </div>
        )}
      </MainPanelLayout>

      {workflow && (
        <WorkflowWarningModal
          isOpen={!!hasNotAcceptedWorkflow}
          onConfirm={() => handleWorkflowAccept(true)}
          onCancel={() => handleWorkflowAccept(false)}
          workflowDetails={{
            title: workflow.title,
            description: workflow.description,
            instructions: workflow.instructions || undefined,
          }}
          hasSecurityWarnings={hasWorkflowSecurityWarnings}
        />
      )}

      {workflow?.parameters && workflow.parameters.length > 0 && !session?.user_workflow_values && (
        <ParameterInputModal
          parameters={workflow.parameters}
          onSubmit={setWorkflowUserParams}
          onClose={() => setView('chat')}
          initialValues={
            (window.appConfig?.get('workflowParameters') as Record<string, string> | undefined) ||
            undefined
          }
        />
      )}

      <CreateWorkflowFromSessionModal
        isOpen={isCreateWorkflowModalOpen}
        onClose={() => setIsCreateWorkflowModalOpen(false)}
        sessionId={chat.sessionId}
        onWorkflowCreated={handleWorkflowCreated}
      />
    </div>
  );
}

export default function BaseChat(props: BaseChatProps) {
  return <BaseChatContent {...props} />;
}
