import { getApiUrl } from '../config';
import { userActionHeaders } from './userAction';

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
