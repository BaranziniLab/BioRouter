import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { ChatTurnError, hasVisibleTurnErrorMessage, presentChatTurnError } from './ChatTurnError';
import type { Message } from '../../api';
import type { ChatTurnErrorData } from '../../types/turnError';

const QUOTA_ERROR =
  'Stream error: provider_failure: Request failed: Stream decode error: Responses API error: Object {"type": String("insufficient_quota"), "code": String("insufficient_quota"), "message": String("You exceeded your current quota, please check your plan and billing details."), "param": Null}';

function error(overrides: Partial<ChatTurnErrorData> = {}): ChatTurnErrorData {
  return {
    message: 'Provider rejected the request',
    code: 'provider_failure',
    scope: 'provider',
    retryable: false,
    ...overrides,
  };
}

describe('ChatTurnError', () => {
  it('presents known provider categories without depending on raw provider wording', () => {
    expect(presentChatTurnError(error({ providerKind: 'quota' }))).toMatchObject({
      title: 'Model quota exceeded',
      message: 'Provider rejected the request',
    });
    expect(presentChatTurnError(error({ providerKind: 'auth' }))).toMatchObject({
      title: 'Model authentication failed',
    });
  });

  it('extracts a provider message from the reported legacy quota payload', () => {
    expect(
      presentChatTurnError(error({ message: QUOTA_ERROR, technicalDetails: QUOTA_ERROR }))
    ).toEqual({
      title: 'Model quota exceeded',
      message: 'You exceeded your current quota, please check your plan and billing details.',
      details: QUOTA_ERROR,
    });
  });

  it('renders unknown future errors with a safe generic fallback', () => {
    render(
      <ChatTurnError
        error={error({
          providerKind: 'brand_new_rejection',
          message: 'A future provider-specific rejection',
        })}
      />
    );

    const alert = screen.getByRole('alert');
    expect(alert).toHaveTextContent('Model request failed');
    expect(alert).toHaveTextContent('A future provider-specific rejection');
  });

  it('keeps long technical details wrapped in a collapsed expandable region', () => {
    const details = `provider_failure: ${'very-long-provider-payload'.repeat(20)}`;
    render(
      <ChatTurnError
        error={error({
          providerKind: 'unknown',
          technicalDetails: details,
        })}
      />
    );

    const disclosure = screen.getByText('Technical details').closest('details');
    expect(disclosure).not.toHaveAttribute('open');
    fireEvent.click(screen.getByText('Technical details'));
    expect(disclosure).toHaveAttribute('open');

    const detailBody = screen.getByText(details);
    expect(detailBody).toHaveClass('break-words');
    expect(detailBody).toHaveClass('[overflow-wrap:anywhere]');
  });

  it('uses connection-specific recovery copy without dropping technical details', () => {
    const presentation = presentChatTurnError(
      error({
        message: 'Failed to fetch',
        scope: 'transport',
        technicalDetails: 'Submit error: Failed to fetch',
      })
    );

    expect(presentation).toMatchObject({
      title: 'Model connection failed',
      message: expect.stringContaining('Check your connection and provider settings'),
      details: 'Submit error: Failed to fetch',
    });
  });

  it('does not duplicate a backend error message that is already in the transcript', () => {
    const turnError = error({ message: 'Authentication failed. Status: 401 Unauthorized' });
    const messages = [
      {
        role: 'assistant',
        content: [
          {
            type: 'text',
            text: 'Ran into this error: Authentication failed. Status: 401 Unauthorized.',
          },
        ],
        metadata: { userVisible: true, agentVisible: true },
      },
    ] as Message[];

    expect(hasVisibleTurnErrorMessage(turnError, messages)).toBe(true);
    expect(hasVisibleTurnErrorMessage(error({ message: 'Failed to fetch' }), messages)).toBe(false);
  });
});
