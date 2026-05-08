import React from 'react';
import { useLabMeeting } from '../../contexts/LabMeetingContext';
import { TuckedCard } from './TuckedCard';

interface Props {
  onCardDragStart: (windowId: string) => (e: React.PointerEvent) => void;
}

export const TuckSidebar: React.FC<Props> = ({ onCardDragStart }) => {
  const lab = useLabMeeting();
  const tucked = lab.state.windows.filter((w) => w.isTucked);

  if (tucked.length === 0) return null;

  return (
    <div className="w-64 flex-shrink-0 h-full flex flex-col bg-background-muted/60 border-l border-border-subtle/40 backdrop-blur-sm">
      <div className="px-3 py-2 text-xs font-semibold text-text-muted uppercase tracking-wider border-b border-border-subtle/30">
        Tucked Chats · {tucked.length}
      </div>
      <div className="flex-1 overflow-y-auto p-2 space-y-2">
        {tucked.map((w) => (
          <TuckedCard
            key={w.windowId}
            win={w}
            preview={[]}
            onEvoke={() => lab.evokeWindow(w.windowId)}
            onClose={() => lab.closeWindow(w.windowId)}
            onDragStart={onCardDragStart(w.windowId)}
          />
        ))}
      </div>
    </div>
  );
};
