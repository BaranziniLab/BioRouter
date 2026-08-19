// ui/desktop/src/components/knowledge/graph/EdgePreview.tsx
import { useEffect, useRef } from 'react';
import { ChevronRight, X } from '../../icons/app-icons';
import type { GraphEdge } from '../../../api/types.gen';
import { Badge } from '../../ui/badge';
import { Button } from '../../ui/button';
import { isNegated, readablePredicate } from './graphModel';
import type { GraphModel } from './graphModel';

/**
 * The edge inspector (ui-spec §4.8).
 *
 * ⚠ **The canvas already tracked a hovered edge and painted its label; nothing
 * could OPEN one.** `onLinkClick` was unwired, so every field an edge carries
 * beyond its predicate — the §8.1 provenance triplet, the §7.3 quantitative
 * bundle, the §7.2 qualifiers — was derived by the daemon, serialized over the
 * wire, and unreachable. A user could see that Metformin treats Diabetes and
 * had no way to ask on whose authority, from which source, at what effect size.
 *
 * ⚠ **A missing provenance field is SHOWN as missing.** "Not stated" is the
 * answer to "who says so"; omitting the row would make an unsourced claim look
 * the same as a sourced one, which is the single most consequential confusion
 * this panel can produce.
 */

interface Props {
  edge: GraphEdge;
  /** For resolving endpoint ids to the labels the canvas draws. */
  model: GraphModel;
  onClose: () => void;
}

/** The three §8.1 fields, in the order the spec lists them. */
const PROVENANCE: { key: 'knowledge_level' | 'agent_type' | 'primary_source'; label: string }[] = [
  { key: 'knowledge_level', label: 'Knowledge level' },
  { key: 'agent_type', label: 'Agent type' },
  { key: 'primary_source', label: 'Primary source' },
];

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex items-baseline gap-3">
      <span className="w-28 flex-none text-supporting text-text-muted">{label}</span>
      <span className="min-w-0 flex-1 break-words text-body text-text-default">{children}</span>
    </div>
  );
}

export function EdgePreview({ edge, model, onClose }: Props) {
  const panelRef = useRef<HTMLDivElement>(null);
  const onCloseRef = useRef(onClose);

  useEffect(() => {
    onCloseRef.current = onClose;
  }, [onClose]);

  useEffect(() => {
    const handlePointerDown = (event: globalThis.PointerEvent) => {
      if (!panelRef.current?.contains(event.target as Node)) onCloseRef.current();
    };
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onCloseRef.current();
    };

    document.addEventListener('pointerdown', handlePointerDown);
    document.addEventListener('keydown', handleKeyDown);
    return () => {
      document.removeEventListener('pointerdown', handlePointerDown);
      document.removeEventListener('keydown', handleKeyDown);
    };
  }, []);

  const label = (id: string) => model.nodes.get(id)?.display ?? id;
  const predicate = readablePredicate(edge);
  const negated = isNegated(edge);
  const quantitative = Object.entries(edge.quantitative ?? {});
  const qualifiers = Object.entries(edge.qualifiers ?? {});

  return (
    <div
      ref={panelRef}
      role="dialog"
      aria-label={`Link ${label(edge.from)} ${predicate} ${label(edge.to)}`}
      data-testid="knowledge-edge-preview"
      className="absolute top-12 right-4 z-[var(--z-dropdown)] flex max-h-[calc(100%-5rem)] w-[min(360px,calc(100%-2rem))] flex-col overflow-hidden rounded-container border border-border-subtle bg-background-default shadow-popover"
    >
      <div className="flex items-start justify-between gap-2 border-b border-border-subtle bg-background-muted px-4 py-3">
        <div className="flex min-w-0 flex-col gap-1">
          {/* The predicate is the edge's identity, so it leads — struck through
              and in the danger ink when the claim is NEGATIVE, the same pairing
              the canvas uses (§5.8's word-level redundancy on the dash). */}
          <div
            className={`text-label ${negated ? 'text-text-danger line-through' : 'text-text-default'}`}
          >
            {predicate}
          </div>
          <div className="flex min-w-0 items-center gap-1 text-supporting text-text-muted">
            <span className="min-w-0 truncate">{label(edge.from)}</span>
            <ChevronRight aria-hidden="true" className="h-3 w-3 flex-none" />
            <span className="min-w-0 truncate">{label(edge.to)}</span>
          </div>
        </div>
        <Button
          variant="ghost"
          size="sm"
          onClick={onClose}
          className="flex-shrink-0"
          aria-label="Close link details"
        >
          <X className="h-4 w-4" />
        </Button>
      </div>

      <div className="flex-1 overflow-y-auto px-4 py-3">
        <div className="flex flex-col gap-4">
          <section aria-label="Provenance" className="flex flex-col gap-1.5">
            <h3 className="text-caps text-text-muted">Provenance</h3>
            {edge.synthesized ? (
              // §4.8 replaces the triplet outright for a derived edge: it has no
              // author, so reporting three "not stated" rows would invite the
              // reader to go looking for an authority that cannot exist.
              <p className="text-body text-text-muted">
                Derived from provenance rather than authored. Write an explicit{' '}
                <span className="font-mono">reported_in</span> edge to make this claim first-class.
              </p>
            ) : (
              PROVENANCE.map(({ key, label: rowLabel }) => (
                <Row key={key} label={rowLabel}>
                  {edge[key] ? (
                    <span className="font-mono text-code">{edge[key]}</span>
                  ) : (
                    <span className="text-text-muted">Not stated</span>
                  )}
                </Row>
              ))
            )}
            {(edge.publications?.length ?? 0) > 0 && (
              <Row label="Publications">
                <span className="flex flex-wrap gap-1">
                  {edge.publications?.map((p) => (
                    <Badge key={p} className="font-mono">
                      {p}
                    </Badge>
                  ))}
                </span>
              </Row>
            )}
          </section>

          {quantitative.length > 0 && (
            <section aria-label="Quantitative" className="flex flex-col gap-1.5">
              <h3 className="text-caps text-text-muted">Quantitative</h3>
              {quantitative.map(([key, value]) => (
                <Row key={key} label={key.replace(/_/g, ' ')}>
                  <span className="font-mono text-code tabular-nums">{String(value)}</span>
                </Row>
              ))}
            </section>
          )}

          {qualifiers.length > 0 && (
            <section aria-label="Context" className="flex flex-col gap-1.5">
              <h3 className="text-caps text-text-muted">Context</h3>
              {qualifiers.map(([key, value]) => (
                <Row key={key} label={key.replace(/_/g, ' ')}>
                  <span className="font-mono text-code">{value}</span>
                </Row>
              ))}
            </section>
          )}
        </div>
      </div>
    </div>
  );
}
