/**
 * The chip a `<biorouter-ref …>` tag renders as (issue #65).
 *
 * The tag is the canonical way a message carries a skill, extension or
 * knowledge-base reference, because it is the only form that survives a name
 * with a space, a quote or an ampersand in it. That makes it ~45 characters of
 * XML in the middle of a sentence, which is exactly what the compact
 * `/skill:name` markers existed to avoid — so the tag is only acceptable once
 * the interface draws it as a first-class object instead.
 *
 * Everything here is presentation over the parse in `utils/resourceRefs`. The
 * chip never re-derives what counts as a reference: it renders what that parser
 * (a port of the backend's) claimed, so a chip on screen is always a resource
 * the agent will actually load.
 */
import { X } from './icons/app-icons';
import { ENTITY_ICONS, type EntityKind } from './icons/entity-icons';
import { Badge } from './ui/badge';
import { segmentRefTags, type RefKind, type RefSpan } from '../utils/resourceRefs';

/** How each kind is named to the user. */
export const REF_KIND_LABEL: Record<RefKind, string> = {
  skill: 'Skill',
  extension: 'Extension',
  knowledge_base: 'Knowledge base',
};

// One glyph, one meaning (design.md §3.9): the chip reuses the marks the
// sidebar, the mention popover and the settings views already use for these
// three entities rather than inventing a fourth spelling of "skill".
const REF_KIND_ENTITY: Record<RefKind, EntityKind> = {
  skill: 'skill',
  extension: 'extension',
  knowledge_base: 'knowledge',
};

/**
 * What the chip reads as.
 *
 * The label when the tag carries one — a knowledge base's id is a slug nobody
 * chose to look at — and the identity otherwise.
 */
export const refDisplayName = (ref: Pick<RefSpan, 'value' | 'label'>): string =>
  ref.label?.trim() || ref.value;

interface ResourceRefChipProps {
  refSpan: Pick<RefSpan, 'kind' | 'value' | 'label'>;
  /** Renders a remove control. Omitted where the reference is already sent. */
  onRemove?: () => void;
  className?: string;
}

/**
 * One reference, drawn as a chip.
 *
 * Rendered through {@link Badge}, the app's one small-label primitive, so it
 * shares the chip radius (`--radius-sm`), size and weight with every other chip
 * instead of being a bespoke pill that drifts. The accent tone is the ~12% fill
 * the primitive documents, not a solid block: the chip has to read as an
 * attached object inside a message bubble that is itself tinted, while staying
 * quiet enough for a canvas whose thesis is calm.
 *
 * Colour comes only from semantic tokens, so the chip follows all three theme
 * families across light and dark with no per-theme branch here.
 */
export function ResourceRefChip({ refSpan, onRemove, className }: ResourceRefChipProps) {
  const Icon = ENTITY_ICONS[REF_KIND_ENTITY[refSpan.kind]];
  const name = refDisplayName(refSpan);
  const kindLabel = REF_KIND_LABEL[refSpan.kind];
  // The identity is worth showing next to a label only when they differ — a
  // knowledge base is picked by name and resolved by id.
  const title =
    name === refSpan.value ? `${kindLabel}: ${name}` : `${kindLabel}: ${name} (${refSpan.value})`;

  return (
    <Badge
      tone="accent"
      data-testid="resource-ref-chip"
      data-ref-kind={refSpan.kind}
      title={title}
      // `max-w-full` + `min-w-0` are load-bearing, not decoration: the Badge
      // primitive is `flex-shrink-0`, and a flex item's `min-width: auto` would
      // otherwise hold the chip at the full width of an unbroken name and bleed
      // it past the bubble it sits in.
      className={`max-w-full min-w-0 align-middle ${className ?? ''}`}
    >
      <Icon className="h-3 w-3 shrink-0" />
      {/* The glyph carries the kind visually; a screen reader gets it in words,
          because "rna-qc" alone does not say what was attached. */}
      <span className="sr-only">{kindLabel}: </span>
      <span data-testid="resource-ref-chip-name" className="min-w-0 truncate">
        {name}
      </span>
      {onRemove && (
        <button
          type="button"
          onClick={onRemove}
          aria-label={`Remove ${kindLabel.toLowerCase()} ${name}`}
          className="-mr-0.5 ml-0.5 shrink-0 cursor-pointer rounded-sm p-0.5 text-text-accent/70 transition-colors duration-[var(--motion-fast)] hover:bg-background-accent/15 hover:text-text-accent"
        >
          <X className="h-2.5 w-2.5" />
        </button>
      )}
    </Badge>
  );
}

interface ResourceRefTextProps {
  text: string;
  className?: string;
}

/**
 * `text` with every reference tag drawn as a chip and everything else left
 * exactly as written.
 *
 * A tag the parser refuses comes back as text, so a message the user typed by
 * hand — or one truncated mid-attribute — degrades to something readable rather
 * than to a blank where a reference used to be. That is also honest: the
 * backend will not resolve it either.
 */
export function ResourceRefText({ text, className }: ResourceRefTextProps) {
  const segments = segmentRefTags(text);

  return (
    <>
      {segments.map((segment, index) =>
        segment.type === 'text' ? (
          <span key={index}>{segment.text}</span>
        ) : (
          <ResourceRefChip key={index} refSpan={segment.ref} className={className} />
        )
      )}
    </>
  );
}
