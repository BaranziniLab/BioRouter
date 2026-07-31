/**
 * BR-71 §5: provenance is structural — any message injected across sessions is
 * permanently labeled in the transcript. Renders nothing for ordinary
 * same-session messages. Styling follows design.md (text-subtle, no ring).
 */
export type MessageProvenanceView = {
  kind: 'agent_injection' | 'user_direct' | 'spawn_context';
  // Nullable, not merely optional: the generated `MessageProvenance` declares
  // these as `string | null` (serde `Option<String>` → `| null`), so a view type
  // with `?: string` would not accept `message.metadata.provenance` directly.
  fromSessionId?: string | null;
  fromSessionName?: string | null;
};

export function ProvenanceChip({ provenance }: { provenance?: MessageProvenanceView }) {
  if (!provenance) return null;
  const label =
    provenance.kind === 'agent_injection'
      ? `injected by ${provenance.fromSessionName ?? provenance.fromSessionId ?? 'another agent'}`
      : provenance.kind === 'user_direct'
        ? 'direct user message'
        : 'spawn context';
  return (
    <span
      className="inline-flex items-center gap-1 rounded-full border border-border-subtle px-2 py-0.5 text-xs text-text-subtle"
      title={provenance.fromSessionId ?? undefined}
      data-provenance-kind={provenance.kind}
    >
      {label}
    </span>
  );
}
