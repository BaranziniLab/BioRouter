/**
 * BR-71 §5: provenance is structural — any message injected across sessions is
 * permanently labeled in the transcript. Renders nothing for ordinary
 * same-session messages. Styling follows design.md (text-subtle, no ring).
 */
import type { ProvenanceKind } from '../api';

export type MessageProvenanceView = {
  // Pinned to the generated enum rather than re-spelled here: the Rust
  // `ProvenanceKind` match in `agents/agent.rs` is deliberately exhaustive, and
  // this is its mirror. Adding a fourth variant on the backend regenerates
  // `ProvenanceKind`, which then fails `labelFor`'s `never` check below instead
  // of silently rendering the last arm's label for a kind nobody wrote copy for.
  kind: ProvenanceKind;
  // Nullable, not merely optional: the generated `MessageProvenance` declares
  // these as `string | null` (serde `Option<String>` → `| null`), so a view type
  // with `?: string` would not accept `message.metadata.provenance` directly.
  fromSessionId?: string | null;
  fromSessionName?: string | null;
};

function labelFor(provenance: MessageProvenanceView): string {
  switch (provenance.kind) {
    case 'agent_injection':
      return `injected by ${provenance.fromSessionName ?? provenance.fromSessionId ?? 'another agent'}`;
    case 'user_direct':
      return 'direct user message';
    case 'spawn_context':
      return 'spawn context';
    default: {
      // Unreachable while the switch stays exhaustive — this line is the type
      // error a new variant trips. It deliberately does not throw: a transcript
      // that crashes is worse than one labeled generically, should a session
      // persisted by a newer daemon ever be replayed here.
      const unhandled: never = provenance.kind;
      return `cross-session message (${String(unhandled)})`;
    }
  }
}

export function ProvenanceChip({ provenance }: { provenance?: MessageProvenanceView }) {
  if (!provenance) return null;
  const label = labelFor(provenance);
  // The session name comes from another session, so its length is not ours to
  // assume: cap the chip and clip the label rather than let it widen the row.
  // The full text (plus the id, which the label may not carry) moves into the
  // tooltip so nothing is lost to the clip.
  const title = provenance.fromSessionId ? `${label} (${provenance.fromSessionId})` : label;
  return (
    <span
      className="inline-flex min-w-0 max-w-xs items-center gap-1 rounded-full border border-border-subtle px-2 py-0.5 text-xs text-text-subtle"
      title={title}
      data-provenance-kind={provenance.kind}
    >
      <span className="min-w-0 truncate">{label}</span>
    </span>
  );
}
