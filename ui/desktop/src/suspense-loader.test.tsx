import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import SuspenseLoader from './suspense-loader';

describe('SuspenseLoader', () => {
  it('renders the Biorouter mark while the application loads', () => {
    render(<SuspenseLoader />);

    expect(screen.getByText('Loading...')).toBeInTheDocument();
    expect(screen.getByRole('img', { name: 'BioRouter' })).toBeInTheDocument();
  });
});
