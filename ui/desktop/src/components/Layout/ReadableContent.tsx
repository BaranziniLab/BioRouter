import React from 'react';

type ReadableContentProps = {
  children: React.ReactNode;
  className?: string;
  size?: 'text' | 'wide' | 'graph';
};

const WIDTH_BY_SIZE: Record<NonNullable<ReadableContentProps['size']>, string> = {
  text: 'max-w-[1120px]',
  wide: 'max-w-[1280px]',
  graph: 'max-w-[1440px]',
};

export function ReadableContent({
  children,
  className = '',
  size = 'text',
}: ReadableContentProps) {
  return (
    <div
      data-size={size}
      className={`biorouter-readable-content mx-auto w-full ${WIDTH_BY_SIZE[size]} ${className}`.trim()}
    >
      {children}
    </div>
  );
}
