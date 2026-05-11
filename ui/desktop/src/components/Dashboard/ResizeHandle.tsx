import React from 'react';

interface Props {
  onPointerDown: (e: React.PointerEvent<HTMLDivElement>) => void;
}

// Minimalist resize corner: two short hairlines on the SE corner. Always visible
// (z-20 so chat content never occludes it) but very subtle — matches the rest of
// the BioRouter UI's light, low-chrome aesthetic. Hit area is the full 16×16 box.
export const ResizeHandle: React.FC<Props> = ({ onPointerDown }) => (
  <div
    onPointerDown={onPointerDown}
    className="absolute bottom-0 right-0 w-4 h-4 cursor-nwse-resize z-20 group"
    title="Drag to resize"
  >
    <svg
      viewBox="0 0 16 16"
      className="absolute inset-0 w-full h-full text-text-muted/60 group-hover:text-text-default transition-colors"
      fill="none"
      stroke="currentColor"
      strokeWidth="1"
      strokeLinecap="round"
    >
      <line x1="12" y1="6" x2="6" y2="12" />
      <line x1="12" y1="10" x2="10" y2="12" />
    </svg>
  </div>
);
