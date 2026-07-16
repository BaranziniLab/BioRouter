import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

vi.mock('../../hooks/use-mobile', () => ({
  useIsMobile: () => true,
}));

import { Sidebar, SidebarProvider, SidebarTrigger } from './sidebar';

describe('responsive Sidebar', () => {
  it('keeps the mobile drawer at the canonical width instead of sizing to its content', () => {
    render(
      <SidebarProvider>
        <Sidebar>
          <div>A conversation title long enough to exceed the sidebar width</div>
        </Sidebar>
        <SidebarTrigger />
      </SidebarProvider>
    );

    fireEvent.click(screen.getByRole('button', { name: 'Toggle Sidebar' }));

    const drawer = screen.getByRole('dialog');
    expect(drawer).toHaveAttribute('data-mobile', 'true');
    expect(drawer).toHaveClass(
      'w-(--sidebar-width)',
      'min-w-(--sidebar-width)',
      'max-w-(--sidebar-width)'
    );
    expect(drawer).not.toHaveClass('!w-fit', '!max-w-none');
    expect(drawer.style.getPropertyValue('--sidebar-width')).toBe('15rem');
  });
});
