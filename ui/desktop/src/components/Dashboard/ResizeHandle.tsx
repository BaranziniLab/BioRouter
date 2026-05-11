import React from 'react';

interface Props {
  onPointerDown: (e: React.PointerEvent<HTMLDivElement>) => void;
}

// The resize handle is always visible so users can always reach it, regardless
// of how small the window is. z-index sits above the chat content so it's never
// occluded by ChatInput controls or messages.
export const ResizeHandle: React.FC<Props> = ({ onPointerDown }) => (
  <div
    onPointerDown={onPointerDown}
    className="absolute bottom-0 right-0 w-5 h-5 cursor-nwse-resize opacity-70 hover:opacity-100 transition-opacity z-20"
    style={{
      backgroundImage:
        'linear-gradient(135deg, transparent 45%, rgba(120,120,120,0.75) 45%, rgba(120,120,120,0.75) 65%, transparent 65%)',
    }}
    title="Drag to resize"
  />
);
