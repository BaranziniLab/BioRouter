import React from 'react';
import { Terminal } from '../icons/app-icons';
import { Tooltip, TooltipContent, TooltipTrigger } from '../ui/Tooltip';
import { Button } from '../ui/button';

interface LaunchTerminalButtonProps {
  /** Working directory the CLI should open in. */
  workingDir?: string;
}

/**
 * Opens the Biorouter CLI in a terminal rooted at the chat's working
 * directory. Lives at the left of the picker row next to DirSwitcher. If the
 * CLI isn't installed yet, it surfaces the same install card the startup check
 * uses (via the `biorouter:open-cli-install` event).
 */
export const LaunchTerminalButton: React.FC<LaunchTerminalButtonProps> = ({ workingDir }) => {
  const handleLaunchCli = async () => {
    try {
      const status = await window.electron.cliStatus();
      if (status?.onPath) {
        const res = await window.electron.launchCli(workingDir);
        if (!res.success) {
          window.dispatchEvent(new CustomEvent('biorouter:open-cli-install'));
        }
      } else {
        window.dispatchEvent(new CustomEvent('biorouter:open-cli-install'));
      }
    } catch {
      window.dispatchEvent(new CustomEvent('biorouter:open-cli-install'));
    }
  };

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          type="button"
          onClick={handleLaunchCli}
          variant="ghost"
          size="sm"
          className="flex items-center justify-center text-text-default/70 hover:text-text-default text-xs cursor-pointer transition-colors"
        >
          <Terminal className="w-4 h-4" />
        </Button>
      </TooltipTrigger>
      <TooltipContent>Open the Biorouter CLI in this folder</TooltipContent>
    </Tooltip>
  );
};
