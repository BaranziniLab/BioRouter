// ui/desktop/src/components/knowledge/graph/GraphShapeGlyph.tsx
import { svgPathForShape } from './nodeShapes';
import type { NodeShape } from '../../../styles/graphPalette';

/**
 * A node's silhouette, in the DOM.
 *
 * The legend and the facet rows TEACH the shape channel, so they must draw the
 * same seven shapes the canvas draws — a legend whose glyph disagrees with the
 * mark is worse than no legend. Both surfaces therefore go through
 * `svgPathForShape`, the same generator the canvas's `pathForShape` mirrors,
 * rather than each picking an icon that looks about right.
 *
 * `aria-hidden` always: the accessible name is the type or family NAME. A shape
 * and a colour are the fast channels for someone who can see them and carry
 * nothing for someone who cannot.
 */
export function GraphShapeGlyph({
  shape,
  fill,
  size = 12,
  className,
}: {
  shape: NodeShape;
  /** A palette hex. `undefined` draws the outline only, in `currentColor`. */
  fill?: string;
  size?: number;
  className?: string;
}) {
  return (
    <svg
      aria-hidden="true"
      focusable="false"
      viewBox="0 0 16 16"
      width={size}
      height={size}
      className={className}
      style={{ flex: 'none' }}
    >
      <path
        d={svgPathForShape(shape)}
        fill={fill ?? 'none'}
        stroke={fill ? 'none' : 'currentColor'}
        strokeWidth={fill ? 0 : 1.5}
      />
    </svg>
  );
}
