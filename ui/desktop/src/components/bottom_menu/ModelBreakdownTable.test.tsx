import { describe, it, expect } from 'vitest';
import { render, screen, within } from '@testing-library/react';
import { ModelBreakdownTable, modelLabel } from './ModelBreakdownTable';
import type { ModelCostRow } from '../../hooks/useCostTracking';

// A fixed payload mirroring the backend rollup: two real models the user
// switched between mid-thread, plus an unknown (null-model) bucket whose price
// is unknown.
const rows: ModelCostRow[] = [
  {
    provider: 'openai',
    model: 'gpt-5',
    inputTokens: 300,
    outputTokens: 70,
    totalTokens: 370,
    turns: 2,
    totalCost: 1.23,
  },
  {
    provider: 'anthropic',
    model: 'claude-fable-5',
    inputTokens: 10,
    outputTokens: 5,
    totalTokens: 15,
    turns: 1,
    totalCost: 0.0004,
  },
  {
    provider: undefined,
    model: undefined,
    inputTokens: 1,
    outputTokens: 2,
    totalTokens: 3,
    turns: 1,
    totalCost: null,
  },
];

describe('modelLabel', () => {
  it('joins provider and model, and folds the null bucket to "unknown"', () => {
    expect(modelLabel({ provider: 'openai', model: 'gpt-5' })).toBe('openai/gpt-5');
    expect(modelLabel({ provider: undefined, model: 'gpt-5' })).toBe('gpt-5');
    expect(modelLabel({ provider: undefined, model: undefined })).toBe('unknown');
  });
});

describe('ModelBreakdownTable', () => {
  it('renders one row per model with hand-checked token and cost cells', () => {
    render(<ModelBreakdownTable rows={rows} />);

    const table = screen.getByTestId('model-breakdown-table');
    const bodyRows = within(table).getAllByRole('row');
    // 1 header + 3 data rows.
    expect(bodyRows).toHaveLength(4);

    // gpt-5 row: label, turns, in, out, priced cost.
    const gptRow = screen.getByText('openai/gpt-5').closest('tr')!;
    const gptCells = within(gptRow).getAllByRole('cell');
    expect(gptCells.map((c) => c.textContent)).toEqual(['openai/gpt-5', '2', '300', '70', '$1.23']);

    // Sub-cent cost renders as the compact "<$0.01".
    const claudeRow = screen.getByText('anthropic/claude-fable-5').closest('tr')!;
    expect(within(claudeRow).getAllByRole('cell')[4].textContent).toBe('<$0.01');

    // Unknown bucket: labelled "unknown", null price shown as an em dash.
    const unknownRow = screen.getByText('unknown').closest('tr')!;
    const unknownCells = within(unknownRow).getAllByRole('cell');
    expect(unknownCells.map((c) => c.textContent)).toEqual(['unknown', '1', '1', '2', '—']);
  });

  it('renders nothing for an empty payload', () => {
    const { container } = render(<ModelBreakdownTable rows={[]} />);
    expect(container.firstChild).toBeNull();
  });

  it('formats large token counts with thousands separators', () => {
    render(
      <ModelBreakdownTable
        rows={[
          {
            provider: 'versa',
            model: 'gpt-5',
            inputTokens: 12_000_000,
            outputTokens: 1_300_000,
            totalTokens: 13_300_000,
            turns: 61,
            totalCost: 42,
          },
        ]}
      />
    );
    const row = screen.getByText('versa/gpt-5').closest('tr')!;
    const cells = within(row).getAllByRole('cell');
    expect(cells[2].textContent).toBe('12,000,000');
    expect(cells[3].textContent).toBe('1,300,000');
  });
});
