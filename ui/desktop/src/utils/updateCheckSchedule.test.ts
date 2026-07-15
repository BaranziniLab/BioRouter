import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  scheduleUpdateChecks,
  STARTUP_UPDATE_CHECK_DELAY_MS,
  UPDATE_CHECK_INTERVAL_MS,
} from './updateCheckSchedule';

describe('scheduleUpdateChecks', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('checks once after startup and then every three hours', async () => {
    const runCheck = vi.fn().mockResolvedValue(undefined);
    scheduleUpdateChecks(runCheck);

    await vi.advanceTimersByTimeAsync(STARTUP_UPDATE_CHECK_DELAY_MS - 1);
    expect(runCheck).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(1);
    expect(runCheck).toHaveBeenNthCalledWith(1, 'startup');

    await vi.advanceTimersByTimeAsync(UPDATE_CHECK_INTERVAL_MS - STARTUP_UPDATE_CHECK_DELAY_MS);
    expect(runCheck).toHaveBeenNthCalledWith(2, 'periodic');

    await vi.advanceTimersByTimeAsync(UPDATE_CHECK_INTERVAL_MS);
    expect(runCheck).toHaveBeenNthCalledWith(3, 'periodic');
  });

  it('can cancel both pending checks', async () => {
    const runCheck = vi.fn();
    const cancel = scheduleUpdateChecks(runCheck);

    cancel();
    await vi.advanceTimersByTimeAsync(UPDATE_CHECK_INTERVAL_MS * 2);

    expect(runCheck).not.toHaveBeenCalled();
  });
});
