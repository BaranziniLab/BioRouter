import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it } from 'vitest';
import { BottomMenuReasoningEffort } from './BottomMenuReasoningEffort';
import {
  getReasoningEffort,
  reasoningEffortForRequest,
  resetReasoningEffortForTests,
} from '../../store/reasoningEffort';

describe('BottomMenuReasoningEffort (BR-63)', () => {
  beforeEach(() => {
    localStorage.clear();
    resetReasoningEffortForTests();
  });

  it('starts on the default and shows no level chip', () => {
    render(<BottomMenuReasoningEffort />);

    expect(screen.getByLabelText('Reasoning effort: Normal')).toBeInTheDocument();
    expect(screen.queryByText('Normal')).not.toBeInTheDocument();
  });

  it('picking deep updates the store, so the next chat request carries it', () => {
    render(<BottomMenuReasoningEffort />);

    fireEvent.click(screen.getByLabelText('Reasoning effort: Normal'));
    fireEvent.click(screen.getByRole('menuitemradio', { name: /Deep/ }));

    expect(getReasoningEffort()).toBe('deep');
    expect(reasoningEffortForRequest()).toBe('deep');
    // The trigger now advertises the non-default level.
    expect(screen.getByLabelText('Reasoning effort: Deep')).toBeInTheDocument();
  });

  it('going back to normal drops the level from the request', () => {
    render(<BottomMenuReasoningEffort />);

    fireEvent.click(screen.getByLabelText('Reasoning effort: Normal'));
    fireEvent.click(screen.getByRole('menuitemradio', { name: /Quick/ }));
    expect(reasoningEffortForRequest()).toBe('quick');

    fireEvent.click(screen.getByLabelText('Reasoning effort: Quick'));
    fireEvent.click(screen.getByRole('menuitemradio', { name: /Normal/ }));
    expect(reasoningEffortForRequest()).toBeUndefined();
  });
});
