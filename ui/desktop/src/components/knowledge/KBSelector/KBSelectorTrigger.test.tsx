import { render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { KBSelectorTrigger } from './KBSelectorTrigger';
import type { Manifest } from '../../../api/types.gen';

const state = vi.hoisted(() => ({
  primaryKb: null as Partial<Manifest> | null,
  visibleBases: [] as Partial<Manifest>[],
}));

vi.mock('../KnowledgeContext', () => ({
  useKnowledge: () => state,
}));

vi.mock('./KBSelectorPalette', () => ({
  KBSelectorPalette: () => null,
}));

function base(id: string) {
  return { id, name: id, color: '#cf6d47' };
}

beforeEach(() => {
  state.primaryKb = null;
  state.visibleBases = [];
});

describe('KBSelectorTrigger', () => {
  it('names the primary and counts the rest of the set', () => {
    state.primaryKb = base('alpha');
    state.visibleBases = [base('alpha'), base('beta'), base('gamma')];
    render(<KBSelectorTrigger />);
    expect(screen.getByText('alpha')).toBeInTheDocument();
    expect(screen.getByText('+2')).toBeInTheDocument();
  });

  // With no primary the name slot names nothing, so every visible base is
  // "other". Subtracting one unconditionally would undercount them.
  it('counts every visible base when there is no primary', () => {
    state.visibleBases = [base('alpha'), base('beta')];
    render(<KBSelectorTrigger />);
    expect(screen.getByText('No primary knowledge base')).toBeInTheDocument();
    expect(screen.getByText('+2')).toBeInTheDocument();
  });

  it('shows no count when the primary is the only base in the chat', () => {
    state.primaryKb = base('alpha');
    state.visibleBases = [base('alpha')];
    render(<KBSelectorTrigger />);
    expect(screen.queryByText(/^\+\d+$/)).not.toBeInTheDocument();
  });
});
