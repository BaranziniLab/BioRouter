import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { ChatTurnError, presentChatTurnError } from './ChatTurnError';

const QUOTA_ERROR =
  'Stream error: provider_failure: Request failed: Stream decode error: Responses API error: Object {"type": String("insufficient_quota"), "code": String("insufficient_quota"), "message": String("You exceeded your current quota, please check your plan and billing details."), "param": Null}';

describe('ChatTurnError', () => {
  it('extracts the provider message from the quota error shown in the reported failure', () => {
    expect(presentChatTurnError(QUOTA_ERROR)).toEqual({
      title: 'Model quota exceeded',
      message: 'You exceeded your current quota, please check your plan and billing details.',
      details: QUOTA_ERROR,
    });
  });

  it('renders a compact inline alert with collapsed technical details', () => {
    render(<ChatTurnError error={QUOTA_ERROR} />);

    const alert = screen.getByRole('alert');
    expect(alert).toHaveTextContent('Model quota exceeded');
    expect(alert).toHaveTextContent(
      'You exceeded your current quota, please check your plan and billing details.'
    );
    expect(screen.getByTestId('chat-turn-error')).toHaveClass('mt-4');

    const details = screen.getByText('Technical details').closest('details');
    expect(details).not.toHaveAttribute('open');
    fireEvent.click(screen.getByText('Technical details'));
    expect(details).toHaveAttribute('open');
    expect(alert).toHaveTextContent('provider_failure');
  });

  it('uses connection-specific recovery copy without dropping the original details', () => {
    const presentation = presentChatTurnError('Submit error: Failed to fetch');

    expect(presentation).toMatchObject({
      title: 'Model connection failed',
      message: expect.stringContaining('Check your connection and provider settings'),
      details: 'Submit error: Failed to fetch',
    });
  });
});
