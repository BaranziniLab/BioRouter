import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import userEvent from '@testing-library/user-event';
import { MessageMeta, MessageMetaAction } from './MessageMeta';

describe('MessageMeta', () => {
  it('sets the timestamp at `supporting`, not at message size', () => {
    render(<MessageMeta timestamp="3:14 PM" />);
    // The whole point of the primitive: 12px metadata, never the 14px the
    // message body uses. `text-supporting` IS 12/16 — asserting the role name
    // rather than a pixel keeps this honest if the scale is ever re-tuned.
    expect(screen.getByText('3:14 PM')).toHaveClass('text-supporting', 'text-text-muted');
  });

  it('keeps actions visible beside the timestamp instead of swapping on hover', () => {
    render(
      <MessageMeta timestamp="3:14 PM">
        <MessageMetaAction icon={<svg />}>Copy</MessageMetaAction>
      </MessageMeta>
    );

    const action = screen.getByRole('button', { name: 'Copy' });
    // The three hand-copied meta rows each hid their actions behind
    // `opacity-0 group-hover:opacity-100` and slid the timestamp out of the way
    // with `-translate-y-4`. Copy and Diverge were undiscoverable, and the two
    // could never be read at once. Neither class may come back.
    expect(action.className).not.toMatch(/opacity-0/);
    expect(action.className).not.toMatch(/translate-y/);
    expect(screen.getByText('3:14 PM')).toBeInTheDocument();
  });

  it('puts the timestamp nearest the aligned edge', () => {
    const { rerender, container } = render(
      <MessageMeta timestamp="3:14 PM">
        <MessageMetaAction icon={<svg />}>Copy</MessageMetaAction>
      </MessageMeta>
    );
    const startRow = container.querySelector('[data-message-meta="start"]')!;
    expect(startRow.firstElementChild).toHaveTextContent('3:14 PM');

    rerender(
      <MessageMeta align="end" timestamp="3:14 PM">
        <MessageMetaAction icon={<svg />}>Copy</MessageMetaAction>
      </MessageMeta>
    );
    const endRow = container.querySelector('[data-message-meta="end"]')!;
    // Mirrored, so the timestamp still hugs the edge the bubble is aligned to.
    expect(endRow.lastElementChild).toHaveTextContent('3:14 PM');
  });

  it('does not fire a disabled action', async () => {
    const onClick = vi.fn();
    render(
      <MessageMetaAction icon={<svg />} disabled onClick={onClick}>
        Diverge
      </MessageMetaAction>
    );
    await userEvent.click(screen.getByRole('button', { name: 'Diverge' }));
    expect(onClick).not.toHaveBeenCalled();
  });
});
