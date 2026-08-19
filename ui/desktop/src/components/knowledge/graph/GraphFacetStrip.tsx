// ui/desktop/src/components/knowledge/graph/GraphFacetStrip.tsx
import { useMemo, useState } from 'react';
import { Search } from '../../icons/app-icons';
import { Badge } from '../../ui/badge';
import { Button } from '../../ui/button';
import { Input } from '../../ui/input';
import { Command, CommandGroup, CommandInput, CommandItem, CommandList } from '../../ui/command';
import { Popover, PopoverContent, PopoverTrigger } from '../../ui/popover';
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuTrigger,
} from '../../ui/dropdown-menu';
import { GRAPH_PALETTE, typeFill, typeShape } from '../../../styles/graphPalette';
import type { GraphMode } from '../../../styles/graphPalette';
import { GraphShapeGlyph } from './GraphShapeGlyph';
import { toggle, UNTYPED_KEY } from './graphFacets';
import type { FacetState } from './graphFacets';
import type { GraphModel } from './graphModel';

/**
 * The facet strip (ui-spec §4.6).
 *
 * **OR within a facet, AND across facets**, and a node that fails takes the
 * search-miss alpha and stays in place — one dimming mechanism, never a second,
 * because "does not match my filter" and "is not related to what I am looking
 * at" are different states.
 *
 * ⚠ **Status is deliberately NOT a canvas channel.** The canvas already carries
 * four encodings (shape = family, fill = type, ring arcs = credibility,
 * dash/dot/taper = negated / synthesized / direction). A fifth is not readable
 * at a 6px node and every candidate collides with one that is already spoken
 * for. Status lives here and in the inspector; a filtered-out status dims.
 */

/** The fixed status vocabulary, so this one facet needs no search box. */
const STATUSES = ['draft', 'stable', 'deprecated', 'stale', 'retracted'] as const;

interface Props {
  model: GraphModel;
  mode: GraphMode;
  facets: FacetState;
  onChange: (next: FacetState) => void;
  /** How many nodes pass, for the `Showing N of M` readout. */
  passing: number;
  total: number;
  active: boolean;
}

export function GraphFacetStrip({
  model,
  mode,
  facets,
  onChange,
  passing,
  total,
  active,
}: Props) {
  return (
    <div
      data-testid="knowledge-graph-facets"
      className="br-graph-dock flex flex-none items-center gap-2 overflow-x-auto border-b border-border-subtle bg-background-default px-3"
    >
      <div className="relative flex-none">
        <Search
          aria-hidden="true"
          className="pointer-events-none absolute left-2 top-1/2 h-icon-row w-icon-row -translate-y-1/2 text-text-muted"
        />
        <Input
          type="text"
          data-testid="knowledge-graph-search"
          aria-label="Filter the graph by name or type"
          value={facets.search}
          onChange={(e) => onChange({ ...facets, search: e.target.value })}
          placeholder="Filter by name or type"
          className="w-[200px] pl-7"
        />
      </div>

      <FacetCombobox
        label="Type"
        // A node with no type is a real state (DR-28), so it gets its own row
        // rather than being unselectable. `Untyped` is the whole legacy base.
        options={[
          ...model.typeCounts.map((t) => ({
            value: t.type,
            label: t.type,
            count: t.count,
            fill: typeFill(t.type, mode),
            shape: typeShape(t.type, mode),
          })),
          ...(model.untyped || model.typeCounts.length === 0
            ? [{ value: UNTYPED_KEY, label: 'Untyped', count: 0, fill: undefined, shape: undefined }]
            : []),
        ]}
        selected={facets.types}
        onToggle={(v) => onChange({ ...facets, types: toggle(facets.types, v) })}
        mode={mode}
      />

      <FacetCombobox
        label="Predicate"
        mono
        options={model.predicateCounts.map((p) => ({
          value: p.predicate,
          label: p.predicate.replace(/^not_/, 'not ').replace(/_/g, ' '),
          count: p.count,
          negated: p.predicate.startsWith('not_'),
        }))}
        selected={facets.predicates}
        onToggle={(v) => onChange({ ...facets, predicates: toggle(facets.predicates, v) })}
        mode={mode}
      />

      <FacetCombobox
        label="Source"
        options={model.sourceOptions.map((s) => ({
          value: s.id,
          label: s.label,
          count: s.count,
        }))}
        selected={facets.sources}
        onToggle={(v) => onChange({ ...facets, sources: toggle(facets.sources, v) })}
        mode={mode}
      />

      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button type="button" variant="secondary" size="sm" className="flex-none">
            Status
            {facets.statuses.size > 0 && <Badge tone="accent">{facets.statuses.size}</Badge>}
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start">
          {STATUSES.map((s) => (
            <DropdownMenuCheckboxItem
              key={s}
              checked={facets.statuses.has(s)}
              onCheckedChange={() => onChange({ ...facets, statuses: toggle(facets.statuses, s) })}
            >
              {s}
            </DropdownMenuCheckboxItem>
          ))}
        </DropdownMenuContent>
      </DropdownMenu>

      {active && (
        <div className="ml-auto flex flex-none items-center gap-2">
          <span className="whitespace-nowrap font-mono text-supporting tabular-nums text-text-muted">
            Showing {passing} of {total}
          </span>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={() =>
              onChange({
                search: '',
                types: new Set(),
                predicates: new Set(),
                sources: new Set(),
                statuses: new Set(),
              })
            }
          >
            Clear filters
          </Button>
        </div>
      )}
    </div>
  );
}

interface Option {
  value: string;
  label: string;
  count: number;
  fill?: string;
  shape?: ReturnType<typeof typeShape>;
  negated?: boolean;
}

/**
 * One facet button and its searchable list (§4.0's one picker pattern).
 *
 * The rows carry the palette swatch INSIDE its family's shape glyph, because
 * that is the mapping the canvas uses and a facet row is where a user is most
 * likely to be learning it.
 */
function FacetCombobox({
  label,
  options,
  selected,
  onToggle,
  mono = false,
  mode,
}: {
  label: string;
  options: Option[];
  selected: Set<string>;
  onToggle: (value: string) => void;
  mono?: boolean;
  mode: GraphMode;
}) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState('');

  const families = GRAPH_PALETTE[mode].families;
  const familyOf = useMemo(() => {
    const map = new Map<string, string>();
    for (const [name, family] of Object.entries(families)) {
      for (const member of family.members) map.set(member, name);
    }
    return map;
  }, [families]);

  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return needle ? options.filter((o) => o.label.toLowerCase().includes(needle)) : options;
  }, [options, query]);

  /**
   * BioOKF groups by family; OKF is flat and sorted by count.
   *
   * The test is whether any option is a member of a curated family at all — in
   * an OKF base every type takes the hashed fallback and the circle, so there
   * are no families to group by and a heading per type would be noise.
   */
  const grouped = useMemo(() => {
    if (label !== 'Type') return null;
    // Seeded in the palette's own FAMILY ORDER, not in encounter order. The
    // ladder — Genomic → Molecular → Anatomy → Clinical → Exposome → Physical →
    // Provenance — is the order the legend teaches and the order the fills
    // ramp in; sorting by whichever family happened to have the commonest type
    // would put a different family first for every base the user opens.
    const groups = new Map<string, Option[]>(Object.keys(families).map((name) => [name, []]));
    let anyFamily = false;
    for (const o of filtered) {
      const family = familyOf.get(o.value);
      if (family) anyFamily = true;
      const key = family ?? 'Other types';
      const bucket = groups.get(key);
      if (bucket) bucket.push(o);
      else groups.set(key, [o]);
    }
    for (const [name, rows] of groups) if (rows.length === 0) groups.delete(name);
    return anyFamily ? groups : null;
  }, [families, filtered, familyOf, label]);

  const row = (o: Option) => (
    <CommandItem key={o.value} selected={selected.has(o.value)} onSelect={() => onToggle(o.value)}>
      {o.fill ? (
        <GraphShapeGlyph shape={o.shape ?? 'circle'} fill={o.fill} className="br-swatch-ring" />
      ) : (
        <span aria-hidden="true" className="h-3 w-3 flex-none" />
      )}
      <span
        className={[
          'min-w-0 flex-1 truncate',
          mono ? 'font-mono' : '',
          o.negated ? 'text-text-danger line-through' : '',
        ]
          .filter(Boolean)
          .join(' ')}
      >
        {o.label}
      </span>
      {o.count > 0 && (
        <span className="flex-none font-mono text-supporting tabular-nums text-text-muted">
          {o.count}
        </span>
      )}
    </CommandItem>
  );

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          type="button"
          variant="secondary"
          size="sm"
          className="flex-none"
          data-testid={`knowledge-graph-facet-${label.toLowerCase()}`}
        >
          {label}
          {selected.size > 0 && <Badge tone="accent">{selected.size}</Badge>}
        </Button>
      </PopoverTrigger>
      <PopoverContent align="start" className="w-64 p-0">
        <Command
          label={`${label} filter`}
          query={query}
          onQueryChange={setQuery}
          className="flex max-h-72 flex-col"
        >
          <CommandInput placeholder={`Search ${label.toLowerCase()}s`} />
          <CommandList>
            {grouped
              ? [...grouped.entries()].map(([family, rows]) => (
                  <CommandGroup
                    key={family}
                    heading={
                      <span className="flex items-center gap-2">
                        <GraphShapeGlyph
                          shape={families[family]?.shape ?? 'circle'}
                          className="text-text-muted"
                        />
                        {family}
                      </span>
                    }
                  >
                    {rows.map(row)}
                  </CommandGroup>
                ))
              : filtered.map(row)}
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  );
}
