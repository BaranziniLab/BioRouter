import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { getUsageReport, getUsageSummary } from '../../../api';
import UsageSection from './UsageSection';

vi.mock('../../../api', () => ({
  getUsageReport: vi.fn(),
  getUsageSummary: vi.fn(),
}));

beforeEach(() => {
  vi.mocked(getUsageSummary).mockReset();
  vi.mocked(getUsageReport).mockReset();
  vi.mocked(getUsageSummary).mockResolvedValue({
    data: {
      month: '2026-07',
      monthToDate: {
        inputTokens: 0,
        outputTokens: 0,
        totalTokens: 0,
        cacheReadTokens: 0,
        cacheCreationTokens: 0,
        turns: 0,
        cost: 0,
        hasUnpriced: false,
        costExcludesCache: false,
      },
      allTime: {
        inputTokens: 0,
        outputTokens: 0,
        totalTokens: 0,
        cacheReadTokens: 0,
        cacheCreationTokens: 0,
        turns: 0,
        cost: 0,
        hasUnpriced: false,
        costExcludesCache: false,
      },
      monthlyTokenLimit: null,
      monthlyDollarLimit: null,
      tokenPercent: null,
      dollarPercent: null,
    },
  } as never);
  vi.mocked(getUsageReport).mockResolvedValue({ data: { rows: [] } } as never);
});

describe('UsageSection', () => {
  it('uses shared pressed-state buttons and reloads when the range changes', async () => {
    const user = userEvent.setup();
    render(<UsageSection />);

    const sevenDays = screen.getByRole('button', { name: '7d' });
    const thirtyDays = screen.getByRole('button', { name: '30d' });
    const ninetyDays = screen.getByRole('button', { name: '90d' });

    for (const button of [sevenDays, thirtyDays, ninetyDays]) {
      expect(button).toHaveAttribute('data-slot', 'button');
    }
    expect(thirtyDays).toHaveAttribute('aria-pressed', 'true');
    expect(sevenDays).toHaveAttribute('aria-pressed', 'false');

    await screen.findByTestId('usage-panel');
    expect(getUsageSummary).toHaveBeenCalledTimes(1);
    expect(getUsageReport).toHaveBeenCalledTimes(2);

    await user.click(sevenDays);

    expect(sevenDays).toHaveAttribute('aria-pressed', 'true');
    expect(thirtyDays).toHaveAttribute('aria-pressed', 'false');
    await waitFor(() => {
      expect(getUsageSummary).toHaveBeenCalledTimes(2);
      expect(getUsageReport).toHaveBeenCalledTimes(4);
    });
  });

  it('keeps a failed fetch visible and offers a working retry', async () => {
    const user = userEvent.setup();
    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    vi.mocked(getUsageSummary).mockRejectedValueOnce(new Error('offline'));

    render(<UsageSection />);

    const alert = await screen.findByTestId('usage-load-error');
    expect(alert).toHaveAttribute('role', 'alert');
    expect(alert).toHaveTextContent('Usage data could not be loaded');
    expect(screen.getByRole('heading', { name: 'Usage' })).toBeTruthy();

    await user.click(screen.getByRole('button', { name: 'Retry' }));
    await screen.findByTestId('usage-panel');
    expect(screen.queryByTestId('usage-load-error')).toBeNull();

    consoleError.mockRestore();
  });
});
