import { getApiUrl } from '../config';
import { userActionHeaders } from './userAction';

const CONTINUATION_OWNER_KEY = 'biorouter.continuation-owner.v1';
let memoryOwnerId: string | null = null;

function newContinuationOwnerId(): string {
  const crypto = globalThis.crypto;
  if (crypto && typeof crypto.randomUUID === 'function') return crypto.randomUUID();
  return `window-${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

/**
 * Stable for one renderer window across reloads, but deliberately not shared
 * through localStorage with other windows. This is an ownership label, not a
 * credential; every mutating request still carries user-action proof.
 */
export function getContinuationOwnerId(): string {
  try {
    const stored = globalThis.sessionStorage?.getItem(CONTINUATION_OWNER_KEY);
    if (stored) return stored;
    const ownerId = newContinuationOwnerId();
    globalThis.sessionStorage?.setItem(CONTINUATION_OWNER_KEY, ownerId);
    return ownerId;
  } catch {
    memoryOwnerId ??= newContinuationOwnerId();
    return memoryOwnerId;
  }
}

export type ContinuationRecoveryAction = 'take_over' | 'abandon';

export interface ContinuationRecoveryResponse {
  resolution: 'taken_over' | 'abandoned';
  superseded_turn_id: string;
  continuation_lease?: string;
}

export async function abandonContinuationLease(
  sessionId: string,
  continuationLease: string
): Promise<void> {
  const response = await fetch(getApiUrl('/agent/continuation/abandon'), {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'X-Secret-Key': await window.electron.getSecretKey(),
      ...(await userActionHeaders()),
    },
    body: JSON.stringify({
      session_id: sessionId,
      continuation_lease: continuationLease,
    }),
  });
  if (!response.ok) {
    throw new Error(`Could not abandon continuation lease: HTTP ${response.status}`);
  }
}

export async function recoverContinuationGroup(
  sessionId: string,
  supersededTurnId: string,
  action: ContinuationRecoveryAction
): Promise<ContinuationRecoveryResponse> {
  const response = await fetch(getApiUrl('/agent/continuation/recover'), {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'X-Secret-Key': await window.electron.getSecretKey(),
      ...(await userActionHeaders()),
    },
    body: JSON.stringify({
      session_id: sessionId,
      superseded_turn_id: supersededTurnId,
      continuation_owner_id: getContinuationOwnerId(),
      action,
    }),
  });
  if (!response.ok) {
    throw new Error(`Could not recover continuation lease: HTTP ${response.status}`);
  }
  return (await response.json()) as ContinuationRecoveryResponse;
}
