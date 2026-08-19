import { cn } from '../../utils';

/**
 * The knowledge-base colour dot — ONE object, ONE diameter.
 *
 * Before this it was drawn at 6, 8, 10 and 12px across five files, so the same
 * mark meant "knowledge base" at four sizes on screens the user moves between.
 * 8px is the section's one dot diameter (ui-spec §4.1, §4.2).
 *
 * It carries `.br-swatch-ring` — an authored 1px ring of `--background-default`
 * (§6.3) — so the colour's ADJACENT colour is a known ground rather than
 * whatever row state it happens to sit on. Without it an 8px fill on a
 * `tint-selected` row measures as low as 2.15:1 against its surround.
 *
 * `aria-hidden`, always: the base's NAME is the accessible label, and a colour
 * is never the only carrier of an identity.
 *
 * The colour itself is DATA — a per-base value the daemon assigns — so it
 * arrives through `style`, not through a class. The fallback is a token.
 */
export function KbDot({ color, className }: { color?: string | null; className?: string }) {
  return (
    <span
      aria-hidden="true"
      className={cn('br-swatch-ring h-2 w-2 shrink-0 rounded-full', className)}
      style={{ background: color || 'var(--text-muted)' }}
    />
  );
}
