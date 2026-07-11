import { useState } from 'react';
import { GitBranch } from 'lucide-react';
import { useDiverge } from '../hooks/useDiverge';

interface MessageDivergeLinkProps {
  /** The session this message belongs to — the conversation that will be branched. */
  sessionId: string;
  /** `created` timestamp (ms) of this assistant message. The branch is trimmed
   * to end exactly at this answer. */
  truncateAfterMs?: number;
}

/**
 * "Diverge" action shown next to Copy on a finished assistant message.
 *
 * Branches the current conversation into a brand-new session that inherits the
 * full history (so the new conversation resumes from exactly here) while the
 * original session stays untouched and ready to keep chatting.
 *
 * - In the Dashboard (canvas) view, the branch opens as a new chat box inline.
 * - Everywhere else (single chat / pair window), the branch opens in a new
 *   BioRouter desktop window.
 */
export default function MessageDivergeLink({
  sessionId,
  truncateAfterMs,
}: MessageDivergeLinkProps) {
  const [busy, setBusy] = useState(false);
  const { diverge, inDashboard } = useDiverge();

  const handleDiverge = async () => {
    if (busy) return;
    setBusy(true);
    try {
      await diverge(sessionId, truncateAfterMs);
    } finally {
      setBusy(false);
    }
  };

  return (
    <button
      onClick={handleDiverge}
      disabled={busy}
      aria-label="Diverge conversation into a new chat"
      title={
        inDashboard
          ? 'Branch this conversation into a new chat box (keeps full history)'
          : 'Branch this conversation into a new window (keeps full history)'
      }
      className="flex font-sans items-center gap-1 text-sm text-text-muted hover:cursor-pointer hover:text-text-default transition-[transform,opacity,color] duration-150 opacity-0 group-hover:opacity-100 -translate-y-4 group-hover:translate-y-0 disabled:opacity-50 disabled:cursor-default"
    >
      <GitBranch className="h-3 w-3" strokeWidth={1.5} />
      <span>{busy ? 'Diverging…' : 'Diverge'}</span>
    </button>
  );
}
