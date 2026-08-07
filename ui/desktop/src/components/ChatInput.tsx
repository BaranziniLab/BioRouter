import React, { useRef, useState, useEffect, useMemo, useCallback } from 'react';
import { ChevronsDownUp, Plus, Send, X } from './icons/app-icons';
import { Popover, PopoverContent, PopoverTrigger } from './ui/popover';
import { useComposerToolbarCollapsed } from './bottom_menu/useComposerToolbarCollapsed';
import { ContextWindowIndicator } from './ContextWindowIndicator';
import { Tooltip, TooltipContent, TooltipTrigger } from './ui/Tooltip';
import { Button } from './ui/button';
import type { View } from '../utils/navigationUtils';
import Stop from './ui/Stop';
import { ChatState } from '../types/chatState';
import debounce from 'lodash/debounce';
import { LocalMessageStorage } from '../utils/localMessageStorage';
import { DirSwitcher } from './bottom_menu/DirSwitcher';
import ModelsBottomBar from './settings/models/bottom_bar/ModelsBottomBar';
import { BottomMenuExtensionSelection } from './bottom_menu/BottomMenuExtensionSelection';
import { BottomMenuSkillSelection } from './bottom_menu/BottomMenuSkillSelection';
import { BottomMenuKnowledgeSelection } from './bottom_menu/BottomMenuKnowledgeSelection';
import { BottomMenuReasoningEffort } from './bottom_menu/BottomMenuReasoningEffort';
import { AlertType, useAlerts } from './alerts';
import { useConfig } from './ConfigContext';
import { useModelAndProvider } from './ModelAndProviderContext';
import MentionPopover, { DisplayItemWithMatch } from './MentionPopover';
import { COST_TRACKING_ENABLED } from '../updates';
import { CostTracker } from './bottom_menu/CostTracker';
import type { ModelCostRow } from '../hooks/useCostTracking';
import { DroppedFile, useFileDrop } from '../hooks/useFileDrop';
import { useDiverge } from '../hooks/useDiverge';
import { Workflow } from '../workflow';
import MessageQueue from './MessageQueue';
import { detectInterruption } from '../utils/interruptionDetector';
import { getSession, llamacppStatus, Message } from '../api';
import { userActionHeaders } from '../utils/userAction';
import type { SessionClassification } from '../api/types.gen';
import { getInitialWorkingDir } from '../utils/workingDir';
import { getPredefinedModelsFromEnv } from './settings/models/predefinedModelsUtils';
import { getNavigationShortcutText } from '../utils/keyboardShortcuts';
import type { UserAttachment } from '../types/message';
import { useStopAcknowledgement } from '../hooks/useStopAcknowledgement';
import { cn } from '../utils';
import {
  appendComposerRef,
  joinComposerText,
  removeComposerRefAt,
  splitComposerText,
} from '../utils/composerRefs';
import { findRefTags } from '../utils/resourceRefs';
import { ResourceRefChip } from './ResourceRefChip';

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
/**
 * The composer toolbar's spacing is made to MEAN something.
 *
 * It used to run at one flat rhythm — `gap-1.5` between every child, divider or
 * not — so the row read as an undifferentiated run of glyphs and the three
 * things it actually holds (where the agent works · what it can reach · which
 * model answers) were indistinguishable. Now: chips inside a group sit at 2px,
 * and a divider opens a 21px channel between groups. Nothing was added or
 * reordered; only the spacing changed, and the spacing is now the grouping.
 */
const TOOLBAR_DIVIDER_CLASS = 'h-4 w-px flex-shrink-0 bg-border-subtle mx-[10px]';
const TOOLBAR_GROUP_CLASS = 'flex flex-shrink-0 items-center gap-0.5';

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

interface ChatInputProps {
  sessionId: string | null;
  handleSubmit: (e: React.FormEvent) => void;
  chatState: ChatState;
  setChatState?: (state: ChatState) => void;
  onStop?: () => void;
  /** BR-61 soft interrupt: inject text into the turn that is already running
   * (no cancel, no lost work). Resolves false when there was nothing to steer,
   * in which case the caller must send/queue the text normally. */
  onSteer?: (text: string) => Promise<boolean>;
  commandHistory?: string[];
  initialValue?: string;
  droppedFiles?: DroppedFile[];
  onFilesProcessed?: () => void;
  setView: (view: View) => void;
  totalTokens?: number;
  accumulatedInputTokens?: number;
  accumulatedOutputTokens?: number;
  /**
   * #22 — the transcript LENGTH, not the array. The composer only ever asks
   * "is the conversation empty?", and taking the whole array as a prop made
   * this 1900-line component re-render on every streamed token (the array
   * identity changes per event, the length almost never does).
   */
  messagesLength?: number;
  /**
   * #44 — authoritative working-dir lock, derived by the owner of the session
   * metadata (BaseChat, via `deriveWorkingDirLocked`). `messagesLength` alone
   * misleads while a resumed transcript hydrates (0 for a non-empty session)
   * and after a failed optimistic first submit (>0 for a server-empty
   * session), so when this prop is provided it wins; the `messagesLength > 0`
   * fallback keeps callers that do not track session metadata working.
   */
  workingDirLocked?: boolean;
  sessionCosts?: {
    [key: string]: {
      inputTokens: number;
      outputTokens: number;
      totalCost: number;
    };
  };
  /** Real per-model usage rows from the token ledger (Issue #1 breakdown). */
  modelCostRows?: ModelCostRow[];
  disableAnimation?: boolean;
  workflow?: Workflow | null;
  workflowAccepted?: boolean;
  initialPrompt?: string;
  toolCount: number;
  append?: (message: Message) => void;
  onWorkingDirChange?: (newDir: string) => void;
  /** Optional override for vision capability. When the chat is bound to a
   * specific session whose model differs from the user's global default
   * (notably tabs and split panes), the override reflects the session's actual
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
  onSteer,
  commandHistory = [],
  initialValue = '',
  droppedFiles = [],
  onFilesProcessed,
  setView,
  totalTokens,
  accumulatedInputTokens,
  accumulatedOutputTokens,
  messagesLength = 0,
  workingDirLocked,
  disableAnimation = false,
  sessionCosts,
  modelCostRows,
  workflowAccepted,
  initialPrompt,
  toolCount,
  append: _append,
  onWorkingDirChange,
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
  // Prefer the session-scoped flag when provided. This matters when several
  // chats are open at once (each may be bound to a different model than the
  // user's global default) and after per-session model switches.
  const currentModelSupportsVision =
    supportsVisionOverride !== undefined ? supportsVisionOverride : globalSupportsVision;
  const currentModelSupportedInputMimeTypes =
    supportedInputMimeTypesOverride !== undefined
      ? supportedInputMimeTypesOverride
      : globalSupportedInputMimeTypes;
  const [tokenLimit, setTokenLimit] = useState<number>(TOKEN_LIMIT_DEFAULT);
  const [isTokenLimitLoaded, setIsTokenLimitLoaded] = useState(false);
  const [sessionWorkingDir, setSessionWorkingDir] = useState<string | null>(null);

  // Branch-the-conversation action shared with the message-level Diverge button.
  const { diverge } = useDiverge();

  useEffect(() => {
    if (!sessionId) {
      return;
    }

    const fetchSessionWorkingDir = async () => {
      try {
        const response = await getSession({
          path: { session_id: sessionId },
          // Issue #56 Task 58: reading a private chat needs the proof-of-user.
          headers: await userActionHeaders(),
        });
        if (response.data?.working_dir) {
          setSessionWorkingDir(response.data.working_dir);
        }
      } catch (error) {
        console.error('[ChatInput] Failed to fetch session working dir:', error);
      }
    };

    fetchSessionWorkingDir();
  }, [sessionId]);

  /**
   * The chat's privacy tier, for the two composer surfaces that need it
   * (issue #56, §14.2 / §14.5): the model chip's dot and the extension
   * selector's pairing state.
   *
   * Its own effect rather than a second read inside the working-directory one
   * above, because it must re-read when a turn ENDS — the classification
   * ratchets on the provider bind, so a chat that becomes private mid-session
   * would otherwise keep showing the tier it had when the composer mounted.
   * Folding it into the working-directory fetch would also re-apply a
   * server-side `working_dir` over a change the user had just made locally.
   *
   * Left `undefined` on any failure. That is not "public": both consumers treat
   * an unresolved tier as "judge nothing", because walling a working tool on a
   * failed read is the same defect as hiding it.
   */
  const [sessionPrivacyTier, setSessionPrivacyTier] = useState<SessionClassification | undefined>(
    undefined
  );
  useEffect(() => {
    // Clear FIRST, on every rebind and not only on the no-session case.
    // `BaseChat` is keyed by tab rather than by session, so this component
    // survives a move from one chat to another; keeping the old value until the
    // new read lands (or forever, if it throws) makes both consumers assert the
    // previous chat's tier about this one. In the private -> public direction
    // that greys out every model the new chat may legitimately run, and in the
    // reverse it paints a Private dot on a chat with no such guarantee.
    setSessionPrivacyTier(undefined);

    if (!sessionId) {
      return;
    }

    // Both of this effect's readers — the mount fetch and every
    // `message-stream-finished` refresh — share this closure, so plain locals
    // are enough and no ref is needed. `cancelled` retires the whole effect
    // when the session rebinds; `latestIssued` orders the reads within it,
    // because two can be in flight at once and they settle in whatever order
    // the daemon answers. "Last to land" is not "newest": without this a slow
    // first read overwrites the fresher answer that already arrived.
    let cancelled = false;
    let latestIssued = 0;

    const readTier = async () => {
      const issued = ++latestIssued;
      try {
        const response = await getSession({
          path: { session_id: sessionId },
          // Issue #56 Task 58: and this read is *about* the tier, so it is
          // exactly the read the gate refuses without the header.
          headers: await userActionHeaders(),
        });
        if (!cancelled && issued === latestIssued && response.data?.privacy_tier) {
          setSessionPrivacyTier(response.data.privacy_tier);
        }
      } catch (error) {
        console.error('[ChatInput] Failed to read the session privacy tier:', error);
      }
    };

    void readTier();
    window.addEventListener('message-stream-finished', readTier);
    return () => {
      cancelled = true;
      window.removeEventListener('message-stream-finished', readTier);
    };
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
    if (workflowAccepted && initialPrompt && messagesLength === 0) {
      setDisplayValue(initialPrompt);
      setValue(initialPrompt);
      setTimeout(() => {
        textAreaRef.current?.focus();
      }, 0);
    }
  }, [workflowAccepted, initialPrompt, messagesLength]);

  // State to track if the IME is composing (i.e., in the middle of Japanese IME input)
  const [isComposing, setIsComposing] = useState(false);
  const [historyIndex, setHistoryIndex] = useState(-1);
  const [savedInput, setSavedInput] = useState('');
  const [isInGlobalHistory, setIsInGlobalHistory] = useState(false);
  const [hasUserTyped, setHasUserTyped] = useState(false);
  const textAreaRef = useRef<HTMLTextAreaElement>(null);
  const timeoutRefsRef = useRef<Set<ReturnType<typeof setTimeout>>>(new Set());
  // Rung 3b of the yield ladder: when the composer's control row is too narrow
  // to lay every picker out, they collapse behind a single "+" at the lower-left
  // instead of overlapping. Measured on the toolbar's own box (see the hook).
  const toolbarRef = useRef<HTMLDivElement>(null);
  const composerToolbarCollapsed = useComposerToolbarCollapsed(toolbarRef);

  // Re-populate the composer when a submit failed before the backend accepted it
  // (e.g. backend unreachable): BaseChat.handleCreateSessionError dispatches
  // 'restore-chat-input' with the text that performSubmit had already cleared, so
  // the user does not silently lose what they typed. Match by sessionId — with
  // `null === null` for the pre-session Home composer — so a broadcast can only
  // restore the input that actually submitted, never a sibling.
  useEffect(() => {
    const handler = (e: Event) => {
      const detail = (e as CustomEvent<{ sessionId?: string | null; value?: string }>).detail;
      if ((detail?.sessionId ?? null) !== (sessionId ?? null)) return;
      if (typeof detail?.value !== 'string' || !detail.value) return;
      setDisplayValue(detail.value);
      setValue(detail.value);
      setHasUserTyped(true);
      textAreaRef.current?.focus();
    };
    window.addEventListener('restore-chat-input', handler);
    return () => window.removeEventListener('restore-chat-input', handler);
  }, [sessionId]);

  // The landing state's suggestion chips write into the composer through this,
  // NOT through `initialValue`. `initialValue` is a MOUNT value whose effect also
  // deletes every pasted image (see above), so re-pointing it at a suggestion
  // would silently discard an attachment the user had already added. This only
  // sets text, and it FILLS rather than SENDS: a suggestion the user cannot edit
  // before it runs is a button pretending to be a prompt. Matched by sessionId
  // (`null === null` for the pre-session composer) exactly like the restore
  // channel, so a broadcast can never land in a sibling chat.
  useEffect(() => {
    const handler = (e: Event) => {
      const detail = (e as CustomEvent<{ sessionId?: string | null; value?: string }>).detail;
      if ((detail?.sessionId ?? null) !== (sessionId ?? null)) return;
      if (typeof detail?.value !== 'string' || !detail.value) return;
      setDisplayValue(detail.value);
      setValue(detail.value);
      setHasUserTyped(true);
      textAreaRef.current?.focus();
    };
    window.addEventListener('insert-chat-input', handler);
    return () => window.removeEventListener('insert-chat-input', handler);
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

  // Stable identities: these are pure functions of their argument, so memoising
  // with an empty dep list keeps the useCallback at the bottom of this component
  // from re-creating on every render.
  const droppedFilePath = useCallback((file: DroppedFile) => file.sourcePath || file.path, []);
  const droppedImageAttachmentPath = useCallback(
    (file: DroppedFile) => file.stagedPath || file.path,
    []
  );

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
        compactIcon: <ChevronsDownUp size={12} />,
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

  // Issue #65 — the composer's two views of one string.
  //
  // `displayValue` stays the whole message, reference tags included, because
  // every other seam in this component already carries it: draft save/restore,
  // the `?prompt=` deep link, history navigation, the queue, steering, submit.
  // Holding references in their own state would mean teaching each of those
  // about them, and each one missed is a reference the user attached and the
  // agent never sees.
  //
  // What the *textarea* binds to is the body — the message with the tags taken
  // out — so the user never sees ~45 characters of XML where they typed a
  // sentence. The tags come back as chips in the rail below. `composerRefs` is
  // the parse, so a chip on screen is always a reference the agent resolves.
  const { body: composerBody, refs: composerRefs } = useMemo(
    () => splitComposerText(displayValue),
    [displayValue]
  );

  const setComposerText = useCallback(
    (next: string) => {
      setDisplayValue(next);
      updateValue(next);
    },
    [updateValue]
  );

  /** Replace the prose, keeping whatever references are attached. */
  const setComposerBody = useCallback(
    (body: string) => setComposerText(joinComposerText(body, composerRefs)),
    [composerRefs, setComposerText]
  );

  const handleRemoveReference = useCallback(
    (index: number) => {
      setComposerText(removeComposerRefAt(displayValue, index));
      textAreaRef.current?.focus();
    },
    [displayValue, setComposerText]
  );

  // Reset textarea height when the prose is empty. Keyed off the body, not the
  // whole message: a message that is nothing but a chip shows an empty box.
  useEffect(() => {
    if (textAreaRef.current && composerBody === '') {
      textAreaRef.current.style.height = 'auto';
    }
  }, [composerBody]);

  const handleChange = (evt: React.ChangeEvent<HTMLTextAreaElement>) => {
    const val = evt.target.value;
    const cursorPosition = evt.target.selectionStart;

    setComposerBody(val);
    setHasUserTyped(true);
    // The textarea's offsets are body offsets, and so is everything the mention
    // popover computes from them.
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
  // Every path that stops a running turn goes through `stopAck.trigger` rather
  // than calling `onStop` directly, so the hard interrupt is confirmed the same
  // way no matter which control fired it (the Stop button, "Stop and Send" on a
  // queued message, or a typed interruption phrase).
  const stopAck = useStopAcknowledgement(onStop);

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

    // The prose again, for the same reason as the /diverge check: "stop" is an
    // interruption whether or not the user also left a skill attached, and the
    // detector's short-input branch would not see it past 45 characters of tag.
    const interruptionMatch = detectInterruption(composerBody.trim());

    if (interruptionMatch && interruptionMatch.shouldInterrupt) {
      setLastInterruption(interruptionMatch.matchedText);
      setChatState?.(ChatState.Idle);
      stopAck.trigger();
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

  // --- BR-61: soft interrupt ("steer") ---------------------------------------
  // Hand a message to the turn that is *already running* instead of queueing it
  // until the turn ends (the default) or stopping the agent outright: the server
  // queues it on the agent, which injects it at its next loop boundary, so no
  // in-flight tool work is thrown away. Text only — a soft interrupt has no
  // attachment channel, so anything with images/files takes the normal path.

  // A fresh view of `isLoading` for callbacks that resolve after an await (the
  // value closed over at click time is stale by the time the POST answers).
  const isLoadingRef = useRef(isLoading);
  useEffect(() => {
    isLoadingRef.current = isLoading;
  }, [isLoading]);

  const canSteer = Boolean(onSteer) && isLoading;

  // Never drop the user's words: if the steer was refused (the turn ended in the
  // meantime) send the text now, or re-queue it if a turn is somehow running.
  const sendOrQueueText = useCallback(
    (content: string) => {
      if (isLoadingRef.current) {
        setQueuedMessages((prev) => [
          ...prev,
          {
            id: Date.now().toString() + Math.random().toString(36).substr(2, 9),
            content,
            attachments: [],
            timestamp: Date.now(),
          },
        ]);
        return;
      }
      LocalMessageStorage.addMessage(content);
      handleSubmit(
        new CustomEvent('submit', {
          detail: { value: content, attachments: [] },
        }) as unknown as React.FormEvent
      );
    },
    [handleSubmit]
  );

  const steerText = useCallback(
    (content: string) => {
      if (!onSteer) return;
      void onSteer(content).then((accepted) => {
        if (accepted) {
          // The agent echoes the steer back as a user message on the live stream
          // once it consumes it, so nothing is appended to the transcript here.
          LocalMessageStorage.addMessage(content);
        } else {
          sendOrQueueText(content);
        }
      });
    },
    [onSteer, sendOrQueueText]
  );

  /** Cmd/Ctrl+Enter while a turn runs: steer with whatever is in the composer. */
  const handleSteerFromComposer = useCallback((): boolean => {
    if (!canSteer) return false;
    const text = displayValue.trim();
    if (!text || pastedImages.length > 0 || allDroppedFiles.length > 0) {
      return false;
    }
    steerText(text);
    setDisplayValue('');
    setValue('');
    return true;
  }, [canSteer, displayValue, pastedImages.length, allDroppedFiles.length, steerText]);

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
      //
      // Matched against the prose, not the whole message: a reference is drawn
      // as a chip and is invisible in the textarea, so comparing the raw text
      // would let an attached chip silently defeat a command that is — to the
      // user, correctly — the only thing in the box.
      const trimmedCandidate = splitComposerText(text ?? displayValue).body.trim();
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
        // The newline belongs to the prose, not after the reference block.
        setComposerBody(composerBody + '\n');
        return;
      }

      evt.preventDefault();

      // BR-61: Cmd/Ctrl+Enter while a turn is running steers it — the message
      // reaches the model on its next step instead of waiting for the turn to end.
      if ((evt.metaKey || evt.ctrlKey) && handleSteerFromComposer()) {
        return;
      }

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
    const beforeMention = composerBody.slice(0, mentionPopover.mentionStart);
    const afterMention = composerBody.slice(
      mentionPopover.mentionStart + 1 + mentionPopover.query.length
    );

    // A picked resource is a reference, not prose: it goes to the chip rail and
    // the `@query` it replaced just disappears. Detected by running the inserted
    // text back through the real parser rather than by asking the popover what
    // kind of item it was — the parser is the same one the agent uses, so the
    // composer cannot draw a chip for something the agent would ignore.
    const inserted = findRefTags(itemText);
    const isReference = inserted.length === 1 && inserted[0].raw === itemText.trim();

    const nextBody = `${beforeMention}${isReference ? '' : itemText}${afterMention}`;
    const nextText = joinComposerText(nextBody, composerRefs);
    setComposerText(
      isReference
        ? appendComposerRef(nextText, inserted[0].kind, inserted[0].value, inserted[0].label)
        : nextText
    );
    setMentionPopover((prev) => ({ ...prev, isOpen: false }));
    textAreaRef.current?.focus();

    // Set cursor position after the inserted file path
    setTimeout(() => {
      if (textAreaRef.current) {
        const newCursorPosition = beforeMention.length + (isReference ? 0 : itemText.length);
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

  /** BR-61: send a queued message into the running turn without stopping it. */
  const handleSteerMessage = (messageId: string) => {
    const messageToSteer = queuedMessages.find((msg) => msg.id === messageId);
    if (!messageToSteer || !canSteer) return;

    setQueuedMessages((prev) => prev.filter((msg) => msg.id !== messageId));
    steerText(messageToSteer.content);
  };

  const handleStopAndSend = (messageId: string) => {
    const messageToSend = queuedMessages.find((msg) => msg.id === messageId);
    if (!messageToSend) return;

    // Stop current processing and temporarily pause queue to prevent double-send
    stopAck.trigger();
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
      className={cn(
        // ONE 12px inset grid. Four used to meet inside this card — `px-4 pt-3
        // pb-3` on the shell, `px-3 pt-3 pb-1.5` on the textarea, `px-2 pt-2
        // pb-1` on the toolbar and `p-4` on the attachments — so the placeholder,
        // the first toolbar chip and an attachment thumbnail each started at a
        // different x. The shell now owns the inset and the children own only
        // the vertical rhythm between them.
        'relative z-10 flex h-auto flex-col p-3 rounded-container',
        // ELEVATION OR A BORDER, NEVER BOTH. The composer was the one element in
        // the app stating its edge twice: a 1px border AND --shadow-composer.
        // The shadow is what lifts it off the canvas; the border was the
        // redundant half. It becomes the shared floating-surface recipe —
        // elevation plus a 1px INSET ring, which is what keeps the edge crisp in
        // dark families where a shadow alone disappears, and which (being inside
        // the box) costs no layout.
        'bg-background-default shadow-composer inset-shadow-hairline',
        'transition-[box-shadow,background-color]',
        // Focus stops being a border-COLOUR shift and becomes the same 2px inset
        // accent ring every other input in the system uses (§3.2) — the
        // `inset-shadow-accent` token, composed with the elevation rather than
        // replacing it. Nothing shifts, so drag-over can now speak the same
        // language one step louder instead of inventing an outset `ring-2` that
        // painted OUTSIDE the composer's box and overlapped the canvas.
        (isFocused || isDraggingOver) && 'inset-shadow-accent',
        isDraggingOver && 'bg-background-medium/80',
        !disableAnimation && 'page-transition'
      )}
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
          onSteerMessage={canSteer ? handleSteerMessage : undefined}
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
        <div className="flex items-start gap-2 px-3 py-2 mb-2 bg-background-medium/60 border border-border-subtle rounded-element text-supporting text-text-muted">
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
      {/* Attached resources (issue #65). The canonical form of a reference is a
 `<biorouter-ref …>` tag, which is the only form that survives a name with a
 space or a quote in it — and is far too much markup to leave sitting in the
 user's sentence. It lives in the message text; this rail is where the user
 sees and manages it. Above the prose because a reference qualifies the whole
 message rather than a point in it, and because the row below the textarea is
 already the attachments area for images and files. */}
      {composerRefs.length > 0 && (
        <div
          data-testid="composer-reference-rail"
          className="mb-1.5 flex flex-wrap items-center gap-1.5 px-1"
        >
          {composerRefs.map((ref, index) => (
            <ResourceRefChip
              key={`${ref.kind}:${ref.value}`}
              refSpan={ref}
              onRemove={() => handleRemoveReference(index)}
            />
          ))}
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
            value={composerBody}
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
            // No inset of its own: the shell's 12px IS the composer's left edge,
            // and the placeholder has to start on it so the first toolbar chip
            // below can line up with the text above it.
            className="w-full resize-none border-none bg-transparent p-0 pb-1.5 text-body text-text-default placeholder:text-text-muted"
          />
        </div>
      </form>

      {/* Combined files and images preview */}
      {(pastedImages.length > 0 || allDroppedFiles.length > 0) && (
        <div className="flex flex-wrap gap-2 pt-3 mt-2 border-t border-border-subtle">
          {/* Render pasted images first */}
          {pastedImages.map((img) => (
            <div key={img.id} className="relative group w-20 h-20">
              {img.dataUrl && (
                <img
                  src={img.dataUrl}
                  alt={`Pasted image ${img.id}`}
                  className={`w-full h-full object-cover rounded-element border ${img.error ? 'border-border-danger' : 'border-border-subtle'}`}
                />
              )}
              {img.isLoading && (
                <div className="absolute inset-0 flex items-center justify-center bg-black bg-opacity-50 rounded-element">
                  <div className="animate-spin rounded-full h-6 w-6 border-t-2 border-b-2 border-white"></div>
                </div>
              )}
              {img.error && !img.isLoading && (
                <div className="absolute inset-0 flex flex-col items-center justify-center bg-black bg-opacity-75 rounded-element p-1 text-center">
                  <p className="text-text-danger text-supporting leading-tight break-all mb-1">
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
                  <X />
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
                      className={`w-full h-full object-cover rounded-element border ${file.error ? 'border-border-danger' : 'border-border-subtle'}`}
                    />
                  )}
                  {file.isLoading && (
                    <div className="absolute inset-0 flex items-center justify-center bg-black bg-opacity-50 rounded-element">
                      <div className="animate-spin rounded-full h-6 w-6 border-t-2 border-b-2 border-white"></div>
                    </div>
                  )}
                  {file.error && !file.isLoading && (
                    <div className="absolute inset-0 flex flex-col items-center justify-center bg-black bg-opacity-75 rounded-element p-1 text-center">
                      <p className="text-text-danger text-supporting leading-tight break-all">
                        {file.error.substring(0, 30)}
                      </p>
                    </div>
                  )}
                </div>
              ) : (
                // File box preview
                <div className="flex items-center gap-2 px-3 py-2 bg-background-medium border border-border-subtle rounded-element min-w-[120px] max-w-[200px]">
                  <div className="flex-shrink-0 w-8 h-8 bg-background-default border border-border-subtle rounded-inner flex items-center justify-center text-supporting font-mono text-text-muted">
                    {file.name.split('.').pop()?.toUpperCase() || 'FILE'}
                  </div>
                  <div className="flex-1 min-w-0">
                    <p className="text-secondary text-text-default truncate" title={file.name}>
                      {file.name}
                    </p>
                    <p className="text-supporting text-text-muted">{file.type || 'Unknown type'}</p>
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
                  <X />
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
        className="relative flex min-w-0 flex-row flex-nowrap items-center overflow-hidden pt-2"
      >
        {(() => {
          // Defined once, arranged two ways: inline when the row is wide enough,
          // and stacked inside a "+" popover when it is not. Only the rendered
          // branch mounts, and each control reads its own state, so composing
          // them here rather than duplicating the JSX keeps the two layouts from
          // drifting.
          // #44: the working dir is choosable only while the chat is completely
          // empty (pre-session #39 path included); the first message locks it for
          // the session's lifetime. Prefer the authoritative lock from BaseChat
          // (hydration- and failed-submit-aware); fall back to the transcript
          // length for callers that do not track session metadata.
          const workingDirIsLocked = workingDirLocked ?? messagesLength > 0;
          const dirSwitcher = (
            <DirSwitcher
              className="mr-0"
              sessionId={sessionId ?? undefined}
              locked={workingDirIsLocked}
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
          );
          const extensionsSkillsKnowledge = (
            <>
              <BottomMenuExtensionSelection
                sessionId={sessionId}
                privacyTier={sessionPrivacyTier}
              />
              <BottomMenuSkillSelection sessionId={sessionId} />
              <BottomMenuKnowledgeSelection />
            </>
          );
          const reasoning = <BottomMenuReasoningEffort />;
          const model = (
            <div className="min-w-0">
              <ModelsBottomBar
                sessionId={sessionId}
                privacyTier={sessionPrivacyTier}
                dropdownRef={dropdownRef}
                setView={setView}
                alerts={alerts}
                hideAlertPopover
              />
            </div>
          );
          const contextGauge = (
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
          );
          const cost = COST_TRACKING_ENABLED ? (
            <CostTracker
              inputTokens={accumulatedInputTokens}
              outputTokens={accumulatedOutputTokens}
              sessionCosts={sessionCosts}
              modelCostRows={modelCostRows}
            />
          ) : null;

          if (composerToolbarCollapsed) {
            return (
              <div className="flex min-w-0 flex-1 items-center gap-1 overflow-hidden">
                <Popover>
                  <PopoverTrigger asChild>
                    <button
                      type="button"
                      aria-label="Chat tools and settings"
                      title="Chat tools and settings"
                      data-testid="composer-tools-collapsed"
                      // The 32px icon-button box, so the collapsed row is exactly
                      // as tall as the expanded one — the composer does not
                      // resize when the artifact panel opens, only its contents
                      // change. It was 28px, which shortened the row by 4px at
                      // the exact moment the layout was already moving.
                      className="flex size-8 flex-shrink-0 items-center justify-center rounded-element text-text-muted tint-interactive transition-colors hover:text-text-default"
                    >
                      <Plus className="size-4" />
                    </button>
                  </PopoverTrigger>
                  {/* Portaled, so the enclosing overflow-hidden never clips it.
                    side/align place it above the "+", growing up out of the row. */}
                  <PopoverContent
                    side="top"
                    align="start"
                    // The standard §3.8 menu container — 4px padding, 2px between
                    // items — rather than the bespoke `w-64 p-2` it carried. The
                    // width now follows the content instead of pinning every
                    // collapsed composer to 256px regardless of what is in it.
                    className="flex min-w-[15rem] max-w-[80vw] flex-col gap-0.5 p-1"
                    data-testid="composer-tools-popover"
                  >
                    {/* The menu changes with the session, exactly as the expanded
                        row does. While the chat is still empty the directory is a
                        CONTROL and sits with the other controls. Once a turn has
                        run it is a FACT — everything the agent did is relative to
                        it — so it moves above the divider into a stated context
                        block with its reason beneath it. It is not greyed out;
                        it stops being a menu item. State what is true, don't
                        disable what was once offered. */}
                    {workingDirIsLocked ? (
                      <div className="flex flex-col gap-0.5 px-2 py-1.5">
                        {dirSwitcher}
                        <p className="text-supporting text-text-muted">
                          Set when this session started. Start a new session to work somewhere else.
                        </p>
                      </div>
                    ) : (
                      dirSwitcher
                    )}
                    <div className="my-0.5 h-px w-full bg-border-subtle" />
                    <div className="flex flex-wrap items-center gap-0.5">
                      {extensionsSkillsKnowledge}
                    </div>
                    <div className="my-0.5 h-px w-full bg-border-subtle" />
                    <div className="flex flex-wrap items-center gap-0.5">
                      {reasoning}
                      {contextGauge}
                      {cost}
                    </div>
                  </PopoverContent>
                </Popover>
                {/* The model NEVER collapses. It changes what the next message
                    costs and what it can do, so hiding it behind a disclosure at
                    exactly the width where the artifact panel is competing for
                    attention is when it matters most. */}
                {model}
              </div>
            );
          }

          return (
            <div className="flex min-w-0 flex-1 items-center overflow-hidden">
              {dirSwitcher}
              <div className={TOOLBAR_DIVIDER_CLASS} />
              <div className={TOOLBAR_GROUP_CLASS}>{extensionsSkillsKnowledge}</div>
              <div className={TOOLBAR_DIVIDER_CLASS} />
              <div className="flex min-w-0 flex-row items-center gap-0.5">
                {reasoning}
                {model}
                {contextGauge}
                {cost && <div className={TOOLBAR_GROUP_CLASS}>{cost}</div>}
              </div>
            </div>
          );
        })()}

        {/* Send / Stop — the row's one primary action, and NEVER collapsed.
            Both were `variant="outline"` repainted into an accent fill by a
            className override, which is the system's own primary variant spelled
            out longhand: the override could (and did) drift from
            `--background-accent-hover`, and a reader had to diff two class
            strings to learn the button was primary. `variant="default"` IS that
            fill. */}
        <div className="ml-auto flex flex-shrink-0 items-center gap-1 pl-2">
          {isLoading && !hasSubmittableContent ? (
            <Button
              type="button"
              onClick={stopAck.trigger}
              size="sm"
              shape="round"
              className={cn('relative', stopAck.acknowledged && 'scale-90')}
              data-testid="chat-stop-button"
              data-stop-acknowledged={stopAck.acknowledged}
              aria-label={stopAck.acknowledged ? 'Stopping response' : 'Stop response'}
              title={stopAck.acknowledged ? 'Stopping…' : 'Stop response'}
            >
              <Stop size={16} />
              {/* The press recoils the button and sends one ring outward from
                  it — motion that reads as "received", distinct from the
                  looping pulses that mean "still working". */}
              {stopAck.acknowledged && (
                <span
                  aria-hidden="true"
                  className="pointer-events-none absolute inset-0 rounded-element ring-2 ring-current animate-ping"
                />
              )}
            </Button>
          ) : (
            <Tooltip>
              <TooltipTrigger asChild>
                <span>
                  <Button
                    type="submit"
                    form="bior-chat-form"
                    size="sm"
                    shape="round"
                    disabled={isSubmitButtonDisabled}
                    aria-label="Send message"
                    className={cn(isSubmitButtonDisabled && 'cursor-not-allowed')}
                  >
                    <Send className="size-4" />
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
