// ui/desktop/src/components/knowledge/graph/NodeSwatch.tsx

/**
 * A node's mark, in the DOM.
 *
 * ⚠ **This replaced `GraphShapeGlyph`, and the replacement had to keep the
 * COLOUR.** That component drew a family silhouette *and* filled it with the
 * type's palette hex, so deleting it to remove the shape channel would have
 * removed the fill with it — the legend, the facet rows and both inspectors
 * would have gone monochrome in the same edit. The silhouette is gone; the fill
 * is the whole point and stays.
 *
 * ⚠ **Solid vs hollow is the one distinction still drawn**, because the canvas
 * draws it: the twenty Biomedical Entity types are filled discs and the eight
 * Provenance & Context types are open rings (`nodeMark.isHollow`). A key that
 * disagrees with the mark teaches the wrong thing, which is the reason
 * `nodeMark.ts` exists at all — so this component takes the same `hollow` flag
 * the painter derives rather than deciding for itself.
 *
 * ⚠ **A circle, not a rounded square.** The swatch matches the mark. An earlier
 * revision drew square swatches beside circular nodes on the argument that a
 * key is not a picture of the mark; with the shape channel gone the mark is one
 * shape everywhere, so there is nothing left for a different swatch shape to
 * distinguish and the disagreement is pure cost.
 *
 * `aria-hidden` always: the accessible name is the type's NAME, which sits
 * beside it. A colour carries nothing for someone who cannot see it, which is
 * why §5.12's live region — not this swatch — is the redundant channel.
 */
export function NodeSwatch({
  fill,
  hollow = false,
  size = 10,
  className,
}: {
  /** A palette hex. Required: an uncoloured swatch is not a swatch. */
  fill: string;
  /** Provenance & Context draws as an open ring, exactly as the canvas does. */
  hollow?: boolean;
  size?: number;
  className?: string;
}) {
  return (
    <span
      aria-hidden="true"
      className={className}
      style={{
        width: size,
        height: size,
        flex: 'none',
        borderRadius: '9999px',
        display: 'inline-block',
        // 1.5px here against the canvas's ~1.7. ⚠ **Both numbers are SCREEN
        // pixels — the canvas one is not in world units, whatever its
        // neighbourhood suggests.** `ForceGraphCanvas` sets
        // `lineWidth = max(1.7, …) / globalScale` inside a world-space
        // transform, and that division exists precisely so the rendered stroke
        // stays ~1.7 device px at every zoom rather than scaling with it. An
        // earlier version of this comment had it backwards, which is worse than
        // saying nothing: it invites someone to "fix" the division and get a
        // ring that thickens as you zoom in. The two differ only because this
        // swatch is 10px and the mark is not; both read as a circle that
        // happens to be open rather than as a donut.
        background: hollow ? 'transparent' : fill,
        boxShadow: hollow ? `inset 0 0 0 1.5px ${fill}` : undefined,
      }}
    />
  );
}
