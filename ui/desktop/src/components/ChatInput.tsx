import React, { useRef, useState, useEffect, useMemo, useCallback } from 'react';
import { ScrollText, ChevronRight, ChevronLeft } from './icons/app-icons';
import { ContextWindowGauge, ContextWindowIndicator } from './ContextWindowIndicator';
import { Tooltip, TooltipContent, TooltipTrigger } from './ui/Tooltip';
import { Popover, PopoverContent, PopoverTrigger } from './ui/popover';
import { Button } from './ui/button';
import type { View } from '../utils/navigationUtils';
import Stop from './ui/Stop';
import { Send, Close } from './icons';
import { ChatState } from '../types/chatState';
import debounce from 'lodash/debounce';
import { LocalMessageStorage } from '../utils/localMessageStorage';
import { DirSwitcher } from './bottom_menu/DirSwitcher';
import ModelsBottomBar from './settings/models/bottom_bar/ModelsBottomBar';
import { BottomMenuExtensionSelection } from './bottom_menu/BottomMenuExtensionSelection';
import { BottomMenuSkillSelection } from './bottom_menu/BottomMenuSkillSelection';
import { BottomMenuKnowledgeSelection } from './bottom_menu/BottomMenuKnowledgeSelection';
import { AlertType, useAlerts } from './alerts';
import { useConfig } from './ConfigContext';
import { useModelAndProvider } from './ModelAndProviderContext';
import MentionPopover, { DisplayItemWithMatch } from './MentionPopover';
import { COST_TRACKING_ENABLED } from '../updates';
import { CostTracker } from './bottom_menu/CostTracker';
import { DroppedFile, useFileDrop } from '../hooks/useFileDrop';
import { useDiverge } from '../hooks/useDiverge';
import { Workflow } from '../workflow';
import MessageQueue from './MessageQueue';
import { detectInterruption } from '../utils/interruptionDetector';
import { getSession, llamacppStatus, Message } from '../api';
import { getInitialWorkingDir } from '../utils/workingDir';
import { getPredefinedModelsFromEnv } from './settings/models/predefinedModelsUtils';
import { getNavigationShortcutText } from '../utils/keyboardShortcuts';
import type { UserAttachment } from '../types/message';

interface QueuedMessage {
  id: string;
  content: string;
  attachments?: UserAttachment[];
  timestamp: number;
}

interface PastedImage {
  id: string;
  dataUrl: string; // For immediate preview
  filePath?: string; // Path on filesystem after saving
  isLoading: boolean;
  error?: string;
}

// Constants for image handling
const MAX_IMAGES_PER_MESSAGE = 5;
const MAX_IMAGE_SIZE_MB = 3;

// Constants for token and tool alerts
const TOKEN_LIMIT_DEFAULT = 128000; // fallback for custom models that the backend doesn't know about
const TOOLS_MAX_SUGGESTED = 60; // max number of tools before we show a warning

// Manual compact trigger message - must match backend constant
const MANUAL_COMPACT_TRIGGER = '/compact';

// Client-side slash command: branch the conversation into a new chat. Handled
// entirely in the renderer (never sent to the agent).
const DIVERGE_TRIGGER = '/diverge';
const TOOLBAR_DIVIDER_CLASS = 'h-4 w-px flex-shrink-0 bg-border-default/70';
const TOOLBAR_GROUP_CLASS = 'flex flex-shrink-0 items-center gap-0.5';
const TOOLBAR_ICON_BUTTON_CLASS =
  'flex h-7 w-7 items-center justify-center rounded-md p-0 text-text-default/70 transition-colors hover:bg-background-medium hover:text-text-default';
const TOOLBAR_COMPACT_WIDTH = 680;

function canonicalMimeType(mimeType: string): string {
  const normalized = mimeType.toLowerCase().trim();
  return normalized === 'image/jpg' ? 'image/jpeg' : normalized;
}

function mimeTypeAllowed(mimeTypes: string[] | null, mimeType: string): boolean {
  if (!mimeTypes) return true;
  const canonical = canonicalMimeType(mimeType);
  return mimeTypes.some((allowedMimeType) => canonicalMimeType(allowedMimeType) === canonical);
}

async function validateImageDataUrl(dataUrl: string): Promise<void> {
  if (
    typeof Image === 'undefined' ||
    typeof HTMLImageElement === 'undefined' ||
    typeof HTMLImageElement.prototype.decode !== 'function'
  ) {
    return;
  }

  const img = new Image();
  img.src = dataUrl;
  await img.decode();
}

interface ModelLimit {
  pattern: string;
  context_limit: number;
}

/** Single row in the secondary-controls popover. Children are rendered
 * directly — the inner control's own leading icon (e.g. brain, tornado,
 * dollar) serves as the left-aligned visual marker. Rows share the same
 * left-padding so every icon lines up cleanly. */
function PickerRow({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex items-center px-2 py-1.5 rounded hover:bg-background-medium/40">
      {children}
    </div>
  );
}

interface ChatInputProps {
  sessionId: string | null;
  handleSubmit: (e: React.FormEvent) => void;
  chatState: ChatState;
  setChatState?: (state: ChatState) => void;
  onStop?: () => void;
  commandHistory?: string[];
  initialValue?: string;
  droppedFiles?: DroppedFile[];
  onFilesProcessed?: () => void;
  setView: (view: View) => void;
  totalTokens?: number;
  accumulatedInputTokens?: number;
  accumulatedOutputTokens?: number;
  messages?: Message[];
  sessionCosts?: {
    [key: string]: {
      inputTokens: number;
      outputTokens: number;
      totalCost: number;
    };
  };
  disableAnimation?: boolean;
  workflow?: Workflow | null;
  workflowAccepted?: boolean;
  initialPrompt?: string;
  toolCount: number;
  append?: (message: Message) => void;
  onWorkingDirChange?: (newDir: string) => void;
  /** When true (dashboard mode), secondary controls live behind a chevron
   * popover. When false (chat tab), they're rendered inline so users with
   * the room get every control at a glance. */
  compactPicker?: boolean;
  /** Optional override for vision capability. When the chat is bound to a
   * specific session whose model differs from the user's global default
   * (notably dashboard windows), the override reflects the session's actual
   * model. Falls back to the global ModelAndProviderContext flag when
   * undefined. */
  supportsVisionOverride?: boolean;
  supportedInputMimeTypesOverride?: string[] | null;
}

export default function ChatInput({
  sessionId,
  handleSubmit,
  chatState = ChatState.Idle,
  setChatState,
  onStop,
  commandHistory = [],
  initialValue = '',
  droppedFiles = [],
  onFilesProcessed,
  setView,
  totalTokens,
  accumulatedInputTokens,
  accumulatedOutputTokens,
  messages = [],
  disableAnimation = false,
  sessionCosts,
  workflowAccepted,
  initialPrompt,
  toolCount,
  append: _append,
  onWorkingDirChange,
  compactPicker = false,
  supportsVisionOverride,
  supportedInputMimeTypesOverride,
}: ChatInputProps) {
  const [_value, setValue] = useState(initialValue);
  const [displayValue, setDisplayValue] = useState(initialValue); // For immediate visual feedback
  const [isFocused, setIsFocused] = useState(false);
  const [pastedImages, setPastedImages] = useState<PastedImage[]>([]);

  // Derived state - chatState != Idle means we're in some form of loading state
  const isLoading = chatState !== ChatState.Idle;
  const wasLoadingRef = useRef(isLoading);

  // Queue functionality - ephemeral, only exists in memory for this chat instance
  const [queuedMessages, setQueuedMessages] = useState<QueuedMessage[]>([]);
  const queuePausedRef = useRef(false);
  const editingMessageIdRef = useRef<string | null>(null);
  const [lastInterruption, setLastInterruption] = useState<string | null>(null);

  const { alerts, addAlert, clearAlerts } = useAlerts();
  const dropdownRef: React.RefObject<HTMLDivElement> = useRef<HTMLDivElement>(
    null
  ) as React.RefObject<HTMLDivElement>;
  const { getProviders, read } = useConfig();
  const {
    getCurrentModelAndProvider,
    currentModel,
    currentProvider,
    currentModelSupportsVision: globalSupportsVision,
    currentModelSupportedInputMimeTypes: globalSupportedInputMimeTypes,
  } = useModelAndProvider();
  // Prefer the session-scoped flag when provided. This matters in dashboard
  // mode (where each window may be bound to a different model than the user's
  // global default) and after per-session model switches.
  const currentModelSupportsVision =
    supportsVisionOverride !== undefined ? supportsVisionOverride : globalSupportsVision;
  const currentModelSupportedInputMimeTypes =
    supportedInputMimeTypesOverride !== undefined
      ? supportedInputMimeTypesOverride
      : globalSupportedInputMimeTypes;
  const [tokenLimit, setTokenLimit] = useState<number>(TOKEN_LIMIT_DEFAULT);
  const [isTokenLimitLoaded, setIsTokenLimitLoaded] = useState(false);
  const [sessionWorkingDir, setSessionWorkingDir] = useState<string | null>(null);
  const toolbarRef = useRef<HTMLDivElement>(null);
  const [isToolbarNarrow, setIsToolbarNarrow] = useState(false);
  // Collapsible group: model, pricing, and context details live behind a
  // chevron so the picker row stays narrow. Session-level actions live in the
  // conversation header, keeping the composer focused on prompt resources.
  const [pickerExpanded, setPickerExpanded] = useState(false);
  const useCompactControls = compactPicker || isToolbarNarrow;

  // Branch-the-conversation action shared with the message-level Diverge button.
  const { diverge } = useDiverge();

  useEffect(() => {
    const toolbar = toolbarRef.current;
    if (!toolbar) return;

    const updateToolbarMode = () => {
      setIsToolbarNarrow(toolbar.clientWidth < TOOLBAR_COMPACT_WIDTH);
    };

    updateToolbarMode();

    if (typeof ResizeObserver === 'undefined') {
      window.addEventListener('resize', updateToolbarMode);
      return () => window.removeEventListener('resize', updateToolbarMode);
    }

    const resizeObserver = new ResizeObserver(updateToolbarMode);
    resizeObserver.observe(toolbar);
    return () => resizeObserver.disconnect();
  }, []);

  useEffect(() => {
    if (!sessionId) {
      return;
    }

    const fetchSessionWorkingDir = async () => {
      try {
        const response = await getSession({ path: { session_id: sessionId } });
        if (response.data?.working_dir) {
          setSessionWorkingDir(response.data.working_dir);
        }
      } catch (error) {
        console.error('[ChatInput] Failed to fetch session working dir:', error);
      }
    };

    fetchSessionWorkingDir();
  }, [sessionId]);

  // Save queue state (paused/interrupted) to storage
  useEffect(() => {
    try {
      window.sessionStorage.setItem(
        'biorouter-queue-paused',
        JSON.stringify(queuePausedRef.current)
      );
    } catch (error) {
      console.error('Error saving queue pause state:', error);
    }
  }, [queuedMessages]); // Save when queue changes

  useEffect(() => {
    try {
      window.sessionStorage.setItem(
        'biorouter-queue-interruption',
        JSON.stringify(lastInterruption)
      );
    } catch (error) {
      console.error('Error saving queue interruption state:', error);
    }
  }, [lastInterruption]);

  // Cleanup effect - save final state on component unmount
  useEffect(() => {
    return () => {
      // Save final queue state when component unmounts
      try {
        window.sessionStorage.setItem(
          'biorouter-queue-paused',
          JSON.stringify(queuePausedRef.current)
        );
        window.sessionStorage.setItem(
          'biorouter-queue-interruption',
          JSON.stringify(lastInterruption)
        );
      } catch (error) {
        console.error('Error saving queue state on unmount:', error);
      }
    };
  }, [lastInterruption]); // Include lastInterruption in dependency array

  // Queue processing
  useEffect(() => {
    if (wasLoadingRef.current && !isLoading && queuedMessages.length > 0) {
      // After an interruption, we should process the interruption message immediately
      // The queue is only truly paused if there was an interruption AND we want to keep it paused
      const shouldProcessQueue = !queuePausedRef.current || lastInterruption;

      if (shouldProcessQueue) {
        const nextMessage = queuedMessages[0];
        LocalMessageStorage.addMessage(nextMessage.content);
        handleSubmit(
          new CustomEvent('submit', {
            detail: { value: nextMessage.content, attachments: nextMessage.attachments ?? [] },
          }) as unknown as React.FormEvent
        );
        setQueuedMessages((prev) => {
          const newQueue = prev.slice(1);
          // If queue becomes empty after processing, clear the paused state
          if (newQueue.length === 0) {
            queuePausedRef.current = false;
            setLastInterruption(null);
          }
          return newQueue;
        });

        // Clear the interruption flag after processing the interruption message
        if (lastInterruption) {
          setLastInterruption(null);
          // Keep the queue paused after sending the interruption message
          // User can manually resume if they want to continue with queued messages
          queuePausedRef.current = true;
        }
      }
    }
    wasLoadingRef.current = isLoading;
  }, [isLoading, queuedMessages, handleSubmit, lastInterruption]);
  const [mentionPopover, setMentionPopover] = useState<{
    isOpen: boolean;
    position: { x: number; y: number };
    query: string;
    mentionStart: number;
    selectedIndex: number;
    isSlashCommand: boolean;
  }>({
    isOpen: false,
    position: { x: 0, y: 0 },
    query: '',
    mentionStart: -1,
    selectedIndex: 0,
    isSlashCommand: false,
  });
  const mentionPopoverRef = useRef<{
    getDisplayFiles: () => DisplayItemWithMatch[];
    selectFile: (index: number) => void;
  }>(null);

  // Update internal value when initialValue changes
  useEffect(() => {
    setValue(initialValue);
    setDisplayValue(initialValue);

    // Use a functional update to get the current pastedImages
    // and perform cleanup. This avoids needing pastedImages in the deps.
    setPastedImages((currentPastedImages) => {
      currentPastedImages.forEach((img) => {
        if (img.filePath) {
          window.electron.deleteTempFile(img.filePath);
        }
      });
      return []; // Return a new empty array
    });

    // Reset history index when input is cleared
    setHistoryIndex(-1);
    setIsInGlobalHistory(false);
    setHasUserTyped(false);
  }, [initialValue]); // Keep only initialValue as a dependency

  // Handle workflow prompt updates
  useEffect(() => {
    // If workflow is accepted and we have an initial prompt, and no messages yet, and we haven't set it before
    if (workflowAccepted && initialPrompt && messages.length === 0) {
      setDisplayValue(initialPrompt);
      setValue(initialPrompt);
      setTimeout(() => {
        textAreaRef.current?.focus();
      }, 0);
    }
  }, [workflowAccepted, initialPrompt, messages.length]);

  // State to track if the IME is composing (i.e., in the middle of Japanese IME input)
  const [isComposing, setIsComposing] = useState(false);
  const [historyIndex, setHistoryIndex] = useState(-1);
  const [savedInput, setSavedInput] = useState('');
  const [isInGlobalHistory, setIsInGlobalHistory] = useState(false);
  const [hasUserTyped, setHasUserTyped] = useState(false);
  const textAreaRef = useRef<HTMLTextAreaElement>(null);
  const timeoutRefsRef = useRef<Set<ReturnType<typeof setTimeout>>>(new Set());

  // Listen for parent-initiated focus requests (e.g. dashboard ChatWindow
  // dispatches 'focus-chat-input' when a folded card is unfolded). The mount
  // autoFocus on textarea doesn't re-fire on visibility change, so we need
  // an explicit poke. Match by sessionId so a stray broadcast doesn't focus
  // every chat input on the page.
  useEffect(() => {
    const handler = (e: Event) => {
      const detail = (e as CustomEvent<{ sessionId?: string | null }>).detail;
      if (detail?.sessionId && detail.sessionId !== sessionId) return;
      textAreaRef.current?.focus();
    };
    window.addEventListener('focus-chat-input', handler);
    return () => window.removeEventListener('focus-chat-input', handler);
  }, [sessionId]);

  // Use shared file drop hook for ChatInput
  const {
    droppedFiles: localDroppedFiles,
    setDroppedFiles: setLocalDroppedFiles,
    isDraggingOver,
    handleDrop: handleLocalDrop,
    handleDragEnter: handleLocalDragEnter,
    handleDragOver: handleLocalDragOver,
    handleDragLeave: handleLocalDragLeave,
  } = useFileDrop();

  // Merge local dropped files with parent dropped files. Keep every dropped
  // item visible: model capability determines whether an image is uploaded as
  // content or sent as a filesystem path, never whether the drop disappears.
  const allDroppedFiles = useMemo(() => {
    return [...droppedFiles, ...localDroppedFiles];
  }, [droppedFiles, localDroppedFiles]);

  const currentModelAcceptsMimeType = useCallback(
    (mimeType: string) =>
      currentModelSupportsVision &&
      Boolean(mimeType) &&
      mimeTypeAllowed(currentModelSupportedInputMimeTypes, mimeType),
    [currentModelSupportedInputMimeTypes, currentModelSupportsVision]
  );

  const canUploadDroppedImage = useCallback(
    (file: DroppedFile) =>
      currentModelSupportsVision &&
      file.isImage &&
      currentModelAcceptsMimeType(file.type) &&
      file.canUploadAsImage === true &&
      !file.error &&
      !file.isLoading &&
      Boolean(file.stagedPath),
    [currentModelAcceptsMimeType, currentModelSupportsVision]
  );

  const canSendDroppedFileAsPath = useCallback(
    (file: DroppedFile) =>
      Boolean(file.sourcePath || file.path) &&
      !file.isLoading &&
      (!file.isImage || !canUploadDroppedImage(file)),
    [canUploadDroppedImage]
  );

  const canSendDroppedFile = useCallback(
    (file: DroppedFile) => canUploadDroppedImage(file) || canSendDroppedFileAsPath(file),
    [canSendDroppedFileAsPath, canUploadDroppedImage]
  );

  const droppedFilePath = (file: DroppedFile) => file.sourcePath || file.path;
  const droppedImageAttachmentPath = (file: DroppedFile) => file.stagedPath || file.path;

  const handleRemoveDroppedFile = (idToRemove: string) => {
    // Remove from local dropped files
    setLocalDroppedFiles((prev) => prev.filter((file) => file.id !== idToRemove));

    // If it's from parent, call the parent's callback
    if (onFilesProcessed && droppedFiles.some((file) => file.id === idToRemove)) {
      onFilesProcessed();
    }
  };

  const handleRemovePastedImage = (idToRemove: string) => {
    const imageToRemove = pastedImages.find((img) => img.id === idToRemove);
    if (imageToRemove?.filePath) {
      window.electron.deleteTempFile(imageToRemove.filePath);
    }
    setPastedImages((currentImages) => currentImages.filter((img) => img.id !== idToRemove));
  };

  const handleRetryImageSave = async (imageId: string) => {
    const imageToRetry = pastedImages.find((img) => img.id === imageId);
    if (!imageToRetry || !imageToRetry.dataUrl) return;

    // Set the image to loading state
    setPastedImages((prev) =>
      prev.map((img) => (img.id === imageId ? { ...img, isLoading: true, error: undefined } : img))
    );

    try {
      const result = await window.electron.saveDataUrlToTemp(imageToRetry.dataUrl, imageId);
      setPastedImages((prev) =>
        prev.map((img) =>
          img.id === result.id
            ? { ...img, filePath: result.filePath, error: result.error, isLoading: false }
            : img
        )
      );
    } catch (err) {
      console.error('Error retrying image save:', err);
      setPastedImages((prev) =>
        prev.map((img) =>
          img.id === imageId
            ? { ...img, error: 'Failed to save image via Electron.', isLoading: false }
            : img
        )
      );
    }
  };

  useEffect(() => {
    if (textAreaRef.current) {
      textAreaRef.current.focus();
    }
  }, []);

  // Load model limits from the API
  const getModelLimits = async () => {
    try {
      const response = await read('model-limits', false);
      if (response) {
        // The response is already parsed, no need for JSON.parse
        return response as ModelLimit[];
      }
    } catch (err) {
      console.error('Error fetching model limits:', err);
    }
    return [];
  };

  // Helper function to find model limit using pattern matching
  const findModelLimit = (modelName: string, modelLimits: ModelLimit[]): number | null => {
    if (!modelName) return null;
    const matchingLimit = modelLimits.find((limit) =>
      modelName.toLowerCase().includes(limit.pattern.toLowerCase())
    );
    return matchingLimit ? matchingLimit.context_limit : null;
  };

  // Load providers and get current model's token limit
  const loadProviderDetails = async () => {
    try {
      // Reset token limit loaded state
      setIsTokenLimitLoaded(false);

      // Get current model and provider first to avoid unnecessary provider fetches
      const { model, provider } = await getCurrentModelAndProvider();
      if (!model || !provider) {
        console.log('No model or provider found');
        setIsTokenLimitLoaded(true);
        return;
      }

      // Llama Server (local models): the real context window is a live property
      // of the loaded model, read from the running server's /props. Prefer it
      // over any static catalog/default so the gauge matches the CLI/backend
      // (e.g. a 262k model instead of the 128k fallback). Only when the sidecar
      // has reported a window; otherwise fall through to the static logic.
      if (provider === 'llamacpp') {
        try {
          const status = await llamacppStatus();
          const ctx = status.data?.sidecar?.context_size;
          if (typeof ctx === 'number' && ctx > 0) {
            setTokenLimit(ctx);
            setIsTokenLimitLoaded(true);
            return;
          }
        } catch (e) {
          console.warn('Failed to read llama-server context window, using fallback:', e);
        }
      }

      // First, check predefined models from environment (highest priority)
      const predefinedModels = getPredefinedModelsFromEnv();
      const predefinedModel = predefinedModels.find((m) => m.name === model);
      if (predefinedModel?.context_limit) {
        setTokenLimit(predefinedModel.context_limit);
        setIsTokenLimitLoaded(true);
        return;
      }

      const providers = await getProviders(true);

      // Find the provider details for the current provider
      const currentProvider = providers.find((p) => p.name === provider);
      if (currentProvider?.metadata?.known_models) {
        // Find the model's token limit from the backend response
        const modelConfig = currentProvider.metadata.known_models.find((m) => m.name === model);
        if (modelConfig?.context_limit) {
          setTokenLimit(modelConfig.context_limit);
          setIsTokenLimitLoaded(true);
          return;
        }
      }

      // Fallback: Use pattern matching logic if no exact model match was found
      const modelLimit = await getModelLimits();
      const fallbackLimit = findModelLimit(model as string, modelLimit);
      if (fallbackLimit !== null) {
        setTokenLimit(fallbackLimit);
        setIsTokenLimitLoaded(true);
        return;
      }

      // If no match found, use the default model limit
      setTokenLimit(TOKEN_LIMIT_DEFAULT);
      setIsTokenLimitLoaded(true);
    } catch (err) {
      console.error('Error loading providers or token limit:', err);
      // Set default limit on error
      setTokenLimit(TOKEN_LIMIT_DEFAULT);
      setIsTokenLimitLoaded(true);
    }
  };

  // Initial load and refresh when model changes
  useEffect(() => {
    loadProviderDetails();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [currentModel, currentProvider]);

  // Handle tool count alerts and token usage
  useEffect(() => {
    clearAlerts();

    // Show alert when either there is registered token usage, or we know the limit
    if ((totalTokens && totalTokens > 0) || (isTokenLimitLoaded && tokenLimit)) {
      addAlert({
        type: AlertType.Info,
        message: 'Context window',
        progress: {
          current: totalTokens || 0,
          total: tokenLimit,
        },
        showCompactButton: true,
        compactButtonDisabled: !totalTokens,
        onCompact: () => {
          window.dispatchEvent(new CustomEvent('hide-alert-popover'));

          const customEvent = new CustomEvent('submit', {
            detail: { value: MANUAL_COMPACT_TRIGGER },
          }) as unknown as React.FormEvent;

          handleSubmit(customEvent);
        },
        compactIcon: <ScrollText size={12} />,
      });
    }

    // Add tool count alert if we have the data
    if (toolCount !== null && toolCount > TOOLS_MAX_SUGGESTED) {
      addAlert({
        type: AlertType.Warning,
        message: `Too many tools can degrade performance.\nTool count: ${toolCount} (recommend: ${TOOLS_MAX_SUGGESTED})`,
        action: {
          text: 'View extensions',
          onClick: () => setView('extensions'),
        },
        autoShow: false, // Don't auto-show tool count warnings
      });
    }
    // We intentionally omit setView as it shouldn't trigger a re-render of alerts
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [totalTokens, toolCount, tokenLimit, isTokenLimitLoaded, addAlert, clearAlerts]);

  // Cleanup effect for component unmount - prevent memory leaks
  useEffect(() => {
    return () => {
      // Clear any pending timeouts from image processing
      setPastedImages((currentImages) => {
        currentImages.forEach((img) => {
          if (img.filePath) {
            try {
              window.electron.deleteTempFile(img.filePath);
            } catch (error) {
              console.error('Error deleting temp file:', error);
            }
          }
        });
        return [];
      });

      // Clear all tracked timeouts
      // eslint-disable-next-line react-hooks/exhaustive-deps
      const timeouts = timeoutRefsRef.current;
      timeouts.forEach((timeoutId) => {
        window.clearTimeout(timeoutId);
      });
      timeouts.clear();

      // Clear alerts to prevent memory leaks
      clearAlerts();
    };
  }, [clearAlerts]);

  const maxHeight = 10 * 24;

  // Immediate function to update actual value - no debounce for better responsiveness
  const updateValue = React.useCallback((value: string) => {
    setValue(value);
  }, []);

  const debouncedAutosize = useMemo(
    () =>
      debounce((element: HTMLTextAreaElement) => {
        element.style.height = '0px'; // Reset height
        const scrollHeight = element.scrollHeight;
        element.style.height = Math.min(scrollHeight, maxHeight) + 'px';
      }, 50),
    [maxHeight]
  );

  useEffect(() => {
    if (textAreaRef.current) {
      debouncedAutosize(textAreaRef.current);
    }
  }, [debouncedAutosize, displayValue]);

  // Reset textarea height when displayValue is empty
  useEffect(() => {
    if (textAreaRef.current && displayValue === '') {
      textAreaRef.current.style.height = 'auto';
    }
  }, [displayValue]);

  const handleChange = (evt: React.ChangeEvent<HTMLTextAreaElement>) => {
    const val = evt.target.value;
    const cursorPosition = evt.target.selectionStart;

    setDisplayValue(val);
    updateValue(val);
    setHasUserTyped(true);
    checkForMentionOrSlash(val, cursorPosition, evt.target);
  };

  const checkForMentionOrSlash = (
    text: string,
    cursorPosition: number,
    textArea: HTMLTextAreaElement
  ) => {
    const beforeCursor = text.slice(0, cursorPosition);
    const lastAtIndex = beforeCursor.lastIndexOf('@');
    let lastSlashIndex = -1;
    for (
      let index = beforeCursor.lastIndexOf('/');
      index >= 0;
      index = beforeCursor.lastIndexOf('/', index - 1)
    ) {
      if (index === 0 || /\s/.test(beforeCursor[index - 1])) {
        lastSlashIndex = index;
        break;
      }
    }
    const triggerIndex = Math.max(lastAtIndex, lastSlashIndex);

    if (triggerIndex === -1) {
      setMentionPopover((prev) => ({ ...prev, isOpen: false }));
      return;
    }

    const trigger = beforeCursor[triggerIndex];
    const query = beforeCursor.slice(triggerIndex + 1);
    if (query.includes(' ') || query.includes('\n')) {
      setMentionPopover((prev) => ({ ...prev, isOpen: false }));
      return;
    }

    // Calculate position for the popover - position it above the chat input
    const textAreaRect = textArea.getBoundingClientRect();

    setMentionPopover((prev) => ({
      ...prev,
      isOpen: true,
      position: {
        x: textAreaRect.left,
        y: textAreaRect.top, // Position at the top of the textarea
      },
      query,
      mentionStart: triggerIndex,
      selectedIndex: 0, // Reset selection when query changes
      isSlashCommand: trigger === '/',
      // filteredFiles will be populated by the MentionPopover component
    }));
  };

  const handlePaste = async (evt: React.ClipboardEvent<HTMLTextAreaElement>) => {
    const files = Array.from(evt.clipboardData.files || []);
    const clipboardImages = files.filter((file) => file.type.startsWith('image/'));
    const imageFiles = clipboardImages.filter((file) => currentModelAcceptsMimeType(file.type));
    const unsupportedImages = clipboardImages.filter(
      (file) => !currentModelAcceptsMimeType(file.type)
    );

    if (clipboardImages.length === 0) return;

    // If the active model does not support vision, ignore image pastes and
    // let the browser handle any plain-text content in the clipboard.
    if (!currentModelSupportsVision) return;

    if (unsupportedImages.length > 0) {
      evt.preventDefault();
      setPastedImages((prev) => [
        ...prev,
        {
          id: `error-${Date.now()}`,
          dataUrl: '',
          isLoading: false,
          error: `This model cannot accept ${unsupportedImages
            .map((file) => file.type || 'this image type')
            .join(', ')} as image input.`,
        },
      ]);

      const timeoutId = setTimeout(() => {
        setPastedImages((prev) => prev.filter((img) => !img.id.startsWith('error-')));
        timeoutRefsRef.current.delete(timeoutId);
      }, 5000);
      timeoutRefsRef.current.add(timeoutId);

      if (imageFiles.length === 0) return;
    }

    // Check if adding these images would exceed the limit
    if (pastedImages.length + imageFiles.length > MAX_IMAGES_PER_MESSAGE) {
      // Show error message to user
      setPastedImages((prev) => [
        ...prev,
        {
          id: `error-${Date.now()}`,
          dataUrl: '',
          isLoading: false,
          error: `Cannot paste ${imageFiles.length} image(s). Maximum ${MAX_IMAGES_PER_MESSAGE} images per message allowed. Currently have ${pastedImages.length}.`,
        },
      ]);

      // Remove the error message after 5 seconds with cleanup tracking
      const timeoutId = setTimeout(() => {
        setPastedImages((prev) => prev.filter((img) => !img.id.startsWith('error-')));
        timeoutRefsRef.current.delete(timeoutId);
      }, 5000);
      timeoutRefsRef.current.add(timeoutId);

      return;
    }

    evt.preventDefault();

    // Process each image file
    const newImages: PastedImage[] = [];

    for (const file of imageFiles) {
      // Check individual file size before processing
      if (file.size > MAX_IMAGE_SIZE_MB * 1024 * 1024) {
        const errorId = `error-${Date.now()}-${Math.random().toString(36).substring(2, 9)}`;
        newImages.push({
          id: errorId,
          dataUrl: '',
          isLoading: false,
          error: `Image too large (${Math.round(file.size / (1024 * 1024))}MB). Maximum ${MAX_IMAGE_SIZE_MB}MB allowed.`,
        });

        // Remove the error message after 5 seconds with cleanup tracking
        const timeoutId = setTimeout(() => {
          setPastedImages((prev) => prev.filter((img) => img.id !== errorId));
          timeoutRefsRef.current.delete(timeoutId);
        }, 5000);
        timeoutRefsRef.current.add(timeoutId);

        continue;
      }

      const imageId = `img-${Date.now()}-${Math.random().toString(36).substring(2, 9)}`;

      // Add the image with loading state
      newImages.push({
        id: imageId,
        dataUrl: '',
        isLoading: true,
      });

      // Process the image asynchronously
      const reader = new FileReader();
      reader.onload = async (e) => {
        const dataUrl = e.target?.result as string;
        if (dataUrl) {
          try {
            await validateImageDataUrl(dataUrl);
          } catch {
            setPastedImages((prev) =>
              prev.map((img) =>
                img.id === imageId
                  ? {
                      ...img,
                      dataUrl: '',
                      error: 'Image preview could not be decoded.',
                      isLoading: false,
                    }
                  : img
              )
            );
            return;
          }

          setPastedImages((prev) =>
            prev.map((img) => (img.id === imageId ? { ...img, dataUrl, isLoading: true } : img))
          );

          try {
            const result = await window.electron.saveDataUrlToTemp(dataUrl, imageId);
            setPastedImages((prev) =>
              prev.map((img) =>
                img.id === result.id
                  ? { ...img, filePath: result.filePath, error: result.error, isLoading: false }
                  : img
              )
            );
          } catch (err) {
            console.error('Error saving pasted image:', err);
            setPastedImages((prev) =>
              prev.map((img) =>
                img.id === imageId
                  ? { ...img, error: 'Failed to save image via Electron.', isLoading: false }
                  : img
              )
            );
          }
        }
      };
      reader.onerror = () => {
        console.error('Failed to read image file:', file.name);
        setPastedImages((prev) =>
          prev.map((img) =>
            img.id === imageId
              ? { ...img, error: 'Failed to read image file.', isLoading: false }
              : img
          )
        );
      };
      reader.readAsDataURL(file);
    }

    // Add all new images to the existing list
    setPastedImages((prev) => [...prev, ...newImages]);
  };

  // Cleanup debounced functions on unmount
  useEffect(() => {
    return () => {
      debouncedAutosize.cancel?.();
    };
  }, [debouncedAutosize]);

  // Handlers for composition events, which are crucial for proper IME behavior
  const handleCompositionStart = () => {
    setIsComposing(true);
  };

  const handleCompositionEnd = () => {
    setIsComposing(false);
  };

  const handleHistoryNavigation = (evt: React.KeyboardEvent<HTMLTextAreaElement>) => {
    const isUp = evt.key === 'ArrowUp';
    const isDown = evt.key === 'ArrowDown';

    // Only handle up/down keys with Cmd/Ctrl modifier
    if ((!isUp && !isDown) || !(evt.metaKey || evt.ctrlKey) || evt.altKey || evt.shiftKey) {
      return;
    }

    // Only prevent history navigation if the user has actively typed something
    // This allows history navigation when text is populated from history or other sources
    // but prevents it when the user is actively editing text
    if (hasUserTyped && displayValue.trim() !== '') {
      return;
    }

    evt.preventDefault();

    // Get global history once to avoid multiple calls
    const globalHistory = LocalMessageStorage.getRecentMessages() || [];

    // Save current input if we're just starting to navigate history
    if (historyIndex === -1) {
      setSavedInput(displayValue || '');
      setIsInGlobalHistory(commandHistory.length === 0);
    }

    // Determine which history we're using
    const currentHistory = isInGlobalHistory ? globalHistory : commandHistory;
    let newIndex = historyIndex;
    let newValue = '';

    // Handle navigation
    if (isUp) {
      // Moving up through history
      if (newIndex < currentHistory.length - 1) {
        // Still have items in current history
        newIndex = historyIndex + 1;
        newValue = currentHistory[newIndex];
      } else if (!isInGlobalHistory && globalHistory.length > 0) {
        // Switch to global history
        setIsInGlobalHistory(true);
        newIndex = 0;
        newValue = globalHistory[newIndex];
      }
    } else {
      // Moving down through history
      if (newIndex > 0) {
        // Still have items in current history
        newIndex = historyIndex - 1;
        newValue = currentHistory[newIndex];
      } else if (isInGlobalHistory && commandHistory.length > 0) {
        // Switch to chat history
        setIsInGlobalHistory(false);
        newIndex = commandHistory.length - 1;
        newValue = commandHistory[newIndex];
      } else {
        // Return to original input
        newIndex = -1;
        newValue = savedInput;
      }
    }

    // Update display if we have a new value
    if (newIndex !== historyIndex) {
      setHistoryIndex(newIndex);
      if (newIndex === -1) {
        setDisplayValue(savedInput || '');
        setValue(savedInput || '');
      } else {
        setDisplayValue(newValue || '');
        setValue(newValue || '');
      }
      // Reset hasUserTyped when we populate from history
      setHasUserTyped(false);
    }
  };

  // Helper function to handle interruption and queue logic when loading
  const handleInterruptionAndQueue = () => {
    if (!isLoading || !hasSubmittableContent) {
      return false;
    }

    const imageAttachments: UserAttachment[] = currentModelSupportsVision
      ? [
          ...pastedImages
            .filter((img) => img.filePath && !img.error && !img.isLoading)
            .map((img) => ({ path: img.filePath as string, kind: 'image' as const })),
          ...allDroppedFiles
            .filter(canUploadDroppedImage)
            .map((file) => ({ path: droppedImageAttachmentPath(file), kind: 'image' as const })),
        ]
      : [];
    const droppedFilePaths = allDroppedFiles.filter(canSendDroppedFileAsPath).map(droppedFilePath);

    let contentToQueue = displayValue.trim();
    if (droppedFilePaths.length > 0) {
      const pathsString = droppedFilePaths.join(' ');
      contentToQueue = contentToQueue ? `${contentToQueue} ${pathsString}` : pathsString;
    }

    const interruptionMatch = detectInterruption(displayValue.trim());

    if (interruptionMatch && interruptionMatch.shouldInterrupt) {
      setLastInterruption(interruptionMatch.matchedText);
      setChatState?.(ChatState.Idle);
      if (onStop) onStop();
      queuePausedRef.current = true;

      // For interruptions, we need to queue the message to be sent after the stop completes
      // rather than trying to send it immediately while the system is still loading
      const interruptionMessage = {
        id: Date.now().toString() + Math.random().toString(36).substr(2, 9),
        content: contentToQueue,
        attachments: imageAttachments,
        timestamp: Date.now(),
      };

      // Add the interruption message to the front of the queue so it gets sent first
      setQueuedMessages((prev) => [interruptionMessage, ...prev]);

      setDisplayValue('');
      setValue('');
      setPastedImages([]);
      if (onFilesProcessed && droppedFiles.length > 0) {
        onFilesProcessed();
      }
      if (localDroppedFiles.length > 0) {
        setLocalDroppedFiles([]);
      }
      return true;
    }

    const newMessage = {
      id: Date.now().toString() + Math.random().toString(36).substr(2, 9),
      content: contentToQueue,
      attachments: imageAttachments,
      timestamp: Date.now(),
    };
    setQueuedMessages((prev) => {
      const newQueue = [...prev, newMessage];
      // If adding to an empty queue, reset the paused state
      if (prev.length === 0) {
        queuePausedRef.current = false;
        setLastInterruption(null);
      }
      return newQueue;
    });
    setDisplayValue('');
    setValue('');
    setPastedImages([]);
    if (onFilesProcessed && droppedFiles.length > 0) {
      onFilesProcessed();
    }
    if (localDroppedFiles.length > 0) {
      setLocalDroppedFiles([]);
    }
    return true;
  };

  const canSubmit =
    !isLoading &&
    (displayValue.trim() ||
      (currentModelSupportsVision &&
        pastedImages.some((img) => img.filePath && !img.error && !img.isLoading)) ||
      allDroppedFiles.some(canSendDroppedFile));

  const performSubmit = useCallback(
    (text?: string) => {
      const validPastedImages = pastedImages.filter(
        (img) => img.filePath && !img.error && !img.isLoading
      );
      const validDroppedImages = allDroppedFiles.filter(canUploadDroppedImage);
      const validDroppedFiles = allDroppedFiles.filter(canSendDroppedFileAsPath);

      // Build structured image attachments (sent as content blocks, not path tokens)
      const imageAttachments: UserAttachment[] = [
        ...(currentModelSupportsVision
          ? validPastedImages.map((img) => ({
              path: img.filePath as string,
              kind: 'image' as const,
            }))
          : []),
        ...validDroppedImages.map((file) => ({
          path: droppedImageAttachmentPath(file),
          kind: 'image' as const,
        })),
      ];

      // Files that cannot or should not be uploaded still go into the text as paths.
      const nonImageFilePaths = validDroppedFiles.map(droppedFilePath);

      // Intercept the client-side /diverge command before it becomes a message.
      // It branches the current conversation instead of being sent to the agent.
      const trimmedCandidate = (text ?? displayValue).trim();
      if (trimmedCandidate === DIVERGE_TRIGGER) {
        if (sessionId) {
          void diverge(sessionId);
        }
        setDisplayValue('');
        setValue('');
        setHasUserTyped(false);
        return;
      }

      let textToSend = text ?? displayValue.trim();

      // Append non-image file paths to the text prompt
      if (nonImageFilePaths.length > 0) {
        const pathsString = nonImageFilePaths.join(' ');
        textToSend = textToSend ? `${textToSend} ${pathsString}` : pathsString;
      }

      if (textToSend || imageAttachments.length > 0) {
        if (displayValue.trim()) {
          LocalMessageStorage.addMessage(displayValue);
        } else if (nonImageFilePaths.length > 0) {
          LocalMessageStorage.addMessage(nonImageFilePaths.join(' '));
        }

        handleSubmit(
          new CustomEvent('submit', {
            detail: { value: textToSend, attachments: imageAttachments },
          }) as unknown as React.FormEvent
        );

        // Auto-resume queue after sending a NON-interruption message (if it was paused due to interruption)
        if (
          queuePausedRef.current &&
          lastInterruption &&
          textToSend &&
          !detectInterruption(textToSend)
        ) {
          queuePausedRef.current = false;
          setLastInterruption(null);
        }

        setDisplayValue('');
        setValue('');
        setPastedImages([]);
        setHistoryIndex(-1);
        setSavedInput('');
        setIsInGlobalHistory(false);
        setHasUserTyped(false);

        // Clear both parent and local dropped files after processing
        if (onFilesProcessed && droppedFiles.length > 0) {
          onFilesProcessed();
        }
        if (localDroppedFiles.length > 0) {
          setLocalDroppedFiles([]);
        }
      }
    },
    [
      allDroppedFiles,
      canSendDroppedFileAsPath,
      canUploadDroppedImage,
      currentModelSupportsVision,
      displayValue,
      diverge,
      droppedFilePath,
      droppedImageAttachmentPath,
      droppedFiles.length,
      handleSubmit,
      lastInterruption,
      localDroppedFiles.length,
      onFilesProcessed,
      pastedImages,
      sessionId,
      setLocalDroppedFiles,
    ]
  );

  const handleKeyDown = (evt: React.KeyboardEvent<HTMLTextAreaElement>) => {
    // If mention popover is open, handle arrow keys and enter
    if (mentionPopover.isOpen && mentionPopoverRef.current) {
      if (evt.key === 'ArrowDown') {
        evt.preventDefault();
        const displayFiles = mentionPopoverRef.current.getDisplayFiles();
        const maxIndex = Math.max(0, displayFiles.length - 1);
        setMentionPopover((prev) => ({
          ...prev,
          selectedIndex: Math.min(prev.selectedIndex + 1, maxIndex),
        }));
        return;
      }
      if (evt.key === 'ArrowUp') {
        evt.preventDefault();
        setMentionPopover((prev) => ({
          ...prev,
          selectedIndex: Math.max(prev.selectedIndex - 1, 0),
        }));
        return;
      }
      if (evt.key === 'Enter') {
        evt.preventDefault();
        mentionPopoverRef.current.selectFile(mentionPopover.selectedIndex);
        return;
      }
      if (evt.key === 'Tab') {
        const displayFiles = mentionPopoverRef.current.getDisplayFiles();
        if (displayFiles.length > 0) {
          evt.preventDefault();
          mentionPopoverRef.current.selectFile(
            displayFiles.length === 1 ? 0 : mentionPopover.selectedIndex
          );
          return;
        }
      }
      if (evt.key === 'Escape') {
        evt.preventDefault();
        setMentionPopover((prev) => ({ ...prev, isOpen: false }));
        return;
      }
    }

    // Handle history navigation first
    handleHistoryNavigation(evt);

    if (evt.key === 'Enter') {
      // should not trigger submit on Enter if it's composing (IME input in progress) or shift/alt(option) is pressed
      if (evt.shiftKey || isComposing) {
        // Allow line break for Shift+Enter, or during IME composition
        return;
      }

      if (evt.altKey) {
        const newValue = displayValue + '\n';
        setDisplayValue(newValue);
        setValue(newValue);
        return;
      }

      evt.preventDefault();

      // Handle interruption and queue logic
      if (handleInterruptionAndQueue()) {
        return;
      }

      if (canSubmit) {
        performSubmit();
      }
    }
  };

  const onFormSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (isLoading && hasSubmittableContent) {
      handleInterruptionAndQueue();
      return;
    }
    const canSubmit =
      !isLoading &&
      (displayValue.trim() ||
        (currentModelSupportsVision &&
          pastedImages.some((img) => img.filePath && !img.error && !img.isLoading)) ||
        allDroppedFiles.some(canSendDroppedFile));
    if (canSubmit) {
      performSubmit();
    }
  };

  const handleMentionItemSelect = (itemText: string) => {
    const beforeMention = displayValue.slice(0, mentionPopover.mentionStart);
    const afterMention = displayValue.slice(
      mentionPopover.mentionStart + 1 + mentionPopover.query.length
    );
    const newValue = `${beforeMention}${itemText}${afterMention}`;

    setDisplayValue(newValue);
    setValue(newValue);
    setMentionPopover((prev) => ({ ...prev, isOpen: false }));
    textAreaRef.current?.focus();

    // Set cursor position after the inserted file path
    setTimeout(() => {
      if (textAreaRef.current) {
        const newCursorPosition = beforeMention.length + itemText.length;
        textAreaRef.current.setSelectionRange(newCursorPosition, newCursorPosition);
      }
    }, 0);
  };

  const hasSubmittableContent =
    displayValue.trim() ||
    (currentModelSupportsVision &&
      pastedImages.some((img) => img.filePath && !img.error && !img.isLoading)) ||
    allDroppedFiles.some(canSendDroppedFile);
  const isAnyImageLoading = pastedImages.some((img) => img.isLoading);
  const isAnyDroppedFileLoading = allDroppedFiles.some((file) => file.isLoading);

  const hasPastedImageAttachments = pastedImages.some(
    (img) => img.filePath && !img.error && !img.isLoading
  );

  const visionMismatch = !currentModelSupportsVision && hasPastedImageAttachments;

  const isSubmitButtonDisabled =
    !hasSubmittableContent ||
    isAnyImageLoading ||
    isAnyDroppedFileLoading ||
    chatState === ChatState.RestartingAgent ||
    visionMismatch;

  // Queue management functions - no storage persistence, only in-memory
  const handleRemoveQueuedMessage = (messageId: string) => {
    setQueuedMessages((prev) => prev.filter((msg) => msg.id !== messageId));
  };

  const handleClearQueue = () => {
    setQueuedMessages([]);
    queuePausedRef.current = false;
    setLastInterruption(null);
  };

  const handleReorderMessages = (reorderedMessages: QueuedMessage[]) => {
    setQueuedMessages(reorderedMessages);
  };

  const handleEditMessage = (messageId: string, newContent: string) => {
    setQueuedMessages((prev) =>
      prev.map((msg) => (msg.id === messageId ? { ...msg, content: newContent } : msg))
    );
  };

  const handleStopAndSend = (messageId: string) => {
    const messageToSend = queuedMessages.find((msg) => msg.id === messageId);
    if (!messageToSend) return;

    // Stop current processing and temporarily pause queue to prevent double-send
    if (onStop) onStop();
    const wasPaused = queuePausedRef.current;
    queuePausedRef.current = true;

    // Remove the message from queue and send it immediately
    setQueuedMessages((prev) => prev.filter((msg) => msg.id !== messageId));
    LocalMessageStorage.addMessage(messageToSend.content);
    handleSubmit(
      new CustomEvent('submit', {
        detail: { value: messageToSend.content, attachments: messageToSend.attachments ?? [] },
      }) as unknown as React.FormEvent
    );

    // Restore previous pause state after a brief delay to prevent race condition
    setTimeout(() => {
      queuePausedRef.current = wasPaused;
    }, 100);
  };

  const handleResumeQueue = () => {
    queuePausedRef.current = false;
    setLastInterruption(null);
    if (!isLoading && queuedMessages.length > 0) {
      const nextMessage = queuedMessages[0];
      LocalMessageStorage.addMessage(nextMessage.content);
      handleSubmit(
        new CustomEvent('submit', {
          detail: { value: nextMessage.content, attachments: nextMessage.attachments ?? [] },
        }) as unknown as React.FormEvent
      );
      setQueuedMessages((prev) => {
        const newQueue = prev.slice(1);
        // If queue becomes empty after processing, clear the paused state
        if (newQueue.length === 0) {
          queuePausedRef.current = false;
          setLastInterruption(null);
        }
        return newQueue;
      });
    }
  };

  return (
    <div
      className={`flex flex-col relative h-auto px-4 pt-3 pb-3 transition-colors ${
        disableAnimation ? '' : 'page-transition'
      } ${
        isDraggingOver
          ? 'border-border-strong bg-background-medium/80 shadow-[var(--shadow-composer)] ring-2 ring-border-strong/30'
          : isFocused
            ? 'border-border-subtle hover:border-border-subtle shadow-[var(--shadow-composer)] bg-background-default'
            : 'border-border-subtle hover:border-border-subtle shadow-[var(--shadow-composer)] bg-background-default'
      } z-10 rounded-2xl border`}
      data-drop-zone="true"
      data-drag-active={isDraggingOver ? 'true' : 'false'}
      onDrop={handleLocalDrop}
      onDragEnter={handleLocalDragEnter}
      onDragOver={handleLocalDragOver}
      onDragLeave={handleLocalDragLeave}
    >
      {/* Message Queue Display */}
      {queuedMessages.length > 0 && (
        <MessageQueue
          queuedMessages={queuedMessages}
          onRemoveMessage={handleRemoveQueuedMessage}
          onClearQueue={handleClearQueue}
          onStopAndSend={handleStopAndSend}
          onReorderMessages={handleReorderMessages}
          onEditMessage={handleEditMessage}
          onTriggerQueueProcessing={handleResumeQueue}
          editingMessageIdRef={editingMessageIdRef}
          isPaused={queuePausedRef.current}
          className="border-b border-border-subtle"
        />
      )}
      {/* Vision-mismatch banner: shown when the user has images attached but the
          current model does not support vision. Blocks Send until resolved. */}
      {visionMismatch && (
        <div className="flex items-start gap-2 px-3 py-2 mb-2 bg-background-medium/60 border border-border-subtle rounded-lg text-xs text-text-muted">
          <svg
            className="w-3.5 h-3.5 flex-shrink-0 mt-0.5 opacity-60"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth={1.5}
            strokeLinecap="round"
            strokeLinejoin="round"
            aria-hidden="true"
          >
            <path d="M9.88 9.88a3 3 0 1 0 4.24 4.24" />
            <path d="M10.73 5.08A10.43 10.43 0 0 1 12 5c7 0 10 7 10 7a13.16 13.16 0 0 1-1.67 2.68" />
            <path d="M6.61 6.61A13.526 13.526 0 0 0 2 12s3 7 10 7a9.74 9.74 0 0 0 5.39-1.61" />
            <line x1="2" y1="2" x2="22" y2="22" />
          </svg>
          <span className="leading-snug">
            The active model can&apos;t read images. Switch to a vision-capable model, or remove
            attached images to send.
          </span>
        </div>
      )}
      {/* Input row — textarea only. Send/Stop button moved to the right end of
          the picker row below so the input width can shrink to the picker row's
          natural width (no extra space stolen by the Send button on this line). */}
      <form id="bior-chat-form" onSubmit={onFormSubmit} className="relative flex items-end">
        <div className="relative flex-1">
          <textarea
            data-testid="chat-input"
            autoFocus
            id="dynamic-textarea"
            placeholder={getNavigationShortcutText()}
            value={displayValue}
            onChange={handleChange}
            onCompositionStart={handleCompositionStart}
            onCompositionEnd={handleCompositionEnd}
            onKeyDown={handleKeyDown}
            onPaste={handlePaste}
            onFocus={() => setIsFocused(true)}
            onBlur={() => setIsFocused(false)}
            ref={textAreaRef}
            rows={1}
            style={{
              maxHeight: `${maxHeight}px`,
              overflowY: 'auto',
            }}
            className="w-full outline-none border-none focus:ring-0 bg-transparent px-3 pt-3 pb-1.5 pr-3 text-sm resize-none text-text-default placeholder:text-text-muted"
          />
        </div>
      </form>

      {/* Combined files and images preview */}
      {(pastedImages.length > 0 || allDroppedFiles.length > 0) && (
        <div className="flex flex-wrap gap-2 p-4 mt-2 border-t border-border-subtle">
          {/* Render pasted images first */}
          {pastedImages.map((img) => (
            <div key={img.id} className="relative group w-20 h-20">
              {img.dataUrl && (
                <img
                  src={img.dataUrl}
                  alt={`Pasted image ${img.id}`}
                  className={`w-full h-full object-cover rounded border ${img.error ? 'border-border-danger' : 'border-border-subtle'}`}
                />
              )}
              {img.isLoading && (
                <div className="absolute inset-0 flex items-center justify-center bg-black bg-opacity-50 rounded">
                  <div className="animate-spin rounded-full h-6 w-6 border-t-2 border-b-2 border-white"></div>
                </div>
              )}
              {img.error && !img.isLoading && (
                <div className="absolute inset-0 flex flex-col items-center justify-center bg-black bg-opacity-75 rounded p-1 text-center">
                  <p className="text-text-danger text-[11px] leading-tight break-all mb-1">
                    {img.error.substring(0, 50)}
                  </p>
                  {img.dataUrl && (
                    <Button
                      type="button"
                      onClick={() => handleRetryImageSave(img.id)}
                      title="Retry saving image"
                      variant="outline"
                      size="xs"
                    >
                      Retry
                    </Button>
                  )}
                </div>
              )}
              {!img.isLoading && (
                <Button
                  type="button"
                  shape="round"
                  onClick={() => handleRemovePastedImage(img.id)}
                  className="absolute -top-1 -right-1 opacity-0 group-hover:opacity-100 focus:opacity-100 transition-opacity z-10"
                  aria-label="Remove image"
                  variant="outline"
                  size="xs"
                >
                  <Close />
                </Button>
              )}
            </div>
          ))}

          {/* Render dropped files after pasted images */}
          {allDroppedFiles.map((file) => (
            <div key={file.id} className="relative group">
              {file.canUploadAsImage ? (
                // Image preview
                <div className="w-20 h-20">
                  {file.dataUrl && (
                    <img
                      src={file.dataUrl}
                      alt={file.name}
                      className={`w-full h-full object-cover rounded border ${file.error ? 'border-border-danger' : 'border-border-subtle'}`}
                    />
                  )}
                  {file.isLoading && (
                    <div className="absolute inset-0 flex items-center justify-center bg-black bg-opacity-50 rounded">
                      <div className="animate-spin rounded-full h-6 w-6 border-t-2 border-b-2 border-white"></div>
                    </div>
                  )}
                  {file.error && !file.isLoading && (
                    <div className="absolute inset-0 flex flex-col items-center justify-center bg-black bg-opacity-75 rounded p-1 text-center">
                      <p className="text-text-danger text-[11px] leading-tight break-all">
                        {file.error.substring(0, 30)}
                      </p>
                    </div>
                  )}
                </div>
              ) : (
                // File box preview
                <div className="flex items-center gap-2 px-3 py-2 bg-background-medium border border-border-subtle rounded-lg min-w-[120px] max-w-[200px]">
                  <div className="flex-shrink-0 w-8 h-8 bg-background-default border border-border-subtle rounded flex items-center justify-center text-xs font-mono text-text-muted">
                    {file.name.split('.').pop()?.toUpperCase() || 'FILE'}
                  </div>
                  <div className="flex-1 min-w-0">
                    <p className="text-sm text-text-default truncate" title={file.name}>
                      {file.name}
                    </p>
                    <p className="text-xs text-text-muted">{file.type || 'Unknown type'}</p>
                  </div>
                </div>
              )}
              {!file.isLoading && (
                <Button
                  type="button"
                  shape="round"
                  onClick={() => handleRemoveDroppedFile(file.id)}
                  className="absolute -top-1 -right-1 opacity-0 group-hover:opacity-100 focus:opacity-100 transition-opacity z-10"
                  aria-label="Remove file"
                  variant="outline"
                  size="xs"
                >
                  <Close />
                </Button>
              )}
            </div>
          ))}
        </div>
      )}

      {/* Secondary actions and controls row below input. */}
      <div
        ref={toolbarRef}
        data-testid="chat-input-toolbar"
        className="flex flex-row flex-nowrap items-center gap-1.5 px-2 pt-2 pb-1 relative min-w-0 overflow-hidden"
      >
        <div className="flex min-w-0 flex-1 items-center gap-1.5 overflow-hidden">
          <DirSwitcher
            className="mr-0"
            sessionId={sessionId ?? undefined}
            workingDir={sessionWorkingDir ?? getInitialWorkingDir()}
            onWorkingDirChange={(newDir) => {
              setSessionWorkingDir(newDir);
              if (onWorkingDirChange) {
                onWorkingDirChange(newDir);
              }
            }}
            onRestartStart={() => setChatState?.(ChatState.RestartingAgent)}
            onRestartEnd={() => setChatState?.(ChatState.Idle)}
          />

          <div className={TOOLBAR_GROUP_CLASS}>
            <BottomMenuExtensionSelection sessionId={sessionId} />
            <BottomMenuSkillSelection sessionId={sessionId} />
            <BottomMenuKnowledgeSelection />
          </div>

          <div className={TOOLBAR_DIVIDER_CLASS} />

          {!useCompactControls && (
            <div className="flex min-w-0 flex-row items-center gap-1">
              <Tooltip>
                <div className="min-w-0">
                  <ModelsBottomBar
                    sessionId={sessionId}
                    dropdownRef={dropdownRef}
                    setView={setView}
                    alerts={alerts}
                    hideAlertPopover
                  />
                </div>
              </Tooltip>
              {COST_TRACKING_ENABLED && (
                <div className={TOOLBAR_GROUP_CLASS}>
                  <CostTracker
                    inputTokens={accumulatedInputTokens}
                    outputTokens={accumulatedOutputTokens}
                    sessionCosts={sessionCosts}
                  />
                </div>
              )}
              <ContextWindowIndicator
                totalTokens={totalTokens}
                tokenLimit={tokenLimit}
                isTokenLimitLoaded={isTokenLimitLoaded}
                onCompact={() => {
                  handleSubmit(
                    new CustomEvent('submit', {
                      detail: { value: MANUAL_COMPACT_TRIGGER },
                    }) as unknown as React.FormEvent
                  );
                }}
              />
            </div>
          )}
        </div>

        {/* Send / Stop button — on far right of picker row. */}
        <div className="ml-auto flex flex-shrink-0 items-center gap-1 pl-1">
          {useCompactControls && (
            <Popover open={pickerExpanded} onOpenChange={setPickerExpanded}>
              <PopoverTrigger asChild>
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  shape="round"
                  className={`${TOOLBAR_ICON_BUTTON_CLASS} cursor-pointer`}
                  aria-label={pickerExpanded ? 'Collapse extra controls' : 'Expand extra controls'}
                >
                  {pickerExpanded ? (
                    <ChevronLeft className="w-4 h-4" />
                  ) : (
                    <ChevronRight className="w-4 h-4" />
                  )}
                </Button>
              </PopoverTrigger>
              <PopoverContent side="top" align="end" className="flex flex-col gap-0.5 w-72 p-1.5">
                <PickerRow>
                  <ModelsBottomBar
                    sessionId={sessionId}
                    dropdownRef={dropdownRef}
                    setView={setView}
                    alerts={alerts}
                    hideAlertPopover
                  />
                </PickerRow>
                {COST_TRACKING_ENABLED && (
                  <PickerRow>
                    <CostTracker
                      inputTokens={accumulatedInputTokens}
                      outputTokens={accumulatedOutputTokens}
                      sessionCosts={sessionCosts}
                    />
                  </PickerRow>
                )}
                <ContextWindowGauge
                  totalTokens={totalTokens}
                  tokenLimit={tokenLimit}
                  isTokenLimitLoaded={isTokenLimitLoaded}
                  onCompact={() => {
                    handleSubmit(
                      new CustomEvent('submit', {
                        detail: { value: MANUAL_COMPACT_TRIGGER },
                      }) as unknown as React.FormEvent
                    );
                    setPickerExpanded(false);
                  }}
                />
              </PopoverContent>
            </Popover>
          )}
          {isLoading && !hasSubmittableContent ? (
            <Button
              type="button"
              onClick={onStop}
              size="sm"
              shape="round"
              variant="outline"
              className="bg-background-accent text-text-on-accent hover:bg-background-accent/90 rounded-md px-3 py-1.5"
            >
              <Stop />
            </Button>
          ) : (
            <Tooltip>
              <TooltipTrigger asChild>
                <span>
                  <Button
                    type="submit"
                    form="bior-chat-form"
                    size="sm"
                    shape="pill"
                    variant="outline"
                    disabled={isSubmitButtonDisabled}
                    className={`rounded-md px-3 py-1.5 flex items-center gap-1 ${
                      isSubmitButtonDisabled
                        ? 'bg-background-accent text-text-on-accent cursor-not-allowed opacity-50'
                        : 'bg-background-accent text-text-on-accent hover:bg-background-accent/90 hover:cursor-pointer'
                    }`}
                  >
                    <Send className="w-3.5 h-3.5" />
                    <span className="text-xs">Send</span>
                  </Button>
                </span>
              </TooltipTrigger>
              <TooltipContent>
                <p>
                  {isAnyImageLoading
                    ? 'Waiting for images to save...'
                    : isAnyDroppedFileLoading
                      ? 'Processing dropped files...'
                      : chatState === ChatState.RestartingAgent
                        ? 'Restarting session...'
                        : 'Send'}
                </p>
              </TooltipContent>
            </Tooltip>
          )}
        </div>
        <MentionPopover
          ref={mentionPopoverRef}
          isOpen={mentionPopover.isOpen}
          isSlashCommand={mentionPopover.isSlashCommand}
          onClose={() => setMentionPopover((prev) => ({ ...prev, isOpen: false }))}
          onSelect={handleMentionItemSelect}
          position={mentionPopover.position}
          query={mentionPopover.query}
          selectedIndex={mentionPopover.selectedIndex}
          onSelectedIndexChange={(index) =>
            setMentionPopover((prev) => ({ ...prev, selectedIndex: index }))
          }
          workingDir={sessionWorkingDir ?? getInitialWorkingDir()}
          sessionId={sessionId}
        />
      </div>
    </div>
  );
}
