import { fireEvent, render, screen } from '@testing-library/react';
import { beforeAll, describe, expect, it, vi } from 'vitest';
import { SidebarProvider, useSidebar } from '../ui/sidebar';
import {
  getSessionTitlePadding,
  getTitlebarControlReserve,
  TitlebarControls,
  TITLEBAR_CONTROL_RESERVE_PROPERTY,
} from './TitlebarControls';

beforeAll(() => {
  Object.defineProperty(window, 'matchMedia', {
    writable: true,
    value: vi.fn().mockImplementation((query: string) => ({
      matches: false,
      media: query,
      onchange: null,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      addListener: vi.fn(),
      removeListener: vi.fn(),
      dispatchEvent: vi.fn(),
    })),
  });
});

function SidebarState() {
  const { state } = useSidebar();
  return <output data-testid="sidebar-state">{state}</output>;
}

describe('TitlebarControls', () => {
  it('keeps every control in a clickable non-drag layer and toggles the sidebar', () => {
    const onNewWindow = vi.fn();
    const onToggleDashboard = vi.fn();

    render(
      <SidebarProvider defaultOpen={false}>
        <TitlebarControls
          hidden={false}
          isMacOS
          isDashboard={false}
          onNewWindow={onNewWindow}
          onToggleDashboard={onToggleDashboard}
        />
        <SidebarState />
      </SidebarProvider>
    );

    const controls = screen.getByTestId('titlebar-controls');
    expect(controls).toHaveClass('no-drag', 'pointer-events-auto', 'z-[190]', 'isolate');
    expect(controls).toHaveStyle({ left: '100px' });
    expect(getTitlebarControlReserve(true)).toBe(204);
    expect(getTitlebarControlReserve(false)).toBe(120);
    expect(TITLEBAR_CONTROL_RESERVE_PROPERTY).toBe('--biorouter-titlebar-control-reserve');
    expect(getSessionTitlePadding(false, true)).toBe(
      'var(--biorouter-titlebar-control-reserve, 204px)'
    );
    expect(getSessionTitlePadding(true, true)).toBe('calc(var(--sidebar-width) + 8px)');
    expect(getSessionTitlePadding(false, false)).toBe('16px');

    fireEvent.click(screen.getByTestId('titlebar-sidebar-toggle'));
    expect(screen.getByTestId('sidebar-state')).toHaveTextContent('expanded');

    fireEvent.click(screen.getByTestId('titlebar-new-window'));
    fireEvent.click(screen.getByTestId('titlebar-dashboard-toggle'));
    expect(onNewWindow).toHaveBeenCalledOnce();
    expect(onToggleDashboard).toHaveBeenCalledOnce();
  });

  it('does not leave an invisible click layer over an open mobile sidebar', () => {
    render(
      <SidebarProvider>
        <TitlebarControls
          hidden
          isMacOS
          isDashboard={false}
          onNewWindow={vi.fn()}
          onToggleDashboard={vi.fn()}
        />
      </SidebarProvider>
    );

    expect(screen.queryByTestId('titlebar-controls')).not.toBeInTheDocument();
  });
});
