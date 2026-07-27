import React, { useState } from 'react';
import { FolderDot } from '../icons/app-icons';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '../ui/Tooltip';
import { updateWorkingDir } from '../../api';
import { toastError } from '../../toasts';
import { ChatState } from '../../types/chatState';

interface DirSwitcherProps {
  className: string;
  sessionId: string | undefined;
  workingDir: string;
  /**
   * #44: the working directory is choosable only while the chat is completely
   * empty. Once the chat has messages, the chip becomes a read-only label —
   * basename only, full path on hover — with no chooser affordance. The
   * backend enforces the same rule with a 409, so this is UX, not the guard.
   */
  locked?: boolean;
  onWorkingDirChange?: (newDir: string) => void;
  onRestartStart?: () => void;
  onRestartEnd?: () => void;
}

/**
 * The short display name for a working directory: its basename ("Desktop" for
 * /Users/wgu/Desktop). A filesystem root ("/", "C:\") has no basename and is
 * shown as-is; the home directory shows its own basename (e.g. "wgu"), which
 * stays unambiguous alongside the full path shown on hover.
 */
export function workingDirLabel(dir: string): string {
  // A Windows drive root ("C:\", "C:/") is a root, not a folder named "C:".
  if (/^[A-Za-z]:[\\/]+$/.test(dir)) return dir;
  const segments = dir.split(/[/\\]+/).filter(Boolean);
  return segments.length > 0 ? segments[segments.length - 1] : dir;
}

/**
 * #44 — whether the folder chip must render read-only, derived from
 * authoritative state rather than the loaded transcript's length alone.
 * `messages.length` misleads in exactly the two ways this function fixes:
 * it is 0 while a resumed transcript is still hydrating (which would briefly
 * unlock a non-empty session), and it is >0 after a FAILED first submit whose
 * optimistic message is retained by design in the transcript so it can go out
 * when the agent lands (which would lock a session the server still considers
 * empty). The server's 409 remains the real guard; this is the UX mirror.
 */
export function deriveWorkingDirLocked(params: {
  sessionId: string | null | undefined;
  /**
   * `message_count` from the last session fetch — the server's own word on
   * whether the chat has messages. `undefined` while the session metadata is
   * still loading.
   */
  persistedMessageCount: number | undefined;
  /** Any assistant message in the transcript proves a message reached the server. */
  hasAssistantMessage: boolean;
  chatState: ChatState;
}): boolean {
  const { sessionId, persistedMessageCount, hasAssistantMessage, chatState } = params;
  // Pre-session (#39): the chooser is always available.
  if (!sessionId) return false;
  // Until the session metadata arrives, assume locked: a resumed session must
  // be locked from first paint until history proves empty.
  if (persistedMessageCount === undefined) return true;
  if (persistedMessageCount > 0) return true;
  // The fetched count can be stale during a live conversation: an assistant
  // reply, or a turn actively in flight, means the first message has reached
  // (or is reaching) the server even though the last fetch said zero.
  if (hasAssistantMessage) return true;
  if (chatState !== ChatState.Idle && chatState !== ChatState.LoadingConversation) return true;
  // Zero persisted messages, no reply, no turn in flight: any message still in
  // the transcript is an optimistic one from a failed submit — the dir is
  // still choosable.
  return false;
}

export const DirSwitcher: React.FC<DirSwitcherProps> = ({
  className,
  sessionId,
  workingDir,
  locked = false,
  onWorkingDirChange,
  onRestartStart,
  onRestartEnd,
}) => {
  const [isTooltipOpen, setIsTooltipOpen] = useState(false);
  const [isDirectoryChooserOpen, setIsDirectoryChooserOpen] = useState(false);

  const handleDirectoryChange = async () => {
    if (isDirectoryChooserOpen) return;
    setIsDirectoryChooserOpen(true);

    let result;
    try {
      result = await window.electron.directoryChooser();
    } finally {
      setIsDirectoryChooserOpen(false);
    }

    if (result.canceled || result.filePaths.length === 0) {
      return;
    }

    const newDir = result.filePaths[0];

    if (sessionId) {
      onRestartStart?.();

      try {
        // `throwOnError` is load-bearing: the generated client otherwise
        // RESOLVES with an error object on a non-2xx (e.g. the #44 409 once
        // the chat has messages) and the catch below would never fire.
        await updateWorkingDir({
          body: { session_id: sessionId, working_dir: newDir },
          throwOnError: true,
        });
        // Reflect the change only after the server accepted it, so a refusal
        // leaves the displayed dir and the recents list untouched.
        window.electron.addRecentDir(newDir);
        onWorkingDirChange?.(newDir);
      } catch (error) {
        console.error('[DirSwitcher] Failed to update working directory:', error);
        const serverMessage =
          typeof error === 'object' &&
          error !== null &&
          'message' in error &&
          typeof (error as { message?: unknown }).message === 'string'
            ? (error as { message: string }).message
            : undefined;
        toastError({
          title: 'Working directory update failed',
          msg: serverMessage ?? 'Failed to update the working directory.',
        });
      } finally {
        onRestartEnd?.();
      }
    } else {
      window.electron.addRecentDir(newDir);
      onWorkingDirChange?.(newDir);
    }
  };

  const handleDirectoryClick = async (event: React.MouseEvent) => {
    if (isDirectoryChooserOpen) {
      event.preventDefault();
      event.stopPropagation();
      return;
    }
    const isCmdOrCtrlClick = event.metaKey || event.ctrlKey;

    if (isCmdOrCtrlClick) {
      event.preventDefault();
      event.stopPropagation();
      await window.electron.openDirectoryInExplorer(workingDir);
    } else {
      await handleDirectoryChange();
    }
  };

  // #44: once the chat has messages the working dir is immutable — render a
  // read-only label (basename, full path on hover) with no chooser affordance.
  if (locked) {
    return (
      <TooltipProvider>
        <Tooltip>
          <TooltipTrigger asChild>
            <span
              data-testid="dir-switcher-locked"
              className={`z-[100] h-7 min-w-0 rounded-md px-1 text-text-default/70 text-xs flex items-center select-none [&>svg]:size-4 ${className}`}
            >
              <FolderDot className="mr-0.5" size={16} />
              <div className="max-w-[112px] min-w-0 truncate">{workingDirLabel(workingDir)}</div>
            </span>
          </TooltipTrigger>
          <TooltipContent side="top">{workingDir}</TooltipContent>
        </Tooltip>
      </TooltipProvider>
    );
  }

  return (
    <TooltipProvider>
      <Tooltip
        open={isTooltipOpen && !isDirectoryChooserOpen}
        onOpenChange={(open) => {
          if (!isDirectoryChooserOpen) setIsTooltipOpen(open);
        }}
      >
        <TooltipTrigger asChild>
          <button
            className={`z-[100] h-7 min-w-0 rounded-md px-1 ${isDirectoryChooserOpen ? 'opacity-50' : 'hover:cursor-pointer hover:bg-background-medium hover:text-text-default'} text-text-default/70 text-xs flex items-center transition-colors [&>svg]:size-4 ${className}`}
            onClick={handleDirectoryClick}
            disabled={isDirectoryChooserOpen}
          >
            <FolderDot className="mr-0.5" size={16} />
            <div className="max-w-[112px] min-w-0 truncate [direction:rtl]">{workingDir}</div>
          </button>
        </TooltipTrigger>
        <TooltipContent side="top">{workingDir}</TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
};
