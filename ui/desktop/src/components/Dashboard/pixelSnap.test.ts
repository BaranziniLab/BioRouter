import { describe, expect, it } from 'vitest';
import { snapSizeToDevicePixel, snapToDevicePixel } from './pixelSnap';

describe('dashboard pixel snapping', () => {
  it('snaps offsets to physical pixels for fractional device pixel ratios', () => {
    expect(snapToDevicePixel(10.21, 0, 1.25)).toBeCloseTo(10.4);
    expect((snapToDevicePixel(10.21, 0, 1.25) * 1.25) % 1).toBeCloseTo(0);
  });

  it('accounts for the viewport origin when snapping canvas transforms', () => {
    const snapped = snapToDevicePixel(10, 0.25, 2);
    expect((snapped + 0.25) * 2).toBeCloseTo(21);
    expect(snapped).toBe(10.25);
  });

  it('snaps live resize dimensions to physical pixels', () => {
    const snapped = snapSizeToDevicePixel(520.26, 2);
    expect(snapped).toBe(520.5);
    expect(snapped * 2).toBe(1041);
  });
});
