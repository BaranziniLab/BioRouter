import React, { useMemo, useState } from 'react';
import BaseChat from '../BaseChat';
import { ChatProvider, DEFAULT_CHAT_TITLE } from '../../contexts/ChatContext';
import { ChatType } from '../../types/chat';
import { DashboardWindow } from '../../contexts/DashboardContext';
import { useDashboard } from '../../contexts/DashboardContext';
import { WindowTitleBar } from './WindowTitleBar';
import { ResizeHandle } from './ResizeHandle';
import { usePointerDrag } from './useDashboardDrag';

interface Props {
  win: DashboardWindow;
  rect: { x: number; y: number; w: number; h: number; zIndex: number };
  isFocused: boolean;
  isSolo: boolean;
  boardSize: { width: number; height: number };
  minSize: { w: number; h: number };
  onTuckByDrag?: (windowId: string) => void;
  sidebarOpen: boolean;
}

export const ChatWindow: React.FC<Props> = ({
  win,
  rect,
  isFocused,
  isSolo,
  boardSize,
  minSize,
  onTuckByDrag,
  sidebarOpen,
}) => {
  const dashboard = useDashboard();
  const [chat, setChat] = useState<ChatType>({
    sessionId: win.sessionId,
    name: win.name || DEFAULT_CHAT_TITLE,
    messages: [],
    workflow: null,
    workflowParameterValues: null,
  });

  const [dragOffset, setDragOffset] = useState<{ dx: number; dy: number }>({ dx: 0, dy: 0 });
  const [resizeDelta, setResizeDelta] = useState<{ dw: number; dh: number }>({ dw: 0, dh: 0 });

  const dragStart = usePointerDrag({
    onMove: ({ dx, dy }) => setDragOffset({ dx, dy }),
    onEnd: ({ dx, dy }, ev) => {
      setDragOffset({ dx: 0, dy: 0 });
      const dropX = rect.x + dx;
      const dropY = rect.y + dy;
      // Drop into sidebar zone? Right strip of board.
      const zoneWidth = sidebarOpen ? boardSize.width * 0.2 : boardSize.width * 0.12;
      if (onTuckByDrag && dropX + rect.w / 2 > boardSize.width - zoneWidth) {
        onTuckByDrag(win.windowId);
        return;
      }
      void ev;
      const clampedX = Math.max(-rect.w + 80, Math.min(boardSize.width - 80, dropX));
      const clampedY = Math.max(0, Math.min(boardSize.height - 40, dropY));
      dashboard.moveWindow(win.windowId, { x: clampedX, y: clampedY });
    },
    onCancel: () => setDragOffset({ dx: 0, dy: 0 }),
  });

  const resizeStart = usePointerDrag({
    onMove: ({ dx, dy }) => setResizeDelta({ dw: dx, dh: dy }),
    onEnd: ({ dx, dy }) => {
      setResizeDelta({ dw: 0, dh: 0 });
      const newW = Math.max(minSize.w, rect.w + dx);
      const newH = Math.max(minSize.h, rect.h + dy);
      dashboard.resizeWindow(win.windowId, { w: newW, h: newH });
    },
    onCancel: () => setResizeDelta({ dw: 0, dh: 0 }),
  });

  const stylePos = useMemo(
    () => ({
      transform: `translate(${rect.x + dragOffset.dx}px, ${rect.y + dragOffset.dy}px)`,
      width: rect.w + resizeDelta.dw,
      height: rect.h + resizeDelta.dh,
      zIndex: rect.zIndex,
    }),
    [rect, dragOffset, resizeDelta]
  );

  const popStyle = useMemo(() => {
    if (!isFocused) return {};
    const TOUCH = 4;
    const leftTouching = rect.x <= TOUCH;
    const rightTouching = rect.x + rect.w >= boardSize.width - TOUCH;
    const topTouching = rect.y <= TOUCH;
    const bottomTouching = rect.y + rect.h >= boardSize.height - TOUCH;
    const ox = leftTouching ? 'left' : rightTouching ? 'right' : 'center';
    const oy = topTouching ? 'top' : bottomTouching ? 'bottom' : 'center';
    return {
      transformOrigin: `${ox} ${oy}`,
    };
  }, [isFocused, rect.x, rect.y, rect.w, rect.h, boardSize.width, boardSize.height]);

  const TOUCH_PX = 4;
  const topTouching = rect.y <= TOUCH_PX;
  const focusClasses = isFocused
    ? isSolo
      ? 'shadow-[0_8px_30px_rgb(0,0,0,0.18)]'
      : `shadow-[0_12px_40px_rgb(0,0,0,0.22)] scale-[1.01] ${topTouching ? '' : '-translate-y-0.5'}`
    : 'shadow-[0_4px_14px_rgb(0,0,0,0.10)]';

  return (
    <div
      className={`absolute top-0 left-0 rounded-2xl bg-background-default border border-border-subtle/30 overflow-hidden flex flex-col transition-shadow ${focusClasses}`}
      style={{ ...stylePos, ...popStyle }}
      onMouseDown={() => {
        if (!isFocused) dashboard.focusWindow(win.windowId);
      }}
    >
      <WindowTitleBar
        name={win.name}
        badge={win.badge}
        accentColor={win.accentColor}
        onRename={(name) => dashboard.renameWindow(win.windowId, name)}
        onClose={() => dashboard.closeWindow(win.windowId)}
        onPointerDownDrag={dragStart}
      />
      <div className="flex-1 min-h-0 relative">
        <ChatProvider chat={chat} setChat={setChat} contextKey={`dashboard-${win.sessionId}`}>
          <BaseChat
            setChat={setChat}
            sessionId={win.sessionId}
            suppressEmptyState={false}
            coherent
            hideStatusBar
          />
        </ChatProvider>
      </div>
      <ResizeHandle onPointerDown={resizeStart} />
    </div>
  );
};
