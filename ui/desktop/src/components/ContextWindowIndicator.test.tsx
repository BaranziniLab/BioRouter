import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { ContextWindowGauge, ContextWindowIndicator } from './ContextWindowIndicator';

const mocks = vi.hoisted(() => ({
  readConfig: vi.fn(),
  upsertConfig: vi.fn(),
}));

vi.mock('../api', () => ({
  readConfig: mocks.readConfig,
  upsertConfig: mocks.upsertConfig,
}));

beforeAll(() => {
  vi.stubGlobal(
    'ResizeObserver',
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    }
  );
});

afterAll(() => {
  vi.unstubAllGlobals();
});

describe('ContextWindowGauge compaction control', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.readConfig.mockResolvedValue({ data: null });
    mocks.upsertConfig.mockResolvedValue({ data: null });
  });

  it('uses an inward compression icon and a BioRouter tooltip when disabled', async () => {
    const user = userEvent.setup();
    render(
      <ContextWindowGauge
        totalTokens={0}
        tokenLimit={1_100_000}
        isTokenLimitLoaded
        onCompact={vi.fn()}
      />
    );

    const button = screen.getByRole('button', { name: 'Nothing to compact yet' });
    expect(button).toBeDisabled();
    expect(button).not.toHaveAttribute('title');
    expect(screen.getByTestId('compact-conversation-icon')).toHaveClass('size-4');
    expect(screen.getByTestId('compact-conversation-icon')).toHaveAttribute('stroke-width', '1.75');

    await user.hover(button.parentElement!);
    const tooltip = await screen.findByRole('tooltip');
    expect(tooltip).toHaveTextContent('Nothing to compact yet');
    expect(document.querySelector('[data-slot="tooltip-content"]')).toHaveClass(
      'bg-background-accent',
      'text-text-on-accent'
    );
  });

  it('runs compaction from the enabled control', async () => {
    const user = userEvent.setup();
    const onCompact = vi.fn();
    render(
      <ContextWindowGauge
        totalTokens={24_000}
        tokenLimit={1_100_000}
        isTokenLimitLoaded
        onCompact={onCompact}
      />
    );

    const button = screen.getByRole('button', { name: 'Compact conversation' });
    await user.click(button);
    expect(onCompact).toHaveBeenCalledTimes(1);
  });

  it('uses a compact multiline context tooltip', async () => {
    const user = userEvent.setup();
    render(
      <ContextWindowIndicator
        totalTokens={0}
        tokenLimit={1_100_000}
        isTokenLimitLoaded
        onCompact={vi.fn()}
      />
    );

    const button = screen.getByRole('button', {
      name: 'Context window usage. 1.1M of 1.1M tokens remaining. 100% remaining, 0% used',
    });
    await user.hover(button);

    await screen.findByRole('tooltip');
    const tooltip = document.querySelector<HTMLElement>('[data-slot="tooltip-content"]');
    expect(tooltip).toHaveClass('w-52', 'text-left');
    const [titleLine, remainingLine, usageLine] = Array.from(tooltip!.children);
    expect(titleLine).toHaveClass('block', 'font-medium');
    expect(titleLine).toHaveTextContent('Context window usage');
    expect(remainingLine).toHaveClass('block');
    expect(remainingLine).toHaveTextContent('1.1M of 1.1M tokens remaining');
    expect(usageLine).toHaveClass('block');
    expect(usageLine).toHaveTextContent('100% remaining, 0% used');
  });
});
