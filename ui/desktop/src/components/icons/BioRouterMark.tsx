import { useEffect, useLayoutEffect, useRef, useState, type SVGProps } from 'react';
import { useResolvedTheme } from '../../contexts/ThemeContext';

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

const NAVY = '#052049';
const CORAL = '#b85a32';
const TEAL = '#18a3ac';
const PLATE = '#faf8f3';

const WEIGHT = 800;
const R_RATIO = 0.74; // "R" cap height relative to "B"
const TRACK = 0.03; // gap between the letters, in em of the B size
const GAP_K = 0.02; // letters -> underline, fraction of cap height
const THICK_K = 0.15; // underline thickness, fraction of cap height
const WORK_SIZE = 100;

const SANS =
  '-apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, Roboto, "Helvetica Neue", Arial, sans-serif';

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
  const dark = darkProp ?? resolved === 'dark';
  const navy = dark ? TEAL : NAVY;

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
    // Square viewBox centered on the union, with even breathing room.
    const side = Math.max(unionW, unionH) * (plate ? 1.7 : 1.2);
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

  useLayoutEffect(() => {
    measureRef.current();
  });
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
        fill={CORAL}
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
            fill={CORAL}
          />
        </>
      )}
    </svg>
  );
}

export default BioRouterMark;
