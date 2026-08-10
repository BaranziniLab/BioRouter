import { useState } from 'react';
import { GitBranch } from './icons/app-icons';
import { useDiverge } from '../hooks/useDiverge';
import { MessageMetaAction } from './MessageMeta';

interface MessageDivergeLinkProps {
  /** The session this message belongs to — the conversation that will be branched. */
  sessionId: string;
  /** `created` timestamp (ms) of this assistant message. The branch is trimmed
   * to end exactly at this answer. */
  truncateAfterMs?: number;
  /** Durable id of this assistant message. Preferred over the timestamp when
   * resolving the exact branch point. */
  truncateAfterId?: string;
}

/**
 * "Diverge" action shown next to Copy on a finished assistant message.
 *
 * Branches the current conversation into a brand-new session that inherits the
 * full history (so the new conversation resumes from exactly here) while the
 * original session stays untouched and ready to keep chatting.
 *
 * The branch opens in a new Biorouter desktop window.
 */
export default function MessageDivergeLink({
  sessionId,
  truncateAfterMs,
  truncateAfterId,
}: MessageDivergeLinkProps) {
  const [busy, setBusy] = useState(false);
  const { diverge } = useDiverge();

  const handleDiverge = async () => {
    if (busy) return;
    setBusy(true);
    try {
      await diverge(sessionId, truncateAfterMs, truncateAfterId);
    } finally {
      setBusy(false);
    }
  };

  return (
    <MessageMetaAction
      onClick={handleDiverge}
      disabled={busy}
      icon={<GitBranch />}
      aria-label="Branch this chat into a new window"
      title="Branch this chat into a new window (keeps full history)"
    >
      {/* "Diverge" is the internal name for this operation (#83 category 5).
          The accessible name above already says "Branch", so leaving the
          visible word as Diverge announced two names for one control. */}
      {busy ? 'Branching…' : 'Branch'}
    </MessageMetaAction>
  );
}
