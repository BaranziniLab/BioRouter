import { describe, expect, it, vi } from 'vitest';
import { storeSubscriptionCleanup } from './storeSubscription';

describe('storeSubscriptionCleanup', () => {
  it('cleans up function-shaped subscriptions', () => {
    const unsubscribe = vi.fn();

    storeSubscriptionCleanup(unsubscribe)();

    expect(unsubscribe).toHaveBeenCalledOnce();
  });

  it('cleans up object-shaped subscriptions', () => {
    const unsubscribe = vi.fn();

    storeSubscriptionCleanup({ unsubscribe })();

    expect(unsubscribe).toHaveBeenCalledOnce();
  });
});
