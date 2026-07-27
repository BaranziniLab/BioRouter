import React, { useState } from 'react';
import { FolderDot } from '../icons/app-icons';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '../ui/Tooltip';
import { updateWorkingDir } from '../../api';
import { toastError } from '../../toasts';

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
  const segments = dir.split(/[/\\]+/).filter(Boolean);
  return segments.length > 0 ? segments[segments.length - 1] : dir;
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

    window.electron.addRecentDir(newDir);

    if (sessionId) {
      onWorkingDirChange?.(newDir);
      onRestartStart?.();

      try {
        await updateWorkingDir({
          body: { session_id: sessionId, working_dir: newDir },
        });
      } catch (error) {
        console.error('[DirSwitcher] Failed to update working directory:', error);
        toastError({
          title: 'Working directory update failed',
          msg: 'Failed to update the working directory.',
        });
      } finally {
        onRestartEnd?.();
      }
    } else {
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
