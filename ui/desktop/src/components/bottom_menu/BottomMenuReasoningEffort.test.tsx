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

    const trigger = screen.getByLabelText('Reasoning effort: Normal');
    expect(trigger).toBeInTheDocument();
    expect(trigger).not.toHaveAttribute('title');
    expect(trigger.querySelector('svg')).toHaveClass('size-[18px]');
    expect(screen.queryByText('Normal')).not.toBeInTheDocument();
  });

  it('keeps every option aligned while showing only the selected tick', () => {
    render(<BottomMenuReasoningEffort />);

    fireEvent.click(screen.getByLabelText('Reasoning effort: Normal'));
    const options = screen.getAllByRole('menuitemradio');

    options.forEach((option) => {
      expect(Array.from(option.children).some((child) => child.tagName === 'svg')).toBe(false);
      expect(option.lastElementChild).toHaveClass('size-3.5', 'shrink-0');
    });
    expect(options.filter((option) => option.querySelector('svg'))).toHaveLength(1);
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
