import React from 'react';

interface Props {
  onPointerDown: (e: React.PointerEvent<HTMLDivElement>) => void;
}

export const ResizeHandle: React.FC<Props> = ({ onPointerDown }) => (
  <div
    onPointerDown={onPointerDown}
    className="absolute bottom-0 right-0 w-4 h-4 cursor-nwse-resize opacity-40 hover:opacity-100 transition-opacity"
    style={{
      backgroundImage:
        'linear-gradient(135deg, transparent 50%, rgba(120,120,120,0.6) 50%, rgba(120,120,120,0.6) 70%, transparent 70%)',
    }}
    title="Drag to resize"
  />
);
