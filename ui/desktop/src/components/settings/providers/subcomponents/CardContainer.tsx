import React from 'react';

interface CardContainerProps {
  header: React.ReactNode;
  body: React.ReactNode;
  onClick: () => void;
  grayedOut: boolean;
  testId?: string;
}

export default function CardContainer({
  header,
  body,
  onClick,
  grayedOut = false,
  testId,
}: CardContainerProps) {
  return (
    <div
      data-testid={testId}
      className={[
        'rounded-xl p-3 flex flex-col transition-all duration-200 h-[160px]',
        'bg-background-card text-text-default border border-border-subtle',
        header ? 'justify-between' : 'justify-center',
        grayedOut
          ? 'opacity-50 cursor-default'
          : 'cursor-pointer hover:bg-background-muted hover:border-border-strong',
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
