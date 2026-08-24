import { useEffect, useRef } from 'react';
import { ARTIFACT_PANEL_ATTR } from '../../utils/tabCycle';
import type { ArtifactSource } from './artifactTypes';
import { basenameFromPath } from './artifactUtils';
import {
  registerPanelAccess,
  type PanelAccessor,
  type PanelDescriptor,
  type PanelTextSnapshot,
} from './panelAccessRegistry';

/** Enough to be useful, small enough not to swamp a turn's context. */
const DEFAULT_TEXT_LIMIT = 20_000;

/**
 * Publishes this chat's artifact panel to the agent.
 *
 * Called from the chat surface rather than from `ArtifactViewer`, because this
 * needs both the panel's content *and* the session it belongs to, and only the
 * chat has both. Read-only transcripts deliberately do not call it: an agent
 * reading a saved session's panel would be reading a different conversation's
 * screen.
 */
export function useArtifactPanelAccess({
  sessionId,
  artifact,
  isOpen,
  tabCount,
  liveBrowserViewId,
}: {
  sessionId: string | null | undefined;
  artifact: ArtifactSource | null;
  isOpen: boolean;
  tabCount?: number;
  /** Set when the panel is showing a live page, so we capture the right contents. */
  liveBrowserViewId?: string | null;
}) {
  // Held in a ref so the accessor closes over *current* values without the
  // registry churning on every render.
  const stateRef = useRef({ artifact, isOpen, tabCount, liveBrowserViewId });
  stateRef.current = { artifact, isOpen, tabCount, liveBrowserViewId };

  useEffect(() => {
    if (!sessionId) return;

    const describe = (): PanelDescriptor => {
      const { artifact: current, isOpen: open, tabCount: tabs } = stateRef.current;
      if (!open || !current) return { open: false };
      return {
        open: true,
        kind: current.kind === 'externalUrl' ? 'webPage' : current.kind,
        title: current.title,
        locator:
          current.kind === 'file'
            ? current.path
            : current.kind === 'externalUrl'
              ? current.url
              : undefined,
        tabCount: tabs,
      };
    };

    const readText = async (maxChars: number): Promise<PanelTextSnapshot | null> => {
      const limit = Math.max(0, maxChars || DEFAULT_TEXT_LIMIT);
      const { artifact: current, isOpen: open, liveBrowserViewId: viewId } = stateRef.current;
      if (!open || !current) return null;

      const clip = (text: string) => ({
        text: text.slice(0, limit),
        truncated: text.length > limit,
      });

      if (current.kind === 'externalUrl') {
        // A live page's text lives in a different WebContents, so it has to be
        // read through the view rather than from anything in this renderer.
        if (!viewId) return null;
        const page = await window.electron?.embeddedBrowser?.readText(viewId, limit);
        if (!page) return null;
        return {
          kind: 'webPage',
          title: page.title || current.title,
          locator: page.url,
          ...clip(page.text),
        };
      }

      if (current.kind === 'html') {
        return { kind: 'html', title: current.title, ...clip(current.html) };
      }

      if (current.kind === 'file') {
        const preview = await window.electron?.readArtifactFile(current.path);
        if (!preview || !('kind' in preview)) return null;
        if (preview.kind === 'text' || preview.kind === 'html') {
          return {
            kind: preview.kind,
            title: current.title || basenameFromPath(current.path),
            locator: current.path,
            ...clip(preview.text),
          };
        }
        // Images, documents and binaries have no text to give. Saying so is
        // more useful than returning an empty string the model would read as
        // "the file is blank".
        return null;
      }

      return null;
    };

    const capture = async () => {
      const { artifact: current, isOpen: open, liveBrowserViewId: viewId } = stateRef.current;
      if (!open || !current) return null;

      // ⚠ A live page is a separate `WebContents` — a sibling native layer, not
      // part of this document — so a window capture would return the panel's
      // chrome with a hole where the page is. It has to capture itself.
      if (current.kind === 'externalUrl' && viewId) {
        const shot = await window.electron?.embeddedBrowser?.capture(viewId);
        return shot ? { path: shot.path, width: 0, height: 0 } : null;
      }

      // Everything else lives in this document, including the sandboxed
      // `srcdoc` frames that no DOM-walking screenshot library can enter —
      // `capturePage` is a compositor grab, so it sees straight through.
      const panel = document.querySelector<HTMLElement>(`[${ARTIFACT_PANEL_ATTR}]`);
      if (!panel) return null;
      const rect = panel.getBoundingClientRect();
      if (rect.width <= 0 || rect.height <= 0) return null;
      return (
        (await window.electron?.captureRegion({
          x: rect.left,
          y: rect.top,
          width: rect.width,
          height: rect.height,
          label: 'panel',
        })) ?? null
      );
    };

    const accessor: PanelAccessor = { describe, readText, capture };
    return registerPanelAccess(sessionId, accessor);
  }, [sessionId]);
}
