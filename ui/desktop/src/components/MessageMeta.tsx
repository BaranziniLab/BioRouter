import * as React from 'react';
import { cn } from '../utils';

/**
 * The one metadata unit under a message.
 *
 * Three copies of this row existed — one under an assistant answer, one under a
 * run of tool calls, one under the user's bubble — and each carried its own
 * hand-written animation stack: `text-sm` (14px, the SAME size as the message it
 * annotates) sliding up and fading out on `group-hover` while an absolutely
 * positioned action row slid down into the space it vacated. Two consequences,
 * both of them bugs rather than taste:
 *
 * 1. **The timestamp cost as much visual weight as the answer.** 14px repeated
 *    under every message is the transcript's single largest density cost, and it
 *    is the least important text on the surface. It drops to `supporting`
 *    (12/16) — the role the design of record names for timestamps, metadata and
 *    captions.
 * 2. **The actions were invisible until hovered**, so Copy and Diverge were
 *    undiscoverable, and the swap meant the timestamp and the actions could
 *    never be read at once. They now sit *beside* the timestamp permanently.
 *
 * The row is a fixed 20px box with `items-center`, which is the §3.10 optical
 * axis: whatever a caller puts in it — a 12px timestamp, a 14px glyph, a 20px
 * button — shares one centre line, so the meta row under a bubble and the one
 * under a tool run cannot drift apart by a pixel.
 */
interface MessageMetaProps {
  /** Preformatted timestamp. Omitted for a row that is only actions. */
  timestamp?: React.ReactNode;
  /**
   * Which edge the row hugs. `end` also reverses the order so the timestamp
   * stays nearest the edge the bubble is aligned to and the actions read
   * inward — the same relationship `start` has, mirrored.
   */
  align?: 'start' | 'end';
  className?: string;
  /** Actions — `MessageMetaAction`s, or anything of the same height. */
  children?: React.ReactNode;
}

export function MessageMeta({ timestamp, align = 'start', className, children }: MessageMetaProps) {
  const time = timestamp ? (
    <span className="text-supporting text-text-muted tabular-nums">{timestamp}</span>
  ) : null;

  return (
    <div
      data-message-meta={align}
      className={cn(
        'flex h-5 items-center gap-3 pt-px',
        align === 'end' ? 'justify-end' : 'justify-start',
        className
      )}
    >
      {align === 'end' ? (
        <>
          {children}
          {time}
        </>
      ) : (
        <>
          {time}
          {children}
        </>
      )}
    </div>
  );
}

/**
 * One action inside a `MessageMeta` row — Copy, Diverge, Edit.
 *
 * The three were byte-similar 40-character class strings that had already
 * drifted (one carried `rounded`, one did not; one had a disabled state, two did
 * not). Here they are one recipe: `supporting` ink, a 14px glyph, muted at rest
 * and full ink on hover, and no motion at all — the row does not move, so
 * nothing has to animate out of the way.
 */
interface MessageMetaActionProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  icon: React.ReactNode;
  children: React.ReactNode;
}

export function MessageMetaAction({ icon, children, className, ...props }: MessageMetaActionProps) {
  return (
    <button
      type="button"
      {...props}
      className={cn(
        'flex items-center gap-1 rounded-inner text-supporting text-text-muted',
        'transition-colors hover:text-text-default hover:cursor-pointer',
        'disabled:cursor-default disabled:opacity-50 disabled:hover:text-text-muted',
        '[&_svg]:size-3.5 [&_svg]:shrink-0',
        className
      )}
    >
      {icon}
      <span>{children}</span>
    </button>
  );
}
