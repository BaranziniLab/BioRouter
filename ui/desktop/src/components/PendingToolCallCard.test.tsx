import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { PendingToolCallCard } from './PendingToolCallCard';

describe('PendingToolCallCard', () => {
  it('renders camel-case tool identifiers as readable words while arguments stream', () => {
    render(
      <PendingToolCallCard pending={{ id: 'tool-1', name: 'skills__installMarketplaceSkill' }} />
    );

    expect(screen.getByText('Install Marketplace Skill')).toBeInTheDocument();
    expect(screen.queryByText('Installmarketplaceskill')).not.toBeInTheDocument();
  });
});
