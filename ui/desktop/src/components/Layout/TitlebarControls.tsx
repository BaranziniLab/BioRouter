import type { CSSProperties, PointerEvent } from 'react';
import { LayoutDashboard, Plus } from '../icons/app-icons';
import { Button } from '../ui/button';
import { SidebarTrigger } from '../ui/sidebar';

const MACOS_TRAFFIC_LIGHT_RESERVE = 100;
const NON_MACOS_TITLEBAR_INSET = 16;
const TITLEBAR_CONTROL_STRIP_WIDTH = 96;
const TITLEBAR_CONTROL_GAP = 8;

export const TITLEBAR_CONTROL_RESERVE_PROPERTY = '--biorouter-titlebar-control-reserve';

export function getTitlebarControlInset(isMacOS: boolean): number {
  return isMacOS ? MACOS_TRAFFIC_LIGHT_RESERVE : NON_MACOS_TITLEBAR_INSET;
}

export function getTitlebarControlReserve(isMacOS: boolean): number {
  return getTitlebarControlInset(isMacOS) + TITLEBAR_CONTROL_STRIP_WIDTH + TITLEBAR_CONTROL_GAP;
}

export function getSessionTitlePadding(
  isCompactSidebarOverlayOpen: boolean,
  reserveTitlebarControls: boolean
): string {
  if (isCompactSidebarOverlayOpen) return 'calc(var(--sidebar-width) + 8px)';
  if (reserveTitlebarControls) {
    return `var(${TITLEBAR_CONTROL_RESERVE_PROPERTY}, 204px)`;
  }
  return '16px';
}

interface TitlebarControlsProps {
  hidden: boolean;
  isMacOS: boolean;
  isDashboard: boolean;
  onNewWindow: () => void;
  onToggleDashboard: () => void;
}

export function TitlebarControls({
  hidden,
  isMacOS,
  isDashboard,
  onNewWindow,
  onToggleDashboard,
}: TitlebarControlsProps) {
  if (hidden) return null;

  const stopTitlebarDrag = (event: PointerEvent<HTMLDivElement>) => event.stopPropagation();

  return (
    <div
      data-testid="titlebar-controls"
      className="no-drag pointer-events-auto absolute top-2 z-[190] isolate flex items-center"
      style={
        {
          left: getTitlebarControlInset(isMacOS),
          WebkitAppRegion: 'no-drag',
        } as CSSProperties
      }
      onPointerDown={stopTitlebarDrag}
    >
      <SidebarTrigger
        data-testid="titlebar-sidebar-toggle"
        size="sm"
        shape="round"
        className="no-drag hover:!bg-background-medium"
      />
      <Button
        data-testid="titlebar-new-window"
        onClick={onNewWindow}
        className="no-drag hover:!bg-background-medium"
        variant="ghost"
        size="sm"
        shape="round"
        title="Start a new session in a new window"
      >
        <Plus className="h-4 w-4" />
      </Button>
      <Button
        data-testid="titlebar-dashboard-toggle"
        onClick={onToggleDashboard}
        className={`no-drag hover:!bg-background-medium ${
          isDashboard ? 'bg-background-medium' : ''
        }`}
        variant="ghost"
        size="sm"
        shape="round"
        title={isDashboard ? 'Exit Dashboard' : 'Open Dashboard'}
      >
        <LayoutDashboard className="h-4 w-4" />
      </Button>
    </div>
  );
}
