// ui/desktop/src/components/knowledge/graph/GraphLegend.tsx
import { useCallback, useEffect, useMemo, useState } from 'react';
import { ChevronDown, ChevronUp } from '../../icons/app-icons';
import { Badge } from '../../ui/badge';
import { Button } from '../../ui/button';
import { GRAPH_PALETTE, typeFill } from '../../../styles/graphPalette';
import type { GraphCredibilityKey, GraphMode } from '../../../styles/graphPalette';
import { GraphShapeGlyph } from './GraphShapeGlyph';
import { toggle, UNTYPED_KEY } from './graphFacets';
import type { FacetState } from './graphFacets';
import type { GraphModel } from './graphModel';

/**
 * The legend dock (ui-spec §4.7).
 *
 * ⚠ **Four credibility entries, not seven, and that is DR-9b's honesty clause
 * made visible.** The canvas draws four *distinguishable* ring treatments — four
 * arcs, one arc, dashed, solid-with-`!` — because a 1.6px stroke subtends 2–3
 * arcmin and the seven ring hues collapse to ΔE00 1.13 under tritanopia. The old
 * legend listed seven treatments against a four-entry key, so `web` and
 * `personal` had no row at all. They are one category here — *not academic* —
 * and the exact tier lives in the inspector and the Source facet.
 *
 * ⚠ **Every chip is a real `<button>`, via `Badge asChild`.** Wrapping a badge
 * in a raw button would defeat D-15: the global focus rule paints
 * `background-color: var(--background-focus)` on the focused control, and a
 * `tone="neutral"` badge is an OPAQUE `bg-background-medium` that covers it
 * completely — a keyboard user would get no focus indication on any legend chip.
 * With `asChild` the chip *is* the button and takes the focus surface directly.
 */

const STORAGE_KEY = 'biorouter:knowledge-legend-expanded';

/** The four treatments the canvas actually distinguishes, in ring order. */
const CREDIBILITY_KEY: { key: GraphCredibilityKey; label: string }[] = [
  { key: 'peer_reviewed', label: 'Well sourced' },
  { key: 'gray_lit', label: 'Weakly sourced' },
  { key: 'web', label: 'Not academic' },
  { key: 'retracted', label: 'Retracted' },
];

function readExpanded(): boolean {
  // In a try/catch because `localStorage` throws outright in a sandboxed frame,
  // and a legend that cannot remember its state is far better than one that
  // takes the panel down with it.
  try {
    return window.localStorage.getItem(STORAGE_KEY) === 'true';
  } catch {
    return false;
  }
}

interface Props {
  model: GraphModel;
  mode: GraphMode;
  facets: FacetState;
  onChange: (next: FacetState) => void;
}

export function GraphLegend({ model, mode, facets, onChange }: Props) {
  const [expanded, setExpanded] = useState(readExpanded);

  useEffect(() => {
    try {
      window.localStorage.setItem(STORAGE_KEY, String(expanded));
    } catch {
      /* a legend that cannot persist still works */
    }
  }, [expanded]);

  const palette = GRAPH_PALETTE[mode];

  /**
   * The families actually present, in ladder order — and `null` in OKF mode.
   *
   * In OKF there are no families: every node takes the hashed fallback and the
   * circle, so the legend lists the types present by count instead. A family
   * heading over one hashed type would teach a mapping that does not exist.
   */
  const families = useMemo(() => {
    const present = new Set(model.typeCounts.map((t) => t.type));
    const rows = Object.entries(palette.families)
      .map(([name, family]) => ({
        name,
        shape: family.shape,
        members: family.members.filter((m) => present.has(m)),
      }))
      .filter((f) => f.members.length > 0);
    return rows.length > 0 ? rows : null;
  }, [model, palette]);

  const flatTypes = useMemo(() => model.typeCounts.slice(0, 24), [model]);

  const toggleType = useCallback(
    (type: string) => onChange({ ...facets, types: toggle(facets.types, type) }),
    [facets, onChange]
  );

  const toggleFamily = useCallback(
    (members: string[]) => {
      const allOn = members.every((m) => facets.types.has(m));
      const next = new Set(facets.types);
      for (const m of members) {
        if (allOn) next.delete(m);
        else next.add(m);
      }
      onChange({ ...facets, types: next });
    },
    [facets, onChange]
  );

  const chip = (type: string) => (
    <Badge
      key={type}
      asChild
      variant="chip"
      tone="neutral"
      // The pressed state is a translucent tint, never `tone="accent"`: the
      // focus fill has to read THROUGH it, and an opaque accent tone would
      // reintroduce the very problem `asChild` exists to solve.
      className={facets.types.has(type) ? 'tint-selected tint-interactive' : undefined}
    >
      <button type="button" aria-pressed={facets.types.has(type)} onClick={() => toggleType(type)}>
        <span
          aria-hidden="true"
          className="br-swatch-ring h-2 w-2 shrink-0 rounded-full"
          style={{ background: typeFill(type, mode) }}
        />
        {type}
      </button>
    </Badge>
  );

  if (model.typeCounts.length === 0 && !model.hasExternal) return null;

  return (
    <div
      data-testid="knowledge-graph-legend"
      className={
        expanded
          ? 'br-graph-legend-expanded flex flex-none flex-col gap-3 overflow-y-auto border-t border-border-subtle bg-background-default px-3 py-2'
          : 'br-graph-dock flex flex-none items-center gap-4 overflow-x-auto border-t border-border-subtle bg-background-default px-3'
      }
    >
      {expanded ? (
        <>
          {(families ?? []).map((family) => (
            <div key={family.name} className="flex flex-col gap-2">
              <button
                type="button"
                onClick={() => toggleFamily(family.members)}
                className="flex items-center gap-2 self-start rounded-inner text-caps text-text-muted"
              >
                <GraphShapeGlyph shape={family.shape} />
                {family.name}
              </button>
              <div className="flex flex-wrap gap-2">{family.members.map(chip)}</div>
            </div>
          ))}
          {!families && (
            <div className="flex flex-wrap gap-2">{flatTypes.map((t) => chip(t.type))}</div>
          )}
          <ExtraRows model={model} mode={mode} facets={facets} onChange={onChange} />
        </>
      ) : (
        <>
          {(families ?? []).map((family) => (
            <div key={family.name} className="flex flex-none items-center gap-2">
              <GraphShapeGlyph shape={family.shape} className="text-text-muted" />
              <span className="whitespace-nowrap text-caps text-text-muted">{family.name}</span>
              <span className="flex flex-none items-center gap-1">
                {family.members.map((m) => (
                  <span
                    key={m}
                    title={m}
                    aria-label={m}
                    className="br-swatch-ring h-2 w-2 rounded-full"
                    style={{ background: typeFill(m, mode) }}
                  />
                ))}
              </span>
            </div>
          ))}
          {!families &&
            flatTypes.map((t) => (
              <span key={t.type} className="flex flex-none items-center gap-2">
                <span
                  aria-hidden="true"
                  className="br-swatch-ring h-2 w-2 rounded-full"
                  style={{ background: typeFill(t.type, mode) }}
                />
                <span className="whitespace-nowrap text-supporting text-text-muted">{t.type}</span>
              </span>
            ))}

          <span aria-hidden="true" className="h-4 w-px flex-none bg-border-subtle" />

          {CREDIBILITY_KEY.map((entry) => (
            <span key={entry.key} className="flex flex-none items-center gap-2">
              <CredibilityGlyph entry={entry.key} mode={mode} />
              <span className="whitespace-nowrap text-supporting text-text-muted">
                {entry.label}
              </span>
            </span>
          ))}
        </>
      )}

      <Button
        type="button"
        variant="ghost"
        size="sm"
        className="ml-auto flex-none self-start"
        aria-expanded={expanded}
        onClick={() => setExpanded((v) => !v)}
      >
        {expanded ? <ChevronDown aria-hidden="true" /> : <ChevronUp aria-hidden="true" />}
        {expanded ? 'Collapse' : 'Expand'}
      </Button>
    </div>
  );
}

/** `External` and `Unrecognised type` — shown only when the graph contains them. */
function ExtraRows({
  model,
  mode,
  facets,
  onChange,
}: {
  model: GraphModel;
  mode: GraphMode;
  facets: FacetState;
  onChange: (next: FacetState) => void;
}) {
  if (!model.hasExternal && !model.hasUnrecognisedTypes && !model.untyped) return null;
  return (
    <div className="flex flex-wrap items-center gap-3">
      {model.hasExternal && (
        <span className="flex items-center gap-2 text-supporting text-text-muted">
          <span
            aria-hidden="true"
            className="h-2 w-2 rounded-full border border-dashed border-text-muted opacity-45"
          />
          Referenced, no page yet
        </span>
      )}
      {model.hasUnrecognisedTypes && (
        <Badge tone="warning">Unrecognised type</Badge>
      )}
      {model.untyped && (
        <Badge
          asChild
          variant="chip"
          tone="neutral"
          className={facets.types.has(UNTYPED_KEY) ? 'tint-selected tint-interactive' : undefined}
        >
          <button
            type="button"
            aria-pressed={facets.types.has(UNTYPED_KEY)}
            onClick={() => onChange({ ...facets, types: toggle(facets.types, UNTYPED_KEY) })}
          >
            <span
              aria-hidden="true"
              className="br-swatch-ring h-2 w-2 shrink-0 rounded-full"
              style={{ background: typeFill('Other', mode) }}
            />
            Untyped
          </button>
        </Badge>
      )}
    </div>
  );
}

/**
 * The credibility ring, drawn in the DOM exactly as the canvas draws it (§5.5).
 *
 * An arc count, a dashed ring or a solid one — never a filled disc. The hue
 * never sits behind text and nothing anywhere fills a surface with a ring hue,
 * so this is a 10px ring with a transparent centre.
 */
function CredibilityGlyph({ entry, mode }: { entry: GraphCredibilityKey; mode: GraphMode }) {
  const palette = GRAPH_PALETTE[mode];
  const colour = palette.credibility[entry];
  const treatment = palette.ringArcs[entry];
  const r = 4;
  const c = 2 * Math.PI * r;

  if (treatment === 'solid' || treatment === 'dashed') {
    return (
      <svg aria-hidden="true" width={10} height={10} viewBox="0 0 10 10" style={{ flex: 'none' }}>
        <circle
          cx={5}
          cy={5}
          r={r}
          fill="none"
          stroke={colour}
          strokeWidth={1.6}
          strokeDasharray={treatment === 'dashed' ? `${c / 16} ${c / 16}` : undefined}
        />
      </svg>
    );
  }

  const n = treatment;
  const gap = 0.5;
  const span = (Math.PI * 2) / n - gap;
  const arcs = Array.from({ length: n }, (_, i) => {
    const start = -Math.PI / 2 + ((Math.PI * 2) / n) * i;
    const end = start + span;
    const x0 = 5 + r * Math.cos(start);
    const y0 = 5 + r * Math.sin(start);
    const x1 = 5 + r * Math.cos(end);
    const y1 = 5 + r * Math.sin(end);
    return `M${x0.toFixed(2)},${y0.toFixed(2)}A${r},${r} 0 0 1 ${x1.toFixed(2)},${y1.toFixed(2)}`;
  }).join(' ');

  return (
    <svg aria-hidden="true" width={10} height={10} viewBox="0 0 10 10" style={{ flex: 'none' }}>
      <path d={arcs} fill="none" stroke={colour} strokeWidth={1.6} strokeLinecap="butt" />
    </svg>
  );
}
