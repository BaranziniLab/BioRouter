import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  DEFAULT_REASONING_EFFORT,
  getReasoningEffort,
  reasoningEffortForRequest,
  resetReasoningEffortForTests,
  setReasoningEffort,
  subscribeToReasoningEffort,
} from './reasoningEffort';

describe('reasoningEffort store (BR-63)', () => {
  beforeEach(() => {
    localStorage.clear();
    resetReasoningEffortForTests();
  });

  it('defaults to normal', () => {
    expect(getReasoningEffort()).toBe(DEFAULT_REASONING_EFFORT);
    expect(DEFAULT_REASONING_EFFORT).toBe('normal');
  });

  it('omits the default from the chat request so /effort is not stomped', () => {
    expect(reasoningEffortForRequest()).toBeUndefined();

    setReasoningEffort('deep');
    expect(reasoningEffortForRequest()).toBe('deep');

    setReasoningEffort('normal');
    expect(reasoningEffortForRequest()).toBeUndefined();
  });

  it('persists the choice and reads it back on a fresh load', () => {
    setReasoningEffort('quick');
    expect(localStorage.getItem('biorouter.reasoningEffort')).toBe('quick');

    resetReasoningEffortForTests();
    expect(getReasoningEffort()).toBe('quick');
  });

  it('ignores a corrupt stored value instead of sending it to the server', () => {
    localStorage.setItem('biorouter.reasoningEffort', 'ludicrous');
    resetReasoningEffortForTests();

    expect(getReasoningEffort()).toBe('normal');
    expect(reasoningEffortForRequest()).toBeUndefined();
  });

  it('notifies subscribers only on a real change', () => {
    const listener = vi.fn();
    const unsubscribe = subscribeToReasoningEffort(listener);

    setReasoningEffort('deep');
    expect(listener).toHaveBeenCalledTimes(1);

    setReasoningEffort('deep');
    expect(listener).toHaveBeenCalledTimes(1);

    unsubscribe();
    setReasoningEffort('quick');
    expect(listener).toHaveBeenCalledTimes(1);
  });
});
