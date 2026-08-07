import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ComposerSuggestions } from './ComposerSuggestions';

function captureInserts() {
  const events: Array<{ sessionId: string | null; value: string }> = [];
  const handler = (e: Event) => events.push((e as CustomEvent).detail);
  window.addEventListener('insert-chat-input', handler);
  return {
    events,
    stop: () => window.removeEventListener('insert-chat-input', handler),
  };
}

afterEach(() => vi.restoreAllMocks());

describe('ComposerSuggestions', () => {
  it('FILLS the composer rather than sending a turn', async () => {
    const captured = captureInserts();
    render(<ComposerSuggestions sessionId="abc" />);

    const chip = screen.getAllByRole('button')[0];
    await userEvent.click(chip);

    // One insert, carrying a complete editable sentence — not a submit. A chip
    // that fired a turn would spend a model call on a prompt nobody meant
    // literally, with no way to take it back.
    expect(captured.events).toHaveLength(1);
    expect(captured.events[0].value.length).toBeGreaterThan(chip.textContent!.length);
    captured.stop();
  });

  it('scopes the insert to its own session so a sibling chat never receives it', async () => {
    const captured = captureInserts();
    render(<ComposerSuggestions sessionId="session-a" />);
    await userEvent.click(screen.getAllByRole('button')[0]);
    expect(captured.events[0].sessionId).toBe('session-a');
    captured.stop();
  });

  it('sends null for the pre-session composer, matching how ChatInput compares', async () => {
    const captured = captureInserts();
    render(<ComposerSuggestions sessionId={null} />);
    await userEvent.click(screen.getAllByRole('button')[0]);
    // ChatInput compares `(detail.sessionId ?? null) !== (sessionId ?? null)`,
    // so the pre-session composer only matches an explicit null.
    expect(captured.events[0].sessionId).toBeNull();
    captured.stop();
  });

  it('stays out of the way when something already filled the composer', () => {
    render(<ComposerSuggestions sessionId="abc" hidden />);
    // A workflow prompt is already in the composer; offering starters that would
    // overwrite it is the interface arguing with itself.
    expect(screen.queryByTestId('composer-suggestions')).not.toBeInTheDocument();
  });
});
