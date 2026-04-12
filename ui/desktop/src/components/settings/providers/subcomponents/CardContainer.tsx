import React from 'react';

interface CardContainerProps {
  header: React.ReactNode;
  body: React.ReactNode;
  onClick: () => void;
  grayedOut: boolean;
  testId?: string;
  borderStyle?: 'solid' | 'dashed';
}

export default function CardContainer({
  header,
  body,
  onClick,
  grayedOut = false,
  testId,
  borderStyle = 'solid',
}: CardContainerProps) {
  return (
    <div
      data-testid={testId}
      className={[
        'rounded-xl p-3 flex flex-col transition-all duration-200 h-[160px]',
        'bg-background-card text-text-default [box-shadow:var(--shadow-default)]',
        header ? 'justify-between' : 'justify-center',
        borderStyle === 'dashed' ? 'border-2 border-dashed border-border-default' : '',
        grayedOut
          ? 'opacity-50 cursor-default'
          : 'cursor-pointer hover:ring-1 hover:ring-border-strong',
      ]
        .filter(Boolean)
        .join(' ')}
      onClick={!grayedOut ? onClick : undefined}
    >
      {header && <div>{header}</div>}
      <div>{body}</div>
    </div>
  );
}
