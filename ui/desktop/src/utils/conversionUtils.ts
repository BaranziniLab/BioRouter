export async function safeJsonParse<T>(
  response: Response,
  errorMessage: string = 'Failed to parse server response'
): Promise<T> {
  try {
    return (await response.json()) as T;
  } catch (error) {
    if (error instanceof SyntaxError) {
      throw new Error(errorMessage);
    }
    throw error;
  }
}

export function errorMessage(err: Error | unknown, default_value?: string) {
  if (err instanceof Error) {
    return err.message;
  } else if (typeof err === 'object' && err !== null && 'message' in err) {
    return String(err.message);
  } else {
    return default_value || String(err);
  }
}

// A fetch to a down/unreachable backend rejects with `TypeError: Failed to fetch`
// (Chromium/Electron) *before* any HTTP response exists — distinct from a real
// HTTP error (a 4xx/5xx throws with a parsed response body, not a TypeError).
// Used to give backend-disconnected UX its own copy without swallowing the
// message. A real HTTP failure returns false so it keeps its own error text.
export function isConnectionError(err: Error | unknown): boolean {
  if (err instanceof TypeError) return true;
  const msg = errorMessage(err).toLowerCase();
  return (
    msg.includes('failed to fetch') ||
    msg.includes('fetch failed') ||
    msg.includes('networkerror') ||
    msg.includes('load failed') || // WebKit wording
    msg.includes('err_connection')
  );
}
