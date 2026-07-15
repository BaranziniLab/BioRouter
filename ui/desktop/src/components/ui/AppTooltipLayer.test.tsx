import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { AppTooltipLayer } from './AppTooltipLayer';

function NativeTitleTarget({ title }: { title?: string }) {
  return (
    <button type="button" data-testid="native-title-target" title={title}>
      <span aria-hidden="true">?</span>
    </button>
  );
}

describe('AppTooltipLayer', () => {
  it('upgrades native titles to the BioRouter tooltip surface', async () => {
    render(
      <>
        <AppTooltipLayer />
        <NativeTitleTarget title="Native action" />
      </>
    );

    const target = screen.getByTestId('native-title-target');
    await waitFor(() => expect(target).toHaveAttribute('title', ''));
    expect(target).toHaveAttribute('data-biorouter-tooltip', 'Native action');
    expect(target).toHaveAccessibleName('Native action');

    fireEvent.pointerOver(target);
    const tooltip = await screen.findByRole('tooltip');
    expect(tooltip).toHaveTextContent('Native action');
    expect(tooltip).toHaveClass(
      'bg-background-inverse',
      'text-text-inverse',
      'rounded-sm',
      'font-sans'
    );
    expect(tooltip).not.toHaveClass('text-balance');
    expect(tooltip.querySelector('[aria-hidden="true"]')).toBeNull();
  });

  it('keeps dynamic tooltip text and generated accessible names synchronized', async () => {
    const { rerender } = render(
      <>
        <AppTooltipLayer />
        <NativeTitleTarget title="First action" />
      </>
    );

    const target = screen.getByTestId('native-title-target');
    await waitFor(() => expect(target).toHaveAttribute('data-biorouter-tooltip', 'First action'));

    rerender(
      <>
        <AppTooltipLayer />
        <NativeTitleTarget title="Updated action" />
      </>
    );
    await waitFor(() => expect(target).toHaveAttribute('data-biorouter-tooltip', 'Updated action'));
    expect(target).toHaveAccessibleName('Updated action');

    rerender(
      <>
        <AppTooltipLayer />
        <NativeTitleTarget />
      </>
    );
    await waitFor(() => expect(target).not.toHaveAttribute('data-biorouter-tooltip'));
    expect(target).not.toHaveAttribute('aria-label');
  });

  it('preserves intentional line breaks in compact native-title tooltips', async () => {
    render(
      <>
        <AppTooltipLayer />
        <NativeTitleTarget title={'Ships with BioRouter.\nRecreated automatically if deleted.'} />
      </>
    );

    const target = screen.getByTestId('native-title-target');
    await waitFor(() => expect(target).toHaveAttribute('title', ''));

    fireEvent.pointerOver(target);
    const tooltip = await screen.findByRole('tooltip');
    expect(tooltip).toHaveTextContent('Ships with BioRouter. Recreated automatically if deleted.', {
      normalizeWhitespace: true,
    });
    expect(tooltip).toHaveClass('whitespace-pre-line', 'text-left', 'leading-4');
  });

  it('uses intrinsic width for short action labels', async () => {
    render(
      <>
        <AppTooltipLayer />
        <NativeTitleTarget title="Delete local model" />
      </>
    );

    const target = screen.getByTestId('native-title-target');
    await waitFor(() => expect(target).toHaveAttribute('title', ''));

    fireEvent.pointerOver(target);
    const tooltip = await screen.findByRole('tooltip');
    expect(tooltip).toHaveTextContent('Delete local model');
    expect(tooltip).toHaveClass('w-max', 'max-w-[min(20rem,calc(100vw-16px))]', 'break-words');
  });

  it('does not open after the pointer leaves during the delay', async () => {
    render(
      <>
        <AppTooltipLayer />
        <NativeTitleTarget title="Delayed action" />
      </>
    );

    const target = screen.getByTestId('native-title-target');
    await waitFor(() => expect(target).toHaveAttribute('title', ''));

    fireEvent.pointerOver(target);
    fireEvent.pointerOut(target);
    await new Promise((resolve) => window.setTimeout(resolve, 550));

    expect(screen.queryByRole('tooltip')).not.toBeInTheDocument();
  });
});
