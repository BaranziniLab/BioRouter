import { describe, expect, it } from 'vitest';
import { formatElapsed } from './formatElapsed';

describe('formatElapsed', () => {
  it('renders sub-minute waits in whole seconds', () => {
    expect(formatElapsed(0)).toBe('0s');
    expect(formatElapsed(2999)).toBe('2s'); // floors, never rounds up
    expect(formatElapsed(59_000)).toBe('59s');
  });

  it('switches to minutes and drops a zero seconds component', () => {
    expect(formatElapsed(60_000)).toBe('1m');
    expect(formatElapsed(80_000)).toBe('1m 20s');
    expect(formatElapsed(12 * 60_000)).toBe('12m');
  });

  it('switches to hours past sixty minutes', () => {
    expect(formatElapsed(60 * 60_000)).toBe('1h');
    expect(formatElapsed(64 * 60_000)).toBe('1h 4m');
  });

  it('never renders a negative elapsed time from clock skew', () => {
    expect(formatElapsed(-5000)).toBe('0s');
  });
});
