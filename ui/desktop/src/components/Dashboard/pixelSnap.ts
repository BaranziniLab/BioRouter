export function getDevicePixelRatio(): number {
  if (typeof window === 'undefined') return 1;
  const dpr = window.devicePixelRatio;
  return Number.isFinite(dpr) && dpr > 0 ? dpr : 1;
}

export function snapToDevicePixel(value: number, origin = 0, dpr = getDevicePixelRatio()): number {
  return Math.round((value + origin) * dpr) / dpr - origin;
}

export function snapSizeToDevicePixel(value: number, dpr = getDevicePixelRatio()): number {
  return Math.round(value * dpr) / dpr;
}
