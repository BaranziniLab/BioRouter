import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import {
  NotificationSurface,
  NotificationContent,
  type NotificationStatus,
} from '../NotificationSurface';

const STATUSES: NotificationStatus[] = ['success', 'error', 'warning', 'info', 'loading'];

describe('NotificationContent (shared layout primitive)', () => {
  it('renders the title and message together', () => {
    render(<NotificationContent status="success" title="Model changed" message="Now using Opus" />);
    expect(screen.getByText('Model changed')).toBeInTheDocument();
    expect(screen.getByText('Now using Opus')).toBeInTheDocument();
  });

  it('renders the message even when NO title is given (regression: message must never be dropped)', () => {
    render(<NotificationContent status="success" message="Workflow saved and ready to use." />);
    expect(screen.getByText('Workflow saved and ready to use.')).toBeInTheDocument();
  });

  it('renders a title with no message', () => {
    render(<NotificationContent status="success" title="All extensions loaded" />);
    expect(screen.getByText('All extensions loaded')).toBeInTheDocument();
  });

  it('renders a lone message at full strength, but mutes it beneath a title', () => {
    const { rerender } = render(<NotificationContent status="info" message="Standalone note" />);
    expect(screen.getByText('Standalone note').className).toContain('text-text-default');
    rerender(<NotificationContent status="info" title="Heads up" message="Standalone note" />);
    expect(screen.getByText('Standalone note').className).toContain('text-text-muted');
  });

  it('renders without throwing when neither title nor message is given, and still shows the status chip', () => {
    const { container } = render(<NotificationContent status="loading" />);
    expect(container.querySelector('[data-status="loading"]')).toBeInTheDocument();
  });

  it.each(STATUSES)('renders a status chip carrying its status and an icon for "%s"', (status) => {
    const { container } = render(<NotificationContent status={status} title="x" />);
    const chip = container.querySelector(`[data-status="${status}"]`);
    expect(chip).toBeInTheDocument();
    expect(chip!.querySelector('svg')).toBeInTheDocument();
  });

  it('top-aligns the status chip so it anchors to the first line at any height (never floats)', () => {
    const { container } = render(<NotificationContent status="error" title="t" message="m" />);
    const chip = container.querySelector('[data-status="error"]')!;
    expect(chip.className).toContain('self-start');
  });

  it('renders an actions region and an expanded children region', () => {
    render(
      <NotificationContent
        status="error"
        title="2 extensions failed to load"
        actions={<button>Copy error</button>}
      >
        <div>expanded detail row</div>
      </NotificationContent>
    );
    expect(screen.getByRole('button', { name: 'Copy error' })).toBeInTheDocument();
    expect(screen.getByText('expanded detail row')).toBeInTheDocument();
  });
});

describe('NotificationSurface (inline banner shell)', () => {
  it('keeps the surface neutral — status is expressed on the chip, not the surface background', () => {
    const { container } = render(
      <NotificationSurface status="error" title="Backend disconnected" />
    );
    const surface = container.querySelector('[data-testid="notification-surface"]')!;
    // The card ground stays neutral; no status fill leaks onto the whole surface.
    expect(surface.className).toContain('bg-background-default');
    expect(surface.className).not.toContain('bg-background-danger');
    // The status hue lives on the chip.
    expect(container.querySelector('[data-status="error"]')).toBeInTheDocument();
  });

  it('renders a dismiss control only when onClose is supplied, and reserves the close gutter once', () => {
    const onClose = vi.fn();
    const { container, rerender } = render(
      <NotificationSurface status="info" title="Heads up" onClose={onClose} />
    );
    const surface = container.querySelector('[data-testid="notification-surface"]')!;
    // Gutter reserved on the container so text/actions can never run under the ×.
    expect(surface.className).toContain('pr-10');
    const dismiss = screen.getByRole('button', { name: /dismiss/i });
    fireEvent.click(dismiss);
    expect(onClose).toHaveBeenCalledTimes(1);

    rerender(<NotificationSurface status="info" title="Heads up" />);
    expect(screen.queryByRole('button', { name: /dismiss/i })).not.toBeInTheDocument();
  });

  it('is flat (no elevation) by default and elevates only when asked', () => {
    const { container, rerender } = render(<NotificationSurface status="info" title="x" />);
    let surface = container.querySelector('[data-testid="notification-surface"]')!;
    expect(surface.className).not.toContain('shadow-popover');
    rerender(<NotificationSurface status="info" title="x" elevated />);
    surface = container.querySelector('[data-testid="notification-surface"]')!;
    expect(surface.className).toContain('shadow-popover');
  });
});
