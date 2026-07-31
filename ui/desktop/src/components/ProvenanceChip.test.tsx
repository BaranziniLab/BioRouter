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

  it('labels direct human input into subagent tabs', () => {
    render(<ProvenanceChip provenance={{ kind: 'user_direct' }} />);
    expect(screen.getByText(/direct user message/i)).toBeTruthy();
  });

  it('renders nothing without provenance', () => {
    const { container } = render(<ProvenanceChip provenance={undefined} />);
    expect(container.firstChild).toBeNull();
  });
});
