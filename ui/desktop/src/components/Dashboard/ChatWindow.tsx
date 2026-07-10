import React, { useEffect, useMemo, useRef, useState } from 'react';
import BaseChat from '../BaseChat';
import { ChatProvider, DEFAULT_CHAT_TITLE } from '../../contexts/ChatContext';
import { ChatType } from '../../types/chat';
import { DashboardWindow } from '../../contexts/DashboardContext';
import { useDashboard } from '../../contexts/DashboardContext';
import { DashboardCanvasContext } from '../../contexts/DashboardCanvasContext';
import { WindowTitleBar } from './WindowTitleBar';
import { ResizeHandle } from './ResizeHandle';
import { usePointerDrag } from './useDashboardDrag';
import { announceSessionName, renameSession } from '../../utils/sessionNameSync';
import { toastError } from '../../toasts';
import { errorMessage } from '../../utils/conversionUtils';
import { FoldedCard } from './FoldedCard';
import { CARD_W, CARD_H } from './DashboardProvider';
import { snapSizeToDevicePixel, snapToDevicePixel } from './pixelSnap';
import { useDiverge } from '../../hooks/useDiverge';

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
  const { diverge } = useDiverge();
  const [chat, setChat] = useState<ChatType>({
    sessionId: win.sessionId,
    name: win.name || DEFAULT_CHAT_TITLE,
    messages: [],
    workflow: null,
    workflowParameterValues: null,
  });

  const [dragOffset, setDragOffset] = useState<{ dx: number; dy: number }>({ dx: 0, dy: 0 });
  const [resizeDelta, setResizeDelta] = useState<{ dw: number; dh: number }>({ dw: 0, dh: 0 });

  // Bump each time the window transitions from folded → unfolded so BaseChat
  // can re-focus the input and scroll to bottom. BaseChat stays mounted while
  // folded (display:none on the chrome wrapper) so mount-time autofocus never
  // re-fires — without this, the user has to click the input after unfolding
  // and the conversation can sit at a stale scroll position that clips the
  // input row.
  const [focusTrigger, setFocusTrigger] = useState(0);
  const prevFoldedRef = useRef(win.folded);
  const handleTitleDiverge = React.useCallback(() => {
    void diverge(win.sessionId);
  }, [diverge, win.sessionId]);

  useEffect(() => {
    if (prevFoldedRef.current && !win.folded) {
      setFocusTrigger((c) => c + 1);
    }
    prevFoldedRef.current = win.folded;
  }, [win.folded]);

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
    dragOffset.dx !== 0 || dragOffset.dy !== 0 || resizeDelta.dw !== 0 || resizeDelta.dh !== 0;
  // Animate transform / size only when the dashboard is in an animation frame
  // (after organize / centerOn) AND the user isn't actively dragging/resizing.
  // Keeps drag-feel instant while making organize and focus-centering smooth.
  const transition =
    !isManipulating && dashboard.state.isAnimating
      ? 'transform 200ms cubic-bezier(0.2, 0.8, 0.2, 1), width 200ms cubic-bezier(0.2, 0.8, 0.2, 1), height 200ms cubic-bezier(0.2, 0.8, 0.2, 1)'
      : 'none';
  // Snap translations and live resize dimensions to physical pixels so nested
  // dashboard transforms do not composite chat text between device pixels.
  const tx = snapToDevicePixel(rect.x + dragOffset.dx);
  const ty = snapToDevicePixel(rect.y + dragOffset.dy);
  const effectiveW = win.folded ? CARD_W : rect.w;
  const effectiveH = win.folded ? CARD_H : rect.h;
  const width = snapSizeToDevicePixel(
    win.folded ? effectiveW : Math.max(minSize.w, effectiveW + resizeDelta.dw)
  );
  const height = snapSizeToDevicePixel(
    win.folded ? effectiveH : Math.max(minSize.h, effectiveH + resizeDelta.dh)
  );
  const stylePos = useMemo(
    () => ({
      transform: `translate(${tx}px, ${ty}px)`,
      width,
      height,
      zIndex: rect.zIndex,
      transition,
    }),
    [tx, ty, width, height, rect.zIndex, transition]
  );

  // Focus indication is via shadow only — no scale/translate so the focused
  // window always sits at exactly its grid slot (no visual size drift).
  const popStyle = {};
  void boardSize;
  const focusClasses = isFocused ? 'shadow-modal' : 'shadow-popover';
  void isSolo;

  return (
    <div
      className={`absolute top-0 left-0 rounded-2xl bg-background-default border border-border-subtle overflow-hidden transition-shadow ${focusClasses}`}
      style={{ ...stylePos, ...popStyle }}
      onMouseDown={(e) => {
        if (isFocused) return;
        const target = e.target as HTMLElement;
        const isInteractive =
          target.closest(
            'button, input, textarea, select, a, [role="button"], [data-radix-popper-content-wrapper], [data-slot^="dropdown-menu"]'
          ) !== null;
        if (isInteractive) return;
        dashboard.focusWindow(win.windowId);
      }}
    >
      {/* Full window chrome — hidden while folded so BaseChat stays mounted. */}
      <div
        className="absolute inset-0 flex flex-col"
        style={{ display: win.folded ? 'none' : 'flex' }}
      >
        <WindowTitleBar
          name={win.name}
          accentColor={win.accentColor}
          onRename={(name) => dashboard.renameWindow(win.windowId, name)}
          onClose={() => dashboard.closeWindow(win.windowId)}
          onShrink={() => {
            if (!isFocused) dashboard.focusWindow(win.windowId);
            dashboard.resizeWindow(
              win.windowId,
              { w: minSize.w, h: minSize.h },
              { x: rect.x, y: rect.y }
            );
          }}
          onEnlarge={() => {
            if (!isFocused) dashboard.focusWindow(win.windowId);
            dashboard.resizeWindow(
              win.windowId,
              { w: ENLARGE_W, h: ENLARGE_H },
              { x: rect.x, y: rect.y }
            );
          }}
          onFold={() => dashboard.foldWindow(win.windowId, true)}
          onDiverge={handleTitleDiverge}
          canDiverge={Boolean(win.sessionId)}
          onPointerDownDrag={dragStart}
        />
        <div className="flex-1 min-h-0 relative">
          <ChatProvider chat={chat} setChat={setChat} contextKey={`dashboard-${win.sessionId}`}>
            {/* Mark this BaseChat as living on the dashboard canvas so a
                Diverge from here spawns a new on-canvas window instead of a
                new Electron window. */}
            <DashboardCanvasContext.Provider value={true}>
              <BaseChat
                setChat={setChat}
                sessionId={win.sessionId}
                suppressEmptyState
                coherent
                hideSessionNamePill
                showPopularTopics={false}
                accentColor={win.accentColor}
                onBusyChange={(busy) => dashboard.setWindowBusy(win.windowId, busy)}
                focusTrigger={focusTrigger}
                onLatestMessage={(text) => dashboard.setWindowPreview(win.windowId, text)}
                onRenameSession={(newName) => {
                  dashboard.renameWindow(win.windowId, newName);
                  announceSessionName({
                    sessionId: win.sessionId,
                    name: newName,
                    userSetName: true,
                    origin: 'user',
                  });
                  void renameSession(win.sessionId, newName, 'user').catch((err) => {
                    if (win.name) {
                      dashboard.renameWindow(win.windowId, win.name);
                      announceSessionName({
                        sessionId: win.sessionId,
                        name: win.name,
                        userSetName: win.userSetName,
                        origin: 'sync',
                      });
                    }
                    toastError({
                      title: 'Failed to rename session',
                      msg: errorMessage(err),
                    });
                  });
                }}
                onSessionUpdate={(s) => {
                  if (s?.name) {
                    dashboard.syncSessionName(win.windowId, s.name, {
                      userSetName: s.userSetName,
                    });
                  }
                }}
              />
            </DashboardCanvasContext.Provider>
          </ChatProvider>
        </div>
        <ResizeHandle onPointerDown={resizeStart} />
      </div>
      {/* Folded card — layered on top when folded. */}
      {win.folded && (
        <div className="absolute inset-0">
          <FoldedCard
            name={win.name}
            cwd={win.cwd}
            accentColor={win.accentColor}
            isBusy={win.isBusy}
            previewTail={win.previewTail}
            onUnfold={() => {
              if (!isFocused) dashboard.focusWindow(win.windowId);
              dashboard.foldWindow(win.windowId, false);
            }}
            onShrink={() => {
              if (!isFocused) dashboard.focusWindow(win.windowId);
              dashboard.foldWindow(win.windowId, false);
              dashboard.resizeWindow(
                win.windowId,
                { w: minSize.w, h: minSize.h },
                { x: rect.x, y: rect.y }
              );
            }}
            onEnlarge={() => {
              if (!isFocused) dashboard.focusWindow(win.windowId);
              dashboard.foldWindow(win.windowId, false);
              dashboard.resizeWindow(
                win.windowId,
                { w: ENLARGE_W, h: ENLARGE_H },
                { x: rect.x, y: rect.y }
              );
            }}
            onClose={() => dashboard.closeWindow(win.windowId)}
            onPointerDownDrag={dragStart}
          />
        </div>
      )}
    </div>
  );
};
