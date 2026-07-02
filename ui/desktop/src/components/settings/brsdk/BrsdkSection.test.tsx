import { describe, it, expect, vi, beforeEach } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { BrsdkSection } from './BrsdkSection';

const mockUpsert = vi.fn();
let mockConfig: Record<string, unknown> = {};

vi.mock('../../ConfigContext', () => ({
  useConfig: () => ({
    config: mockConfig,
    upsert: mockUpsert,
  }),
}));

const toggles = [
  ['PII / PHI guardrail', 'brsdk_pii_guardrail'],
  ['Goal stop-hook guardrail', 'brsdk_llm_guardrails'],
  ['Encrypted vault', 'brsdk_encryption'],
  ['Agent tracing', 'brsdk_tracing'],
] as const;

const switchFor = (label: string) => screen.getByRole('switch', { name: `Toggle ${label}` });

describe('BrsdkSection', () => {
  beforeEach(() => {
    mockConfig = {};
    mockUpsert.mockReset();
    mockUpsert.mockResolvedValue(undefined);
  });

  it('defaults every App SDK safety framework switch off', () => {
    render(<BrsdkSection />);

    for (const [label] of toggles) {
      expect(switchFor(label)).toHaveAttribute('aria-checked', 'false');
    }
  });

  it('reflects persisted true values independently', () => {
    mockConfig = {
      brsdk_pii_guardrail: true,
      brsdk_tracing: true,
    };

    render(<BrsdkSection />);

    expect(switchFor('PII / PHI guardrail')).toHaveAttribute('aria-checked', 'true');
    expect(switchFor('Goal stop-hook guardrail')).toHaveAttribute('aria-checked', 'false');
    expect(switchFor('Encrypted vault')).toHaveAttribute('aria-checked', 'false');
    expect(switchFor('Agent tracing')).toHaveAttribute('aria-checked', 'true');
  });

  it('writes each backend config key as a non-secret setting', async () => {
    render(<BrsdkSection />);

    for (const [label, key] of toggles) {
      fireEvent.click(switchFor(label));
      await waitFor(() => {
        expect(mockUpsert).toHaveBeenCalledWith(key, true, false);
      });
    }
  });
});
