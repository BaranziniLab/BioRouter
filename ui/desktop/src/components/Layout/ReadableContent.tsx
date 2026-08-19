import React from 'react';

type ReadableContentProps = {
  children: React.ReactNode;
  className?: string;
  size?: 'chat' | 'text' | 'wide' | 'graph';
};

/**
 * `chat` is the column the composer and every chat message occupy
 * (`max-w-measure-chat`, see BaseChat.tsx / ChatInput.tsx). A view that sits
 * directly above the composer — Home — must use it, or its edges will not line
 * up. It names the TOKEN rather than a literal precisely so that alignment
 * survives the measure changing, which it since has: the flat 760px this line
 * used to quote is now the floor of a clamp.
 */
/**
 * ⚠ Every one of these is a CLAMP, not a flat cap, for the reason spelled out
 * on `--measure-chat` in main.css: a fixed pixel ceiling means a wider window
 * buys dead margin instead of content. The floor of each is exactly the value
 * it replaced, so nothing narrow moves — `max-width` cannot force a box wider
 * than its parent — and the percentage resolves against the content pane rather
 * than the viewport, so the sidebar opening does not widen the column.
 *
 * `chat` stays keyed to the token so it and the composer can never drift; the
 * other three are page measures with their own ceilings.
 */
const WIDTH_BY_SIZE: Record<NonNullable<ReadableContentProps['size']>, string> = {
  chat: 'max-w-measure-chat',
  text: 'max-w-[clamp(1120px,88%,1720px)]',
  wide: 'max-w-[clamp(1280px,92%,1920px)]',
  // `graph` is the Knowledge section's canvas measure and is the ONE size here
  // that reads a token rather than restating a clamp: `--measure-graph` is the
  // widest column in the app, and a literal beside two named siblings is the
  // drift `--measure-chat` already had to be rescued from (ui-spec §3.1, §6.2).
  graph: 'max-w-measure-graph',
};

export function ReadableContent({ children, className = '', size = 'text' }: ReadableContentProps) {
  return (
    <div
      data-size={size}
      className={`biorouter-readable-content mx-auto w-full ${WIDTH_BY_SIZE[size]} ${className}`.trim()}
    >
      {children}
    </div>
  );
}
