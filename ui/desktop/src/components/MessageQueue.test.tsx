import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { ComponentProps } from 'react';
import { describe, expect, it, vi } from 'vitest';
import { MessageQueue } from './MessageQueue';
import { refTag } from '../utils/resourceRefs';

const queuedMessage = {
  id: 'message-1',
  content: 'and add better comments',
  timestamp: Date.now(),
};

const renderQueue = (overrides: Partial<ComponentProps<typeof MessageQueue>> = {}) => {
  const onSteerMessage = vi.fn();
  const onStopAndSend = vi.fn();

  render(
    <MessageQueue
      queuedMessages={[queuedMessage]}
      onRemoveMessage={vi.fn()}
      onClearQueue={vi.fn()}
      onSteerMessage={onSteerMessage}
      onStopAndSend={onStopAndSend}
      {...overrides}
    />
  );

  return { onSteerMessage, onStopAndSend };
};

describe('MessageQueue actions', () => {
  it('distinguishes adding to the current turn from stopping and sending', async () => {
    const user = userEvent.setup();
    const { onSteerMessage, onStopAndSend } = renderQueue();

    const addNow = screen.getByRole('button', {
      name: 'Add this message to the current turn',
    });
    const stopAndSend = screen.getByRole('button', {
      name: 'Stop the current turn and send this message as a new turn',
    });

    expect(addNow).toHaveTextContent('Add now');
    expect(addNow.querySelector('.lucide-message-square-plus')).not.toBeNull();
    expect(stopAndSend).toHaveTextContent('→Stop & send');
    expect(stopAndSend.querySelectorAll('svg')).toHaveLength(2);

    await user.click(addNow);
    await user.click(stopAndSend);

    expect(onSteerMessage).toHaveBeenCalledWith(queuedMessage.id);
    expect(onStopAndSend).toHaveBeenCalledWith(queuedMessage.id);
  });

  it('uses the same explicit actions in the expanded queue', async () => {
    const user = userEvent.setup();
    renderQueue();

    await user.click(screen.getByRole('button', { name: '1 message queued. Expand queue.' }));

    expect(
      screen.getByRole('button', { name: 'Add this message to the current turn' })
    ).toHaveTextContent('Add now');
    expect(
      screen.getByRole('button', {
        name: 'Stop the current turn and send this message as a new turn',
      })
    ).toHaveTextContent('Stop & send');
  });

  it('keeps attachment messages out of the text-only add-now path', () => {
    renderQueue({
      queuedMessages: [
        {
          ...queuedMessage,
          attachments: [{ path: '/tmp/image.png', kind: 'image' }],
        },
      ],
    });

    expect(
      screen.queryByRole('button', { name: 'Add this message to the current turn' })
    ).toBeNull();
    const stopAndSend = screen.getByRole('button', {
      name: 'Stop the current turn and send this message as a new turn',
    });
    expect(within(stopAndSend).getByText('Stop & send')).toBeInTheDocument();
  });
});

// Issue #65 — the queue renders inside the composer, so the same rule applies:
// the user never sees `<biorouter-ref …>` markup, and the reference survives an
// edit. A queued message is one the composer already built, tags and all.
describe('MessageQueue references', () => {
  const withRef = {
    id: 'message-1',
    content: `and add better comments ${refTag('skill', 'my skill')}`,
    timestamp: Date.now(),
  };

  it('shows a chip in the queued row instead of the markup', () => {
    renderQueue({ queuedMessages: [withRef] });

    expect(screen.getByTestId('resource-ref-chip-name')).toHaveTextContent('my skill');
    expect(document.body.textContent).not.toContain('biorouter-ref');
  });

  it('keeps the markup out of the inline editor and the reference on the message', async () => {
    const user = userEvent.setup();
    const onEditMessage = vi.fn();
    renderQueue({ queuedMessages: [withRef], onEditMessage });

    await user.click(screen.getByRole('button', { name: /queued\. Expand queue\./i }));
    await user.click(screen.getByText('and add better comments'));
    const box = screen.getByRole('textbox') as HTMLTextAreaElement;
    expect(box.value).toBe('and add better comments');

    await user.clear(box);
    await user.type(box, 'and tidy up');
    await user.click(screen.getByRole('button', { name: /^save$/i }));

    expect(onEditMessage).toHaveBeenCalledWith(
      'message-1',
      `and tidy up ${refTag('skill', 'my skill')}`
    );
  });
});
