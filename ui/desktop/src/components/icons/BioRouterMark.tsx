import { useEffect, useLayoutEffect, useRef, useState, type SVGProps } from 'react';
import { useResolvedTheme, useThemeFamily } from '../../contexts/ThemeContext';
import { GENERATED_THEMES } from '../../styles/themes.generated';

/**
 * The BioRouter square MARK — the "BR" monogram, for the spots too small for the
 * full wordmark (a collapsed sidebar, a message avatar, a loader).
 *
 * Same brand as the app icon (logo-icon-studio.html, D-38): a heavy navy "B",
 * a smaller coral "R", over a split navy/coral underline, centered as one body
 * (letters + underline). Navy becomes UCSF teal on a dark surface. `plate` adds
 * the cream rounded square (the app-icon look); without it the mark is
 * transparent and drops onto whatever surface it sits on.
 *
 * Geometry is measured at runtime (getBBox), like the wordmark, so it stays
 * correct in any font and re-centers once webfonts settle.
 */

/**
 * The mark's inks are PER FAMILY, read from the generated theme data — the same
 * source the pre-React boot splash paints from. They used to be fixed module
 * constants, which is why Roche Limit flashed an orange mark on the splash and
 * then hydrated a Parchment coral/navy one: the splash was family-aware and the
 * component was not.
 *
 * `dark` swaps the primary ink for the family's own dark-mode value (Parchment
 * and Alma Mater lift to a teal; Roche lifts to its light ink), which is what
 * the splash's `--br-navy` does too.
 */
const PLATE = '#faf8f3';

const WEIGHT = 800;
const R_RATIO = 0.74; // "R" cap height relative to "B"
const TRACK = 0.03; // gap between the letters, in em of the B size
const GAP_K = 0.02; // letters -> underline, fraction of cap height
const THICK_K = 0.15; // underline thickness, fraction of cap height
const WORK_SIZE = 100;

// The brand mark is set in Inter (bundled @font-face in main.css), not the native
// UI stack, so the BR monogram matches the app icon on every platform. The native
// stack stays only as a load-time fallback.
const SANS =
  'Inter, -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, Roboto, "Helvetica Neue", Arial, sans-serif';

type Geo = {
  vb: string;
  rx: number; // R x position
  sR: number; // R font-size
  barLeft: number;
  splitX: number;
  barRight: number;
  ulTop: number;
  thick: number;
};

interface BioRouterMarkProps extends Omit<SVGProps<SVGSVGElement>, 'ref' | 'title'> {
  dark?: boolean;
  plate?: boolean;
  title?: string;
}

export function BioRouterMark({
  className,
  style,
  dark: darkProp,
  plate = false,
  title = 'BioRouter',
  ...rest
}: BioRouterMarkProps) {
  const bRef = useRef<SVGTextElement>(null);
  const rRef = useRef<SVGTextElement>(null);
  const [geo, setGeo] = useState<Geo | null>(null);
  const resolved = useResolvedTheme();
  const family = useThemeFamily();
  const marks = GENERATED_THEMES[family] ?? GENERATED_THEMES.parchment;
  const dark = darkProp ?? resolved === 'dark';
  // Teal only replaces navy when the mark drops onto a dark surface. On the
  // cream `plate` the letters always sit on a light ground, so they stay navy
  // regardless of the app theme.
  const navy = dark && !plate ? marks.dark.mark.navy : marks.light.mark.navy;

  const measure = () => {
    const b = bRef.current;
    const r = rRef.current;
    // jsdom has no getBBox (see BioRouterWordmark) — stay unmeasured under tests.
    if (!b || !r || typeof b.getBBox !== 'function' || typeof r.getBBox !== 'function') return;
    const bb = b.getBBox();
    if (bb.width === 0 || bb.height === 0) return;
    const rb = r.getBBox();

    const left = Math.min(bb.x, rb.x);
    const right = Math.max(bb.x + bb.width, rb.x + rb.width);
    const lettersBottom = Math.max(bb.y + bb.height, rb.y + rb.height);
    const top = Math.min(bb.y, rb.y);
    const cap = bb.height;

    const gap = cap * GAP_K;
    const thick = cap * THICK_K;
    const ulTop = lettersBottom + gap;
    const ulBottom = ulTop + thick;
    const splitX = bb.x + bb.width + (WORK_SIZE * TRACK) / 2;

    const unionTop = top;
    const unionBottom = ulBottom;
    const unionW = right - left;
    const unionH = unionBottom - unionTop;
    // Square viewBox centered on the union, with even breathing room. On the
    // plate the union fills 60% of the icon (studio-approved), so side = union /
    // 0.60 ≈ ×1.667; the transparent mark sits a touch tighter.
    const side = Math.max(unionW, unionH) * (plate ? 1.667 : 1.2);
    const cx = (left + right) / 2;
    const cy = (unionTop + unionBottom) / 2;
    const next: Geo = {
      vb: `${(cx - side / 2).toFixed(2)} ${(cy - side / 2).toFixed(2)} ${side.toFixed(2)} ${side.toFixed(2)}`,
      rx: bb.x + bb.width + WORK_SIZE * TRACK,
      sR: WORK_SIZE * R_RATIO,
      barLeft: left,
      splitX,
      barRight: right,
      ulTop,
      thick,
    };
    // Bail if unchanged, so the every-render measure() can't loop (see wordmark).
    setGeo((prev) => (prev && JSON.stringify(prev) === JSON.stringify(next) ? prev : next));
  };

  const measureRef = useRef(measure);
  measureRef.current = measure;

  // Measure on mount and when `plate` changes (it affects the viewBox padding),
  // plus once webfonts settle — never every render (see BioRouterWordmark: an
  // unguarded per-render getBBox loops on sub-pixel jitter).
  useLayoutEffect(() => {
    measureRef.current();
  }, [plate]);
  useEffect(() => {
    if (!document.fonts?.ready) return;
    let alive = true;
    document.fonts.ready.then(() => {
      if (alive) measureRef.current();
    });
    return () => {
      alive = false;
    };
  }, []);

  const side = geo ? parseFloat(geo.vb.split(' ')[2]) : WORK_SIZE * 1.2;
  const plateX = geo ? parseFloat(geo.vb.split(' ')[0]) : 0;
  const plateY = geo ? parseFloat(geo.vb.split(' ')[1]) : 0;

  return (
    <svg
      {...rest}
      className={className}
      style={style}
      viewBox={geo ? geo.vb : `0 -${WORK_SIZE} ${WORK_SIZE * 1.2} ${WORK_SIZE * 1.2}`}
      role="img"
      aria-label={title}
      opacity={geo ? 1 : 0}
    >
      <title>{title}</title>
      {plate && geo && (
        <rect x={plateX} y={plateY} width={side} height={side} rx={side * 0.225} fill={PLATE} />
      )}
      <text
        ref={bRef}
        x={0}
        y={0}
        fontFamily={SANS}
        fontWeight={WEIGHT}
        fontSize={WORK_SIZE}
        fill={navy}
      >
        B
      </text>
      <text
        ref={rRef}
        x={geo ? geo.rx : WORK_SIZE * 0.7}
        y={0}
        fontFamily={SANS}
        fontWeight={WEIGHT}
        fontSize={geo ? geo.sR : WORK_SIZE * R_RATIO}
        fill={marks[dark && !plate ? 'dark' : 'light'].mark.coral}
      >
        R
      </text>
      {geo && (
        <>
          <rect
            x={geo.barLeft}
            y={geo.ulTop}
            width={Math.max(0, geo.splitX - geo.barLeft)}
            height={geo.thick}
            fill={navy}
          />
          <rect
            x={geo.splitX}
            y={geo.ulTop}
            width={Math.max(0, geo.barRight - geo.splitX)}
            height={geo.thick}
            fill={marks[dark && !plate ? 'dark' : 'light'].mark.coral}
          />
        </>
      )}
    </svg>
  );
}

export default BioRouterMark;
