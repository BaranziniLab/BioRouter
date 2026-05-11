import React, { useMemo, useState } from 'react';
import BaseChat from '../BaseChat';
import { ChatProvider, DEFAULT_CHAT_TITLE } from '../../contexts/ChatContext';
import { ChatType } from '../../types/chat';
import { DashboardWindow } from '../../contexts/DashboardContext';
import { useDashboard } from '../../contexts/DashboardContext';
import { WindowTitleBar } from './WindowTitleBar';
import { ResizeHandle } from './ResizeHandle';
import { usePointerDrag } from './useDashboardDrag';
import { updateSessionName } from '../../api';

// Default "comfort" size used by the Enlarge button — matches the standalone
// chat window dimensions in main.ts.
const ENLARGE_W = 940;
const ENLARGE_H = 800;

interface Props {
  win: DashboardWindow;
  rect: { x: number; y: number; w: number; h: number; zIndex: number };
  isFocused: boolean;
  isSolo: boolean;
  boardSize: { width: number; height: number };
  minSize: { w: number; h: number };
  /** Called once at the start of drag/resize to freeze every other on-canvas
   * window in place, so manipulating this one never reflows the others. */
  onManipulateStart?: () => void;
}

export const ChatWindow: React.FC<Props> = ({
  win,
  rect,
  isFocused,
  isSolo,
  boardSize,
  minSize,
  onManipulateStart,
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
    onMove: ({ dx, dy }) => {
      if (dx === 0 && dy === 0) return;
      onManipulateStart?.();
      setDragOffset({ dx, dy });
    },
    onEnd: ({ dx, dy }, ev) => {
      setDragOffset({ dx: 0, dy: 0 });
      void ev;
      // Canvas is infinite — no clamping to viewport.
      const dropX = rect.x + dx;
      const dropY = rect.y + dy;
      dashboard.moveWindow(win.windowId, { x: dropX, y: dropY }, { w: rect.w, h: rect.h });
    },
    onCancel: () => setDragOffset({ dx: 0, dy: 0 }),
  });

  const resizeStart = usePointerDrag({
    onMove: ({ dx, dy }) => {
      if (dx === 0 && dy === 0) return;
      onManipulateStart?.();
      setResizeDelta({ dw: dx, dh: dy });
    },
    onEnd: ({ dx, dy }) => {
      setResizeDelta({ dw: 0, dh: 0 });
      const newW = Math.max(minSize.w, rect.w + dx);
      const newH = Math.max(minSize.h, rect.h + dy);
      dashboard.resizeWindow(win.windowId, { w: newW, h: newH }, { x: rect.x, y: rect.y });
    },
    onCancel: () => setResizeDelta({ dw: 0, dh: 0 }),
  });

  const isManipulating =
    dragOffset.dx !== 0 ||
    dragOffset.dy !== 0 ||
    resizeDelta.dw !== 0 ||
    resizeDelta.dh !== 0;
  // Animate transform / size only when the dashboard is in an animation frame
  // (after organize / centerOn) AND the user isn't actively dragging/resizing.
  // Keeps drag-feel instant while making organize and focus-centering smooth.
  const transition =
    !isManipulating && dashboard.state.isAnimating
      ? 'transform 200ms cubic-bezier(0.2, 0.8, 0.2, 1), width 200ms cubic-bezier(0.2, 0.8, 0.2, 1), height 200ms cubic-bezier(0.2, 0.8, 0.2, 1)'
      : 'none';
  const stylePos = useMemo(
    () => ({
      transform: `translate(${rect.x + dragOffset.dx}px, ${rect.y + dragOffset.dy}px)`,
      width: rect.w + resizeDelta.dw,
      height: rect.h + resizeDelta.dh,
      zIndex: rect.zIndex,
      transition,
    }),
    [rect, dragOffset, resizeDelta, transition]
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
    return { transformOrigin: `${ox} ${oy}` };
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
        accentColor={win.accentColor}
        onRename={(name) => dashboard.renameWindow(win.windowId, name)}
        onClose={() => dashboard.closeWindow(win.windowId)}
        onShrink={() =>
          dashboard.resizeWindow(
            win.windowId,
            { w: minSize.w, h: minSize.h },
            { x: rect.x, y: rect.y }
          )
        }
        onEnlarge={() =>
          dashboard.resizeWindow(
            win.windowId,
            { w: ENLARGE_W, h: ENLARGE_H },
            { x: rect.x, y: rect.y }
          )
        }
        onPointerDownDrag={dragStart}
      />
      <div className="flex-1 min-h-0 relative">
        <ChatProvider chat={chat} setChat={setChat} contextKey={`dashboard-${win.sessionId}`}>
          <BaseChat
            setChat={setChat}
            sessionId={win.sessionId}
            suppressEmptyState={false}
            coherent
            hideSessionNamePill
            accentColor={win.accentColor}
            onRenameSession={(newName) => {
              dashboard.renameWindow(win.windowId, newName);
              // Propagate to biorouterd so History reflects it.
              void updateSessionName({
                path: { session_id: win.sessionId },
                body: { name: newName },
              });
            }}
            onSessionUpdate={(s) => {
              if (s?.name) dashboard.syncSessionName(win.windowId, s.name);
            }}
          />
        </ChatProvider>
      </div>
      <ResizeHandle onPointerDown={resizeStart} />
    </div>
  );
};
