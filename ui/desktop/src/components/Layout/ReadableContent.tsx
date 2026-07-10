import React from 'react';

type ReadableContentProps = {
  children: React.ReactNode;
  className?: string;
  size?: 'chat' | 'text' | 'wide' | 'graph';
};

/**
 * `chat` is the column the composer and every chat message occupy
 * (`max-w-[760px]`, see BaseChat.tsx / ChatInput.tsx). A view that sits directly
 * above the composer — Home — must use it, or its edges will not line up.
 */
const WIDTH_BY_SIZE: Record<NonNullable<ReadableContentProps['size']>, string> = {
  chat: 'max-w-[760px]',
  text: 'max-w-[1120px]',
  wide: 'max-w-[1280px]',
  graph: 'max-w-[1440px]',
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
