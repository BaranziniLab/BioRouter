import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';
import { ProvenanceChip } from './ProvenanceChip';

describe('ProvenanceChip', () => {
  it('labels agent injections with the source session name', () => {
    render(
      <ProvenanceChip
        provenance={{
          kind: 'agent_injection',
          fromSessionId: 's1',
          fromSessionName: 'Planning chat',
        }}
      />
    );
    expect(screen.getByText(/injected by Planning chat/i)).toBeTruthy();
  });

  it('falls back to the source session id when it has no name', () => {
    render(<ProvenanceChip provenance={{ kind: 'agent_injection', fromSessionId: 's1' }} />);
    expect(screen.getByText(/injected by s1/i)).toBeTruthy();
  });

  it('still names an origin when neither id nor name survived', () => {
    render(<ProvenanceChip provenance={{ kind: 'agent_injection' }} />);
    expect(screen.getByText(/injected by another agent/i)).toBeTruthy();
  });

  it('labels the context a spawning session handed down', () => {
    render(<ProvenanceChip provenance={{ kind: 'spawn_context', fromSessionId: 's-parent' }} />);
    expect(screen.getByText(/spawn context/i)).toBeTruthy();
  });

  it('labels direct human input into subagent tabs', () => {
    render(<ProvenanceChip provenance={{ kind: 'user_direct' }} />);
    expect(screen.getByText(/direct user message/i)).toBeTruthy();
  });

  it('renders nothing without provenance', () => {
    const { container } = render(<ProvenanceChip provenance={undefined} />);
    expect(container.firstChild).toBeNull();
  });
});
