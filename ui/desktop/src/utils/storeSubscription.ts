type StoreSubscription = (() => void) | { unsubscribe: () => void };

export function storeSubscriptionCleanup(subscription: StoreSubscription): () => void {
  return () => {
    if (typeof subscription === 'function') {
      subscription();
    } else {
      subscription.unsubscribe();
    }
  };
}
