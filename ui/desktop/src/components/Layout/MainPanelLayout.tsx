import React from 'react';

export const MainPanelLayout: React.FC<{
  children: React.ReactNode;
  removeTopPadding?: boolean;
  backgroundColor?: string;
}> = ({ children, removeTopPadding = false, backgroundColor = 'bg-background-muted' }) => {
  // We deliberately use `h-full` here, not `h-dvh`. `h-dvh` (dynamic viewport
  // height) forces the layout to viewport size regardless of the parent — which
  // breaks small chat surfaces: a 420px-tall chat pane would render its panel
  // at ~1050px and push the ChatInput off-screen. With `h-full`, the panel
  // fills its container, whether that's the viewport (standalone /pair, etc.)
  // or a chat pane's rect.
  return (
    <div
      className={`flex flex-col ${backgroundColor} h-full min-w-0 min-h-0 ${removeTopPadding ? '' : 'pt-[32px]'}`}
    >
      {children}
    </div>
  );
};
