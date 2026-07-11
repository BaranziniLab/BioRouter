import { describe, it, expect } from 'vitest';
import { isConnectionError } from './conversionUtils';

describe('isConnectionError', () => {
  it('flags a fetch-to-down-backend TypeError', () => {
    // Chromium/Electron rejects a fetch to an unreachable host with this.
    expect(isConnectionError(new TypeError('Failed to fetch'))).toBe(true);
  });

  it('flags connection-ish error messages regardless of constructor', () => {
    expect(isConnectionError(new Error('fetch failed'))).toBe(true);
    expect(isConnectionError(new Error('NetworkError when attempting to fetch resource'))).toBe(
      true
    );
    expect(isConnectionError(new Error('net::ERR_CONNECTION_REFUSED'))).toBe(true);
    expect(isConnectionError(new Error('Load failed'))).toBe(true);
  });

  it('does NOT flag a real HTTP error (which keeps its own message)', () => {
    expect(isConnectionError(new Error('HTTP 500 Internal Server Error'))).toBe(false);
    expect(isConnectionError({ message: 'Bad Request' })).toBe(false);
    expect(isConnectionError('permission denied')).toBe(false);
  });
});
